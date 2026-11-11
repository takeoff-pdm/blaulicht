"""Split WhisperKit's large-v3 AudioEncoder under the Neural Engine's 1 GiB weight limit.

    venv-ct/bin/python split_models.py <whisperkit openai_whisper-large-v3 folder> <out folder>

A Core ML program with more than 1 GiB of weights is rejected by the Neural Engine as a whole and runs
on the CPU. coremltools' bisect_model cuts the encoder into two chunks (614 + 600 MiB) and merges them
into a pipeline; all 683 ops then run on the Neural Engine (check with diag plan).
Rewritten 2026-11-10 after the scratch copy was lost; not yet re-run.
"""
import shutil
import subprocess
import sys
from pathlib import Path

import coremltools as ct
from coremltools.models.utils import bisect_model, load_spec

from ane_decoder import XCRUN

src, out = Path(sys.argv[1]), Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)
enc = src / "AudioEncoder.mlmodelc"
spec = load_spec(str(enc / "model.mlmodel")) if (enc / "model.mlmodel").exists() else None
assert spec is not None, "expected model.mlmodel inside the compiled AudioEncoder"
model = ct.models.MLModel(spec, weights_dir=str(enc / "weights"), skip_model_load=True)
tmp = out / "_split"
shutil.rmtree(tmp, ignore_errors=True)
bisect_model(model, str(tmp), merge_chunks_to_pipeline=True, check_output_correctness=False)
pipe = next(tmp.glob("*pipeline*.mlpackage"))
for chunk in tmp.glob("*chunk*.mlpackage"):
    size = sum(f.stat().st_size for f in chunk.rglob("*") if f.is_file()) / 2**20
    print(f"{chunk.name}: {size:.0f} MiB", "(over 1 GiB: will run on the CPU)" if size > 1024 else "")
shutil.rmtree(out / "AudioEncoder.mlmodelc", ignore_errors=True)
subprocess.run(XCRUN + ["coremlcompiler", "compile", str(pipe), str(tmp)], check=True, capture_output=True)
shutil.move(str(tmp / (pipe.stem + ".mlmodelc")), str(out / "AudioEncoder.mlmodelc"))
shutil.rmtree(tmp)
for f in ("MelSpectrogram.mlmodelc", "config.json", "generation_config.json"):
    if (src / f).exists():
        (shutil.copytree if (src / f).is_dir() else shutil.copy)(src / f, out / f)
print(out / "AudioEncoder.mlmodelc")
