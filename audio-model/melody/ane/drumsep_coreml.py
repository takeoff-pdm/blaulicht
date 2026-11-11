"""DrumSep (MDX23C, analyze/drumparts.py) with its network on Core ML (GPU or Neural Engine).

    python drumsep_coreml.py convert <models dir> <out folder>                   one .mlmodelc
    python drumsep_coreml.py run <models dir> <mlmodelc> <drums.wav> <out dir>   separate (RF_UNITS=gpu|ane|cpu)
    python drumsep_coreml.py ref <models dir> <drums.wav> <out dir>              audio-separator unchanged (mps)

Same idea as roformer_ane.py: audio-separator does the chunking and overlap; inside TFC_TDF_net only
the part between the STFT and the inverse STFT runs in Core ML (Core ML has no complex numbers), so the
output matches audio-separator's up to fp16 rounding. The net is traced at batch 1 and the wrapper loops
over audio-separator's batch.
Settings as drumparts.py: segment 256, model chunk size, batch 4, overlap 2.
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
MODEL = "MDX23C-DrumSep-aufr33-jarredou.ckpt"


def separator(models_dir, out_dir="."):
    from audio_separator.separator import Separator
    sep = Separator(output_dir=str(out_dir), model_file_dir=str(models_dir), output_format="WAV", log_level=30,
                    mdxc_params={"segment_size": 256, "override_model_segment_size": False, "batch_size": 4, "overlap": 2, "pitch_shift": 0})
    sep.load_model(model_filename=MODEL)
    return sep


class Net(nn.Module):
    """TFC_TDF_net.forward without the STFT and its inverse: spectrogram -> spectrogram."""

    def __init__(self, m):
        super().__init__()
        self.m = m

    def forward(self, x):
        m = self.m
        mix = x = m.cac2cws(x)
        first_conv_out = x = m.first_conv(x)
        x = x.transpose(-1, -2)
        enc = []
        for block in m.encoder_blocks:
            x = block.tfc_tdf(x)
            enc.append(x)
            x = block.downscale(x)
        x = m.bottleneck_block(x)
        for block in m.decoder_blocks:
            x = block.upscale(x)
            x = torch.cat([x, enc.pop()], 1)
            x = block.tfc_tdf(x)
        x = x.transpose(-1, -2) * first_conv_out
        x = m.final_conv(torch.cat([mix, x], 1))
        return m.cws2cac(x)


def convert(models_dir, out_dir):
    import coremltools as ct
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from htdemucs_coreml import _fix_int_cast
    _fix_int_cast()
    sep = separator(models_dir)
    m = sep.model_instance.model_run.cpu().float().eval()
    L = sep.model_instance.model_data_cfgdict.audio.chunk_size
    spec = m.stft(torch.randn(1, 2, L))
    net = Net(m).eval()
    with torch.no_grad():
        traced = torch.jit.trace(net, spec)
    ml = ct.convert(traced, convert_to="mlprogram", inputs=[ct.TensorType(name="spec", shape=tuple(spec.shape), dtype=np.float32)],
                    outputs=[ct.TensorType(name="out", dtype=np.float32)], minimum_deployment_target=ct.target.macOS15,
                    compute_precision=ct.precision.FLOAT16, skip_model_load=True)
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    pkg = out_dir / "DrumSep.mlpackage"
    shutil.rmtree(pkg, ignore_errors=True)
    shutil.rmtree(out_dir / "DrumSep.mlmodelc", ignore_errors=True)
    ml.save(str(pkg))
    subprocess.run(XCRUN + ["coremlcompiler", "compile", str(pkg), str(out_dir)], check=True, capture_output=True)
    shutil.rmtree(pkg)
    print(out_dir / "DrumSep.mlmodelc", "chunk", L, "spec", tuple(spec.shape), flush=True)


class CoreMLRun(nn.Module):
    """Stands in for model_run: [B, 2, L] audio -> [B, stems, 2, L], network in Core ML."""

    def __init__(self, m, mlmodelc, units):
        super().__init__()
        import coremltools as ct
        cu = {"gpu": ct.ComputeUnit.CPU_AND_GPU, "ane": ct.ComputeUnit.CPU_AND_NE, "cpu": ct.ComputeUnit.CPU_ONLY}[units]
        self.cm = ct.models.CompiledMLModel(str(mlmodelc), compute_units=cu)
        self.stft, self.num_target_instruments = m.stft, m.num_target_instruments
        self.dummy = nn.Parameter(torch.zeros(1), requires_grad=False)  # audio-separator asks for a parameter's device
        self.net_s, self.calls = 0.0, 0

    def forward(self, x):
        x = x.float().cpu()
        spec = self.stft(x)
        outs = []
        for i in range(spec.shape[0]):
            t0 = time.perf_counter()
            outs.append(self.cm.predict({"spec": spec[i:i + 1].numpy()})["out"])
            self.net_s += time.perf_counter() - t0
            self.calls += 1
        y = torch.from_numpy(np.concatenate(outs))
        if self.num_target_instruments > 1:
            b, c, f, t = y.shape
            y = y.reshape(b, self.num_target_instruments, -1, f, t)
        return self.stft.inverse(y)


def main():
    a = sys.argv[1:]
    if a[0] == "convert":
        convert(a[1], a[2])
        return
    if a[0] == "run":
        sep = separator(a[1], a[4])
        mi = sep.model_instance
        mi.model_run = CoreMLRun(mi.model_run.cpu().float(), a[2], os.environ.get("RF_UNITS", "gpu"))
        mi.torch_device = torch.device("cpu")
        audio, out = a[3], a[4]
    else:
        sep = separator(a[1], a[3])
        audio, out = a[2], a[3]
    t0 = time.perf_counter()
    files = sep.separate(audio)
    run = sep.model_instance.model_run
    print(f"{a[0]}: {time.perf_counter() - t0:.1f} s", f"net {run.net_s:.1f} s in {run.calls} calls" if a[0] == "run" else "", files, flush=True)


if __name__ == "__main__":
    main()
