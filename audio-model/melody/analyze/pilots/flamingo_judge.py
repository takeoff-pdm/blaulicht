"""On the Mac, after pulling: score the Music Flamingo pilot against the other analyzers (no ground truth,
so "agrees with"). Writes `results/flamingo_metrics.json` and `results/flamingo.txt`.

    python3 audio-model/melody/analyze/pilots/flamingo_judge.py
"""

import base64
import itertools
import json
import random
import re
import statistics as st
from collections import Counter
from pathlib import Path

HERE = Path(__file__).parent
FEATURES = HERE.parents[2] / "data/melody/features/flamingo"
RAW = HERE / "results/flamingo"
OUT = HERE / "results"
SONGS = ["58e556af6855", "fe0b3c607b1b", "8a0e3b75f4a1", "dcb7a7d3e846", "6a67050f048d"]
KIND = {"Intro": "intro", "Build": "build-up", "Drop": "drop", "Breakdown": "breakdown", "Outro": "outro"}
rng = random.Random(0)


def load(p):
    return json.loads(Path(p).read_text())


def overlap(a, b, regions):
    return sum(max(0.0, min(b, y) - max(a, x)) for x, y in regions) / max(b - a, 1e-6)


def main_section(a, b, sections):
    return max(sections, key=lambda s: min(b, s["end"]) - max(a, s["start"]))


def rms_energy(c, a, b):
    e = base64.b64decode(c["energy"]["mix"])
    fps = c["fps"]
    seg = e[int(a * fps): max(int(a * fps) + 1, int(b * fps))]
    return sum(seg) / len(seg)


def norm(s):
    return re.sub(r"\s+", " ", str(s or "").lower().replace("-", " ")).strip()


def pearson(x, y):
    mx, my = st.mean(x), st.mean(y)
    sx, sy = sum((a - mx) ** 2 for a in x) ** 0.5, sum((b - my) ** 2 for b in y) ** 0.5
    return sum((a - mx) * (b - my) for a, b in zip(x, y)) / (sx * sy) if sx and sy else 0.0


def pair_stats(parts):
    """Agreement between parts with the same letter and, as the baseline, with different letters in the same song."""
    same, diff = [], []
    for p, q in itertools.combinations(parts, 2):
        a, b = p["parsed"], q["parsed"]
        if not a or not b or a["energy"] is None or b["energy"] is None:
            continue
        row = {"energy_diff": abs(a["energy"] - b["energy"]), "function": a["function"] == b["function"], "vocals": a["vocals"] == b["vocals"],
               "lead": norm(a["lead_instrument"]) == norm(b["lead_instrument"]), "mood": norm(a["mood"]) == norm(b["mood"])}
        (same if p["letter"] == q["letter"] else diff).append(row)
    return same, diff


def summarise_pairs(rows):
    if not rows:
        return {"pairs": 0}
    return {"pairs": len(rows), "energy_diff": round(st.mean(r["energy_diff"] for r in rows), 2),
            **{k: round(st.mean(r[k] for r in rows), 2) for k in ("function", "vocals", "lead", "mood")}}


def drop_rises(c, rows, value):
    """Per drop: mean energy of the Breakdown and Build clips before it, and of the Drop clip after it."""
    out = []
    for d in c["drops"]:
        t = d["time"]
        pre = [r for r in rows if r["end"] <= t + 0.01 and r["kind"] in ("Breakdown", "Build") and r["end"] > t - 60 and value(r) is not None]
        post = [r for r in rows if abs(r["start"] - t) < 0.5 and r["kind"] == "Drop" and value(r) is not None]
        if pre and post:
            out.append({"time": round(t, 1), "pre": round(st.mean(value(r) for r in pre), 1), "post": round(st.mean(value(r) for r in post), 1)})
    return out


def boundaries(parsed):
    b = sorted({s["start"] for s in parsed} | {s["end"] for s in parsed})
    return b


def nearest(x, xs):
    return min(abs(x - y) for y in xs) if xs else None


def chance_distance(n, duration, targets, sims=500):
    if not n or not targets:
        return None
    vals = [st.median(nearest(rng.uniform(0, duration), targets) for _ in range(n)) for _ in range(sims)]
    return round(st.mean(vals), 1)


def stamps(raw):
    return [sum(int(p) * 60 ** i for i, p in enumerate(reversed(m.split(":")))) for m in re.findall(r"\b\d{1,2}:\d{2}\b", raw)]


def main():
    contexts = {s: load(HERE / "context" / f"{s}.json") for s in SONGS}
    docs = {s: load(FEATURES / f"{s}.json") for s in SONGS}
    sections = load(RAW / "sections.json")
    controls = load(RAW / "controls.json")
    stamps_doc = load(RAW / "timestamps.json")
    robust = load(RAW / "robust.json")
    batch = load(RAW / "batch.json")
    long_doc = load(RAW / "long.json")
    m, lines = {}, []

    # parsing and speed
    parts = [(s, p) for s in SONGS for p in docs[s]["parts"]]
    sec_rows = [(s, r) for s in SONGS for r in sections[s]]
    all_clips = parts + sec_rows
    seconds = [p["seconds"] for _, p in all_clips]
    whole = [docs[s]["whole"][k]["seconds"] for s in SONGS for k in ("describe", "sections")]
    m["speed"] = {"clips": len(all_clips), "parse_ok": sum(p["parsed"] is not None for _, p in all_clips), "seconds_per_clip_mean": round(st.mean(seconds), 2),
                  "seconds_per_clip_max": max(seconds), "seconds_whole_song_query_mean": round(st.mean(whole), 1),
                  "hours_175_songs_10_parts": round(175 * 10 * st.mean(seconds) / 3600, 2), "hours_175_songs_with_2_whole_queries": round((175 * 10 * st.mean(seconds) + 175 * 2 * st.mean(whole)) / 3600, 2),
                  "batch": [{k: b[k] for k in ("batch", "clips", "seconds", "same_as_batch_1")} for b in batch],
                  "memory": {s: docs[s]["memory"] for s in SONGS}, "long": {k: {a: b for a, b in v.items() if a != "raw"} for k, v in long_doc.items()}}

    # same letter, same answer?
    same, diff = [], []
    per_song = {}
    for s in SONGS:
        a, b = pair_stats(docs[s]["parts"])
        same += a
        diff += b
        per_song[s] = {"same": summarise_pairs(a), "different": summarise_pairs(b)}
    m["letters"] = {"same_letter": summarise_pairs(same), "different_letter": summarise_pairs(diff), "per_song": per_song}

    # energy across drops
    rises = {}
    for s in SONGS:
        c, rows = contexts[s], sections[s]
        for r in rows:
            r["level"] = rms_energy(c, r["start"], r["end"])
        rises[s] = {"flamingo": drop_rises(c, rows, lambda r: r["parsed"]["energy"] if r["parsed"] else None), "signal_level": drop_rises(c, rows, lambda r: r["level"])}
    flat = [x for s in SONGS for x in rises[s]["flamingo"]]
    part_rise = []
    for s in SONGS:
        ps = docs[s]["parts"]
        for d in contexts[s]["drops"]:
            i = next((i for i, p in enumerate(ps) if abs(p["start"] - d["time"]) < 0.5), None)
            if i and ps[i]["parsed"] and ps[i - 1]["parsed"]:
                part_rise.append({"song": s, "time": round(d["time"], 1), "before": ps[i - 1]["parsed"]["energy"], "after": ps[i]["parsed"]["energy"]})
    x, y = [], []
    for s, r in sec_rows:
        if r["parsed"] and r["parsed"]["energy"] is not None:
            x.append(r["parsed"]["energy"])
            y.append(r["level"])
    m["energy"] = {"sections_per_drop": rises, "rose": sum(r["post"] > r["pre"] for r in flat), "drops": len(flat), "mean_rise": round(st.mean(r["post"] - r["pre"] for r in flat), 2),
                   "parts_around_drops": part_rise, "parts_rose": sum(r["after"] > r["before"] for r in part_rise), "parts_same": sum(r["after"] == r["before"] for r in part_rise),
                   "corr_energy_vs_signal_level": round(pearson(x, y), 2), "energy_histogram": dict(sorted(Counter(x).items()))}

    # energy and function on clips of equal length around each drop, since energy grows with clip length
    lengths = [(r["end"] - r["start"], r["parsed"]["energy"]) for _, r in all_clips if r["parsed"] and r["parsed"]["energy"] is not None]
    m["energy"]["corr_energy_vs_clip_length"] = round(pearson([a for a, _ in lengths], [b for _, b in lengths]), 2)
    fixed = load(RAW / "fixed.json")
    eq = {}
    for length in (8, 16, 30):
        rows = [r for s in SONGS for r in fixed[s] if r["length"] == length]
        row = {"drops": len(rows)}
        for w in ("default", "reversed"):
            pairs = [(r[f"before_{w}"]["parsed"], r[f"after_{w}"]["parsed"]) for r in rows]
            pairs = [(a, b) for a, b in pairs if a and b and a["energy"] is not None and b["energy"] is not None]
            row[w] = {"rose": sum(b["energy"] > a["energy"] for a, b in pairs), "same": sum(b["energy"] == a["energy"] for a, b in pairs), "fell": sum(b["energy"] < a["energy"] for a, b in pairs),
                      "mean_before": round(st.mean(a["energy"] for a, _ in pairs), 2), "mean_after": round(st.mean(b["energy"] for _, b in pairs), 2),
                      "says_drop_after": sum(b["function"] == "drop" for _, b in pairs), "says_drop_before": sum(a["function"] == "drop" for a, _ in pairs)}
        eq[str(length)] = row
    m["equal_length_around_drops"] = eq

    # function
    conf = Counter()
    for s, r in sec_rows:
        if r["kind"] in KIND and r["parsed"]:
            conf[(KIND[r["kind"]], r["parsed"]["function"])] += 1
    total = sum(conf.values())
    truth = Counter()
    for (a, _), n in conf.items():
        truth[a] += n
    conf_parts = Counter()
    for s, p in parts:
        k = main_section(p["start"], p["end"], contexts[s]["sections"])["kind"]
        if k in KIND and p["parsed"]:
            conf_parts[(KIND[k], p["parsed"]["function"])] += 1
    m["function"] = {"section_clips": {"n": total, "agree": round(sum(n for (a, b), n in conf.items() if a == b) / total, 2), "always_commonest_label": round(truth.most_common(1)[0][1] / total, 2),
                                       "answers": dict(Counter(b for (_, b), n in conf.items() for _ in range(n)))},
                     "confusion": {f"{a} -> {b}": n for (a, b), n in sorted(conf.items(), key=str)},
                     "part_clips": {"n": sum(conf_parts.values()), "agree": round(sum(n for (a, b), n in conf_parts.items() if a == b) / max(sum(conf_parts.values()), 1), 2),
                                    "answers": dict(Counter(b for (_, b), n in conf_parts.items() for _ in range(n)))}}

    # vocals
    table = Counter()
    per = {}
    for s, r in all_clips:
        if not r["parsed"]:
            continue
        share = overlap(r["start"], r["end"], contexts[s]["vocal_regions"] or [])
        klass = "regions mostly" if share >= 0.5 else "regions little" if share <= 0.1 else "regions some"
        table[(klass, r["parsed"]["vocals"])] += 1
        per.setdefault(s, Counter())[(klass, r["parsed"]["vocals"])] += 1
    fmt = lambda t: {f"{a}, says {'yes' if b else 'no'}": n for (a, b), n in sorted(t.items(), key=str)}
    clear = {k: n for k, n in table.items() if k[0] != "regions some"}
    agree = sum(n for (k, v), n in clear.items() if (k == "regions mostly") == v)
    m["vocals"] = {"table": fmt(table), "clear_clips": sum(clear.values()), "agree_on_clear_clips": round(agree / max(sum(clear.values()), 1), 2),
                   "per_song": {contexts[s]["name"]: {**fmt(t), "vocal_share": contexts[s]["vocal_share"], "lyric_lines": len(contexts[s]["lines"] or [])} for s, t in per.items()}}

    # instruments
    inst = {}
    for s in SONGS:
        lead, other = Counter(), Counter()
        for p in docs[s]["parts"] + sections[s]:
            if p["parsed"]:
                lead[norm(p["parsed"].get("lead_instrument"))] += 1
                for o in p["parsed"].get("other_instruments") or []:
                    other[norm(o)] += 1
        inst[contexts[s]["name"]] = {"flamingo_lead": lead.most_common(5), "flamingo_other": other.most_common(6), "mt3_notes": contexts[s]["mt3_summary"], "styles": [x[0] for x in contexts[s]["styles"][:2]]}
    m["instruments"] = inst
    names = Counter()
    for _, p in all_clips:
        if p["parsed"]:
            for o in [p["parsed"].get("lead_instrument")] + list(p["parsed"].get("other_instruments") or []):
                names[norm(o)] += 1
    m["instrument_vocabulary"] = names.most_common(25)
    m["acoustic_names"] = {w: sum(n for k, n in names.items() if w in k) for w in ("piano", "guitar", "strings", "brass", "organ", "synth", "bass", "drum")}

    # timestamps
    ts = {}
    for s in SONGS:
        c = contexts[s]
        parsed = docs[s]["whole"]["sections"]["parsed"]
        b = boundaries(parsed)
        part_bounds = sorted({p[0] for p in c["parts"]} | {c["parts"][-1][1]})
        drop_t = [d["time"] for d in c["drops"]]
        dists = [nearest(x, part_bounds) for x in b]
        direct = stamps(stamps_doc[s]["drops_direct"]["raw"])
        crop, pad = stamps_doc[s]["cropped_60"], stamps_doc[s]["padded_30"]
        cb, pb = boundaries(crop["sections"]["parsed"]), boundaries(pad["sections"]["parsed"])
        ts[c["name"]] = {
            "duration": c["duration"], "said": [[p["start"], p["end"], p["name"]] for p in parsed], "last_said": max(b) if b else None,
            "multiple_of_15_s": round(st.mean(x % 15 == 0 for x in b), 2) if b else None, "median_distance_to_part_boundary": round(st.median(dists), 1) if dists else None,
            "chance_distance": chance_distance(len(b), c["duration"], part_bounds), "drop_to_nearest_said_boundary": [round(nearest(d, b), 1) for d in drop_t] if b else None,
            "drops": [round(d, 1) for d in drop_t], "drops_direct": direct, "drops_direct_distance": [round(nearest(x, drop_t), 1) for x in direct] if drop_t else None,
            "cropped_60_boundaries": cb, "cropped_60_median_distance_after_undoing_shift": round(st.median(nearest(x + 60, b) for x in cb), 1) if cb and b else None,
            "cropped_60_median_distance_without_undoing": round(st.median(nearest(x, b) for x in cb), 1) if cb and b else None,
            "padded_30_boundaries": pb, "padded_30_median_distance_after_undoing_shift": round(st.median(nearest(x - 30, b) for x in pb), 1) if pb and b else None,
            "padded_30_median_distance_without_undoing": round(st.median(nearest(x, b) for x in pb), 1) if pb and b else None,
            "drops_direct_cropped": stamps(crop["drops_direct"]["raw"]), "drops_direct_padded": stamps(pad["drops_direct"]["raw"]),
        }
    m["timestamps"] = ts

    # does the answer depend on the wording?
    rob = {k: Counter() for k in ("reversed", "no_hint")}
    fn_answers = {k: Counter() for k in ("default", "reversed", "no_hint")}
    energy_pairs = {k: ([], []) for k in rob}
    for s in SONGS:
        for p, r in zip(docs[s]["parts"], robust[s]):
            if not p["parsed"]:
                continue
            fn_answers["default"][p["parsed"]["function"]] += 1
            for k in rob:
                q = r[k]["parsed"]
                if not q:
                    continue
                fn_answers[k][q["function"]] += 1
                rob[k]["n"] += 1
                rob[k]["function"] += q["function"] == p["parsed"]["function"]
                rob[k]["vocals"] += q["vocals"] == p["parsed"]["vocals"]
                rob[k]["lead"] += norm(q["lead_instrument"]) == norm(p["parsed"]["lead_instrument"])
                if q["energy"] is not None and p["parsed"]["energy"] is not None:
                    energy_pairs[k][0].append(q["energy"])
                    energy_pairs[k][1].append(p["parsed"]["energy"])
                    rob[k]["energy_diff"] += abs(q["energy"] - p["parsed"]["energy"])
    m["wording"] = {k: {"n": v["n"], "same_function": round(v["function"] / v["n"], 2), "same_vocals": round(v["vocals"] / v["n"], 2), "same_lead": round(v["lead"] / v["n"], 2),
                        "energy_diff": round(v["energy_diff"] / v["n"], 2), "energy_corr": round(pearson(*energy_pairs[k]), 2)} for k, v in rob.items()}
    m["function_answers_by_wording"] = {k: dict(v) for k, v in fn_answers.items()}

    # controls
    m["controls"] = {k: {"rms_dbfs": v["rms_dbfs"], "part": v["part"]["raw"], "hear": v["hear"]["raw"], "hear_careful_system": v.get("hear_careful_system", {}).get("raw"), "describe": v["describe"]["raw"][:400]}
                     for k, v in controls.items()}

    (OUT / "flamingo_metrics.json").write_text(json.dumps(m, indent=1, ensure_ascii=False, default=str))

    # the readable text
    w = lines.append
    w("Music Flamingo pilot, 5 songs. No ground truth: \"agrees with\" the other analyzers.\n")
    w(f"Speed {json.dumps(m['speed'], ensure_ascii=False)}\n")
    w(f"Same letter against different letter: {json.dumps(m['letters']['same_letter'])} / {json.dumps(m['letters']['different_letter'])}\n")
    w(f"Energy: {json.dumps({k: m['energy'][k] for k in ('rose', 'drops', 'mean_rise', 'parts_rose', 'parts_same', 'corr_energy_vs_signal_level', 'energy_histogram')}, default=str)}\n")
    w(f"Energy against clip length: r = {m['energy']['corr_energy_vs_clip_length']}. Equal lengths around drops: {json.dumps(m['equal_length_around_drops'])}\n")
    w(f"Function: {json.dumps(m['function'], ensure_ascii=False)}\n")
    w(f"Vocals: {json.dumps(m['vocals'], ensure_ascii=False)}\n")
    w(f"Wording: {json.dumps(m['wording'])}  {json.dumps(m['function_answers_by_wording'], default=str)}\n")
    w("Instruments:")
    for k, v in m["instruments"].items():
        w(f"  {k}: {json.dumps(v, ensure_ascii=False)}")
    w(f"\nNames across all clips: {m['acoustic_names']}\n")
    w("Timestamps:")
    for k, v in ts.items():
        w(f"  {k}: {json.dumps(v, ensure_ascii=False)}")
    w("\nControls:")
    for k, v in m["controls"].items():
        w(f"  [{k}] {v['rms_dbfs']} dBFS\n    part: {v['part']}\n    hear: {v['hear']}\n    careful: {v['hear_careful_system']}\n    describe: {v['describe']}")
    w("\nWhole-song answers:")
    for s in SONGS:
        w(f"  [{contexts[s]['name']}] key {contexts[s]['key']}, {contexts[s]['bpm']} BPM\n{docs[s]['whole']['describe']['raw']}\n")
    w("Part answers:")
    for s in SONGS:
        w(f"  [{contexts[s]['name']}]")
        for p in docs[s]["parts"]:
            w(f"    {p['start']:.1f}-{p['end']:.1f} {p['letter']}: {p['raw']}")
    (OUT / "flamingo.txt").write_text("\n".join(lines) + "\n")
    print((OUT / "flamingo.txt").read_text()[:200])


if __name__ == "__main__":
    main()
