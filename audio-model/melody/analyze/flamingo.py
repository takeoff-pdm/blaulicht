"""What a music language model says about a song and each of its parts.

Music Flamingo (NVIDIA, nvidia/music-flamingo-2601-hf, 8B parameters, BF16): 16 kHz mono in 30 s
windows, up to 20 minutes. The card asks for "our fork" of transformers but names none; PyPI's
transformers 5.22 has the model.

Pilot on 5 songs (`pilots/results/flamingo.txt`): not worth running at scale. It answers fast (1.5 s a clip, 17 GB)
but names the same synths on silence and on drums alone, its function follows the order of the options in the
prompt, its energy grows with clip length as much as across a drop, and its timestamps mostly follow a regular grid.

Output: `whole` (a free description and a section breakdown with timestamps, which the card doesn't
promise), `parts` (per part of `cues.parts`: lead and other instruments, vocals, energy, mood, function
as parsed JSON, always with the raw text), and how long and how much GPU memory it took.

`cues.parts` is made on the Mac by `pipeline/show_compile.py`, so run `pilots/export_context.py` and push
before running this on new songs.
"""

import json
import re
import time
from pathlib import Path

import numpy as np
from scipy.signal import resample_poly

from analyze import common

NAME, VERSION = "flamingo", 1
GPU = True
MODEL = common.ROOT / "models/music-flamingo-2601-hf"
CONTEXT = Path(__file__).parent / "pilots/context"
SR16 = 16000

DESCRIBE = ("Describe this track in full detail - tell me the genre, tempo, and key, then dive into the instruments, "
            "production style, and overall mood it creates.")
SECTIONS = ("Give a section-by-section breakdown of this track with timestamps. Write one line per section in the form "
            "mm:ss-mm:ss | section name | what happens. Cover the whole track from start to end.")
PART = ("This clip is one part of an electronic dance track. Answer with one JSON object and nothing else, with exactly "
        'these keys: {"lead_instrument": "the most prominent instrument or sound", "other_instruments": ["instrument", ...], '
        '"vocals": "yes" or "no", "vocal_type": "for example female sung, male rap, spoken sample, or none", '
        '"energy": an integer from 0 to 10, "mood": "one or two words", '
        '"function": "intro" or "build-up" or "drop" or "breakdown" or "outro"}')
FUNCTIONS = ("intro", "build-up", "drop", "breakdown", "outro")


def setup():
    import torch
    from transformers import AutoProcessor, MusicFlamingoForConditionalGeneration

    processor = AutoProcessor.from_pretrained(MODEL)
    model = MusicFlamingoForConditionalGeneration.from_pretrained(MODEL, device_map="auto", dtype=torch.bfloat16).eval()
    return torch, processor, model


def to16k(x: np.ndarray) -> np.ndarray:
    """Mono 44.1 kHz stem to the model's 16 kHz."""
    return resample_poly(x, 160, 441).astype(np.float32)


def fit(audio: np.ndarray) -> np.ndarray:
    """Drop a tail under 1 s past a 30 s multiple. The processor makes it a window of its own, and one of a few
    samples (Spaceman's 480007-sample drop) gets no audio tokens: an index error on the GPU that ends the process."""
    tail = len(audio) % (30 * SR16)
    return audio[:-tail] if len(audio) > 30 * SR16 and 0 < tail < SR16 else audio


def ask(state, audio: np.ndarray, prompt: str, max_new_tokens: int = 160, system: str | None = None) -> dict:
    """One greedy answer about 16 kHz audio."""
    torch, processor, model = state
    messages = [{"role": "user", "content": [{"type": "text", "text": prompt}, {"type": "audio", "audio": fit(audio)}]}]
    if system:
        messages.insert(0, {"role": "system", "content": system})
    t0 = time.time()
    inputs = processor.apply_chat_template(messages, tokenize=True, add_generation_prompt=True, return_dict=True).to(model.device)
    inputs["input_features"] = inputs["input_features"].to(model.dtype)
    with torch.inference_mode():
        out = model.generate(**inputs, max_new_tokens=max_new_tokens, do_sample=False)
    new = out[:, inputs.input_ids.shape[1]:]
    return {"raw": processor.batch_decode(new, skip_special_tokens=True)[0].strip(), "seconds": round(time.time() - t0, 2),
            "new_tokens": int(new.shape[1])}


def parse_part(raw: str) -> dict | None:
    """The answer's JSON with the fields normalised, or None if there is no parsable object."""
    m = re.search(r"\{.*\}", raw, re.S)
    if not m:
        return None
    try:
        d = json.loads(m.group(0))
    except json.JSONDecodeError:
        return None
    out = dict(d)
    out["vocals"] = str(d.get("vocals", "")).strip().lower() in ("yes", "true")
    try:
        out["energy"] = float(d.get("energy"))
    except (TypeError, ValueError):
        out["energy"] = None
    fn = str(d.get("function", "")).strip().lower().replace(" ", "-").replace("buildup", "build-up")
    out["function"] = fn if fn in FUNCTIONS else None
    return out


def seconds(stamp: str) -> float:
    """"1:23" or "01:02:03" to seconds."""
    v = 0.0
    for p in stamp.split(":"):
        v = v * 60 + float(p)
    return v


def parse_timestamps(raw: str) -> list[dict]:
    """Lines like `00:15-00:30 | Build-up | text`, with whatever separators the model chose."""
    out = []
    for line in raw.splitlines():
        m = re.search(r"(\d{1,2}:\d{2}(?::\d{2})?)\s*(?:-|–|—|to)\s*(\d{1,2}:\d{2}(?::\d{2})?)\s*[|:\-–—]*\s*(.*)", line)
        if m:
            name = m.group(3).split("|")[0].strip(" *:-–—")
            out.append({"start": seconds(m.group(1)), "end": seconds(m.group(2)), "name": name, "line": line.strip()})
    return out


def peak_memory(state) -> dict:
    torch = state[0]
    return {"allocated_gb": round(torch.cuda.max_memory_allocated() / 2**30, 2), "reserved_gb": round(torch.cuda.max_memory_reserved() / 2**30, 2)}


def analyse(song, state):
    torch = state[0]
    context = CONTEXT / f"{song.id}.json"
    if not context.exists():
        raise FileNotFoundError(f"{context}: run pilots/export_context.py on the Mac and push")
    parts_in = json.loads(context.read_text())["parts"]
    x = to16k(common.stems(song)["mix"])
    t0 = time.time()
    torch.cuda.reset_peak_memory_stats()
    whole = {"describe": ask(state, x, DESCRIBE, 500), "sections": ask(state, x, SECTIONS, 800)}
    whole["sections"]["parsed"] = parse_timestamps(whole["sections"]["raw"])
    memory = {"whole": peak_memory(state)}
    torch.cuda.reset_peak_memory_stats()
    parts = []
    for start, end, letter in parts_in:
        a = ask(state, x[int(start * SR16): int(end * SR16)], PART)
        parts.append({"start": start, "end": end, "letter": letter, **a, "parsed": parse_part(a["raw"])})
    memory["parts"] = peak_memory(state)
    return {"model": "nvidia/music-flamingo-2601-hf", "whole": whole, "parts": parts, "memory": memory,
            "seconds": round(time.time() - t0, 1)}
