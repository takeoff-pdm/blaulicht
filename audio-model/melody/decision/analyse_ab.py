"""What the blind A/B answers say, and the stop rules fixed before any answer came in.

    python3 audio-model/melody/decision/analyse_ab.py

"Can't tell" counts half a win. One judge, 14 songs: the result holds for this judge on this kind of song, and that
is the point, since this judge is who the show must please. Stop rules (fixed in advance):

  1. Judge's consistency: 6 repeats with the sides swapped. Under 4 of 6 the same way: the judge can't separate these
     clips reliably, so no decision model can learn from these judgements; stop and improve the looks.
  2. Random against rules, 14 trials: rules need 11 or more wins (one-sided p = 0.029). Under that, the sequencing of
     this vocabulary barely matters: stop decision-model work and spend the time on new looks instead.
  3. Beam or LLM against rules, 14 trials each: the candidate needs 11 or more wins to replace the rules. 5 or
     fewer wins: reject it. Between: no decision, keep the rules (cheaper, explainable).
"""

import json
import math
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
TRIALS = json.loads((HERE / "ab_trials.json").read_text())
ANS = HERE / "ab_results/answers.jsonl"


def binom_tail(k, n):
    """P(X >= k) for X ~ Binomial(n, 1/2); half wins round down, which is conservative."""
    k = math.floor(k)
    return sum(math.comb(n, i) for i in range(k, n + 1)) / 2**n


def main():
    if not ANS.exists():
        raise SystemExit("no answers yet")
    answers = {}
    for line in ANS.read_text().splitlines():
        a = json.loads(line)
        answers[a["n"]] = a
    wins = defaultdict(lambda: [0.0, 0])
    per = []
    for t in TRIALS:
        a = answers.get(t["n"])
        if a is None or "repeat_of" in t:
            continue
        v = t["against"]
        winner = None if a["choice"] == "same" else t[a["choice"]]
        wins[v][0] += 1.0 if winner == v else 0.5 if winner is None else 0.0
        wins[v][1] += 1
        per.append((t["name"], v, winner, a.get("replays"), a.get("seconds")))
    print(f"answered {len(answers)} of {len(TRIALS)}")
    for v, (w, n) in wins.items():
        print(f"{v} over rules: {w} of {n}; rules over {v}: {n - w} of {n}; one-sided p for rules winning {binom_tail(n - w, n):.3f}, for {v} winning {binom_tail(w, n):.3f}")
    same = total = 0
    for t in TRIALS:
        if "repeat_of" not in t or t["n"] not in answers or t["repeat_of"] not in answers:
            continue
        a, b = answers[t["n"]], answers[t["repeat_of"]]
        wa = None if a["choice"] == "same" else t[a["choice"]]
        first = TRIALS[t["repeat_of"]]
        wb = None if b["choice"] == "same" else first[b["choice"]]
        same += wa == wb
        total += 1
    print(f"consistency: same winner in {same} of {total} repeats")
    secs = [a.get("seconds", 0) for a in answers.values()]
    print(f"time: {sum(secs) / 60:.0f} minutes in all, median {sorted(secs)[len(secs) // 2]} s per trial")
    if len(answers) < len(TRIALS):
        print("\nno verdict yet: the stop rules need all the trials")
    else:
        print("\nverdict by the stop rules:")
        if same < 4 * total / 6:
            print("  1. the judge isn't consistent on these clips: stop; no decision model can learn from these judgements")
        r = wins["random"]
        if r[1] - r[0] < 11:
            print("  2. random legal plans aren't clearly worse than the rules: sequencing barely matters; work on looks instead")
        for v in ("beam", "llm"):
            w, n = wins[v]
            print(f"  3. {v}: " + ("replaces the rules" if w >= 11 else "rejected" if w <= 5 else "no decision: keep the rules"))
    for row in per:
        print("   ", row)


if __name__ == "__main__":
    main()
