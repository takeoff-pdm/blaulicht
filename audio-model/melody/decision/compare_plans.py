"""Descriptive numbers for the plan variants. These say what the plans do, not whether they look good.

    python3 audio-model/melody/decision/compare_plans.py [variants...]    # needs checks/<variant>.json from harness.cjs check

  validity     hard-rule violations in the 50 fps checks; the LLM's validator problems before repair
  variety      within a song (consecutive drops treated differently) and between songs (distinct drop
               sequences per song, colour pair spread)
  reasoned     does a drop's size follow how big the drop is (Spearman, size vs breakdown length and low-end jump)?
               By construction for beam; open for rules, random and llm
  coherence    when a structure letter returns in an energetic segment, is the look the same?
  stability    (llm only) the same song and prompt, run three times: how often do the choices agree, against other songs
"""

import json
import math
import sys
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
SIZE = {"spin": 1, "pairs": 2, "blinder": 3, "all": 4}


def spearman(a, b):
    def rank(x):
        order = sorted(range(len(x)), key=lambda i: x[i])
        r = [0.0] * len(x)
        i = 0
        while i < len(x):
            j = i
            while j + 1 < len(x) and x[order[j + 1]] == x[order[i]]:
                j += 1
            for k in range(i, j + 1):
                r[order[k]] = (i + j) / 2
            i = j + 1
        return r
    ra, rb = rank(a), rank(b)
    ma, mb = sum(ra) / len(ra), sum(rb) / len(rb)
    cov = sum((x - ma) * (y - mb) for x, y in zip(ra, rb))
    va, vb = sum((x - ma) ** 2 for x in ra), sum((y - mb) ** 2 for y in rb)
    return cov / math.sqrt(va * vb) if va and vb else float("nan")


def entropy(counter):
    n = sum(counter.values())
    return -sum(c / n * math.log2(c / n) for c in counter.values() if c)


def seg_looks(st, plan):
    """The look actually shown per segment: a hook's look wins, as in showAt."""
    if plan is None:
        hooks = dict(st["hooks"])
        return [hooks.get(s["hook"], s["rule_anim"]) if s["hook"] >= 0 else s["rule_anim"] for s in st["segs"]]
    hk = {int(k): v for k, v in plan.get("hook", {}).items()}
    return [hk.get(s["hook"], plan["seg"][i]["anim"]) if s["hook"] >= 0 else plan["seg"][i]["anim"] for i, s in enumerate(st["segs"])]


def describe(variant, ids=None):
    checks = {r["id"]: r for r in json.loads((HERE / f"checks/{variant}.json").read_text())}
    ids = [i for i in (ids or checks) if i in checks]
    V = Counter()
    sizes, breakdown, jump = [], [], []
    consec_type = consec_all = consec_n = 0
    sigs, sigs_type, pal = Counter(), Counter(), Counter()
    letter_same = letter_n = 0
    adj_same = adj_n = 0
    looks_per_song = []
    within_song_rank = []
    for sid in ids:
        r = checks[sid]
        V.update(r["violations"])
        st = json.loads((HERE / f"structure/{sid}.json").read_text())
        plan = None if variant == "rules" else json.loads((HERE / f"plans/{variant}/{sid}.json").read_text())
        pal[r["palette"]] += 1
        ds = r["drops"]
        sigs[tuple((d["type"], d["beats"], d["rate"]) for d in ds)] += 1
        sigs_type[tuple(d["type"] for d in ds)] += 1
        for a, b in zip(ds, ds[1:]):
            consec_n += 1
            consec_type += a["type"] != b["type"]
            consec_all += (a["type"], a["beats"], a["rate"]) != (b["type"], b["beats"], b["rate"])
        for d, q in zip(st["drops"], ds):
            sizes.append(SIZE[q["type"]])
            breakdown.append(d["breakdown_bars"])
            jump.append(d["low_jump_db"])
        if len(ds) >= 3:
            x = [SIZE[q["type"]] for q in ds]
            y = [d["breakdown_bars"] for d in st["drops"]]
            if len(set(x)) > 1 and len(set(y)) > 1:
                within_song_rank.append(spearman(x, y))
        looks = seg_looks(st, plan)
        seen = {}
        last = last_part = None
        vis = []
        for s, look in zip(st["segs"], looks):
            if s["energetic"] <= 0.3:
                continue
            vis.append(look)
            if s["part"] in seen and s["part"] != last_part:  # came back after something else
                letter_n += 1
                letter_same += seen[s["part"]] == look
            seen[s["part"]] = look
            if last is not None:
                adj_n += 1
                adj_same += last == look
            last, last_part = look, s["part"]
        looks_per_song.append(len(set(vis)))
    n = len(ids)
    return {"songs": n, "violations": sum(V.values()), "drops_differ_type": round(consec_type / max(1, consec_n), 3), "drops_differ_any": round(consec_all / max(1, consec_n), 3),
            "distinct_drop_sequences_per_100_songs": round(100 * len(sigs) / n, 1), "distinct_type_sequences_per_100_songs": round(100 * len(sigs_type) / n, 1), "palette_entropy_bits": round(entropy(pal), 2),
            "size_vs_breakdown": round(spearman(sizes, breakdown), 3), "size_vs_low_jump": round(spearman(sizes, jump), 3), "within_song_size_vs_breakdown_median": round(sorted(within_song_rank)[len(within_song_rank) // 2], 3) if within_song_rank else None,
            "returning_letter_same_look": round(letter_same / max(1, letter_n), 3), "adjacent_same_look": round(adj_same / max(1, adj_n), 3), "looks_per_song_median": sorted(looks_per_song)[n // 2]}


def llm_stats():
    raw = sorted((HERE / "plans/llm_raw").glob("*.json"))
    infos = [json.loads(p.read_text()) for p in raw]
    probs = [p for i in infos for p in i["problems"]]
    out = {"songs": len(infos), "songs_with_problems": sum(bool(i["problems"]) for i in infos), "problems": len(probs), "seconds_median": sorted(i["seconds"] for i in infos)[len(infos) // 2],
           "cost_usd_mean": round(sum(i.get("cost_usd") or 0 for i in infos) / max(1, len(infos)), 3), "examples": probs[:8]}
    # stability: the same song three times
    rep = []
    for sid in [p.stem for p in (HERE / "plans/llm_r2").glob("*.json")]:
        ps = [json.loads((HERE / f"plans/{v}/{sid}.json").read_text()) for v in ("llm", "llm_r2", "llm_r3") if (HERE / f"plans/{v}/{sid}.json").exists()]
        st = json.loads((HERE / f"structure/{sid}.json").read_text())
        vis = [i for i, s in enumerate(st["segs"]) if s["energetic"] > 0.3]
        looks = [seg_looks(st, p) for p in ps]
        for a in range(len(ps)):
            for b in range(a + 1, len(ps)):
                rep.append({"palette": ps[a]["palette"] == ps[b]["palette"], "looks": sum(looks[a][i] == looks[b][i] for i in vis) / max(1, len(vis)),
                            "types": sum(x["type"] == y["type"] for x, y in zip(ps[a]["drop"], ps[b]["drop"])) / max(1, len(ps[a]["drop"])),
                            "size_corr": spearman([SIZE[x["type"]] for x in ps[a]["drop"]], [SIZE[y["type"]] for y in ps[b]["drop"]]) if len(ps[a]["drop"]) >= 3 else None})
    if rep:
        out["repeats"] = {"pairs": len(rep), "same_palette": round(sum(r["palette"] for r in rep) / len(rep), 3), "same_look_share": round(sum(r["looks"] for r in rep) / len(rep), 3), "same_type_share": round(sum(r["types"] for r in rep) / len(rep), 3),
                          "chance_look": 0.2, "chance_palette": round(1 / 6, 3)}
    return out


def main():
    res = {}
    llm_ids = [p.stem for p in (HERE / "plans/llm").glob("*.json")]
    for v in sys.argv[1:] or ("rules", "random", "beam", "llm"):
        res[v] = describe(v)
        if v != "llm" and llm_ids:
            res[v + " (LLM's 40 songs)"] = describe(v, llm_ids)
    res["llm_stats"] = llm_stats()
    (HERE / "checks/compare.json").write_text(json.dumps(res, indent=1))
    keys = [k for k in next(iter(res.values())) if k != "songs"]
    rows = [k for k in res if k != "llm_stats"]
    print("metric".ljust(44) + "".join(r[:13].rjust(14) for r in rows))
    for k in ["songs"] + keys:
        print(k.ljust(44) + "".join(str(res[r].get(k)).rjust(14) for r in rows))
    print(json.dumps(res["llm_stats"], indent=1))


if __name__ == "__main__":
    main()
