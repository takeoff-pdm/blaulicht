"""Core ML stand-ins for the slow networks, for the Mac backend. Switched on by MELODY_COREML=<models folder>.

The folder holds what the converters in this directory build:
    roformer/RoformerCore.mlmodelc       vocals (roformer_ane.py convert)          GPU
    htdemucs/HTDemucs{0..3}.mlmodelc     stems (htdemucs_coreml.py convert)        GPU
    game/cm/SegHead*.mlmodelc, seg_tail.onnx   GAME segmenter (game_coreml.py prepare)   GPU + ORT CPU
    drumsep/DrumSep.mlmodelc             drum parts (drumsep_coreml.py convert)    Neural Engine
Each patch keeps the original's chunking and pre/post-processing, so outputs match up to fp16 rounding.
"""

import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def root():
    d = os.environ.get("MELODY_COREML")
    return Path(d) if d else None


def vocals(sep):
    """audio-separator's Mel-Band RoFormer: the network between band split and masks in Core ML."""
    import coremltools as ct
    import torch.nn as nn
    from roformer_ane import CoreMLBlock
    m = sep.model_instance.model_run
    units = {"ane": ct.ComputeUnit.CPU_AND_NE, "gpu": ct.ComputeUnit.CPU_AND_GPU}[os.environ.get("RF_UNITS", "gpu")]
    block = CoreMLBlock(ct.models.CompiledMLModel(str(root() / "roformer/RoformerCore.mlmodelc"), compute_units=units))
    m.band_split = block.to(next(m.parameters()).device)
    m.layers = nn.ModuleList()
    m.mask_estimators = nn.ModuleList([nn.Identity()])
    return sep


def drumsep(sep):
    """audio-separator's MDX23C DrumSep: the network between STFT and iSTFT on the Neural Engine."""
    import torch
    from drumsep_coreml import CoreMLRun
    mi = sep.model_instance
    mi.model_run = CoreMLRun(mi.model_run.cpu().float(), root() / "drumsep/DrumSep.mlmodelc", os.environ.get("DS_UNITS", "ane"))
    mi.torch_device = torch.device("cpu")
    return sep


def demucs(bag):
    """A demucs bag (htdemucs_ft): each member's network in Core ML; use apply_model with device="cpu"."""
    from htdemucs_coreml import patch
    patch(bag.cpu().eval(), root() / "htdemucs", os.environ.get("RF_UNITS", "gpu"))
    return bag


def game(g):
    """game_batch.Game: the segmenter's network in Core ML, its post-processing in ONNX Runtime."""
    from game_coreml import Segmenter
    g.s["segmenter"] = Segmenter(root() / "game/cm")
    return g
