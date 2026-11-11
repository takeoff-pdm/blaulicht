"""htdemucs_ft (the stems teacher) with its network on Core ML (GPU or Neural Engine).

    python htdemucs_coreml.py convert <out folder>                     four .mlmodelc, one per bag member
    python htdemucs_coreml.py run <models folder> <audio> <out.npy>    stems like stems/teacher.py (RF_UNITS=gpu|ane|cpu)
    python htdemucs_coreml.py ref <audio> <out.npy>                    the same with PyTorch (DEMUCS_DEVICE=mps|cpu)

Same idea as roformer_ane.py: only the network runs in Core ML. The STFT, the complex mask and the
inverse STFT stay in PyTorch on the CPU (Core ML has no complex numbers), and demucs' own apply_model
does the chunking, overlap and shifts, so the output matches demucs' output up to fp16 rounding.
Each bag member is traced at the training segment (7.8 s, 343980 samples), the only length apply_model
passes to it.
"""

import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
import torch
import torch.nn as nn

XCRUN = ["env", "-i", f"HOME={Path.home()}", "PATH=/usr/bin:/bin", "DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer", "xcrun"]
SR = 44_100


class Net(nn.Module):
    """HTDemucs.forward between the STFT and the mask: (mix, magnitude) -> (spectral out, time out)."""

    def __init__(self, m):
        super().__init__()
        self.m = m

    def forward(self, mix, mag):
        m = self.m
        x = mag
        B, C, Fq, T = x.shape
        mean = x.mean(dim=(1, 2, 3), keepdim=True)
        std = x.std(dim=(1, 2, 3), keepdim=True)
        x = (x - mean) / (1e-5 + std)
        xt = mix
        meant = xt.mean(dim=(1, 2), keepdim=True)
        stdt = xt.std(dim=(1, 2), keepdim=True)
        xt = (xt - meant) / (1e-5 + stdt)
        saved, saved_t, lengths, lengths_t = [], [], [], []
        for idx, encode in enumerate(m.encoder):
            lengths.append(x.shape[-1])
            inject = None
            if idx < len(m.tencoder):
                lengths_t.append(xt.shape[-1])
                tenc = m.tencoder[idx]
                xt = tenc(xt)
                if not tenc.empty:
                    saved_t.append(xt)
                else:
                    inject = xt
            x = encode(x, inject)
            if idx == 0 and m.freq_emb is not None:
                frs = torch.arange(x.shape[-2], device=x.device)
                emb = m.freq_emb(frs).t()[None, :, :, None].expand_as(x)
                x = x + m.freq_emb_scale * emb
            saved.append(x)
        if m.crosstransformer:
            if m.bottom_channels:
                b, c, f, t = x.shape
                x = m.channel_upsampler(x.reshape(b, c, f * t)).reshape(b, -1, f, t)
                xt = m.channel_upsampler_t(xt)
            x, xt = m.crosstransformer(x, xt)
            if m.bottom_channels:
                b, c, f, t = x.shape
                x = m.channel_downsampler(x.reshape(b, c, f * t)).reshape(b, -1, f, t)
                xt = m.channel_downsampler_t(xt)
        for idx, decode in enumerate(m.decoder):
            skip = saved.pop(-1)
            x, pre = decode(x, skip, lengths.pop(-1))
            offset = m.depth - len(m.tdecoder)
            if idx >= offset:
                tdec = m.tdecoder[idx - offset]
                length_t = lengths_t.pop(-1)
                if tdec.empty:
                    xt, _ = tdec(pre[:, :, 0], None, length_t)
                else:
                    xt, _ = tdec(xt, saved_t.pop(-1), length_t)
        S = len(m.sources)
        x = x.view(B, S, -1, Fq, T) * std[:, None] + mean[:, None]
        xt = xt.view(B, S, -1, mix.shape[-1]) * stdt[:, None] + meant[:, None]
        return x, xt


def bag():
    from demucs.pretrained import get_model
    return get_model("htdemucs_ft").cpu().eval()


def seg_len(m):
    return int(m.segment * m.samplerate)


def _fix_int_cast():
    """coremltools' int() of a folded (1,)-shaped constant calls int(array), which numpy 2 refuses."""
    from coremltools.converters.mil import Builder as mb
    from coremltools.converters.mil.frontend.torch import ops
    from coremltools.converters.mil.frontend.torch.torch_op_registry import register_torch_op

    @register_torch_op(torch_alias=["int"], override=True)
    def _int(context, node):
        x = ops._get_inputs(context, node, expected=1)[0]
        if x.can_be_folded_to_const():
            context.add(mb.const(val=int(np.asarray(x.val).reshape(-1)[0]), name=node.name))
        else:
            ops._cast(context, node, int, "int32")


def convert(out_dir):
    import coremltools as ct
    _fix_int_cast()
    torch.backends.mha.set_fastpath_enabled(False)  # the fused MHA op has no Core ML conversion
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    b = bag()
    for i, m in enumerate(b.models):
        L = seg_len(m)
        mix = torch.randn(1, 2, L)
        mag = m._magnitude(m._spec(mix))
        net = Net(m).eval()
        with torch.no_grad():
            ref = net(mix, mag)
            traced = torch.jit.trace(net, (mix, mag))
        ml = ct.convert(traced, convert_to="mlprogram",
                        inputs=[ct.TensorType(name="mix", shape=tuple(mix.shape), dtype=np.float32),
                                ct.TensorType(name="mag", shape=tuple(mag.shape), dtype=np.float32)],
                        outputs=[ct.TensorType(name="spec_out", dtype=np.float32), ct.TensorType(name="time_out", dtype=np.float32)],
                        minimum_deployment_target=ct.target.macOS15, compute_precision=ct.precision.FLOAT16, skip_model_load=True)
        name = f"HTDemucs{i}"
        pkg = out_dir / f"{name}.mlpackage"
        shutil.rmtree(pkg, ignore_errors=True)
        shutil.rmtree(out_dir / f"{name}.mlmodelc", ignore_errors=True)
        ml.save(str(pkg))
        subprocess.run(XCRUN + ["coremlcompiler", "compile", str(pkg), str(out_dir)], check=True, capture_output=True)
        shutil.rmtree(pkg)
        print(out_dir / f"{name}.mlmodelc", "mix", tuple(mix.shape), "mag", tuple(mag.shape),
              "out", tuple(ref[0].shape), tuple(ref[1].shape), flush=True)


def patch(b, models_dir, units):
    """Replace each member's forward with STFT (torch) -> Core ML net -> mask + iSTFT (torch)."""
    import coremltools as ct
    cu = {"gpu": ct.ComputeUnit.CPU_AND_GPU, "ane": ct.ComputeUnit.CPU_AND_NE, "cpu": ct.ComputeUnit.CPU_ONLY}[units]
    stats = {"calls": 0, "net_s": 0.0}
    for i, m in enumerate(b.models):
        cm = ct.models.CompiledMLModel(str(Path(models_dir) / f"HTDemucs{i}.mlmodelc"), compute_units=cu)
        L = seg_len(m)

        def fwd(mix, m=m, cm=cm, L=L):
            assert mix.shape[-1] == L and mix.shape[0] == 1, mix.shape
            z = m._spec(mix)
            mag = m._magnitude(z)
            t0 = time.perf_counter()
            o = cm.predict({"mix": mix.numpy().astype(np.float32), "mag": mag.numpy().astype(np.float32)})
            stats["net_s"] += time.perf_counter() - t0
            stats["calls"] += 1
            x = torch.from_numpy(o["spec_out"]).float()
            xt = torch.from_numpy(o["time_out"]).float()
            return xt + m._ispec(m._mask(z, x), L)

        m.forward = fwd
    return stats


def load_audio(path):
    pcm = subprocess.run(["ffmpeg", "-v", "error", "-i", str(path), "-ac", "2", "-ar", str(SR), "-f", "f32le", "-"],
                         check=True, capture_output=True).stdout
    return torch.from_numpy(np.frombuffer(pcm, np.float32).reshape(-1, 2).T.copy())


def separate(b, wav, device):
    """Same call as stems/teacher.py."""
    import random
    from demucs.apply import apply_model
    random.seed(int(os.environ.get("DEMUCS_SEED", "0")))  # apply_model's shift is random; fixed for comparisons
    ref = wav.mean(0)
    w = (wav - ref.mean()) / ref.std()
    with torch.no_grad():
        out = apply_model(b, w[None], device=device, shifts=int(os.environ.get("DEMUCS_SHIFTS", "1")), split=True, overlap=0.25, progress=False)[0]
    return out * ref.std() + ref.mean()


def main():
    a = sys.argv[1:]
    if a[0] == "convert":
        convert(a[1])
        return
    b = bag()
    if a[0] == "run":
        stats = patch(b, a[1], os.environ.get("RF_UNITS", "gpu"))
        wav, out, device = load_audio(a[2]), a[3], "cpu"
    else:
        stats, wav, out, device = None, load_audio(a[1]), a[2], os.environ.get("DEMUCS_DEVICE", "mps")
        b.to(device)
    t0 = time.perf_counter()
    stems = separate(b, wav, device).cpu().numpy()
    dt = time.perf_counter() - t0
    np.save(out, stems.astype(np.float32))
    print(f"{a[0]}: {dt:.1f} s for {wav.shape[-1] / SR:.0f} s of audio", stats or "", flush=True)


if __name__ == "__main__":
    main()
