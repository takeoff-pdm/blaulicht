"""Whisper large-v3 decoder rebuilt for the Neural Engine (Core ML, all ops on the ANE), version 1.

    venv-ct/bin/python ane_decoder.py test <n layers>              random weights: convert, place, time
    venv-ct/bin/python ane_decoder.py build <model.safetensors> <out folder>

Same math as WhisperKit's TextDecoder (and OpenAI's), reorganised so it fits the Neural Engine:
  - two decoder halves of 16 layers (each under the Neural Engine's 1 GiB weight limit);
  - the self-attention key/value cache lives in Core ML state on the device (no 70 MB copy per token);
  - the cross-attention keys/values are computed once per 30 s window by a separate CrossKV model and
    kept in state, instead of being recomputed from the encoder output for every token (315 GFLOP/token);
  - the token/position embedding lookup (a gather, which the Neural Engine cannot run) is done by the
    caller: the halves take the summed embedding x [1, 1280, 1, 1].
Layout follows WhisperKit/ml-ane-transformers: [batch, channels, 1, sequence], 1x1 convs for linear
layers, the same matmul shapes for attention, LayerNorm over channels.
Weights: model.safetensors of openai/whisper-large-v3 (Hugging Face layout).
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path

import coremltools as ct
import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

C, H, D, CTX, FRAMES, VOCAB, LAYERS = 1280, 20, 64, 448, 1500, 51866, 32
# Neural Engine buffers want 64-byte rows: 1500 fp16 frames (3000 B) are not, so the cross-attention
# state rows are 1536 frames; the caller fills the first 1500 and the model slices them off.
FRAMES_STATE = 1536
# large-v3 alignment heads (layer, head), from openai/whisper-large-v3 generation_config.json
ALIGN = [(7, 0), (10, 17), (12, 18), (13, 12), (16, 1), (17, 14), (19, 11), (21, 4), (24, 1), (25, 6)]
HERE = Path(__file__).resolve().parent
XCRUN = ["env", "-i", f"HOME={Path.home()}", "PATH=/usr/bin:/bin", "DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer", "xcrun"]


class Weights:
    def __init__(self, path=None, seed=0):
        self.f = None
        if path:
            from safetensors import safe_open
            self.f = safe_open(str(path), "pt")
        self.g = torch.Generator().manual_seed(seed)

    def __call__(self, key, shape):
        if self.f is not None:
            t = self.f.get_tensor(key).float()
            assert tuple(t.shape) == tuple(shape), (key, t.shape, shape)
            return t
        if key.endswith("layer_norm.weight"):
            return torch.ones(shape)
        return torch.randn(shape, generator=self.g) * 0.02


def conv(w, b=None, scale=1.0):
    m = nn.Conv2d(w.shape[1], w.shape[0], 1, bias=b is not None)
    m.weight.data = (w * scale).reshape(w.shape[0], w.shape[1], 1, 1)
    if b is not None:
        m.bias.data = b * scale
    m.requires_grad_(False)
    return m


class LayerNorm(nn.Module):
    def __init__(self, w, b, eps=1e-5):
        super().__init__()
        self.w = nn.Parameter(w.reshape(1, -1, 1, 1), requires_grad=False)
        self.b = nn.Parameter(b.reshape(1, -1, 1, 1), requires_grad=False)
        self.eps = eps

    def forward(self, x):
        z = x - x.mean(1, keepdim=True)
        return z * torch.rsqrt((z * z).mean(1, keepdim=True) + self.eps) * self.w + self.b


class Layer(nn.Module):
    def __init__(self, W, i):
        super().__init__()
        p, s = f"model.decoder.layers.{i}.", D ** -0.5
        ln = lambda n: LayerNorm(W(p + n + ".weight", (C,)), W(p + n + ".bias", (C,)))
        lin = lambda n, bias=True, scale=1.0, o=C, k=C: conv(W(p + n + ".weight", (o, k)), W(p + n + ".bias", (o,)) if bias else None, scale)
        self.ln1, self.ln2, self.ln3 = ln("self_attn_layer_norm"), ln("encoder_attn_layer_norm"), ln("final_layer_norm")
        self.q, self.k = lin("self_attn.q_proj", scale=s), lin("self_attn.k_proj", bias=False)
        self.v, self.o = lin("self_attn.v_proj"), lin("self_attn.out_proj")
        self.cq, self.co = lin("encoder_attn.q_proj", scale=s), lin("encoder_attn.out_proj")
        self.fc1, self.fc2 = lin("fc1", o=4 * C, k=C), lin("fc2", o=C, k=4 * C)

    def forward(self, x, K, V, CK, CV, m, kpm):
        h = self.ln1(x)
        q = self.q(h).reshape(1, H, D, 1)
        K = K * (1 - m) + self.k(h) * m
        V = V * (1 - m) + self.v(h) * m
        w = torch.softmax(torch.matmul(q.transpose(2, 3), K.reshape(1, H, D, CTX)) + kpm, dim=-1)
        x = x + self.o(torch.matmul(V.reshape(1, H, D, CTX), w.transpose(2, 3)).reshape(1, C, 1, 1))
        q = self.cq(self.ln2(x)).reshape(1, H, D, 1)
        cw = torch.softmax(torch.matmul(q.transpose(2, 3), CK.reshape(1, H, D, FRAMES)), dim=-1)
        x = x + self.co(torch.matmul(CV.reshape(1, H, D, FRAMES), cw.transpose(2, 3)).reshape(1, C, 1, 1))
        x = x + self.fc2(F.gelu(self.fc1(self.ln3(x))))
        return x, K, V, cw


class Half(nn.Module):
    """Decoder layers [first, first + n). States: self-attention cache and cross-attention keys/values."""

    def __init__(self, W, first, n, last):
        super().__init__()
        self.first, self.n, self.last = first, n, last
        self.layers = nn.ModuleList(Layer(W, i) for i in range(first, first + n))
        for name, t in (("k_cache", CTX), ("v_cache", CTX), ("cross_k", FRAMES_STATE), ("cross_v", FRAMES_STATE)):
            self.register_buffer(name, torch.zeros(1, n * C, 1, t, dtype=torch.float16))
        if last:
            self.ln = LayerNorm(W("model.decoder.layer_norm.weight", (C,)), W("model.decoder.layer_norm.bias", (C,)))
            self.head = conv(W("model.decoder.embed_tokens.weight", (VOCAB, C)))

    def forward(self, x, update_mask, key_padding_mask):
        m = update_mask.reshape(1, 1, 1, CTX)
        kpm = key_padding_mask.reshape(1, 1, 1, CTX)
        ks, vs, rows = [], [], []
        for j, layer in enumerate(self.layers):
            sl = slice(j * C, (j + 1) * C)
            x, K, V, cw = layer(x, self.k_cache[:, sl].float(), self.v_cache[:, sl].float(),
                                self.cross_k[:, sl, :, :FRAMES].float(), self.cross_v[:, sl, :, :FRAMES].float(), m, kpm)
            ks.append(K)
            vs.append(V)
            for li, hd in ALIGN:
                if li == self.first + j:
                    rows.append(cw[:, hd])  # [1, 1, FRAMES]: this alignment head's cross-attention probabilities
        self.k_cache[:, :, :, :] = torch.cat(ks, 1)
        self.v_cache[:, :, :, :] = torch.cat(vs, 1)
        align = torch.cat(rows, 1)  # [1, heads in this half, FRAMES], in ALIGN order
        if self.last:
            return self.head(self.ln(x)).reshape(1, 1, VOCAB), align
        return x, align


class CrossKV(nn.Module):
    """Encoder output -> cross-attention keys/values for both halves (run once per 30 s window)."""

    def __init__(self, W, split=16):
        super().__init__()
        p = "model.decoder.layers.{}.encoder_attn."
        def stack(kind, layers, bias):
            w = torch.cat([W(p.format(i) + kind + ".weight", (C, C)) for i in layers])
            b = torch.cat([W(p.format(i) + kind + ".bias", (C,)) for i in layers]) if bias else None
            return conv(w, b)
        a, b = range(split), range(split, LAYERS)
        self.ka, self.va = stack("k_proj", a, False), stack("v_proj", a, True)
        self.kb, self.vb = stack("k_proj", b, False), stack("v_proj", b, True)

    def forward(self, encoder_output_embeds):
        # Unpadded (padding with concat would run on the CPU); the caller copies into 1536-frame state rows.
        e = encoder_output_embeds
        return self.ka(e), self.va(e), self.kb(e), self.vb(e)


def convert(module, inputs, outputs, states, path):
    module.eval()
    ex = [torch.zeros(tuple(i.shape.shape)) for i in inputs]
    with torch.no_grad():
        traced = torch.jit.trace(module, ex)
    ml = ct.convert(
        traced, convert_to="mlprogram", inputs=inputs, outputs=outputs, states=states or None,
        minimum_deployment_target=ct.target.macOS15, compute_precision=ct.precision.FLOAT16, skip_model_load=True)
    ml.save(str(path))
    return path


def compile_model(pkg, out_dir):
    subprocess.run(XCRUN + ["coremlcompiler", "compile", str(pkg), str(out_dir)], check=True, capture_output=True)
    shutil.rmtree(pkg)
    return out_dir / (pkg.stem + ".mlmodelc")


def half_io(n, last):
    st = lambda name, t: ct.StateType(wrapped_type=ct.TensorType(shape=(1, n * C, 1, t), dtype=np.float16), name=name)
    inputs = [ct.TensorType(name="x", shape=(1, C, 1, 1), dtype=np.float16),
              ct.TensorType(name="update_mask", shape=(1, CTX), dtype=np.float16),
              ct.TensorType(name="key_padding_mask", shape=(1, CTX), dtype=np.float16)]
    outputs = [ct.TensorType(name="logits" if last else "x_out", dtype=np.float16), ct.TensorType(name="align", dtype=np.float16)]
    states = [st("k_cache", CTX), st("v_cache", CTX), st("cross_k", FRAMES_STATE), st("cross_v", FRAMES_STATE)]
    return inputs, outputs, states


def build_half(W, first, n, last, out_dir, name):
    inputs, outputs, states = half_io(n, last)
    return compile_model(convert(Half(W, first, n, last), inputs, outputs, states, out_dir / f"{name}.mlpackage"), out_dir)


def build_crosskv(W, out_dir):
    inputs = [ct.TensorType(name="encoder_output_embeds", shape=(1, C, 1, FRAMES), dtype=np.float16)]
    # fp32 outputs: coremltools' MLState.write_state takes fp32 arrays
    outputs = [ct.TensorType(name=n, dtype=np.float32) for n in ("cross_k_a", "cross_v_a", "cross_k_b", "cross_v_b")]
    return compile_model(convert(CrossKV(W), inputs, outputs, [], out_dir / "CrossKV.mlpackage"), out_dir)


def export_embeddings(W, out_dir):
    W("model.decoder.embed_tokens.weight", (VOCAB, C)).half().numpy().tofile(out_dir / "token_embedding.f16")
    W("model.decoder.embed_positions.weight", (CTX, C)).half().numpy().tofile(out_dir / "position_embedding.f16")


def main():
    if sys.argv[1] == "test":
        n = int(sys.argv[2])
        out = HERE / "anetest"
        out.mkdir(exist_ok=True)
        print(build_half(Weights(), 0, n, False, out, f"Half{n}"))
    elif sys.argv[1] == "build":
        W, out = Weights(sys.argv[2]), Path(sys.argv[3])
        out.mkdir(parents=True, exist_ok=True)
        export_embeddings(W, out)
        print(build_crosskv(W, out), flush=True)
        print(build_half(W, 0, 16, False, out, "DecoderA"), flush=True)
        print(build_half(W, 16, 16, True, out, "DecoderB"), flush=True)
        (out / "ane_decoder.json").write_text(json.dumps({"layers": LAYERS, "split": 16, "align_heads": ALIGN, "align_output": "per head"}))


if __name__ == "__main__":
    main()
