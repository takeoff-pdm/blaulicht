"""An LLM's second look at a compiled song: it reads the numbers and flags what looks wrong. It changes nothing.

Uses the `claude` CLI in print mode with no tools, no settings and no MCP servers, so all it sees is the table below.
It never hears the song. Writes data/melody/reviews/<id>.json; the next compile shows its flags next to checks.py's, and
a person decides with the labelling mode or `song.py fix`. Use it through `song.py review`.
"""

import json
import re
import subprocess
import time

import library as lib

SYSTEM = """You check the automatic analysis of one dance music track before a light show runs on it. You get a table of what the analyzers measured. You can't hear the song.

Point out what looks wrong, and only that: a section or drop in an implausible place, sources that disagree, a tempo or bar length that doesn't add up, a drop the section rule probably missed (usually a long outro, or a place where the low end level jumps and no drop is listed). The light show strobes on drops, so a wrong or missing drop matters most. A flag that isn't backed by a number in the table is worse than no flag. Most songs are fine: an empty list is a good answer.

Answer with JSON only, no prose around it:
{"flags": [{"at": 123.4, "what": "drop|section|grid|bars|other", "problem": "one sentence with the numbers that show it", "sure": "low|medium|high"}]}"""


def table(doc: dict) -> str:
    b, show, c, F = doc.get("beats") or {}, doc["show"], doc.get("check") or {}, doc.get("features") or {}
    lines = [f"Song: {doc['name']}", f"Length {doc['duration']:.0f} s. Tempo {b.get('bpm')} BPM. Key {doc.get('key')}. Rekordbox phrase mood {doc.get('mood')}.",
             f"Beat trackers: Beat This! {(F.get('beats') or {}).get('bpm')} BPM, All-In-One {(F.get('allinone') or {}).get('bpm')} BPM. Share of their beats on the grid {c.get('trackers_on_grid')}, kicks on the beat {c.get('kicks_on_beat')}. Grid source: {c.get('grid')}.",
             "", "Sections from the rule (start s, end s, kind):"]
    lines += [f"{s['start']:.1f} {s['end']:.1f} {s['kind']}" for s in show["sections"]]
    lines += ["", "Drops (time s, bars of quiet before):"] + [f"{d['time']:.1f} {d['breakdown_bars']}" for d in show["drops"]]
    lines += ["", "Rekordbox's phrases (start s, label):"] + [f"{p['start']:.1f} {p['label']}" for p in doc.get("phrases") or []]
    lines += ["", "All-In-One's segments (start s, label):"] + [f"{s[0]:.1f} {s[2]}" for s in (F.get("allinone") or {}).get("segments", []) if s[2] not in ("start", "end")]
    downs = [t for t, k in zip(b.get("beats") or [], b.get("beat_in_bar") or []) if k == 1]
    if downs and show.get("bars"):
        lines += ["", "Low end level per 4 bars (bar start s, drums + bass, 1.0 = the song's 90th percentile):"]
        lines += [f"{downs[i]:.1f} {sum(show['bars'][i:i + 4]) / len(show['bars'][i:i + 4]):.2f}" for i in range(0, min(len(downs), len(show["bars"])), 4)]
    lines += ["", "What the automatic checks already flagged (no need to repeat these):"] + [f"{f.get('at', '-')} {f['text']}" for f in c.get("flags", []) if f["kind"] != "review"]
    return "\n".join(lines)


def review(sid: str, model: str = "opus") -> dict:
    doc = json.loads((lib.COMPILED / f"{sid}.json").read_text())
    t0 = time.time()
    r = subprocess.run(["claude", "-p", "--model", model, "--tools", "", "--system-prompt", SYSTEM, "--output-format", "json", "--no-session-persistence",
                        "--setting-sources", "", "--strict-mcp-config", "--disable-slash-commands"],
                       input=table(doc), capture_output=True, text=True, timeout=600, cwd=str(lib.HERE))
    if r.returncode != 0:
        raise RuntimeError(r.stderr[-400:] or r.stdout[-400:])
    envelope = json.loads(r.stdout)
    answer = envelope.get("result", "")
    m = re.search(r"\{.*\}", answer, re.S)
    if not m:
        raise RuntimeError(f"no JSON in the answer: {answer[:300]}")
    flags = json.loads(m.group(0)).get("flags") or []
    # a flag has to point at a place in the song and say something
    flags = [f for f in flags if isinstance(f, dict) and f.get("problem") and (f.get("at") is None or 0 <= f["at"] <= doc["duration"])]
    out = {"id": sid, "model": model, "seconds": round(time.time() - t0, 1), "cost_usd": envelope.get("total_cost_usd"), "flags": flags}
    lib.write_atomic(lib.REVIEWS / f"{sid}.json", json.dumps(out, ensure_ascii=False, indent=1) + "\n")
    return out
