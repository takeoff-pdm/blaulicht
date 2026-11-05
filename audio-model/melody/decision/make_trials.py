"""Build the blind A/B trial list for ab.html.

    python3 audio-model/melody/decision/make_trials.py

Songs: ab_songs.txt, the first 14 songs with two or more drops in the labelling queue's fixed random order, so an
unbiased sample of the electronic songs. Every song is compared to the rules three times: against the random legal
plan, the beam plan and the LLM plan. Six of those comparisons come back at the end with the sides swapped, to measure
how consistent the judge is. A trial plays each side's two windows, one bar before and six bars after two
drops of the song (the first and the biggest), then asks which is better.

Which side holds which plan is random and only in ab_trials.json; the page never sees a variant's name.
"""

import json
import random
from pathlib import Path

HERE = Path(__file__).resolve().parent
PRE_BARS, POST_BARS = 1, 6
VARIANTS = ("random", "beam", "llm")
SEED = 20261105


def main():
    rng = random.Random(SEED)
    songs = [x.strip() for x in (HERE / "ab_songs.txt").read_text().splitlines() if x.strip()]
    trials = []
    for sid in songs:
        st = json.loads((HERE / "structure" / f"{sid}.json").read_text())
        bar = 4 * 60 / st["bpm"]
        ds = st["drops"]
        first = ds[0]
        big = max(ds[1:], key=lambda d: (d["breakdown_bars"], d["low_jump_db"]))
        windows = [[round(max(0, d["time"] - PRE_BARS * bar), 3), round(min(st["duration"], d["time"] + POST_BARS * bar), 3)] for d in (first, big)]
        for v in VARIANTS:
            if not (HERE / "plans" / v / f"{sid}.json").exists():
                raise SystemExit(f"no {v} plan for {sid}")
            a_is_rules = rng.random() < 0.5
            trials.append({"song": sid, "name": st["name"], "windows": windows, "A": "rules" if a_is_rules else v, "B": v if a_is_rules else "rules", "against": v})
    rng.shuffle(trials)
    # the repeats: two comparisons per variant, sides swapped, at the end
    repeats = []
    for v in VARIANTS:
        for t in rng.sample([t for t in trials if t["against"] == v], 2):
            repeats.append({**t, "A": t["B"], "B": t["A"], "repeat_of": trials.index(t)})
    rng.shuffle(repeats)
    trials += repeats
    for i, t in enumerate(trials):
        t["n"] = i
    (HERE / "ab_trials.json").write_text(json.dumps(trials, indent=1))
    dur = sum(2 * sum(b - a for a, b in t["windows"]) for t in trials)
    print(len(trials), "trials,", round(dur / 60, 1), "minutes of playback at one listen per side")


if __name__ == "__main__":
    main()
