"""Whisper large-v3 decoder for the Neural Engine, version 2 (faster; not yet built or measured).

    venv-ct/bin/python ane_decoder2.py test <n layers>                 random weights, small
    venv-ct/bin/python ane_decoder2.py build <model.safetensors> <out folder> [--fp16] [--split 16|32]

Changes vs ane_decoder.py:
  - int8 weights (per-channel symmetric). The Neural Engine decompresses them on the fly; in a synthetic
    test that halved a bandwidth-bound token (16.1 ms -> 8.2 ms).
  - Each half is ONE multifunction model whose functions share the same Core ML state (tested to work):
      fill   encoder output (padded to 1536 frames by the caller) -> cross-attention keys/values, written
             straight into state (replaces CrossKV + Python write_state, which cost ~0.9 s per window)
      step   one token (generation)
      multi  MULTI tokens at once (prompt, the whole word-alignment pass, later speculative verification)
    Weights are shared between functions when saved.
  - Generic masks: update_mask [1, S, 448] one-hot rows (all-zero row = padding token, writes nothing),
    attn_mask [1, S, 448] additive (causal and padding built by the caller).
  - --split 32 puts all layers in one model, if int8 weights count at their compressed size against the
    1 GiB Neural Engine limit (unverified: check placement with diag plan after building).
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path

import coremltools as ct
import coremltools.optimize.coreml as cto
import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

from ane_decoder import ALIGN, C, CTX, D, FRAMES, FRAMES_STATE, H, LAYERS, VOCAB, XCRUN, LayerNorm, Weights, conv, export_embeddings

MULTI = 64
HERE = Path(__file__).resolve().parent


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
        self.ck, self.cv = lin("encoder_attn.k_proj", bias=False), lin("encoder_attn.v_proj")
        self.fc1, self.fc2 = lin("fc1", o=4 * C, k=C), lin("fc2", o=C, k=4 * C)

    def forward(self, x, K, V, CK, CV, M, keep, mask, S):
        """x [1,C,1,S]; K,V [1,C,1,CTX]; CK,CV [1,C,1,FRAMES]; M [S,CTX] one-hot; keep [1,1,1,CTX]; mask [1,1,S,CTX]."""
        h = self.ln1(x)
        K = K * keep + torch.matmul(self.k(h).reshape(C, S), M).reshape(1, C, 1, CTX)
        V = V * keep + torch.matmul(self.v(h).reshape(C, S), M).reshape(1, C, 1, CTX)
        q = self.q(h).reshape(1, H, D, S)
        w = torch.softmax(torch.matmul(q.transpose(2, 3), K.reshape(1, H, D, CTX)) + mask, dim=-1)       # [1,H,S,CTX]
        x = x + self.o(torch.matmul(V.reshape(1, H, D, CTX), w.transpose(2, 3)).reshape(1, C, 1, S))
        q = self.cq(self.ln2(x)).reshape(1, H, D, S)
        cw = torch.softmax(torch.matmul(q.transpose(2, 3), CK.reshape(1, H, D, FRAMES)), dim=-1)         # [1,H,S,FRAMES]
        x = x + self.co(torch.matmul(CV.reshape(1, H, D, FRAMES), cw.transpose(2, 3)).reshape(1, C, 1, S))
        x = x + self.fc2(F.gelu(self.fc1(self.ln3(x))))
        return x, K, V, cw


class Half(nn.Module):
    """Layers [first, first+n). mode: 'fill' | 'tokens'. All modes declare the same four states."""

    def __init__(self, layers, first, last, mode, W, S=1):
        super().__init__()
        self.S = S  # static token count (traced shapes must be constants)
        self.layers, self.first, self.n, self.last, self.mode = layers, first, len(layers), last, mode
        for name, t in (("k_cache", CTX), ("v_cache", CTX), ("cross_k", FRAMES_STATE), ("cross_v", FRAMES_STATE)):
            self.register_buffer(name, torch.zeros(1, self.n * C, 1, t, dtype=torch.float16))
        if last and mode == "tokens":
            self.ln = LayerNorm(W("model.decoder.layer_norm.weight", (C,)), W("model.decoder.layer_norm.bias", (C,)))
            self.head = conv(W("model.decoder.embed_tokens.weight", (VOCAB, C)))

    def forward(self, *args):
        return self.fill(*args) if self.mode == "fill" else self.tokens(*args)

    def fill(self, enc):
        # enc [1, C, 1, 1536] (frames 1500.. are zero padding from the caller): no concat on the device
        self.cross_k[:, :, :, :] = torch.cat([l.ck(enc) for l in self.layers], 1).half()
        self.cross_v[:, :, :, :] = torch.cat([l.cv(enc) for l in self.layers], 1).half()
        # keep the self-attention states referenced so every function declares the same states
        return enc[:, :1, :, :1] * 0 + self.k_cache[:, :1, :, :1].float() * 0 + self.v_cache[:, :1, :, :1].float() * 0

    def tokens(self, x, update_mask, attn_mask):
        S = self.S
        M = update_mask.reshape(S, CTX)
        keep = 1 - M.sum(0).reshape(1, 1, 1, CTX)
        mask = attn_mask.reshape(1, 1, S, CTX)
        ks, vs, rows = [], [], []
        for j, layer in enumerate(self.layers):
            sl = slice(j * C, (j + 1) * C)
            x, K, V, cw = layer(x, self.k_cache[:, sl].float(), self.v_cache[:, sl].float(),
                                self.cross_k[:, sl, :, :FRAMES].float(), self.cross_v[:, sl, :, :FRAMES].float(), M, keep, mask, S)
            ks.append(K)
            vs.append(V)
            rows += [cw[:, hd] for li, hd in ALIGN if li == self.first + j]  # [1, S, FRAMES]
        self.k_cache[:, :, :, :] = torch.cat(ks, 1).half()
        self.v_cache[:, :, :, :] = torch.cat(vs, 1).half()
        align = torch.stack(rows, 1) if rows else x[:, :1, :, :1] * 0  # [1, heads here, S, FRAMES]
        if self.last:
            return self.head(self.ln(x)).reshape(1, VOCAB, S), align
        return x, align


def states(n):
    st = lambda name, t: ct.StateType(wrapped_type=ct.TensorType(shape=(1, n * C, 1, t), dtype=np.float16), name=name)
    return [st("k_cache", CTX), st("v_cache", CTX), st("cross_k", FRAMES_STATE), st("cross_v", FRAMES_STATE)]


def convert(module, inputs, outputs, n, int8):
    module.eval()
    with torch.no_grad():
        traced = torch.jit.trace(module, [torch.zeros(tuple(i.shape.shape)) for i in inputs])
    ml = ct.convert(traced, convert_to="mlprogram", inputs=inputs, outputs=outputs, states=states(n),
                    minimum_deployment_target=ct.target.macOS15, compute_precision=ct.precision.FLOAT16, skip_model_load=True)
    if int8:
        ml = cto.linear_quantize_weights(ml, cto.OptimizationConfig(global_config=cto.OpLinearQuantizerConfig(
            mode="linear_symmetric", dtype="int8", granularity="per_channel", weight_threshold=2048)))
    return ml


def build_half(W, first, n, last, out_dir, name, int8=True):
    layers = nn.ModuleList(Layer(W, i) for i in range(first, first + n))
    f16 = np.float16
    pkgs = {}
    fill_in = [ct.TensorType(name="encoder_output", shape=(1, C, 1, FRAMES_STATE), dtype=f16)]
    pkgs["fill"] = convert(Half(layers, first, last, "fill", W), fill_in, [ct.TensorType(name="done", dtype=f16)], n, int8)
    out = "logits" if last else "x_out"
    for fn, S in (("step", 1), ("multi", MULTI)):
        ins = [ct.TensorType(name="x", shape=(1, C, 1, S), dtype=f16),
               ct.TensorType(name="update_mask", shape=(1, S, CTX), dtype=f16),
               ct.TensorType(name="attn_mask", shape=(1, S, CTX), dtype=f16)]
        pkgs[fn] = convert(Half(layers, first, last, "tokens", W, S), ins,
                           [ct.TensorType(name=out, dtype=f16), ct.TensorType(name="align", dtype=f16)], n, int8)
    tmp = out_dir / f"_{name}"
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir()
    desc = ct.utils.MultiFunctionDescriptor()
    for fn, ml in pkgs.items():
        ml.save(str(tmp / f"{fn}.mlpackage"))
        desc.add_function(str(tmp / f"{fn}.mlpackage"), src_function_name="main", target_function_name=fn)
    desc.default_function_name = "step"
    pkg = out_dir / f"{name}.mlpackage"
    ct.utils.save_multifunction(desc, str(pkg))  # identical weights are stored once
    shutil.rmtree(tmp)
    shutil.rmtree(out_dir / f"{name}.mlmodelc", ignore_errors=True)
    subprocess.run(XCRUN + ["coremlcompiler", "compile", str(pkg), str(out_dir)], check=True, capture_output=True)
    shutil.rmtree(pkg)
    return out_dir / f"{name}.mlmodelc"


def main():
    a = sys.argv[1:]
    int8 = "--fp16" not in a
    split = int(a[a.index("--split") + 1]) if "--split" in a else 16
    if a[0] == "test":
        out = HERE / "anetest2"
        out.mkdir(exist_ok=True)
        print(build_half(Weights(), 0, int(a[1]), True, out, f"Half{a[1]}", int8))
        return
    W, out = Weights(a[1]), Path(a[2])
    out.mkdir(parents=True, exist_ok=True)
    export_embeddings(W, out)
    halves = [(0, split, split == LAYERS, "DecoderA")] + ([(split, LAYERS - split, True, "DecoderB")] if split < LAYERS else [])
    for first, n, last, name in halves:
        print(build_half(W, first, n, last, out, name, int8), flush=True)
    (out / "ane_decoder.json").write_text(json.dumps({
        "version": 2, "layers": LAYERS, "halves": [[f, n, nm] for f, n, _, nm in halves], "align_heads": ALIGN,
        "multi": MULTI, "int8": int8, "functions": ["fill", "step", "multi"]}))


if __name__ == "__main__":
    main()
