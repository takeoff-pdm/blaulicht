"""The comparison in one table: python -m analyze.fx_train.tabulate dev|test"""

import json
import sys

from analyze.fx_train.evaluate import RESULTS

split = sys.argv[1] if len(sys.argv) > 1 else "test"
r = json.loads((RESULTS / f"real_{split}.json").read_text())
names = [n for n in r if n not in ("split", "songs", "dsp")]
print(split, r["songs"], "songs,", r["dsp"]["riser"]["n_drops"], "drops,", r["dsp"]["riser"]["n_null"], "null lines,", r["dsp"]["riser"]["n_quiet"], "quiet null lines")


def line(n, s):
    a = s["at_null_rate"]
    at = lambda k: "-" if not a.get(k) else f"{a[k]['drop']:.3f}"
    return (f"{n:11s} drop {s['drop']:.3f} null {s['null']:.3f} quiet {s['quiet_null']:.3f} lift {s['lift']} {s['lift_ci']} liftQ {s['lift_quiet']} {s['lift_quiet_ci']} "
            f"auc {s.get('auc')} aucQ {s.get('auc_quiet')} @1% {at('0.01')} @3% {at('0.03')} @5% {at('0.05')} @10% {at('0.1')}")


for kind in ("riser", "impact"):
    print(kind.upper())
    print(line("dsp", r["dsp"][kind]))
    for n in names:
        print(line(n, r[n][kind]))
print("per minute, DSP:", r["dsp"]["per_min"])
print("REVERSAL any-frame lift (null/quiet): forward, reversed riser, reversed downlifter | per minute riser impact downlifter horn siren revcym | riser dur, seam share")
for n in names:
    v, f = r[n]["reversal"], r[n]["fire"]
    print(f"{n:11s} fwd {v['fwd']['lift']}/{v['fwd']['lift_quiet']} rev {v['rev']['lift']}/{v['rev']['lift_quiet']} revdown {v['rev_down']['lift']}/{v['rev_down']['lift_quiet']} | "
          f"{f['riser']['per_min']} {f['impact']['per_min']} {f['downlifter']['per_min']} {f['horn']['per_min']} {f['siren']['per_min']} {f['revcym']['per_min']} | {f['riser']['dur']} {r[n]['share_of_events_within_0.2s_of_5s_multiple']}")
try:
    syn = json.loads((RESULTS / "synth.json").read_text())
    print("SYNTHETIC mean event F1 (frame/event per class at synthetic thresholds); empty fake drops at real thresholds")
    for n in names:
        for s, row in syn.get(n, {}).items():
            print(f"{n:11s} {s:9s} {row['mean_event_f1']:.3f} " + " ".join(f"{c[:4]} {row[c]['frame_f1']:.2f}/{row[c]['event_f1']:.2f}" for c in row if c not in ("empty_fake_drops", "mean_event_f1"))
                  + f" empty n={row['empty_fake_drops']['n']} riser {row['empty_fake_drops']['riser_fires']} impact {row['empty_fake_drops']['impact_fires']}")
except OSError:
    pass
