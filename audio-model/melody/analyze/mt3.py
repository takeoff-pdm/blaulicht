"""Which instruments play when, and their notes: multi-instrument transcription of the whole mix.

YourMT3+ (Chang et al., MLSP 2024; the mimbres/YourMT3 Hugging Face Space), checkpoint
"YPTF.MoE+Multi (noPS)": Perceiver-TF encoder with mixture-of-experts layers and a 13-channel decoder,
16 kHz mono, in pieces of about 2 s. The Space and the source headers say Apache-2.0 but the GitHub
repo says GPL-3.0, so treat it as research only until that is cleared up.

Output, by class (piano, chromatic, organ, guitar, bass, strings, brass, reed, pipe, lead, pad,
singing, drums): `notes` [[onset, offset, pitch]], `active` [[start, end]] (a class counts as playing
over runs of 2 s or more with gaps up to 2 s bridged), and how many notes of each.

It doesn't know club music, so the viewer shows it and the lights don't use it. On the 175 electronic
songs only 65 get 20 or more bass notes (median 10; Acraze's bassline gets 1), while 155 get piano
and 150 guitar: synths land in whichever class is nearest. Its singing is nearly all inside the
RoFormer's vocal regions (93%) but covers only 62% of them.
"""

import os
import sys

import numpy as np
from scipy.signal import resample_poly

from analyze import common

NAME, VERSION = "mt3", 1
GPU = True
SRC = common.ROOT / "models/src/YourMT3"
CHECKPOINT = "mc13_256_g4_all_v7_mt3f_sqr_rms_moe_wf4_n8k2_silu_rope_rp_b36_nops@last.ckpt"
ARGS = [CHECKPOINT, "-p", "2024", "-tk", "mc13_full_plus_256", "-dec", "multi-t5", "-nl", "26", "-enc", "perceiver-tf",
        "-sqr", "1", "-ff", "moe", "-wf", "4", "-nmoe", "8", "-kmoe", "2", "-act", "silu", "-epe", "rope",
        "-rp", "1", "-ac", "spec", "-hop", "300", "-atc", "1", "-pr", "16"]
GROUPS = ["piano", "chromatic", "organ", "guitar", "bass", "strings", "strings", "brass", "reed", "pipe", "lead", "pad"]


def group(note):
    if note.is_drum:
        return "drums"
    if note.program in (100, 101):
        return "singing"
    return GROUPS[note.program // 8] if note.program // 8 < len(GROUPS) else "other"


def setup():
    import torch

    os.environ.setdefault("WANDB_MODE", "disabled")
    sys.path[:0] = [str(SRC), str(SRC / "amt/src")]
    os.chdir(SRC)  # its checkpoint paths are relative to the Space's root
    from model_helper import load_model_checkpoint
    from utils.audio import slice_padded_array
    from utils.event2note import merge_zipped_note_events_and_ties_to_notes
    from utils.note2event import mix_notes

    model = load_model_checkpoint(args=ARGS, device="cpu").cuda()
    return torch, model, slice_padded_array, merge_zipped_note_events_and_ties_to_notes, mix_notes


def analyse(song, state):
    torch, model, slice_padded_array, merge, mix_notes = state
    x = resample_poly(common.stems(song)["mix"], 160, 441).astype(np.float32)
    n, sr = model.audio_cfg["input_frames"], model.audio_cfg["sample_rate"]
    pieces = slice_padded_array(x[None], n, n)
    audio = torch.from_numpy(pieces.astype("float32")).cuda().unsqueeze(1)
    with torch.no_grad():
        tokens, _ = model.inference_file(bsz=8, audio_segments=audio)
    starts = [n * i / sr for i in range(len(pieces))]
    per_channel = []
    for ch in range(model.task_manager.num_decoding_channels):
        zipped, _, _ = model.task_manager.detokenize_list_batches([a[:, ch, :] for a in tokens], starts, return_events=True)
        per_channel.append(merge(zipped)[0])
    notes, active = {}, {}
    for note in mix_notes(per_channel):
        notes.setdefault(group(note), []).append([round(note.onset, 3), round(note.offset, 3), int(note.pitch)])
    fps, dur = 10, len(x) / sr
    for k, v in notes.items():
        v.sort()
        on = np.zeros(int(dur * fps) + 1, bool)
        for a, b, _ in v:
            on[int(a * fps): max(int(a * fps) + 1, int(b * fps))] = True
        active[k] = [[round(a / fps, 2), round(b / fps, 2)] for a, b in common.runs(on, 2 * fps, 2 * fps)]
    return {"notes": notes, "active": active, "summary": {k: len(v) for k, v in sorted(notes.items())}}
