# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "scipy", "torch"]
# ///
"""Train the beat-synchronous drop countdown model on exported features.

One step per beat: each step summarizes the mel frames of one beat (mean,
max and onset flux per band), and the model predicts how many beats ahead the
next drop (see drops.py) lands, from 1 to MAX_AHEAD, or that none is coming.
In training the steps follow the Rekordbox beat grid; live they follow the
beat tracker, which keeps predicting beats through breakdowns.

    uv run audio-model/train_drops.py --features audio-model/data/features --run drops-beats

Validation also runs on the beats the live tracker produced for the
validation tracks (`--tracker-beats`, CSV traces from `beat_eval --trace-dir`),
and compares against a rule that only knows how long the breakdown has lasted.
"""

import argparse
import csv
import json
import time
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import torch
import torch.nn.functional as F

from drops import Drop, find_drops
from model import BeatTCN, ModelConfig
from train import Track, augment, load_tracks

HERE = Path(__file__).resolve().parent
VAL_PERCENT = 15
# Countdown classes: drop 1..MAX_AHEAD beats ahead, plus "none".
MAX_AHEAD = 32
NONE = MAX_AHEAD
HORIZON_BEATS = 16  # "within 4 bars", the trigger the lights would use
CROP_BEATS = 192
WARMUP_BEATS = 32  # loss and scoring skip steps with less than this context
LOW_BAND_MAX_HZ = 150.0
# A beat matches a drop when it lies within this many mel frames.
MATCH_FRAMES = 7


@dataclass
class DropTrack:
    track: Track
    drops: list[Drop]


# ---------------------------------------------------------------------------
# Beat-synchronous features and targets
# ---------------------------------------------------------------------------


def beat_features(mel: np.ndarray, beats: np.ndarray) -> np.ndarray:
    """(len(beats) - 1, 3 * n_mels): mean, max and mean positive flux of every
    band over each beat interval [beats[i], beats[i + 1])."""
    beats = beats[(beats >= 0) & (beats <= len(mel))]
    mel = mel[: beats[-1]].astype(np.float32)
    flux = np.maximum(np.diff(mel, axis=0, prepend=mel[:1]), 0.0)
    starts = beats[:-1]
    lengths = np.diff(beats)[:, None].astype(np.float32)
    mean = np.add.reduceat(mel, starts) / lengths
    peak = np.maximum.reduceat(mel, starts)
    onset = np.add.reduceat(flux, starts) / lengths
    return np.concatenate([mean, peak, onset], axis=1)


def drop_steps(drops: list[Drop], beats: np.ndarray) -> list[int]:
    """Index of the beat each drop lands on; drops the grid misses are left out."""
    out = []
    for d in drops:
        i = int(np.argmin(np.abs(beats - d.frame)))
        if abs(int(beats[i]) - d.frame) <= MATCH_FRAMES:
            out.append(i)
    return out


def countdown(drop_beats: list[int], n_steps: int) -> np.ndarray:
    """Class per step. Step i covers beat i and its output is ready when beat
    i + 1 starts, so a drop on beat D is D - (i + 1) beats ahead."""
    y = np.full(n_steps, NONE, np.int64)
    for d in sorted(drop_beats, reverse=True):
        ahead = d - (np.arange(n_steps) + 1)
        mask = (ahead >= 1) & (ahead <= MAX_AHEAD)
        y[mask] = ahead[mask] - 1
    return y


def unique_sorted(frames: np.ndarray) -> np.ndarray:
    frames = np.sort(frames.astype(np.int64))
    return frames[np.concatenate([[True], np.diff(frames) > 0])]


class Sampler:
    def __init__(self, tracks: list[DropTrack], args, seed: int):
        self.tracks = [t for t in tracks if len(t.track.beats) > CROP_BEATS + 2]
        weights = np.array([len(t.track.beats) for t in self.tracks], float)
        self.p = weights / weights.sum()
        self.with_drops = [i for i, t in enumerate(self.tracks) if t.drops]
        self.rng = np.random.default_rng(seed)
        self.augment = not args.no_augment

    def crop(self) -> tuple[np.ndarray, np.ndarray]:
        rng = self.rng
        if rng.random() < 0.5:
            dt = self.tracks[rng.choice(self.with_drops)]
            drop_beat = drop_steps([dt.drops[rng.integers(len(dt.drops))]], dt.track.beats)
            anchor = drop_beat[0] if drop_beat else None
        else:
            dt = self.tracks[rng.choice(len(self.tracks), p=self.p)]
            anchor = None
        beats = dt.track.beats
        if self.augment:
            # Imitate a live tracker: a constant phase error plus jitter.
            beats = unique_sorted(
                beats + rng.normal(0, 1.5) + rng.normal(0, 0.7, len(beats))
            )
            beats = beats[(beats >= 0) & (beats < len(dt.track.mel))]
        n = len(beats) - 1
        if anchor is not None:
            end = int(np.clip(anchor + rng.integers(-36, 6), CROP_BEATS, n))
        else:
            end = int(rng.integers(CROP_BEATS, n + 1))
        start = end - CROP_BEATS
        window = beats[start : end + 1]
        mel = dt.track.mel[window[0] : window[-1]].astype(np.float32)
        if self.augment:
            mel = augment(mel, rng)
        x = beat_features(mel, window - window[0])
        y = countdown([d - start for d in drop_steps(dt.drops, beats)], len(x))
        return x, y

    def batch(self, size: int) -> tuple[np.ndarray, np.ndarray]:
        xs, ys = zip(*(self.crop() for _ in range(size)))
        return np.stack([x.T for x in xs]), np.stack(ys)


# ---------------------------------------------------------------------------
# Breakdown-length baseline
# ---------------------------------------------------------------------------


def breakdown_lengths(mel: np.ndarray, beats: np.ndarray, centers: np.ndarray) -> np.ndarray:
    """Per step, how many consecutive beats (causally) the kick/bass has been
    below half its recent loud level."""
    low = (np.expm1(mel[:, centers <= LOW_BAND_MAX_HZ].astype(np.float32)) / 1000).sum(1)
    energy = np.add.reduceat(low[: beats[-1]], beats[:-1]) / np.diff(beats)
    run = np.zeros(len(energy), int)
    for i, e in enumerate(energy):
        loud = np.percentile(energy[max(0, i - 128) : i + 1], 90)
        quiet = i >= 16 and e < 0.5 * loud
        run[i] = run[i - 1] + 1 if quiet and i else 0
    return run


class BreakdownBaseline:
    """Countdown distribution given only the breakdown length, from training
    tracks (lengths capped at 160 beats)."""

    CAP = 160

    def __init__(self, tracks: list[DropTrack], centers: np.ndarray):
        counts = np.ones((self.CAP + 1, MAX_AHEAD + 1))  # Laplace smoothing
        for dt in tracks:
            beats = dt.track.beats
            run = np.minimum(breakdown_lengths(dt.track.mel, beats, centers), self.CAP)
            y = countdown(drop_steps(dt.drops, beats), len(run))
            np.add.at(counts, (run, y), 1)
        self.table = counts / counts.sum(1, keepdims=True)
        self.centers = centers

    def probs(self, mel: np.ndarray, beats: np.ndarray) -> np.ndarray:
        run = np.minimum(breakdown_lengths(mel, beats, self.centers), self.CAP)
        return self.table[run]


# ---------------------------------------------------------------------------
# Evaluation
# ---------------------------------------------------------------------------


def load_tracker_beats(trace_dir: Path, track_id: str) -> np.ndarray | None:
    """Beats the live tracker published, with the gaps where the engine mutes
    them (bass gone, i.e. breakdowns) filled at the local beat period, as the
    tracker's internal beat clock keeps running there."""
    path = trace_dir / f"{track_id}.csv"
    if not path.exists():
        return None
    with path.open() as f:
        times = [float(r["time_ms"]) for r in csv.DictReader(f) if r["kind"] == "beat"]
    if len(times) < 8:
        return None
    beats = np.floor(np.asarray(times) / 10)
    filled = [beats[0]]
    for prev, nxt, i in zip(beats[:-1], beats[1:], range(1, len(beats))):
        period = np.median(np.diff(beats[max(0, i - 9) : i + 1]))
        steps = int(round((nxt - prev) / period))
        filled.extend(prev + (nxt - prev) * np.arange(1, steps) / steps)
        filled.append(nxt)
    return unique_sorted(np.asarray(filled))


def triggers(score: np.ndarray, threshold: float) -> list[int]:
    """Fire on crossing `threshold`; re-arm once below half of it for 8 beats."""
    fired, active, quiet = [], False, 8
    for i, v in enumerate(score):
        if active:
            if v < threshold / 2:
                active, quiet = False, 0
        else:
            quiet += 1
            if v >= threshold and quiet >= 8:
                active = True
                fired.append(i)
    return fired


def score_run(runs: list[tuple[np.ndarray, list[int], float]], threshold: float) -> dict:
    """`runs`: per track (class probabilities per step, drop beats, hours)."""
    caught, total, false_alarms, hours, leads, exact = 0, 0, 0, 0.0, [], 0
    one_bar_total = 0
    for probs, drop_beats, track_hours in runs:
        hours += track_hours
        within = probs[:, :HORIZON_BEATS].sum(1)
        within[:WARMUP_BEATS] = 0.0
        fired = triggers(within, threshold)
        for i in fired:
            if not any(1 <= d - (i + 1) <= HORIZON_BEATS + 4 for d in drop_beats):
                false_alarms += 1
        for d in drop_beats:
            if d < WARMUP_BEATS + 4:
                continue
            total += 1
            ok = [i for i in fired if 2 <= d - (i + 1) <= HORIZON_BEATS + 4]
            if ok:
                caught += 1
                leads.append(d - (ok[0] + 1))
            # One bar ahead: does the countdown name the drop beat (±1)?
            i = d - 5
            if 0 <= i < len(probs):
                one_bar_total += 1
                exact += abs(int(np.argmax(probs[i, :MAX_AHEAD])) + 1 - 4) <= 1
    return {
        "threshold": round(float(threshold), 2),
        "caught": caught,
        "drops": total,
        "median_lead_beats": float(np.median(leads)) if leads else 0.0,
        "false_alarms_per_hour": false_alarms / max(hours, 1e-9),
        "countdown_exact_1_bar_ahead": exact / max(one_bar_total, 1),
    }


def sweep(runs) -> list[dict]:
    return [score_run(runs, th) for th in np.arange(0.2, 0.96, 0.05)]


def best_under(rows: list[dict], max_fa: float) -> dict:
    ok = [r for r in rows if r["false_alarms_per_hour"] <= max_fa] or [rows[-1]]
    return max(ok, key=lambda r: (r["caught"], -r["false_alarms_per_hour"]))


def describe(name: str, rows: list[dict]) -> str:
    strict, loose = best_under(rows, 4.0), best_under(rows, 12.0)
    return (
        f"{name}: caught {strict['caught']}/{strict['drops']} at ≤4 FA/h, "
        f"{loose['caught']} at ≤12 FA/h ({loose['median_lead_beats']:.0f} beats ahead), "
        f"countdown exact {strict['countdown_exact_1_bar_ahead']:.0%}"
    )


@torch.no_grad()
def model_probs(model, mel, beats, mean, std, device) -> np.ndarray:
    x = (beat_features(mel, beats) - mean) / std
    logits = model(torch.from_numpy(x.T[None].astype(np.float32)).to(device))[0]
    return torch.softmax(logits, 0).T.cpu().numpy()


def evaluate(model, val, grids: dict, mean, std, device, baseline=None) -> dict:
    """`grids`: name -> {track id: beat frames}."""
    model.eval()
    result = {}
    for grid_name, grid in grids.items():
        runs, base_runs = [], []
        for dt in val:
            beats = grid.get(dt.track.id)
            if beats is None or len(beats) < WARMUP_BEATS + 8:
                continue
            hours = len(dt.track.mel) / 100 / 3600
            drop_beats = drop_steps(dt.drops, beats)
            runs.append((model_probs(model, dt.track.mel, beats, mean, std, device), drop_beats, hours))
            if baseline is not None:
                base_runs.append((baseline.probs(dt.track.mel, beats), drop_beats, hours))
        result[grid_name] = {"model": sweep(runs)}
        if baseline is not None:
            result[grid_name]["baseline"] = sweep(base_runs)
    model.train()
    return result


# ---------------------------------------------------------------------------


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--features", type=Path, required=True)
    ap.add_argument("--run", required=True)
    ap.add_argument("--tracker-beats", type=Path, help="beat_eval trace dir for val tracks")
    ap.add_argument("--epochs", type=int, default=30)
    ap.add_argument("--steps", type=int, default=100)
    ap.add_argument("--batch", type=int, default=32)
    ap.add_argument("--lr", type=float, default=2e-3)
    ap.add_argument("--channels", type=int, default=48)
    ap.add_argument("--dropout", type=float, default=0.2)
    ap.add_argument("--none-weight", type=float, default=0.3)
    ap.add_argument("--no-augment", action="store_true")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--device", default="mps" if torch.backends.mps.is_available() else "cpu")
    args = ap.parse_args()

    torch.manual_seed(args.seed)
    run_dir = HERE / "runs" / args.run
    run_dir.mkdir(parents=True, exist_ok=True)
    device = torch.device(args.device)

    index, tracks = load_tracks(args.features)
    centers = np.asarray(index["frontend"]["band_centers_hz"])
    labelled = [DropTrack(t, find_drops(t.mel, t.downbeats, centers)) for t in tracks]
    is_val = lambda dt: int(dt.track.id, 16) % 100 < VAL_PERCENT  # noqa: E731
    train = [dt for dt in labelled if not is_val(dt)]
    val = [dt for dt in labelled if is_val(dt)]
    print(
        f"train {len(train)} tracks / {sum(len(d.drops) for d in train)} drops, "
        f"val {len(val)} tracks / {sum(len(d.drops) for d in val)} drops"
    )

    feats = np.concatenate([beat_features(dt.track.mel, dt.track.beats) for dt in train])
    mean = feats.mean(0).astype(np.float32)
    std = (feats.std(0) + 1e-3).astype(np.float32)
    del feats

    grids = {"rekordbox": {dt.track.id: dt.track.beats for dt in val}}
    if args.tracker_beats:
        grids["tracker"] = {
            dt.track.id: b
            for dt in val
            if (b := load_tracker_beats(args.tracker_beats, dt.track.id)) is not None
        }
    baseline = BreakdownBaseline(train, centers)

    config = ModelConfig(
        n_mels=mean.shape[0],
        channels=args.channels,
        dilations=(1, 2, 4, 8, 16, 32),
        dropout=args.dropout,
        n_outputs=MAX_AHEAD + 1,
    )
    model = BeatTCN(config).to(device)
    n_params = sum(p.numel() for p in model.parameters())
    print(f"{n_params / 1e3:.0f}k params, receptive field {config.receptive_field()} beats")

    class_weight = torch.ones(MAX_AHEAD + 1, device=device)
    class_weight[NONE] = args.none_weight
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=5e-2)
    schedule = torch.optim.lr_scheduler.OneCycleLR(
        optimizer, args.lr, total_steps=args.epochs * args.steps, pct_start=0.05
    )
    sampler = Sampler(train, args, args.seed)
    mean_t = torch.from_numpy(mean).to(device)[:, None]
    std_t = torch.from_numpy(std).to(device)[:, None]

    log = (run_dir / "log.jsonl").open("w")
    best_score = -1.0
    started = time.time()
    for epoch in range(1, args.epochs + 1):
        losses = []
        for _ in range(args.steps):
            x_np, y_np = sampler.batch(args.batch)
            x = (torch.from_numpy(x_np).to(device) - mean_t) / std_t
            y = torch.from_numpy(y_np).to(device)
            logits = model(x)[..., WARMUP_BEATS:]
            loss = F.cross_entropy(logits, y[:, WARMUP_BEATS:], weight=class_weight)
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            optimizer.step()
            schedule.step()
            losses.append(loss.item())

        scores = evaluate(
            model, val, grids, mean, std, device, baseline if epoch == 1 else None
        )
        if epoch == 1:
            for grid_name, s in scores.items():
                print(f"  baseline on {grid_name} beats: {describe('rule', s['baseline'])}")
            baseline_scores = {g: s.pop("baseline") for g, s in scores.items()}
        rb = best_under(scores["rekordbox"]["model"], 4.0)
        # Select on caught drops at ≤4 FA/h, then countdown accuracy.
        score = rb["caught"] + rb["countdown_exact_1_bar_ahead"]
        entry = {"epoch": epoch, "loss": float(np.mean(losses)), "score": score,
                 "minutes": (time.time() - started) / 60}
        log.write(json.dumps(entry) + "\n")
        log.flush()
        print(
            f"epoch {epoch:3d}  loss {entry['loss']:.3f}  "
            + "  ".join(describe(g, s["model"]) for g, s in scores.items())
            + f"  {entry['minutes']:.1f} min",
            flush=True,
        )
        if score > best_score:
            best_score = score
            torch.save(
                {
                    "model": model.state_dict(),
                    "model_config": config.to_dict(),
                    "frontend": index["frontend"],
                    "max_ahead": MAX_AHEAD,
                    "horizon_beats": HORIZON_BEATS,
                    "mean": mean,
                    "std": std,
                    "epoch": epoch,
                    "args": {k: str(v) for k, v in vars(args).items()},
                },
                run_dir / "model.pt",
            )
            (run_dir / "scores.json").write_text(
                json.dumps({"epoch": epoch, "model": scores, "baseline": baseline_scores}, indent=1)
            )

    print(f"best checkpoint: {run_dir / 'model.pt'}")


if __name__ == "__main__":
    main()
