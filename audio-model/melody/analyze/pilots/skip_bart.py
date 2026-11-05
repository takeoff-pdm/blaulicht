"""Pilot: Skip-BART (Zhao et al., ICLR 2026), music to light hue and brightness at 10 fps, on 5 club songs.

OpenL3 (music, mel256, 512 dims, hop 0.1 s) goes into a BART trained on stage footage, mostly rock and
metal, with hue 0-179 and value 0-255 as outputs. The clone in `models/src/Skip-BART` stays untouched:
this wraps its `model.py` and `util.py`. Differences from its `generate.py`:
- it stops at 1024 frames (102.4 s) and saves the embeddings as its `_gen.npy`; here windows of 1024
  frames run back to back, each seeded with the last frame of the one before;
- the decoder keeps its cache instead of rerunning the whole model every frame (`check` compares
  the two), which takes a window from minutes to seconds;
- several seeds in one batch, and the expected value per frame next to the sampled one, because the
  sampling (top-p 0.9, temperature 1.1) would otherwise bury anything locked to the music.

On ae-charlie, in code/, with CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=2:
    ../.venv-skipbart/bin/python -m analyze.pilots.skip_bart embed      # CPU; TensorFlow
    ../.venv-skipbart/bin/python -m analyze.pilots.skip_bart check
    ../.venv-skipbart/bin/python -m analyze.pilots.skip_bart generate
    ../.venv-skipbart/bin/python -m analyze.pilots.skip_bart control    # the repo's own sample videos
    ../.venv-skipbart/bin/python -m analyze.pilots.skip_bart measure    # metrics, plots, videos
    ../.venv-skipbart/bin/python -m analyze.pilots.skip_bart teacher    # teacher-forced accuracy, as in the authors' log
    ../.venv-skipbart/bin/python -m analyze.pilots.skip_bart seeds      # how much of the brightness is the random first frame
    ../.venv-skipbart/bin/python -m analyze.pilots.skip_bart report     # skip_bart.txt

Result (`results/skip_bart.txt`): not worth running at scale. On club music each run holds one colour and one
brightness, which depend mostly on the seed; no beat lock, and next to no reaction at drops or part boundaries.
"""

import base64
import json
import os
import subprocess
import sys
import time

import numpy as np

from analyze import common

SRC = common.ROOT / "models/src/Skip-BART"
TRAINED = common.ROOT / "models/skip-bart/trained"
CONTEXT = common.ROOT / "code/analyze/pilots/context"
OUT = common.ROOT / "out/pilots/skip_bart"
SONGS = ["58e556af6855", "fe0b3c607b1b", "8a0e3b75f4a1", "dcb7a7d3e846", "6a67050f048d"]
FPS, WINDOW, SEEDS = 10, 1024, 3
P, T = (0.9, 0.9), (1.1, 1.1)  # generate.py's defaults


# ---- model ----------------------------------------------------------------------------------------


def openl3_embed(mono: np.ndarray, sr: int) -> np.ndarray:
    import openl3

    emb, _ = openl3.get_audio_embedding(mono, sr, embedding_size=512, verbose=False)
    return emb.astype(np.float32)


def load_model():
    import torch

    sys.path[:0] = [str(SRC)]
    from model import ML_BART, ML_Classifier
    from peft import get_peft_model
    from transformers import BartConfig

    cfg = BartConfig(max_position_embeddings=WINDOW, d_model=1024, encoder_layers=8, encoder_ffn_dim=2048, encoder_attention_heads=8,
                     decoder_layers=8, decoder_ffn_dim=2048, decoder_attention_heads=8)
    bart = ML_BART(cfg, class_num=[180, 256]).cuda()
    head = ML_Classifier(hidden_dim=1024, class_num=[180, 256]).cuda()
    bart.bart = get_peft_model(bart.bart, bart.lora_config)
    # The published weights call the light embeddings decoder_emb2, the published model.py decoder_emb (same shapes),
    # so generate.py's strict load fails as shipped.
    state = torch.load(TRAINED / "bart_finetune.pth", map_location="cuda")
    bart.load_state_dict({k.replace("decoder_emb2.", "decoder_emb."): v for k, v in state.items()}, strict=True)
    head.load_state_dict(torch.load(TRAINED / "head_finetune.pth", map_location="cuda"), strict=True)
    torch.set_grad_enabled(False)
    return bart.eval(), head.eval()


def window(bart, head, music, h0, v0):
    """One window. music (B, T, 512), h0 and v0 (B,) the frame before it. Returns sampled h, v and expected v, (B, T)."""
    import torch
    from util import sampling

    B, n, _ = music.shape
    mask = torch.ones(B, n, device=music.device)
    memory = bart.bart.encoder(inputs_embeds=bart.encoder(music), attention_mask=mask).last_hidden_state
    side = bart.decoder(music)
    h_prev, v_prev = torch.as_tensor(h0, device=music.device).long(), torch.as_tensor(v0, device=music.device).long()
    past, out = None, np.zeros((3, B, n), np.float32)
    scale = torch.arange(256, device=music.device).float()
    for i in range(n):
        emb = torch.cat([bart.decoder_emb[0](h_prev), bart.decoder_emb[1](v_prev), side[:, i]], -1)[:, None]
        y = bart.bart.decoder(inputs_embeds=emb, encoder_hidden_states=memory, encoder_attention_mask=mask,
                              past_key_values=past, use_cache=True)
        past = y.past_key_values
        hl, vl = head(y.last_hidden_state[:, 0])
        h_new, v_new = [], []
        for j in range(B):
            h_new.append(sampling(hl[j, :-1], p=P[0], t=T[0]))
            v_new.append(sampling(vl[j, :-1], p=P[1], t=T[1]))
        h_prev = torch.as_tensor(np.array(h_new), device=music.device).long()
        v_prev = torch.as_tensor(np.array(v_new), device=music.device).long()
        out[0, :, i], out[1, :, i] = h_prev.cpu().numpy(), v_prev.cpu().numpy()
        out[2, :, i] = (torch.softmax(vl[:, :-1] / T[1], -1) * scale).sum(-1).cpu().numpy()
    return out


def generate(bart, head, emb: np.ndarray, seeds: int = SEEDS, base_seed: int = 0) -> dict:
    """Back-to-back windows over a whole song: h, v, ev as (seeds, frames)."""
    import torch

    np.random.seed(base_seed)
    h0, v0 = np.random.randint(0, 180, seeds), np.random.randint(0, 256, seeds)
    music = torch.from_numpy(emb).float().cuda()[None].repeat(seeds, 1, 1)
    chunks = []
    for start in range(0, music.shape[1], WINDOW):
        o = window(bart, head, music[:, start: start + WINDOW], h0, v0)
        chunks.append(o)
        h0, v0 = o[0, :, -1], o[1, :, -1]
    h, v, ev = np.concatenate(chunks, axis=2)
    return {"h": h, "v": v, "ev": ev}


def check(bart, head, emb: np.ndarray) -> dict:
    """Logits with the decoder cache against generate.py's whole-model call, on 300 frames."""
    import torch

    n = 300
    music = torch.from_numpy(emb[:n]).float().cuda()[None]
    rng = np.random.default_rng(1)
    light = torch.as_tensor(np.stack([rng.integers(0, 180, n), rng.integers(0, 256, n)], -1)).long().cuda()[None]
    mask = torch.ones(1, n).cuda()
    t0 = time.time()
    hl_full, vl_full = head(bart(music, light, mask, mask))
    one_call = time.time() - t0
    memory = bart.bart.encoder(inputs_embeds=bart.encoder(music), attention_mask=mask).last_hidden_state
    side, past, worst = bart.decoder(music), None, 0.0
    t0 = time.time()
    for i in range(n):
        emb_i = torch.cat([bart.decoder_emb[0](light[:, i, 0]), bart.decoder_emb[1](light[:, i, 1]), side[:, i]], -1)[:, None]
        y = bart.bart.decoder(inputs_embeds=emb_i, encoder_hidden_states=memory, encoder_attention_mask=mask, past_key_values=past, use_cache=True)
        past = y.past_key_values
        hl, vl = head(y.last_hidden_state[:, 0])
        worst = max(worst, float((hl - hl_full[:, i]).abs().max()), float((vl - vl_full[:, i]).abs().max()))
    return {"max_logit_difference": worst, "whole_model_call_s": round(one_call, 3), "cached_s_per_frame": round((time.time() - t0) / n, 4)}


# ---- measures -------------------------------------------------------------------------------------


def context(sid: str) -> dict:
    return json.loads((CONTEXT / f"{sid}.json").read_text())


def smooth(x, n):
    return common.smooth(np.asarray(x, np.float64), n)


def energy_10fps(c: dict, frames: int) -> np.ndarray:
    """The viewer's mix energy (0-255 at `fps`) at 10 fps."""
    e = np.frombuffer(base64.b64decode(c["energy"]["mix"]), np.uint8).astype(np.float64)
    step = int(round(c["fps"] / FPS))
    means = np.convolve(e, np.ones(step) / step, "same")  # centred, so a frame stays on its time
    return means[np.minimum(np.arange(frames) * step, len(e) - 1)]


def corr(a, b) -> float:
    a, b = a - np.mean(a), b - np.mean(b)
    d = np.sqrt((a * a).sum() * (b * b).sum())
    return float((a * b).sum() / d) if d > 0 else 0.0


def percentile_of(value, null) -> float:
    return round(float((np.asarray(null) < value).mean() * 100), 1)


def drop_windows(c: dict) -> list[dict]:
    """Per drop: the breakdown and build before it, and the drop section after it (at most 30 s each)."""
    out = []
    secs = c["sections"]
    for d in c["drops"]:
        t = d["time"]
        pre = [s for s in secs if s["end"] <= t + 0.01 and s["kind"] in ("Breakdown", "Build") and s["end"] > t - 60]
        post = next((s for s in secs if abs(s["start"] - t) < 0.5 and s["kind"] == "Drop"), None)
        if pre and post:
            out.append({"time": t, "pre": (max(pre[0]["start"], t - 30), t), "post": (t, min(post["end"], t + 30))})
    return out


def drop_rise(x, w) -> float:
    a, b = w["pre"], w["post"]
    return float(np.mean(x[int(b[0] * FPS): int(b[1] * FPS)]) - np.mean(x[int(a[0] * FPS): int(a[1] * FPS)]))


def drops(x, c, rng) -> dict:
    """Rise at each drop, and where it falls among the same two windows placed at random in the song."""
    rows = []
    for w in drop_windows(c):
        la, lb = w["pre"][1] - w["pre"][0], w["post"][1] - w["post"][0]
        ts = rng.uniform(la, len(x) / FPS - lb, 500)
        null = [drop_rise(x, {"pre": (t - la, t), "post": (t, t + lb)}) for t in ts]
        rise = drop_rise(x, w)
        rows.append({"time": round(w["time"], 1), "rise": round(rise, 1), "percentile": percentile_of(rise, null)})
    return {"drops": rows, "mean_rise": round(float(np.mean([r["rise"] for r in rows])), 1) if rows else None}


def beat_phase(beats: list[float], frames: int, detune: float = 1.0) -> np.ndarray:
    """Beat phase in radians per frame from the beat list; detune > 1 runs that much too fast."""
    t = np.arange(frames) / FPS
    b = np.asarray(beats)
    k = np.interp(t, b, np.arange(len(b)), left=np.nan, right=np.nan)
    return 2 * np.pi * k * detune


def locking(x, beats) -> dict:
    """How strongly a track follows the beat: lag and phase measures against neighbouring tempi."""
    d = x - smooth(x, 2 * FPS + 1)
    b = np.asarray(beats)
    period = float(np.median(np.diff(b))) * FPS
    acf = np.correlate(d - d.mean(), d - d.mean(), "full")[len(d) - 1:]
    acf = acf / (acf[0] + 1e-12)
    lag = lambda p: float(np.interp(p, np.arange(len(acf)), acf))
    neighbours = [period * f for f in (0.7, 0.8, 0.9, 1.1, 1.2, 1.3)]

    def strength(detune):
        ph = beat_phase(beats, len(d), detune)
        ok = np.isfinite(ph)
        return float(np.abs(np.mean(d[ok] * np.exp(1j * ph[ok]))) / (np.std(d[ok]) + 1e-12))

    detuned = [strength(f) for f in (0.8, 0.85, 0.9, 0.93, 0.96, 1.04, 1.07, 1.1, 1.15, 1.2)]
    r = strength(1.0)
    return {"acf_at_beat": round(lag(period), 3), "acf_neighbours_mean": round(float(np.mean([lag(p) for p in neighbours])), 3),
            "phase_lock": round(r, 3), "phase_lock_detuned_mean": round(float(np.mean(detuned)), 3), "phase_lock_detuned_max": round(float(np.max(detuned)), 3)}


def hue_shift(h, times, n=2 * FPS):
    """Circular distance in hue units (0-90) between the mean hue of the n frames before and after each time."""
    c = np.concatenate([[0], np.cumsum(np.exp(1j * h * 2 * np.pi / 180))])
    out = []
    for t in np.atleast_1d(times):
        k = int(t * FPS)
        if k < n or k + n > len(h):
            continue
        before, after = c[k] - c[k - n], c[k + n] - c[k]
        d = np.angle(after * np.conj(before))
        out.append(abs(d) * 180 / (2 * np.pi))
    return np.array(out)


def hues(h, c, rng) -> dict:
    """Hue change at part boundaries against random moments at least 4 s from any boundary."""
    bounds = [p[0] for p in c["parts"][1:]]
    dur = len(h) / FPS
    at = hue_shift(h, bounds)
    cand = rng.uniform(2, dur - 2, 4000)
    cand = cand[np.min(np.abs(cand[:, None] - np.array(bounds)[None]), axis=1) > 4][:600]
    null = hue_shift(h, cand)
    return {"boundaries": len(at), "boundary_shift_median": round(float(np.median(at)), 1), "random_shift_median": round(float(np.median(null)), 1),
            "percentile_of_median": percentile_of(np.median(at), null), "share_of_boundaries_above_random_p90": round(float((at > np.percentile(null, 90)).mean()), 2)}


def seed_agreement(h, v) -> dict:
    """Do different seeds do the same thing? 2 s smoothed brightness correlation; mean hue distance, against a time shift."""
    vs = np.array([smooth(x, 20) for x in v])
    n = len(v)
    pairs = [(i, j) for i in range(n) for j in range(i + 1, n)]
    rv = [corr(vs[i], vs[j]) for i, j in pairs]
    rv0 = [corr(vs[i], np.roll(vs[j], 600)) for i, j in pairs]
    hd = lambda a, b: float(np.mean(np.abs((a - b + 90) % 180 - 90)))
    return {"brightness_corr": round(float(np.mean(rv)), 2), "brightness_corr_shifted_60s": round(float(np.mean(rv0)), 2),
            "hue_distance": round(float(np.mean([hd(h[i], h[j]) for i, j in pairs])), 1),
            "hue_distance_shifted_60s": round(float(np.mean([hd(h[i], np.roll(h[j], 600)) for i, j in pairs])), 1)}


def by_section(x, c) -> dict:
    out = {}
    for s in c["sections"]:
        a, b = int(s["start"] * FPS), int(s["end"] * FPS)
        if b > a:
            out.setdefault(s["kind"], []).append(x[a: min(b, len(x))])
    return {k: round(float(np.mean(np.concatenate(v))), 1) for k, v in out.items()}


def measure_song(c: dict, h, v, ev, rng) -> dict:
    frames = v.shape[1]
    energy = energy_10fps(c, frames)
    v_mean, ev_mean = v.mean(0), ev.mean(0)
    env, ev_slow = smooth(energy, 21), smooth(ev_mean, 21)
    shifted = [corr(ev_slow, np.roll(env, s)) for s in rng.integers(100, frames - 100, 300)]
    r = corr(ev_slow, env)
    return {
        "name": c["name"], "bpm": c["bpm"], "frames": frames,
        "value": {"mean": round(float(v.mean()), 1), "std": round(float(v.std()), 1), "frame_step": round(float(np.abs(np.diff(v, axis=1)).mean()), 1),
                  "expected_std": round(float(ev.std()), 1)},
        "drops_sampled": drops(v_mean, c, rng), "drops_expected": drops(ev_mean, c, rng), "drops_audio_energy": drops(energy, c, rng),
        "by_section_expected": by_section(ev_mean, c), "by_section_audio_energy": by_section(energy, c),
        "corr_with_audio_energy_2s": {"r": round(r, 2), "percentile_among_time_shifts": percentile_of(r, shifted)},
        "beat_sampled": locking(v[0], c["beats"]), "beat_expected": locking(ev_mean, c["beats"]), "beat_audio_energy": locking(energy, c["beats"]),
        "hue": hues(h[0], c, rng), "seeds": seed_agreement(h, v),
    }


# ---- what a person looks at -----------------------------------------------------------------------


def hsv_to_rgb(h, v):
    """OpenCV-style hue 0-179, full saturation, value 0-255, to uint8 RGB, as generate.py draws it."""
    import matplotlib.colors as mc

    hsv = np.stack([np.asarray(h) / 180.0, np.ones_like(h, float), np.asarray(v) / 255.0], -1)
    return (mc.hsv_to_rgb(hsv) * 255).astype(np.uint8)


def plot(c: dict, h, v, ev, path) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    frames = v.shape[1]
    t = np.arange(frames) / FPS
    energy = energy_10fps(c, frames)
    fig, ax = plt.subplots(3, 1, figsize=(16, 6), sharex=True, gridspec_kw={"height_ratios": [1, 2, 1.2]})
    ax[0].imshow(hsv_to_rgb(h[0], np.full(frames, 255))[None], aspect="auto", extent=[0, frames / FPS, 0, 1])
    ax[0].set_yticks([])
    ax[0].set_title(f'{c["name"]}: Skip-BART hue (seed 0), brightness, and the song\'s energy', loc="left", fontsize=11)
    ax[1].plot(t, smooth(v[0], 5), color="#b8b8b8", lw=0.8, label="sampled, seed 0, 0.5 s smoothed")
    ax[1].plot(t, ev.mean(0), color="#1f5fbf", lw=1.2, label="expected value, 3 seeds")
    ax[1].set_ylim(0, 255)
    ax[1].set_ylabel("brightness 0-255")
    ax[1].legend(loc="upper right", frameon=False, fontsize=8)
    ax[2].plot(t, smooth(energy, 5), color="#444444", lw=1)
    ax[2].set_ylabel("mix energy")
    ax[2].set_xlabel("seconds")
    for a in ax[1:]:
        a.spines[["top", "right"]].set_visible(False)
        a.grid(axis="y", color="#eeeeee", lw=0.6)
    for a, b, letter in c["parts"]:
        for x in ax:
            x.axvline(a, color="#888888", lw=0.5, ls=":")
        ax[1].text(a + 0.5, 244, letter, fontsize=8, color="#555555")
    for d in c["drops"]:
        for x in ax:
            x.axvline(d["time"], color="#c0392b", lw=1.2)
    fig.tight_layout()
    fig.savefig(path, dpi=110)
    plt.close(fig)


def video(song, h, v, path) -> None:
    """The colour field with the song's audio, like generate.py's video."""
    rgb = hsv_to_rgb(h, v)
    cmd = ["ffmpeg", "-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", "16x16", "-r", str(FPS), "-i", "-", "-i", str(song.audio),
           "-vf", "scale=240:240:flags=neighbor", "-c:v", "libx264", "-crf", "30", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "64k", "-shortest", str(path)]
    frames = np.repeat(np.repeat(rgb[:, None, None], 16, 1), 16, 2)
    subprocess.run(cmd, input=frames.tobytes(), check=True)


# ---- the repo's own sample videos, to check the wrapper -----------------------------------------


def video_hv(path) -> tuple[np.ndarray, np.ndarray]:
    """Hue and value per frame of a solid-colour sample video (decoded at 10 fps)."""
    import matplotlib.colors as mc

    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(path), "-r", str(FPS), "-vf", "scale=8:8", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                         check=True, capture_output=True).stdout
    a = np.frombuffer(raw, np.uint8).reshape(-1, 8, 8, 3).mean((1, 2)) / 255
    hsv = mc.rgb_to_hsv(a)
    return hsv[:, 0] * 180, hsv[:, 2] * 255


def summary(h, v) -> dict:
    return {"value_mean": round(float(np.mean(v)), 1), "value_std": round(float(np.std(v)), 1), "value_step": round(float(np.abs(np.diff(v)).mean()), 1),
            "hue_step": round(float(np.abs((np.diff(h) + 90) % 180 - 90).mean()), 1)}


def control(bart, head) -> dict:
    root = SRC / "video results"
    doc = {}
    for i in (1, 2, 3):
        pcm = subprocess.run(["ffmpeg", "-v", "error", "-i", str(root / "Ground Truth" / f"{i}.mp4"), "-ac", "1", "-ar", "44100", "-f", "f32le", "-"],
                             check=True, capture_output=True).stdout
        mono = np.frombuffer(pcm, np.float32).copy()
        emb = np.load(OUT / f"control_{i}_emb.npy") if (OUT / f"control_{i}_emb.npy").exists() else None
        if emb is None:
            raise FileNotFoundError("run embed first")
        g = generate(bart, head, emb, seeds=8, base_seed=i)
        gh, gv = video_hv(root / "Ground Truth" / f"{i}.mp4")
        ah, av = video_hv(root / "Skip-BART (ours)" / f"{i}.mp4")
        n = min(len(gv), g["v"].shape[1], len(av))
        doc[str(i)] = {"seconds": round(len(mono) / 44100, 1), "ground_truth": summary(gh[:n], gv[:n]), "authors_video": summary(ah[:n], av[:n]),
                       "this_wrapper": summary(g["h"][0][:n], g["v"][0][:n]),
                       "brightness_corr_with_ground_truth_2s": {
                           "authors_video": round(corr(smooth(av[:n], 21), smooth(gv[:n], 21)), 2),
                           "this_wrapper_expected": round(corr(smooth(g["ev"].mean(0)[:n], 21), smooth(gv[:n], 21)), 2)}}
        print(i, doc[str(i)], flush=True)
    return doc


# ---- commands -------------------------------------------------------------------------------------


def cmd_embed():
    OUT.mkdir(parents=True, exist_ok=True)
    os.environ["CUDA_VISIBLE_DEVICES"] = ""
    times = {}
    for s in [s for s in common.songs() if s.id in SONGS]:
        p = OUT / f"{s.id}_emb.npy"
        if p.exists():
            continue
        t0 = time.time()
        mix = np.asarray(np.load(common.ROOT / "stems_songs" / f"{s.id}.npy", mmap_mode="r")[0], np.float32)
        np.save(p, openl3_embed(mix, common.SR))
        times[s.id] = round(time.time() - t0, 1)
        print(s.name, times[s.id], "s", np.load(p).shape, flush=True)
    for i in (1, 2, 3):
        p = OUT / f"control_{i}_emb.npy"
        if p.exists():
            continue
        pcm = subprocess.run(["ffmpeg", "-v", "error", "-i", str(SRC / "video results/Ground Truth" / f"{i}.mp4"), "-ac", "1", "-ar", "44100", "-f", "f32le", "-"],
                             check=True, capture_output=True).stdout
        np.save(p, openl3_embed(np.frombuffer(pcm, np.float32).copy(), 44100))
    (OUT / "embed_times.json").write_text(json.dumps(times))


def cmd_check():
    bart, head = load_model()
    emb = np.load(OUT / f"{SONGS[0]}_emb.npy")
    print(check(bart, head, emb), flush=True)


def cmd_generate():
    import torch

    bart, head = load_model()
    rows = {}
    for sid in SONGS:
        emb = np.load(OUT / f"{sid}_emb.npy")
        torch.cuda.reset_peak_memory_stats()
        t0 = time.time()
        g = generate(bart, head, emb)
        np.savez_compressed(OUT / f"{sid}.npz", **g)
        rows[sid] = {"frames": len(emb), "seconds": round(time.time() - t0, 1), "peak_gb": round(torch.cuda.max_memory_allocated() / 2**30, 2)}
        print(sid, rows[sid], flush=True)
    (OUT / "generate_times.json").write_text(json.dumps(rows, indent=1))


def cmd_control():
    bart, head = load_model()
    doc = control(bart, head)
    (OUT / "control.json").write_text(json.dumps(doc, indent=1))


def cmd_teacher():
    """Is the wrapper wired like the authors' model? Their test accuracy (log.txt: about 0.42 hue, 0.12 value) is teacher-forced,
    so do the same on their ground-truth sample videos, next to simply repeating the last frame, and with another clip's music."""
    import torch

    bart, head = load_model()
    root = SRC / "video results/Ground Truth"
    doc = {}
    for i in (1, 2, 3):
        gh, gv = video_hv(root / f"{i}.mp4")
        emb = np.load(OUT / f"control_{i}_emb.npy")
        other = np.load(OUT / f"control_{i % 3 + 1}_emb.npy")
        n = min(len(gv), len(emb), len(other))
        light = torch.as_tensor(np.stack([np.clip(np.round(gh[:n]), 0, 179), np.clip(np.round(gv[:n]), 0, 255)], -1)).long().cuda()[None]
        before = torch.cat([light[:, :1], light[:, :-1]], 1)
        mask = torch.ones(1, n).cuda()
        row = {"repeat_last_hue": round(float((before[..., 0] == light[..., 0]).float().mean()), 3), "repeat_last_value": round(float((before[..., 1] == light[..., 1]).float().mean()), 3)}
        for name, e in (("own_music", emb), ("other_music", other)):
            hl, vl = head(bart(torch.from_numpy(e[:n]).float().cuda()[None], before, mask, mask))
            row[name] = {"hue": round(float((hl.argmax(-1) == light[..., 0]).float().mean()), 3), "value": round(float((vl.argmax(-1) == light[..., 1]).float().mean()), 3),
                         "value_within_5": round(float(((vl.argmax(-1) - light[..., 1]).abs() <= 5).float().mean()), 3)}
        doc[str(i)] = row
        print(i, row, flush=True)
    (OUT / "teacher.json").write_text(json.dumps(doc, indent=1))


def cmd_seeds():
    """How much of a song's brightness is the random first frame? 16 seeds on two songs."""
    import torch

    bart, head = load_model()
    doc = {}
    for sid in (SONGS[0], SONGS[1]):
        np.random.seed(7)
        emb = np.load(OUT / f"{sid}_emb.npy")
        n = 16
        music = torch.from_numpy(emb).float().cuda()[None].repeat(n, 1, 1)
        h0, v0 = np.random.randint(0, 180, n), np.random.randint(0, 256, n)
        first = (h0.copy(), v0.copy())
        chunks = []
        for start in range(0, music.shape[1], WINDOW):
            o = window(bart, head, music[:, start: start + WINDOW], h0, v0)
            chunks.append(o)
            h0, v0 = o[0, :, -1], o[1, :, -1]
        h, v, _ = np.concatenate(chunks, axis=2)
        means = v.mean(1)
        doc[sid] = {"first_v": first[1].tolist(), "mean_v": [round(float(x), 1) for x in means], "corr_first_v_with_mean_v": round(corr(first[1].astype(float), means), 2),
                    "corr_first_v_with_v_after_10s": round(corr(first[1].astype(float), v[:, 95:105].mean(1)), 2),
                    "between_seed_share_of_brightness_variance": round(float(means.var() / v.var()), 2), "value_std_over_seeds_of_song_mean": round(float(means.std()), 1),
                    "first_h": first[0].tolist(), "mean_hue_circular": [round(float((np.angle(np.exp(1j * x * 2 * np.pi / 180).mean()) % (2 * np.pi)) * 180 / (2 * np.pi)), 1) for x in h]}
        print(sid, doc[sid], flush=True)
    (OUT / "seeds.json").write_text(json.dumps(doc, indent=1))


def cmd_report():
    """A readable summary next to metrics.json."""
    m = json.loads((OUT / "metrics.json").read_text())
    times = json.loads((OUT / "generate_times.json").read_text())
    lines = ["Skip-BART on 5 club songs, 3 seeds, 10 fps. Percentiles: among the same windows placed at random in the song.", ""]
    for sid, d in m.items():
        lines += [f'{d["name"]}, {d["bpm"]} BPM, {times[sid]["seconds"]} s to generate',
                  f'  value mean {d["value"]["mean"]}, std {d["value"]["std"]}, per frame step {d["value"]["frame_step"]}',
                  f'  rise at drops (expected value): {d["drops_expected"]["drops"]}',
                  f'  rise at drops (audio energy):   {d["drops_audio_energy"]["drops"]}',
                  f'  expected value by section: {d["by_section_expected"]}',
                  f'  audio energy by section:   {d["by_section_audio_energy"]}',
                  f'  correlation with audio energy, 2 s: {d["corr_with_audio_energy_2s"]}',
                  f'  beat lock, sampled:   {d["beat_sampled"]}', f'  beat lock, expected:  {d["beat_expected"]}', f'  beat lock, audio:     {d["beat_audio_energy"]}',
                  f'  hue at part boundaries: {d["hue"]}', f'  between seeds: {d["seeds"]}', ""]
    for name in ("control", "seeds", "teacher"):
        p = OUT / f"{name}.json"
        if p.exists():
            lines += [f"{name}: {p.read_text()}", ""]
    (OUT / "skip_bart.txt").write_text("\n".join(lines))


def cmd_measure():
    songs = {s.id: s for s in common.songs()}
    rng = np.random.default_rng(0)
    doc = {}
    for sid in SONGS:
        c, g = context(sid), np.load(OUT / f"{sid}.npz")
        doc[sid] = measure_song(c, g["h"], g["v"], g["ev"], rng)
        plot(c, g["h"], g["v"], g["ev"], OUT / f"{sid}.png")
        video(songs[sid], g["h"][0], g["v"][0], OUT / f"{sid}.mp4")
        print(json.dumps(doc[sid]), flush=True)
    (OUT / "metrics.json").write_text(json.dumps(doc, indent=1, ensure_ascii=False))


if __name__ == "__main__":
    globals()[f"cmd_{sys.argv[1]}"]()
