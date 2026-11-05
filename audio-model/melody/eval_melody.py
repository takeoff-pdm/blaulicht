"""Final evaluation of the melody students, plus their MIDI for the Rekordbox songs.

Uses each run's last checkpoint (best.pt was chosen on the test songs). A voicing offset is picked
on 25 training songs (max OA against Melodia; the `_bias0` rows report the untuned decision).
Then frame metrics on the held-out Rekordbox songs and the MUSDB18 test mixes against offline
Melodia, and on the MUSDB18 test mixes against the vocal reference (SwiftF0 on the isolated
vocal stem; RPA/RCA there measure the vocal line only). Offline Melodia is scored against the
vocal reference too. With --midi, the chosen run also transcribes every Rekordbox song to
out/student/<id>.mid (+ f0 .npy).

    python eval_melody.py --runs la0,la10,la30,la100 --midi la30
"""

import argparse
import json
from pathlib import Path

import numpy as np
import torch

from melody_common import ROOT, is_test_song, metrics, summarize
from melodia_batch import f0_to_notes, write_midi
from student_melody import N_PITCH, MelodyConfig, MelodyStudent, bin_to_hz
from train_melody import HOP, load_audio

BIASES = [x / 4 for x in range(-8, 9)]  # voicing offsets (log-odds) tried on training songs


def load(run: str, device):
    ck = torch.load(ROOT / "runs" / run / "last.pt", map_location="cpu", weights_only=False)
    cfg = MelodyConfig(**{k: tuple(v) if isinstance(v, list) else v for k, v in ck["config"].items()})
    m = MelodyStudent(cfg)
    m.load_state_dict(ck["model"])
    return m.to(device).eval(), ck["step"]


@torch.no_grad()
def predict_parts(model, wav, device, chunk_s=120):
    """Whole-track pitch candidate (Hz) and voicing margin log(P(pitch)) - log(P(no melody)) per
    frame, so the voicing threshold can be chosen afterwards. Chunked like train_melody.predict."""
    n_frames = len(wav) // HOP + 1
    f0, margin = np.zeros(n_frames, np.float32), np.zeros(n_frames, np.float32)
    ctx, step = 2000, chunk_s * 100
    k = torch.arange(N_PITCH, device=device)
    for g in range(0, n_frames, step):
        a, b = max(0, g - ctx), min(n_frames, g + step + ctx)
        x = torch.from_numpy(wav[a * HOP: (b - 1) * HOP + 1]).to(device)[None]
        with torch.autocast("cuda", torch.bfloat16):
            p = model(x)[0].float().softmax(-1)
        pp = p[:, :N_PITCH]
        peak = pp.argmax(-1)
        w = pp * ((k - peak[:, None]).abs() <= 2)
        cand = bin_to_hz((w * k).sum(-1) / w.sum(-1).clamp(min=1e-8))
        m = torch.log(pp.sum(-1) + 1e-9) - torch.log(p[:, N_PITCH] + 1e-9)
        sl = slice(g - a, g - a + min(step, n_frames - g))
        f0[g: min(n_frames, g + step)] = cand[sl].cpu().numpy()
        margin[g: min(n_frames, g + step)] = m[sl].cpu().numpy()
    return f0, margin


def voiced(f0, margin, bias):
    return np.where(margin > bias, f0, 0).astype(np.float32)


def sets():
    songs = [(ROOT / "data/audio/songs" / l.name, l) for l in sorted((ROOT / "data/labels/songs").glob("*.npy"))
             if is_test_song(l.stem)]
    musdb = [(ROOT / "data/audio/musdb_test" / l.name, l) for l in sorted((ROOT / "data/labels/musdb_test").glob("*.npy"))]
    vocals = [(a, ROOT / "data/labels/musdb_test_vocals" / l.name) for a, l in musdb]
    train = [(ROOT / "data/audio/songs" / l.name, l) for l in sorted((ROOT / "data/labels/songs").glob("*.npy"))
             if not is_test_song(l.stem)][::8]  # 25 training songs for the voicing offset
    return {"tune": train, "songs_vs_melodia": songs, "musdb_vs_melodia": musdb, "musdb_vs_vocals": vocals}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", default="la0,la10,la30,la100")
    ap.add_argument("--midi", default="")
    args = ap.parse_args()
    device = "cuda"
    data = sets()
    results = {}
    # offline Melodia vs the vocal reference (its labels are the musdb_test labels)
    ms = [metrics(np.load(v), np.load(l)) for (_, l), (_, v) in zip(data["musdb_vs_melodia"], data["musdb_vs_vocals"])]
    results["melodia_offline"] = {"musdb_vs_vocals": summarize(ms)}
    for run in args.runs.split(","):
        model, step = load(run, device)
        parts = {}
        for name, items in data.items():
            for audio, _ in items:
                if audio not in parts:
                    parts[audio] = predict_parts(model, load_audio(audio, None), device)
        tune = {b: summarize([metrics(np.load(ref), voiced(*parts[a], b)) for a, ref in data["tune"]])["OA"]
                for b in BIASES}
        bias = max(tune, key=tune.get)
        res = {"step": step, "voicing_bias": bias, "tune_OA": tune[bias], "tune_OA_bias0": tune[0.0]}
        for name, items in data.items():
            if name == "tune":
                continue
            res[name] = summarize([metrics(np.load(ref), voiced(*parts[a], bias)) for a, ref in items])
            res[name + "_bias0"] = summarize([metrics(np.load(ref), voiced(*parts[a], 0.0)) for a, ref in items])
        results[run] = res
        print(run, json.dumps({k: ({m: round(x, 4) for m, x in v.items()} if isinstance(v, dict) else v)
                               for k, v in res.items()}), flush=True)
    path = ROOT / "runs/eval.json"
    merged = json.loads(path.read_text()) if path.exists() else {}
    merged.update(results)  # runs evaluated earlier stay
    path.write_text(json.dumps(merged, indent=1))
    if args.midi:
        model, _ = load(args.midi, device)
        bias = results[args.midi]["voicing_bias"]
        out = ROOT / "out/student"
        out.mkdir(parents=True, exist_ok=True)
        for audio in sorted((ROOT / "data/audio/songs").glob("*.npy")):
            f0 = voiced(*predict_parts(model, load_audio(audio, None), device), bias)
            np.save(out / audio.name, f0)
            write_midi(f0_to_notes(f0, 0.01, smooth=3), out / f"{audio.stem}.mid")
        print("midi done", flush=True)


if __name__ == "__main__":
    main()
