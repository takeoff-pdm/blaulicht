"""GAME (vocal melody notes, game_batch.py) with its segmenter network on Core ML (GPU).

    python game_coreml.py prepare <GAME onnx folder> <out folder>          ONNX surgery + Core ML buckets
    python game_coreml.py run <stems.npy> <GAME onnx folder> <out folder>   time + notes vs the CPU original

The segmenter takes 87% of GAME's time on the CPU (8 diffusion-like steps per chunk, ~0.5 s each), and
ONNX Runtime's CoreML provider made it slower (2.5 s per call: the graph is split across devices).
Here its network runs as one Core ML program on the GPU (60 ms for 11 s of audio):
  - the one random op (RandomUniformLike, which drops some previous boundaries each step) becomes an
    input `u`, drawn with numpy, so the network has no random ops;
  - the network up to the boundary logits (/Squeeze_output_0) is converted ONNX -> PyTorch (onnx2torch)
    -> Core ML at fixed lengths (buckets); chunks are padded to the next bucket with maskT = 0;
  - the post-processing tail (sigmoid, threshold, peak picking; 37 ONNX nodes) stays in ONNX Runtime on
    the CPU, unchanged.
onnx2torch needs three fixes for this graph: CumSum's `reverse` defaults to 1 there (ONNX: 0), Clip
without a minimum, and LessOrEqual/GreaterOrEqual opset 16. coremltools needs expand to a 0-d shape.
Encoder, bd2dur and estimator stay on the CPU (11 s per song together).
"""

import shutil
import subprocess
import sys
import time
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
XCRUN = ["env", "-i", f"HOME={Path.home()}", "PATH=/usr/bin:/bin", "DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer", "xcrun"]
BUCKETS = (1000, 2000, 3000)  # frames (10 ms); game_batch cuts chunks at 30 s
NOISE = "/RandomUniformLike_output_0"
LOGITS = "/Squeeze_output_0"
HEAD_IN = ["x_seg", "language", "known_boundaries", "prev_boundaries", "t", "maskT", NOISE]
TAIL_IN = [LOGITS, "maskT", "threshold", "radius", "known_boundaries"]


def surgery(src_dir, out_dir):
    import onnx
    from onnx import TensorProto, helper
    from onnx.utils import extract_model
    m = onnx.load(str(Path(src_dir) / "segmenter.onnx"))
    g = m.graph
    rnd = [n for n in g.node if n.op_type == "RandomUniformLike"]
    assert len(rnd) == 1 and rnd[0].output[0] == NOISE
    g.node.remove(rnd[0])
    g.input.append(helper.make_tensor_value_info(NOISE, TensorProto.FLOAT, ["B", "T"]))
    for i, n in enumerate(list(g.node)):
        if n.op_type == "Clip" and len(n.input) > 1 and n.input[1] == "":
            g.node.remove(n)
            g.node.insert(i, helper.make_node("Min", [n.input[0], n.input[2]], list(n.output), name=n.name + "_min"))
        if n.op_type == "CumSum" and not any(a.name == "reverse" for a in n.attribute):
            n.attribute.append(helper.make_attribute("reverse", 0))
    noise = Path(out_dir) / "seg_noise.onnx"
    onnx.save(m, str(noise))
    extract_model(str(noise), str(Path(out_dir) / "seg_head.onnx"), HEAD_IN, [LOGITS])
    extract_model(str(noise), str(Path(out_dir) / "seg_tail.onnx"), TAIL_IN, ["boundaries"])
    noise.unlink()


def _patch_converters():
    import torch
    from coremltools.converters.mil.frontend.torch import ops
    from coremltools.converters.mil.frontend.torch.torch_op_registry import register_torch_op
    from onnx2torch.node_converters.registry import add_converter
    from onnx2torch.utils.common import OnnxToTorchModule, OperationConverterResult, onnx_mapping_from_node

    class Bin(torch.nn.Module, OnnxToTorchModule):
        def __init__(self, f):
            super().__init__()
            self.f = f

        def forward(self, a, b):
            return self.f(a, b)

    for op, f in (("LessOrEqual", torch.le), ("GreaterOrEqual", torch.ge)):
        try:
            add_converter(operation_type=op, version=16)(
                lambda node, graph, f=f: OperationConverterResult(torch_module=Bin(f), onnx_mapping=onnx_mapping_from_node(node)))
        except ValueError:
            pass  # already registered

    orig_expand = ops.expand

    @register_torch_op(torch_alias=["expand_copy", "broadcast_to"], override=True)
    def expand(context, node):
        x, shape = ops._get_inputs(context, node, min_expected=2)[:2]
        if (isinstance(shape, (list, tuple)) and len(shape) == 0) or (hasattr(shape, "shape") and tuple(shape.shape) == (0,)):
            context.add(x, node.name)  # expand to a 0-d shape: nothing to broadcast
            return
        orig_expand(context, node)


def convert_head(out_dir, T):
    import coremltools as ct
    import onnx
    import torch
    from onnx2torch import convert
    head = convert(onnx.load(str(Path(out_dir) / "seg_head.onnx"))).eval()

    class W(torch.nn.Module):  # Core ML inputs are float; the graph wants bool masks and int64 language
        def __init__(self, h):
            super().__init__()
            self.h = h

        def forward(self, x_seg, language, kb, pb, t, mask, u):
            return self.h(x_seg, language.long(), kb > 0.5, pb > 0.5, t, mask > 0.5, u)

    ex = (torch.randn(1, T, 256), torch.zeros(1, dtype=torch.int32), torch.zeros(1, T), (torch.rand(1, T) < 0.05).float(),
          torch.tensor([0.5]), torch.ones(1, T), torch.rand(1, T))
    with torch.no_grad():
        traced = torch.jit.trace(W(head).eval(), ex)
    f32 = lambda n, s: ct.TensorType(name=n, shape=s, dtype=np.float32)
    ml = ct.convert(traced, convert_to="mlprogram",
                    inputs=[f32("x_seg", (1, T, 256)), ct.TensorType(name="language", shape=(1,), dtype=np.int32),
                            f32("known_boundaries", (1, T)), f32("prev_boundaries", (1, T)), f32("t", (1,)), f32("maskT", (1, T)), f32("u", (1, T))],
                    outputs=[ct.TensorType(name="logits", dtype=np.float32)],
                    minimum_deployment_target=ct.target.macOS15, compute_precision=ct.precision.FLOAT16, skip_model_load=True)
    pkg = Path(out_dir) / f"SegHead{T}.mlpackage"
    shutil.rmtree(pkg, ignore_errors=True)
    shutil.rmtree(Path(out_dir) / f"SegHead{T}.mlmodelc", ignore_errors=True)
    ml.save(str(pkg))
    subprocess.run(XCRUN + ["coremlcompiler", "compile", str(pkg), str(out_dir)], check=True, capture_output=True)
    shutil.rmtree(pkg)
    print(Path(out_dir) / f"SegHead{T}.mlmodelc", flush=True)


def prepare(src_dir, out_dir):
    Path(out_dir).mkdir(parents=True, exist_ok=True)
    surgery(src_dir, out_dir)
    _patch_converters()
    for T in BUCKETS:
        convert_head(out_dir, T)


class Segmenter:
    """Drop-in for the segmenter ONNX session: .run(None, feed) -> [boundaries]."""

    def __init__(self, models_dir, units="gpu", seed=0):
        import coremltools as ct
        import onnxruntime as ort
        cu = {"gpu": ct.ComputeUnit.CPU_AND_GPU, "ane": ct.ComputeUnit.CPU_AND_NE, "cpu": ct.ComputeUnit.CPU_ONLY}[units]
        self.heads = {T: ct.models.CompiledMLModel(str(Path(models_dir) / f"SegHead{T}.mlmodelc"), compute_units=cu) for T in BUCKETS}
        so = ort.SessionOptions()
        so.intra_op_num_threads = 4
        self.tail = ort.InferenceSession(str(Path(models_dir) / "seg_tail.onnx"), so, providers=["CPUExecutionProvider"])
        self.rng = np.random.default_rng(seed)

    def run(self, _outputs, feed):
        T = feed["x_seg"].shape[1]
        P = next(b for b in BUCKETS if b >= T)
        pad = lambda a: np.pad(a.astype(np.float32), [(0, 0), (0, P - T)] + [(0, 0)] * (a.ndim - 2))
        u = self.rng.random((1, T), dtype=np.float32)
        logits = self.heads[P].predict({
            "x_seg": pad(feed["x_seg"]), "language": feed["language"].astype(np.int32),
            "known_boundaries": pad(feed["known_boundaries"]), "prev_boundaries": pad(feed["prev_boundaries"]),
            "t": feed["t"].astype(np.float32), "maskT": pad(feed["maskT"]), "u": pad(u)})["logits"][:, :T]
        return self.tail.run(None, {LOGITS: logits.astype(np.float32), "maskT": feed["maskT"], "threshold": feed["threshold"],
                                    "radius": feed["radius"], "known_boundaries": feed["known_boundaries"]})


def transcribe_song(game, wav):
    import game_batch as G
    notes = []
    for a, b in G.chunks(wav):
        notes += [(a / G.SR + s, a / G.SR + e, p) for s, e, p in game.transcribe(wav[a:b])]
    return notes


def note_f1(ref, est, onset=0.05, pitch=0.5):
    used, hit = set(), 0
    for s, e, p in est:
        for j, (rs, re_, rp) in enumerate(ref):
            if j not in used and abs(rs - s) <= onset and abs(rp - p) <= pitch:
                used.add(j)
                hit += 1
                break
    return 2 * hit / max(1, len(ref) + len(est))


def run(stems, src_dir, models_dir):
    import game_batch as G
    a = np.load(stems, mmap_mode="r")[3]
    wav = (a.mean(0) if a.ndim == 2 else a).astype(np.float32)
    cpu = G.Game(Path(src_dir), "cpu")
    res = {}
    for name in ("cpu", "cpu again", "coreml"):
        game = cpu
        if name == "coreml":
            game = G.Game(Path(src_dir), "cpu")
            game.s["segmenter"] = Segmenter(models_dir)
        t0 = time.perf_counter()
        res[name] = transcribe_song(game, wav)
        print(f"{name}: {time.perf_counter() - t0:.1f} s, {len(res[name])} notes", flush=True)
    print("note F1 (50 ms onset, 0.5 semitone): cpu vs cpu again", round(note_f1(res["cpu"], res["cpu again"]), 3),
          "| cpu vs coreml", round(note_f1(res["cpu"], res["coreml"]), 3))


if __name__ == "__main__":
    a = sys.argv[1:]
    if a[0] == "prepare":
        prepare(a[1], a[2])
    else:
        run(a[1], a[2], a[3])
