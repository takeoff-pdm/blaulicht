"""Kim's Mel-Band RoFormer (vocals) with its network on the Neural Engine.

    venv-sep/bin/python roformer_ane.py convert <models dir> <out dir> [--int8]
    venv-sep/bin/python roformer_ane.py run <models dir> <RoformerCore.mlmodelc> <audio> <out dir> [overlap] [batch]

audio-separator keeps everything it does around the network: chunking, overlap and windowing, the STFT,
gathering frequency bins into the 60 mel bands, scattering and averaging the masks, the iSTFT.
Only the network in the middle (band split -> 6 x (time transformer over 1101 frames, frequency
transformer over 60 bands) -> mask estimator) runs as Core ML: one 11 s chunk per call, [1, 1101, F]
gathered STFT features in, [1, 1101, F] masks out (F = 2 complex x 2 channels x bins over all bands).
Changes for fp16 on the Neural Engine, same math otherwise:
  - RMSNorm divides by the max-abs value first, so the sum of squares cannot overflow or underflow;
  - fixed sequence lengths, so the rotary embeddings are constants.
"""

import shutil
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

MODEL = "vocals_mel_band_roformer.ckpt"
FRAMES = 1101  # the config's inference dim_t: 11 s chunks (audio-separator with override_model_segment_size False)
XCRUN = ["env", "-i", f"HOME={Path.home()}", "PATH=/usr/bin:/bin", "DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer", "xcrun"]


def separator(models_dir, out_dir=".", overlap=2, batch=1):
    import logging

    from audio_separator.separator import Separator

    sep = Separator(output_dir=str(out_dir), model_file_dir=str(models_dir), output_format="WAV", log_level=logging.WARNING,
                    mdxc_params={"segment_size": 256, "override_model_segment_size": False, "batch_size": batch,
                                 "overlap": overlap, "pitch_shift": 0})
    sep.load_model(model_filename=MODEL)
    return sep


def robust_rmsnorm(self, x):
    m = x.abs().amax(-1, keepdim=True) + 1e-6
    y = x / m
    return y * torch.rsqrt((y * y).sum(-1, keepdim=True) + 1e-12) * self.scale * self.gamma


def _rotate_half(x, shape):
    B, H, N, Dh = shape
    x = x.reshape(B, H, N, Dh // 2, 2)
    return torch.stack((-x[..., 1], x[..., 0]), -1).reshape(B, H, N, Dh)


def fixed_attention(self, x):
    """Attention.forward with constant shapes (batch, seq) and precomputed rotary cos/sin; same math."""
    B, N, H, Dh = self.fixed
    x = self.norm(x)
    qkv = self.to_qkv(x).reshape(B, N, 3, H, Dh).permute(2, 0, 3, 1, 4)
    q, k, v = qkv[0], qkv[1], qkv[2]  # [B, H, N, Dh]
    q = q * self.rot_cos + _rotate_half(q, (B, H, N, Dh)) * self.rot_sin
    k = k * self.rot_cos + _rotate_half(k, (B, H, N, Dh)) * self.rot_sin
    w = torch.softmax(torch.matmul(q, k.transpose(-1, -2)) * Dh ** -0.5, dim=-1)
    out = torch.matmul(w, v) * torch.sigmoid(self.to_gates(x)).permute(0, 2, 1).reshape(B, H, N, 1)
    return self.to_out(out.permute(0, 2, 1, 3).reshape(B, N, H * Dh))


def fix_attention(attn, batch, seq):
    re = attn.rotary_embed
    pos = torch.arange(seq, dtype=torch.float32)
    fr = torch.repeat_interleave(torch.einsum("n,f->nf", pos, re.freqs.float()), 2, -1)  # [seq, Dh]
    attn.register_buffer("rot_cos", fr.cos(), persistent=False)
    attn.register_buffer("rot_sin", fr.sin(), persistent=False)
    attn.fixed = (batch, seq, attn.heads, fr.shape[-1])


class Core(nn.Module):
    def __init__(self, m, frames=FRAMES):
        super().__init__()
        self.band_split, self.layers, self.mask = m.band_split, m.layers, m.mask_estimators[0]
        self.shape = (1, frames, len(m.band_split.dim_inputs), m.layers[0][0].layers[0][0].to_out[0].out_features)
        b, t, f, d = self.shape
        for time_tf, freq_tf in self.layers:
            for attn, _ in time_tf.layers:
                fix_attention(attn, b * f, t)
            for attn, _ in freq_tf.layers:
                fix_attention(attn, b * t, f)

    def forward(self, x):  # x [1, T, F]
        x = self.band_split(x)  # [1, T, bands, d]
        b, t, f, d = self.shape
        for time_tf, freq_tf in self.layers:
            x = time_tf(x.permute(0, 2, 1, 3).reshape(b * f, t, d)).reshape(b, f, t, d).permute(0, 2, 1, 3)
            x = freq_tf(x.reshape(b * t, f, d)).reshape(b, t, f, d)
        return self.mask(x)


# --- Neural Engine layout: [batch, channels, 1, sequence], 1x1 convs, LayerNorm-style reductions over channels ---
def _conv(lin):
    c = nn.Conv2d(lin.in_features, lin.out_features, 1, bias=lin.bias is not None)
    c.weight.data = lin.weight.data.reshape(lin.out_features, lin.in_features, 1, 1).clone()
    if lin.bias is not None:
        c.bias.data = lin.bias.data.clone()
    return c


class NormC(nn.Module):
    """RMSNorm over channels (dim 1), with the max-abs prescale."""

    def __init__(self, norm):
        super().__init__()
        self.g = nn.Parameter((norm.gamma.data * norm.scale).reshape(1, -1, 1, 1).clone(), requires_grad=False)

    def forward(self, x):
        y = x / (x.abs().amax(1, keepdim=True) + 1e-6)
        return y * torch.rsqrt((y * y).sum(1, keepdim=True) + 1e-12) * self.g


class AttnC(nn.Module):
    def __init__(self, attn, batch, seq):
        super().__init__()
        self.B, self.N, self.H = batch, seq, attn.heads
        inner = attn.to_qkv.out_features // 3
        self.Dh = inner // self.H
        self.norm, self.qkv, self.gates, self.out = NormC(attn.norm), _conv(attn.to_qkv), _conv(attn.to_gates), _conv(attn.to_out[0])
        fr = torch.repeat_interleave(torch.einsum("n,f->nf", torch.arange(seq, dtype=torch.float32), attn.rotary_embed.freqs.float()), 2, -1)
        self.register_buffer("cos", fr.T.reshape(1, 1, self.Dh, seq).clone(), persistent=False)  # [1,1,Dh,N]
        self.register_buffer("sin", fr.T.reshape(1, 1, self.Dh, seq).sin().clone(), persistent=False)
        self.cos = self.cos.cos()
        # rotate_half (pairs (2i, 2i+1) -> (-x[2i+1], x[2i])) is a signed permutation of q's and k's output
        # channels, so fold it into extra convs instead of a stack (which ran on the CPU): same math.
        idx = torch.arange(inner)
        src = idx ^ 1                                  # partner channel within each pair
        sign = torch.where(idx % 2 == 0, -1.0, 1.0)
        w = self.qkv.weight.data.reshape(3, inner, -1)
        self.qrot = nn.Conv2d(w.shape[-1], inner, 1, bias=False)
        self.krot = nn.Conv2d(w.shape[-1], inner, 1, bias=False)
        self.qrot.weight.data = (w[0][src] * sign[:, None]).reshape(inner, -1, 1, 1).clone()
        self.krot.weight.data = (w[1][src] * sign[:, None]).reshape(inner, -1, 1, 1).clone()

    def forward(self, x):  # [B, C, 1, N]
        B, H, Dh, N = self.B, self.H, self.Dh, self.N
        h = self.norm(x)
        qkv = self.qkv(h).reshape(B, 3, H, Dh, N)
        qr, kr = self.qrot(h).reshape(B, H, Dh, N), self.krot(h).reshape(B, H, Dh, N)
        q = (qkv[:, 0] * self.cos + qr * self.sin) * Dh ** -0.5
        k = qkv[:, 1] * self.cos + kr * self.sin
        v = qkv[:, 2]
        w = torch.softmax(torch.matmul(q.transpose(2, 3), k), dim=-1)  # [B,H,N,N]
        o = torch.matmul(v, w.transpose(2, 3))  # [B,H,Dh,N]
        o = o * torch.sigmoid(self.gates(h)).reshape(B, H, 1, N)
        return self.out(o.reshape(B, H * Dh, 1, N))


class FFC(nn.Module):
    def __init__(self, ff):
        super().__init__()
        self.norm, self.a, self.b = NormC(ff.net[0]), _conv(ff.net[1]), _conv(ff.net[4])

    def forward(self, x):
        return self.b(F.gelu(self.a(self.norm(x))))


class TransformerC(nn.Module):
    def __init__(self, tf, batch, seq):
        super().__init__()
        self.layers = nn.ModuleList(nn.ModuleList([AttnC(a, batch, seq), FFC(f)]) for a, f in tf.layers)
        self.norm = NormC(tf.norm)

    def forward(self, x):
        for a, f in self.layers:
            x = a(x) + x
            x = f(x) + x
        return self.norm(x)


class CoreC(nn.Module):
    """Core in the Neural Engine layout. Band split and mask estimator keep the original [1, T, F] layout
    (they were already fast on the NE); the transformers run as [bands, C, 1, T] and [T, C, 1, bands]."""

    def __init__(self, m, frames=FRAMES):
        super().__init__()
        self.band_split, self.mask = m.band_split, m.mask_estimators[0]
        nb, d = len(m.band_split.dim_inputs), m.layers[0][0].layers[0][0].to_out[0].out_features
        self.shape = (frames, nb, d)
        self.layers = nn.ModuleList(nn.ModuleList([TransformerC(t, nb, frames), TransformerC(f, frames, nb)]) for t, f in m.layers)

    def forward(self, x):
        T, Fb, d = self.shape
        x = self.band_split(x).reshape(T, Fb, d)          # [T, bands, d]
        x = x.permute(1, 2, 0).reshape(Fb, d, 1, T)       # [bands, d, 1, T]
        for tt, ft in self.layers:
            x = tt(x)
            x = ft(x.permute(3, 1, 2, 0))                  # [T, d, 1, bands]
            x = x.permute(3, 1, 2, 0)                      # back to [bands, d, 1, T]
        x = x.reshape(Fb, d, T).permute(2, 0, 1).reshape(1, T, Fb, d)
        return self.mask(x)


def convert(models_dir, out_dir, int8=False, ane_layout=False):
    import coremltools as ct
    import coremltools.optimize.coreml as cto
    from audio_separator.separator.uvr_lib_v5.roformer import mel_band_roformer as mbr

    orig_norm, orig_attn = mbr.RMSNorm.forward, mbr.Attention.forward
    mbr.RMSNorm.forward = robust_rmsnorm
    mbr.Attention.forward = fixed_attention
    m = separator(models_dir).model_instance.model_run.cpu().float().eval()
    nfeat = sum(m.band_split.dim_inputs)
    core = (CoreC(m) if ane_layout else Core(m)).eval()
    ex = torch.randn(1, FRAMES, nfeat)
    with torch.no_grad():
        ref = core(ex)
        mbr.RMSNorm.forward, mbr.Attention.forward = orig_norm, orig_attn
        check = Core(m).eval()(ex) if ane_layout else core(ex)  # the original code on the same weights
        print("rewrite vs original: max abs diff", float((ref - check).abs().max()), "of max", float(check.abs().max()))
        mbr.RMSNorm.forward, mbr.Attention.forward = robust_rmsnorm, fixed_attention
        traced = torch.jit.trace(core, ex)
    ml = ct.convert(traced, convert_to="mlprogram", inputs=[ct.TensorType(name="x", shape=(1, FRAMES, nfeat), dtype=np.float16)],
                    outputs=[ct.TensorType(name="masks", dtype=np.float16)], minimum_deployment_target=ct.target.macOS15,
                    compute_precision=ct.precision.FLOAT16, skip_model_load=True)
    if int8:
        ml = cto.linear_quantize_weights(ml, cto.OptimizationConfig(global_config=cto.OpLinearQuantizerConfig(
            mode="linear_symmetric", dtype="int8", granularity="per_channel", weight_threshold=2048)))
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    name = "RoformerCore" + ("ANE" if ane_layout else "") + ("Int8" if int8 else "")
    pkg = out_dir / f"{name}.mlpackage"
    shutil.rmtree(pkg, ignore_errors=True)
    shutil.rmtree(out_dir / f"{name}.mlmodelc", ignore_errors=True)
    ml.save(str(pkg))
    subprocess.run(XCRUN + ["coremlcompiler", "compile", str(pkg), str(out_dir)], check=True, capture_output=True)
    shutil.rmtree(pkg)
    print(out_dir / f"{name}.mlmodelc", "features", nfeat, "ref std", float(ref.std()))


class CoreMLBlock(nn.Module):
    """Stands in for band_split; returns the masks. layers is emptied and mask_estimators[0] is identity."""

    def __init__(self, mlmodel):
        super().__init__()
        self.ml = mlmodel
        self.dummy = nn.Parameter(torch.zeros(1))  # forward() reads band_split's parameter dtype
        self.seconds, self.calls = 0.0, 0

    def forward(self, x):
        t = time.perf_counter()
        outs = []
        for i in range(x.shape[0]):
            xi = x[i: i + 1].detach().cpu().float().numpy().astype(np.float16)
            assert xi.shape[1] == FRAMES, xi.shape
            outs.append(torch.from_numpy(self.ml.predict({"x": xi})["masks"].astype(np.float32)))
        self.seconds += time.perf_counter() - t
        self.calls += x.shape[0]
        return torch.cat(outs).to(x.device)


def run(models_dir, mlmodelc, audio, out_dir, overlap=2, batch=1):
    import coremltools as ct

    sep = separator(models_dir, out_dir, overlap, batch)
    m = sep.model_instance.model_run
    import os

    units = {"ane": ct.ComputeUnit.CPU_AND_NE, "gpu": ct.ComputeUnit.CPU_AND_GPU}[os.environ.get("RF_UNITS", "ane")]
    block = CoreMLBlock(ct.models.CompiledMLModel(str(mlmodelc), compute_units=units))
    m.band_split = block.to(next(m.parameters()).device)
    m.layers = nn.ModuleList()
    m.mask_estimators = nn.ModuleList([nn.Identity()])
    t = time.perf_counter()
    out = sep.separate(str(audio))
    print(f"overlap {overlap}: separate {time.perf_counter() - t:.1f} s, Core ML {block.seconds:.1f} s for {block.calls} chunks"
          f" ({1000 * block.seconds / max(block.calls, 1):.0f} ms each) ->", out)


def profile(models_dir, out_dir):
    """Convert and time each part alone: band split, one time transformer, one frequency transformer, mask estimator."""
    import coremltools as ct
    from audio_separator.separator.uvr_lib_v5.roformer import mel_band_roformer as mbr

    mbr.RMSNorm.forward, mbr.Attention.forward = robust_rmsnorm, fixed_attention
    m = separator(models_dir).model_instance.model_run.cpu().float().eval()
    core = Core(m)
    b, t, f, d = core.shape
    nfeat = sum(m.band_split.dim_inputs)
    parts = {"split": (m.band_split, (1, t, nfeat)), "time": (m.layers[0][0], (f, t, d)),
             "freq": (m.layers[0][1], (t, f, d)), "mask": (m.mask_estimators[0], (1, t, f, d))}
    out_dir = Path(out_dir)
    for name, (mod, shape) in parts.items():
        with torch.no_grad():
            traced = torch.jit.trace(mod.eval(), torch.randn(shape))
        ml = ct.convert(traced, convert_to="mlprogram", inputs=[ct.TensorType(name="x", shape=shape, dtype=np.float16)],
                        minimum_deployment_target=ct.target.macOS15, compute_precision=ct.precision.FLOAT16, skip_model_load=True)
        pkg = out_dir / f"part_{name}.mlpackage"
        shutil.rmtree(pkg, ignore_errors=True)
        ml.save(str(pkg))
        subprocess.run(XCRUN + ["coremlcompiler", "compile", str(pkg), str(out_dir)], check=True, capture_output=True)
        shutil.rmtree(pkg)
        mc = out_dir / f"part_{name}.mlmodelc"
        diag = str(Path(__file__).resolve().parent / "diag" / "diag")
        for dev in ("ane", "gpu"):
            r = subprocess.run([str(Path.home() / "work/ane-lyrics/diag"), "bench", str(mc), dev, "5"], capture_output=True, text=True)
            print(name, shape, r.stdout.strip(), flush=True)
        shutil.rmtree(mc)


if __name__ == "__main__":
    a = sys.argv[1:]
    if a[0] == "profile":
        profile(a[1], a[2])
    elif a[0] == "convert":
        convert(a[1], a[2], "--int8" in a, "--ane" in a)
    else:
        run(a[1], a[2], a[3], a[4], *(int(v) for v in a[5:7]))