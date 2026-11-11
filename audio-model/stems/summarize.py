"""Collect eval JSONs into Markdown tables for the README.

    python summarize.py runs/eval_refs.json runs/eval_la*.json ...
"""

import json
import sys
from pathlib import Path

LATENCY_MS = {"la0": 46, "la10": 146, "la30": 346, "la100": 1046, "gt_la30": 346, "nomusdb_la30": 346}
STEMS = ("drums", "bass", "other", "vocals")


def main() -> None:
    rows = {}
    for p in sys.argv[1:]:
        rows |= json.loads(Path(p).read_text())
    print("| model | latency | MUSDB uSDR avg | drums | bass | other | vocals | cSDR' avg |")
    print("|---|---|---|---|---|---|---|---|")
    for name, r in rows.items():
        m = r.get("musdb_test")
        if not m:
            continue
        run = name.split("/")[0]
        lat = f"{LATENCY_MS[run]} ms" if run in LATENCY_MS else "offline" if "teacher" in name else "-"
        u = m["usdr"]
        print(f"| {name} | {lat} | **{u['avg']:.2f}** | " + " | ".join(f"{u[s]:.2f}" for s in STEMS)
              + f" | {m['csdr']['avg']:.2f} |")
    print()
    print("| model | latency | uSDR vs teacher avg | drums | bass | other | vocals | env corr avg | "
          "active F1 avg |")
    print("|---|---|---|---|---|---|---|---|---|")
    for name, r in rows.items():
        e = r.get("edm_test")
        if not e:
            continue
        run = name.split("/")[0]
        lat = f"{LATENCY_MS[run]} ms" if run in LATENCY_MS else "-"
        u = e["usdr_vs_teacher"]
        print(f"| {name} | {lat} | **{u['avg']:.2f}** | " + " | ".join(f"{u[s]:.2f}" for s in STEMS)
              + f" | {e['env_corr']['avg']:.3f} | {e['active_f1']['avg']:.3f} |")


if __name__ == "__main__":
    main()
