"""The plan layer: a per-song choice of palette, looks and strobe treatments, always through one validator.

    python3 audio-model/melody/decision/plan.py random beam    # plans/<variant>/<id>.json for every song

A plan holds only legal choices of the existing vocabulary:

    {"palette": 0-5, "seg": [{"anim", "speed": -1|0|1, "lead": 0|1}, ...one per segment],
     "hook": {"<id>": anim}, "drop": [{"type", "beats": 1|2|4, "rate": 2|4}, ...one per drop]}

The user's hard rules are not the plan's to break: club_plan.js keeps the strobe window (only from the
drop's downbeat, never over the bar or the low end's hold), the darkness while strobing, the 30% rope flashes,
the narrow patterns and the beat-grid stepping; a plan only picks among them. validate() enforces the rest:
one of the six colour pairs for the whole song, at most one "all" per song, consecutive drops of different
type. Variants:

    random  uniformly random legal choices: the control. If this ties with the rules, sequencing doesn't matter.
    beam    a beam search over a hand-written score: rules in another form, not learned.
    llm     plan_llm.py: an LLM reads a cue table and writes the plan.
"""

import json
import random
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
STRUCT = HERE / "structure"
PLANS = HERE / "plans"

LOOKS = ("wave", "cross", "fan", "circle", "sweep")
TYPES = ("blinder", "spin", "pairs", "all")
BEATS = (1, 2, 4)
RATES = (2, 4)
N_PAIRS = 6  # PAIRS in club.js: red+orange, pink+purple, magenta+cyan, blue+violet, cyan+blue, green+teal


def load_structure(sid):
    return json.loads((STRUCT / f"{sid}.json").read_text())


def validate(plan, st):
    """(clean plan, problems): every field outside the grammar is named and replaced by the rules' own choice."""
    problems = []
    clean = {"palette": st["rule_palette"], "seg": [], "hook": {}, "drop": []}
    pal = plan.get("palette")
    if isinstance(pal, int) and not isinstance(pal, bool) and 0 <= pal < N_PAIRS:
        clean["palette"] = pal
    else:
        problems.append(f"palette {pal!r}")
    segs = plan.get("seg") or []
    if len(segs) != len(st["segs"]):
        problems.append(f"{len(segs)} segments, expected {len(st['segs'])}")
    for i, sg in enumerate(st["segs"]):
        q = segs[i] if i < len(segs) and isinstance(segs[i], dict) else {}
        anim = q.get("anim")
        if anim not in LOOKS:
            if i < len(segs):
                problems.append(f"seg {i} look {anim!r}")
            anim = sg["rule_anim"] if sg["rule_anim"] in LOOKS else "wave"
        speed = q.get("speed", 0)
        if speed not in (-1, 0, 1):
            problems.append(f"seg {i} speed {speed!r}")
            speed = 0
        lead = q.get("lead", 0)
        if lead not in (0, 1):
            problems.append(f"seg {i} lead {lead!r}")
            lead = 0
        clean["seg"].append({"anim": anim, "speed": speed, "lead": lead})
    known = {str(h) for sg in st["segs"] for h in [sg["hook"]] if h >= 0} | {str(i) for i, _ in st.get("hooks", [])}
    for h, anim in (plan.get("hook") or {}).items():
        if str(h) in known and anim in LOOKS:
            clean["hook"][str(h)] = anim
        else:
            problems.append(f"hook {h!r}: {anim!r}")
    drops = plan.get("drop") or []
    if len(drops) != len(st["drops"]):
        problems.append(f"{len(drops)} drops, expected {len(st['drops'])}")
    used_all, prev = False, None
    for i, d in enumerate(st["drops"]):
        q = drops[i] if i < len(drops) and isinstance(drops[i], dict) else {}
        typ, beats, rate = q.get("type"), q.get("beats", 4), q.get("rate", 4)
        if typ not in TYPES:
            problems.append(f"drop {i} type {typ!r}")
            typ = d["rule_type"]
        if typ == "all" and used_all:
            problems.append(f"drop {i}: second 'all'")
            typ = "blinder" if prev != "blinder" else "pairs"
        if typ == prev:
            problems.append(f"drop {i}: same type as the drop before")
            typ = next(t for t in ("spin", "pairs", "blinder") if t != prev)
        if beats not in BEATS:
            problems.append(f"drop {i} beats {beats!r}")
            beats = 4
        if rate not in RATES:
            problems.append(f"drop {i} rate {rate!r}")
            rate = 4
        used_all = used_all or typ == "all"
        prev = typ
        clean["drop"].append({"type": typ, "beats": beats, "rate": rate})
    return clean, problems


# ---- control -------------------------------------------------------------------------------------


def gen_random(st):
    rng = random.Random("random:" + st["id"])
    plan = {"palette": rng.randrange(N_PAIRS), "hook": {},
            "seg": [{"anim": rng.choice(LOOKS), "speed": rng.choice((-1, 0, 1)), "lead": rng.choice((0, 1))} for _ in st["segs"]], "drop": []}
    prev, used_all = None, False
    for _ in st["drops"]:
        typ = rng.choice([t for t in TYPES if t != prev and not (t == "all" and used_all)])
        used_all = used_all or typ == "all"
        prev = typ
        plan["drop"].append({"type": typ, "beats": rng.choice(BEATS), "rate": rng.choice(RATES)})
    return plan


# ---- beam search over a hand-written score ---------------------------------------------------------

# What makes songs differ: looks and colours per style family (my choices, not measured; the point of the
# experiment is whether any such layer shows, and the A/B decides). Palette indices as in N_PAIRS.
FAMILIES = {
    "hard": {"match": ("hardstyle", "hardcore", "hard techno", "gabber", "hands up", "donk", "makina", "happy", "bassline"), "looks": ("cross", "fan"), "palettes": (0, 2, 1)},
    "techno": {"match": ("techno", "minimal", "tech house", "acid"), "looks": ("sweep", "cross"), "palettes": (3, 4, 0)},
    "trance": {"match": ("trance", "goa", "psy"), "looks": ("wave", "circle"), "palettes": (4, 3, 2)},
    "bass": {"match": ("dubstep", "drum n bass", "trap", "halftime", "garage", "grime", "breakbeat"), "looks": ("cross", "circle"), "palettes": (5, 2, 0)},
    "house": {"match": ("house", "disco", "funk", "pop", "synthwave", "italodance", "k-pop", "dance"), "looks": ("wave", "fan"), "palettes": (1, 2, 5)},
}
INTENSITY = {"sweep": 0.2, "fan": 0.4, "wave": 0.55, "circle": 0.7, "cross": 0.9}


def family(st):
    style = (st.get("preset") or {}).get("style", "").lower()
    for name, f in FAMILIES.items():
        if any(m in style for m in f["match"]):
            return name, f
    return "house", FAMILIES["house"]


def melody_look(sg):  # the rules' own reading of the melody, as a preference here
    if sg["notes_per_bar"] * max(sg["bars"], 0.25) < 4:
        return "sweep"
    if sg["pitch_range"] > 12:
        return "circle"
    if sg["stepwise"] > 0.65:
        return "wave"
    return "fan" if sg["pitch_range"] <= 4 else "cross"


def seg_score(sg, look, speed, fam, st):
    s = 0.8 if look in fam["looks"] else 0.0
    s += 1.0 if look == melody_look(sg) else 0.0
    intensity = 0.5 * sg["drums"] + 0.5 * sg["bass"]
    s -= 1.5 * abs(INTENSITY[look] - intensity)
    want = 1 if sg["hats_per_bar"] >= 14 or sg["hits_per_bar"] >= 7 else -1 if sg["hits_per_bar"] < 2.5 else 0
    s -= 0.7 * abs(speed - want)
    return s


def search_segments(st, fam, width=128):
    """Beam: a state remembers the look of each structure letter and hook, so music that comes back after something else can
    look the same, while every music change still shows a new look (club.js's own principle)."""
    # score, choices, last visible look, last visible letter, look by letter, look by hook, looks used
    beam = [(0.0, [], None, None, {}, {}, frozenset())]
    for sg in st["segs"]:
        visible = sg["energetic"] > 0.3
        options = []
        for score, picks, last, last_part, by_part, by_hook, used in beam:
            looks = LOOKS if visible else ("wave", "circle")
            if sg["hook"] >= 0 and sg["hook"] in by_hook:
                looks = (by_hook[sg["hook"]],)  # the same hook always gets the same look
            for look in looks:
                for speed in (-1, 0, 1):
                    s = seg_score(sg, look, speed, fam, st) if visible else -0.2 * abs(speed)
                    if visible:
                        if sg["part"] in by_part and sg["part"] != last_part:
                            s += 2.0 if by_part[sg["part"]] == look else -1.0  # the music came back
                        if look == last:
                            s -= 2.5  # a music change should show
                        if look not in used and len(used) < 4:
                            s += 0.8  # a song should use more than its family's two looks
                    bp, bh = by_part, by_hook
                    if visible and sg["part"] not in by_part:
                        bp = {**by_part, sg["part"]: look}
                    if sg["hook"] >= 0 and sg["hook"] not in by_hook:
                        bh = {**by_hook, sg["hook"]: look}
                    options.append((score + s, picks + [(look, speed)], look if visible else last, sg["part"] if visible else last_part, bp, bh, used | {look} if visible else used))
        options.sort(key=lambda o: -o[0])
        beam = options[:width]
    best = beam[0]
    return best[1], best[5]


def drop_strengths(st):
    """How big each drop is within the song, 0..1 by rank of a plain sum of what the analyzers measured."""
    ds = st["drops"]
    if not ds:
        return []
    raw = [0.5 * min(d["breakdown_bars"], 32) / 16 + 0.15 * max(-6, min(d["low_jump_db"], 24)) / 6 + 0.5 * d["audio_blackout"] + 0.3 * (d["build_source"] != "sections") + 0.1 * min(d["snare_rate"], 16) / 4 for d in ds]
    order = sorted(range(len(ds)), key=lambda i: raw[i])
    rank = [0.0] * len(ds)
    for r, i in enumerate(order):
        rank[i] = r / max(1, len(ds) - 1)
    return rank


SIZE = {"spin": 0.15, "pairs": 0.45, "blinder": 0.75, "all": 1.0}


def search_drops(st, fam_name, width=256):
    strengths = drop_strengths(st)
    beam = [(0.0, [], None, False)]
    bpm = st["bpm"] or 128
    for d, stg in zip(st["drops"], strengths):
        options = []
        for score, picks, prev, used_all in beam:
            for typ in TYPES:
                if typ == prev or (typ == "all" and (used_all or d["breakdown_bars"] < 8)):
                    continue
                for beats in BEATS:
                    for rate in RATES:
                        s = -2.0 * abs(SIZE[typ] - stg)
                        s -= 1.0 * abs({1: 0.0, 2: 0.4, 4: 1.0}[beats] - (0.3 + 0.7 * stg))  # small drops get a short burst
                        hard = fam_name == "hard" or bpm >= 145
                        s -= 0.4 if (rate == 4) == hard else 0.0  # sixteenths get frantic above about 145 BPM (my taste call)
                        if picks and picks[-1]["rate"] == rate and picks[-1]["beats"] == beats:
                            s -= 0.3  # let drops differ in more than the fixture
                        options.append((score + s, picks + [{"type": typ, "beats": beats, "rate": rate}], typ, used_all or typ == "all"))
        options.sort(key=lambda o: -o[0])
        beam = options[:width]
    return beam[0][1]


def gen_beam(st):
    name, fam = family(st)
    picks, by_hook = search_segments(st, fam)
    # the song's pair: the family's three, chosen by the key's old index so related keys stay related
    pal = fam["palettes"][st["rule_palette"] % 3]
    return {"palette": pal, "seg": [{"anim": a, "speed": sp, "lead": 0} for a, sp in picks],
            "hook": {str(k): v for k, v in by_hook.items()}, "drop": search_drops(st, name)}


GENERATORS = {"random": gen_random, "beam": gen_beam}


def main():
    for variant in sys.argv[1:]:
        out = PLANS / variant
        out.mkdir(parents=True, exist_ok=True)
        problems = 0
        for p in sorted(STRUCT.glob("*.json")):
            st = json.loads(p.read_text())
            clean, bad = validate(GENERATORS[variant](st), st)
            problems += len(bad)
            (out / p.name).write_text(json.dumps(clean, separators=(",", ":")))
        print(variant, "plans written,", problems, "validator problems")


if __name__ == "__main__":
    main()
