"""An LLM as show designer: it reads a song's cue table and writes the plan; plan.validate() has the last word.

    python3 audio-model/melody/decision/plan_llm.py --ids id1 id2 ... [--model opus] [--jobs 3]

Uses the `claude` CLI in print mode with no tools, no settings and no MCP servers, so all it sees is the
prompt below. Writes plans/llm/<id>.json (validated) and plans/llm_raw/<id>.json (the answer, its problems
and the rationale, for reading). It never sees the audio, only what the analyzers measured.
"""

import argparse
import concurrent.futures as cf
import json
import re
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.dont_write_bytecode = True
import plan as PL  # noqa: E402

SYSTEM = """You are the lighting designer of a small dark techno club. You write the light plan of one song, offline, from a table of what audio analyzers measured. You can't hear the song and must not assume instruments or events the table doesn't show.

The room: a ceiling grid of 32 downlights, five LED ropes in a spider web, uplights, coves, and four white VP1 panels. The renderer already keeps everything on the beat grid, pulses on the real drum hits, and plays the melody on the ceiling. You only choose among these:

palette: one colour pair for the whole song, plus white:
  0 red + orange, 1 pink + purple, 2 magenta + cyan, 3 blue + violet, 4 cyan + blue, 5 green + teal.
seg: one entry per row of the segment table, in order:
  anim: the ceiling and rope pattern, seen only in rows with energetic > 0.3 (other rows show a dim wave or circle):
    sweep = one row stepping along the room; fan = rows opening out from the middle; wave = a diagonal running through;
    circle = a ring opening from the centre; cross = a sparse checker that jumps every step (the busiest).
  speed: -1 half as fast, 0 as the melody asks, +1 twice as fast.
  lead: 0 or 1, swaps which colour of the pair leads in this segment.
hook: optional, {hook id: anim}: this repeated melody block always gets this look, whatever its segment says.
drop: one entry per row of the drop table, in order. The strobe burst that starts when the bass hits on the drop:
  type: spin = one 30% rope segment at a time going round the hub (smallest); pairs = two 30% segments on two ropes; blinder = the four VP1 panels (big); all = VP1s and rope pairs together (biggest, at most once in a song).
  beats: 1, 2 or 4: how long the burst lasts, at most one bar. Rate: flashes per beat, 2 (eighths, heavier) or 4 (sixteenths, more frantic).

Rules you can't change, enforced after you: strobes only start on the drop when the bass hits; two consecutive drops must have different types; one colour pair per song; few lights at once.

What the owner cares about: snappy and aligned to the music, and shows that differ between songs and between the drops of a song, for a reason the music gives. When the same music comes back (same structure letter or hook), decide on purpose whether it should look the same or grow.

Answer with JSON only, no prose around it:
{"palette": 0, "why": "one sentence", "seg": [{"anim": "wave", "speed": 0, "lead": 0}], "hook": {"0": "wave"}, "drop": [{"type": "blinder", "beats": 4, "rate": 4, "why": "short reason"}]}"""


def table(st):
    head = f"Song: {st['name']}\nTempo: {st['bpm']:.1f} BPM. Key: {st['key']}. Length {st['duration']:.0f} s."
    pre = st.get("preset") or {}
    head += f"\nStyle tags: {[s[0].split('---')[-1] for s in (st.get('tags') or {}).get('styles') or []]}. Moods: {pre.get('moods')}."
    rows = ["\nSegments (bars; kind; structure letter: same letter = the music came back; hook id or -; share energetic; loudness of drums, bass, other, vocals 0..1; drum hits, hi-hats and chord stabs per bar; melody notes per bar, pitch range in semitones, share stepwise):",
            "i start bars kind letter hook energetic drums bass other vocals hits hats stabs notes range stepwise"]
    for s in st["segs"]:
        letter = chr(65 + s["part"]) if 0 <= s["part"] < 26 else "-"
        if s["start"] < 0:  # the renderer's empty segment before the first music change
            rows.append(f"{s['i']} (empty, before the song starts: give it anything)")
            continue
        rows.append(f"{s['i']} {s['start']:.1f} {s['bars']:.1f} {s['kind']} {letter} {s['hook'] if s['hook'] >= 0 else '-'} {s['energetic']:.2f} {s['drums']:.2f} {s['bass']:.2f} {s['other']:.2f} {s['vocals']:.2f} "
                    f"{s['hits_per_bar']:.1f} {s['hats_per_bar']:.1f} {s['stabs_per_bar']:.1f} {s['notes_per_bar']:.1f} {s['pitch_range']} {s['stepwise']:.2f}")
    rows.append("\nDrops (time s; breakdown bars before it; build bars and what the build was found from; whether the audio has its own silence before the drop; low-end jump in dB; structure letter; snare hits in the last bar):")
    rows.append("i time breakdown build source silence jump letter snares")
    for d in st["drops"]:
        letter = chr(65 + d["part"]) if 0 <= d["part"] < 26 else "-"
        rows.append(f"{d['i']} {d['time']:.1f} {d['breakdown_bars']} {d['build_bars']:.1f} {d['build_source']} {'yes' if d['audio_blackout'] else 'no'} {d['low_jump_db']:.1f} {letter} {d['snare_rate']}")
    return head + "\n" + "\n".join(rows) + f"\n\nWrite the plan: exactly {len(st['segs'])} seg entries and {len(st['drops'])} drop entries."


def extract(text):
    m = re.search(r"\{.*\}", text, re.S)
    if not m:
        raise ValueError("no JSON in the answer")
    return json.loads(m.group(0))


def one(sid, model, variant="llm"):
    st = PL.load_structure(sid)
    prompt = table(st)
    t0 = time.time()
    r = subprocess.run(["claude", "-p", "--model", model, "--tools", "", "--system-prompt", SYSTEM, "--output-format", "json", "--no-session-persistence",
                        "--setting-sources", "", "--strict-mcp-config", "--disable-slash-commands"],
                       input=prompt, capture_output=True, text=True, timeout=600, cwd=str(HERE))
    if r.returncode != 0:
        return sid, {"error": r.stderr[-400:] or r.stdout[-400:]}
    envelope = json.loads(r.stdout)
    answer = envelope.get("result", "")
    try:
        raw = extract(answer)
    except (ValueError, json.JSONDecodeError) as e:
        return sid, {"error": f"bad answer: {e}", "answer": answer[:400]}
    clean, problems = PL.validate(raw, st)
    (PL.PLANS / variant).mkdir(parents=True, exist_ok=True)
    (PL.PLANS / f"{variant}_raw").mkdir(parents=True, exist_ok=True)
    (PL.PLANS / variant / f"{sid}.json").write_text(json.dumps(clean, separators=(",", ":")))
    info = {"id": sid, "name": st["name"], "model": model, "seconds": round(time.time() - t0, 1), "problems": problems, "raw": raw, "cost_usd": envelope.get("total_cost_usd"), "prompt_chars": len(prompt)}
    (PL.PLANS / f"{variant}_raw" / f"{sid}.json").write_text(json.dumps(info, indent=1))
    return sid, {"problems": problems, "seconds": info["seconds"]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ids", nargs="+", required=True)
    ap.add_argument("--model", default="opus")
    ap.add_argument("--jobs", type=int, default=3)
    ap.add_argument("--variant", default="llm", help="output folder under plans/, for repeat runs")
    args = ap.parse_args()
    with cf.ThreadPoolExecutor(args.jobs) as ex:
        for sid, res in ex.map(lambda s: one(s, args.model, args.variant), args.ids):
            print(sid, json.dumps(res)[:300], flush=True)


if __name__ == "__main__":
    main()
