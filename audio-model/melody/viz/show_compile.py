"""The show compiler: every offline analysis of a song merged into one cue list for the lights.

Input: the song's analysis as serve.py builds it (aligned beat grid, sections and drops, melody
line, and the analyzer features from audio-model/melody/analyze). Output `doc["cues"]`, all
times in seconds of the audio, snapped to the beat grid where that is what the music does. Every
analyzer is optional: a song without features compiles to the section rule's cues.

  hits       per drum piece (kick, snare, hh, crash): [time, strength], from DrumSep, keeping a kick,
             snare or hat only if ADTOF's transcription has it too, or it is strong and on a 16th.
             On the electronic songs DrumSep's unconfirmed kicks are 18% of its kicks and land on a
             16th only 63% of the time (about 50% by chance; confirmed ones 97%), while ADTOF alone
             covers just as many of the drop's beats. The club pulses on kicks, accents snares,
             sparkles on hats, and hits on crashes;
  builds     per drop: where its build really starts (the earliest riser or snare roll running
             into it, else the section rule's 4 bars), the source, and its intensity curve
             [[time, 0..1]]: linear over the build, pushed up by the snare-hit rate per beat in
             the last bars (a roll going 1/4 -> 1/8 -> 1/16 climbs) and by a riser's progress.
             The rate counts all of DrumSep's snares: it hears twice ADTOF's before a drop, on the grid.
             Risers come from the DSP analyzer and the trained effect head (fxhead), which reaches
             17% of the held-out drops against 5%, at a 2.6% false-alarm rate against 1%;
  blackouts  per drop: the silence right before it if the audio has one (a gap ending within a
             quarter beat of the drop), else the last half beat;
  impacts    big broadband hits away from the drops (and strong crashes), snapped to 16ths;
  risers     noise sweeps, ends snapped to the next downbeat within a beat;
  events     sound events worth a cue: air horn, siren, whoosh, explosion / boom, crowd / cheer,
             white noise, gunshot, [start, end, kind, peak]. From PANNs and PretrainedSED: where
             both hear it SED gives the sharper start; SED alone must score 0.4, and its air horns
             must be short (a 12 s "horn" is a horn-like synth);
  stabs      chord stabs of the synths (3+ notes at once, Basic Pitch), snapped to 16ths;
  chords     chord segments [start, end, label] with changes snapped to beats, from the own tracker;
  chords2    BTC's large-vocabulary chords for the viewer, without its N and anything under a beat.
             Not used by the lights: 28 changes a minute against 15, 15% of them under a beat (the
             colours follow the chords), and it shares the own tracker's root only 54% of the time;
  keys       key segments;
  parts      structure segments [start, end, letter] (same letter = the same music came back),
             boundaries snapped to bar lines. The boundaries come from All-In-One where it ran, plus
             a cut at every drop, the letters from the own structure (whichever letter overlaps
             most): on the electronic songs 76% of All-In-One's boundaries fall on a bar line
             against 19% of the own ones, and its labels can't tell repeated sections apart;
  functions  All-In-One's labels [start, end, label] (intro, verse, chorus, inst, break ...) for the
             viewer. Not used by the lights: it was trained on pop and has no drop or build label,
             so drops come out as chorus, inst and sometimes verse or intro;
  lines      sung lyric lines [start, end, text] (the vocal phrases), and chops;
  pump       sidechain depth per beat;
  pan        the "other" stem's stereo position per bar (-1 .. 1);
  preset     the song's character from the style / mood tags: energy 0..1 (aggressive, party,
             danceable), calm 0..1 (relaxed, sad), its top style, the CLAP prompts;
  grid_check how the Rekordbox grid agrees with Beat This! (tempo, downbeat phase);
  summary    counts, for the HUD and the song info.

Standard library only, like the rest of the server.
"""

import base64
import bisect
import statistics


def _snap(t, grid, tol):
    i = bisect.bisect_left(grid, t)
    best = min((grid[j] for j in (i - 1, i) if 0 <= j < len(grid)), key=lambda g: abs(g - t), default=t)
    return best if abs(best - t) <= tol else t


def _sixteenths(beats):
    out = []
    for a, b in zip(beats, beats[1:]):
        out += [a + (b - a) * k / 4 for k in range(4)]
    return out + beats[-1:]


def _near(grid, t, tol):
    i = bisect.bisect_left(grid, t)
    return any(0 <= j < len(grid) and abs(grid[j] - t) <= tol for j in (i - 1, i))


def _track(b64):
    return base64.b64decode(b64) if b64 else b""


EVENT_KINDS = {"Air horn, truck horn": "airhorn", "Siren": "siren", "Civil defense siren": "siren",
               "Whoosh, swoosh, swish": "whoosh", "Explosion": "boom", "Boom": "boom", "Cheering": "crowd",
               "Applause": "crowd", "Crowd": "crowd", "White noise": "noise", "Pink noise": "noise",
               "White noise, pink noise": "noise", "Gunshot, gunfire": "gunshot"}


def compile_show(doc: dict) -> dict:
    grid = doc.get("beats") or {}
    beats = grid.get("beats") or []
    empty = {"hits": {}, "builds": [], "blackouts": [], "impacts": [], "risers": [], "events": [], "stabs": [], "chords": [], "chords2": [],
             "keys": [], "parts": [], "functions": [], "lines": [], "chops": [], "pump": [], "pan": [], "preset": {}, "grid_check": {}, "summary": {}}
    if len(beats) < 8:
        return empty
    inbar = grid.get("beat_in_bar") or []
    downs = [t for t, k in zip(beats, inbar) if k == 1]
    six = _sixteenths(beats)
    beat_len = (beats[-1] - beats[0]) / (len(beats) - 1)
    bar_len = 4 * beat_len
    F = doc.get("features") or {}
    drops = (doc.get("show") or {}).get("drops") or []
    near_drop = lambda t, w: any(abs(t - d["time"]) < w for d in drops)

    # drum pieces
    pieces = (F.get("drumparts") or {}).get("pieces") or {}
    hits = {k: [[h[0], h[1]] for h in pieces.get(k, {}).get("hits", [])] for k in ("kick", "snare", "hh", "crash")}
    snare_t = [h[0] for h in hits["snare"] if h[1] >= 0.25]  # before the check below: the builds want every snare
    adtof = (F.get("adtof") or {}).get("hits") or {}
    unconfirmed = 0
    for k in ("kick", "snare", "hh"):
        other = [h[0] for h in adtof.get(k, [])]  # sorted; a song ADTOF barely hears gets no second opinion
        if len(other) >= 20:
            kept = [h for h in hits[k] if _near(other, h[0], 0.05) or (h[1] >= 0.75 and _near(six, h[0], 0.03))]
            unconfirmed += len(hits[k]) - len(kept)
            hits[k] = kept

    # risers / rolls
    ris = F.get("risers") or {}
    risers = []
    for a, b, strength, _src in ris.get("risers", []):
        i = bisect.bisect_left(downs, b)
        end = downs[i] if i < len(downs) and downs[i] - b <= beat_len else b
        risers.append([round(a, 3), round(end, 3), strength])
    rolls = [[a, b, rate] for a, b, rate in ris.get("rolls", [])]
    # The trained effect head only counts when it runs into a drop: elsewhere it fires at 2.6% of bar lines.
    head_risers = [[e[0], e[1], e[3]] for e in (F.get("fxhead") or {}).get("events", []) if e[2] == "riser"]

    def snare_rate(x):  # snare hits per beat over the beat before x
        return bisect.bisect_left(snare_t, x) - bisect.bisect_left(snare_t, x - beat_len)

    builds, blackouts = [], []
    for d in drops:
        t = d["time"]
        rule_start = d.get("build_start", t - 4 * bar_len)
        feeding = [("riser", a, b, s) for a, b, s in risers if t - bar_len <= b <= t + 0.25 * beat_len and a >= t - 32 * bar_len]
        feeding += [("roll", a, b, r) for a, b, r in rolls if t - bar_len <= b <= t + 0.25 * beat_len and a >= t - 32 * bar_len and r >= 10]
        feeding += [("fxhead", a, b, pk) for a, b, pk in head_risers if t - bar_len <= b <= t + beat_len and a >= t - 32 * bar_len]
        # a snare roll from the separated snare: the hit rate per beat climbing to 4+ over the last 8 bars
        if snare_t and snare_rate(t - 0.05) >= 4:
            k = 1
            while k < 32 and snare_rate(t - k * beat_len) >= 2:
                k += 1
            if k >= 4:
                feeding.append(("snare", t - k * beat_len, t, snare_rate(t - 0.05)))
        if feeding:
            src, start = min(((kind, a) for kind, a, _, _ in feeding), key=lambda x: x[1])
            start = min(start, rule_start)
            start = downs[max(0, bisect.bisect_right(downs, start + 1e-3) - 1)] if downs else start
        else:
            src, start = "sections", rule_start
        n = 32
        curve = []
        for k in range(n + 1):
            x = start + (t - start) * k / n
            v = (x - start) / max(1e-3, t - start)
            if snare_t:
                v = max(v, min(1.0, snare_rate(x) / 4) * (0.5 + 0.5 * v))
            for kind, a, b, s in feeding:
                if kind in ("riser", "fxhead") and a <= x <= b:
                    v = max(v, 0.3 + 0.7 * (x - a) / max(1e-3, b - a))
            curve.append([round(x, 3), round(min(1.0, v), 3)])
        builds.append({"drop": t, "start": round(start, 3), "source": src, "curve": curve})
        gap = next((g for g in ris.get("gaps", []) if abs(g[1] - t) <= 0.25 * beat_len and g[1] - g[0] >= 0.05), None)
        blackouts.append({"drop": t, "start": round(gap[0] if gap else t - 0.5 * beat_len, 3), "end": round(t, 3),
                          "source": "audio" if gap else "grid"})

    impacts = [[round(_snap(t, six, 0.06), 3), s, tail] for t, s, tail in ris.get("impacts", []) if not near_drop(t, 0.5 * beat_len)]
    impacts += [[round(_snap(t, six, 0.06), 3), round(25 * s, 1), 1.0] for t, s in hits["crash"] if s >= 0.75 and not near_drop(t, 0.5 * beat_len)]
    impacts.sort()

    ev = F.get("events") or {}
    heard = []
    for a, b, name, peak in ev.get("events", []):
        kind = EVENT_KINDS.get(name)
        if kind and peak >= (0.3 if kind in ("crowd", "noise") else 0.2):
            heard.append([a, b, kind, peak])
    for a, b, name, peak in (F.get("sed") or {}).get("events", []):
        kind = EVENT_KINDS.get(name)
        both = next((e for e in heard if e[2] == kind and a < e[1] and e[0] < b), None)
        if both:
            if abs(a - both[0]) < 0.5:
                both[0] = a
            both[3] = max(both[3], peak)
        elif kind and peak >= 0.4 and (kind != "airhorn" or b - a <= 4):
            heard.append([a, b, kind, peak])
    events = []
    for a, b, kind, peak in sorted(heard):
        same = next((e for e in reversed(events) if e[2] == kind), None)
        if same and a - same[1] < 0.5:
            same[1], same[3] = max(same[1], b), max(same[3], peak)
        else:
            events.append([a, b, kind, peak])

    tr = F.get("transcribe") or {}
    stabs = [[round(_snap(t, six, 0.05), 3), n, amp] for t, n, amp in tr.get("stabs", [])]

    ch = F.get("chords") or {}
    chords = [[round(_snap(a, beats, 0.12), 3), round(_snap(b, beats, 0.12), 3), lab] for a, b, lab in ch.get("chords", [])]
    keys = ch.get("keys", [])
    chords2 = []
    for a, b, lab in (F.get("btc") or {}).get("chords", []):
        if lab in ("N", "X") or b - a < beat_len:
            continue
        if chords2 and chords2[-1][2] == lab:
            chords2[-1][1] = round(b, 3)
        else:
            chords2.append([round(a, 3), round(b, 3), lab])

    own = (F.get("structure") or {}).get("segments", [])
    aio = [s for s in (F.get("allinone") or {}).get("segments", []) if s[2] not in ("start", "end")]
    segments = own
    if own and aio:
        drop_t = [d["time"] for d in drops]
        cuts = sorted({s[0] for s in aio if not near_drop(s[0], 2 * beat_len)} | {t for t in drop_t if aio[0][0] < t < aio[-1][1]})
        segments = []
        for a, b in zip(cuts, cuts[1:] + [aio[-1][1]]):
            overlap = {}
            for x, y, lab in own:
                overlap[lab] = overlap.get(lab, 0) + max(0, min(b, y) - max(a, x))
            lab = max(overlap, key=overlap.get)
            if segments and segments[-1][2] == lab and a not in drop_t:
                segments[-1][1] = b
            else:
                segments.append([a, b, lab])
    on_bars = lambda x: round(_snap(x, downs, 0.6 * bar_len), 3)
    parts = [[on_bars(a), on_bars(b), lab] for a, b, lab in segments]
    functions = []
    for a, b, lab in aio:
        if functions and functions[-1][2] == lab:
            functions[-1][1] = on_bars(b)
        else:
            functions.append([on_bars(a), on_bars(b), lab])

    ly = F.get("lyrics") or {}
    lines, chops = ly.get("lines", []), ly.get("chops", [])

    pump = []
    kd = (F.get("timbre") or {}).get("pump") or []
    kt = [k[0] for k in kd]
    for b in beats:
        i = bisect.bisect_right(kt, b + 0.05) - 1
        if i >= 0 and b - kt[i] < 0.6 * beat_len:
            pump.append([round(b, 3), kd[i][1]])

    pan = []
    se = F.get("stereo") or {}
    if se.get("pan", {}).get("other"):
        tr_ = _track(se["pan"]["other"])
        fps = se.get("fps", 10)
        for a, b in zip(downs, downs[1:]):
            vals = [tr_[i] / 255 * 2 - 1 for i in range(int(a * fps), min(len(tr_), int(b * fps)))]
            if vals:
                pan.append([round(a, 3), round(statistics.fmean(vals), 3)])

    tags = F.get("tags") or {}
    moods = tags.get("moods") or {}
    emb = F.get("embed") or {}
    preset = {}
    if moods:
        preset = {"energy": round(min(1.0, 0.45 * moods.get("aggressive", 0) + 0.35 * moods.get("party", 0) + 0.2 * moods.get("danceable", 0) * 1.0), 3),
                  "calm": round(min(1.0, 0.6 * moods.get("relaxed", 0) + 0.4 * moods.get("sad", 0)), 3),
                  "style": (tags.get("styles") or [["", 0]])[0][0].split("---")[-1], "styles": tags.get("styles", [])[:5], "moods": moods}
    # MuQ-MuLan's top prompts, CLAP's only as a fallback: with style-name prompts MuLan agrees with
    # Essentia 37% of the time against CLAP's 5%. "Sounds like" stays on CLAP (serve.py), because
    # MuLan's nearest neighbours mostly follow the tempo. Essentia stays the main style signal.
    text = (F.get("mulan") or {}).get("song") or emb.get("song")
    if text:
        preset["prompts"] = [n for n, _ in text[:3]]

    gc = {}
    bt = F.get("beats") or {}
    if bt.get("downbeats") and downs:
        offs = []  # which beat of the Rekordbox bar each Beat This! downbeat lands on (0 = they agree)
        for t in bt["downbeats"]:
            i = bisect.bisect_left(beats, t)
            j = min((j for j in (i - 1, i) if 0 <= j < len(beats)), key=lambda j: abs(beats[j] - t))
            if abs(beats[j] - t) < 0.1 * beat_len and j < len(inbar):
                offs.append((inbar[j] - 1) % 4)
        if offs:
            phase = statistics.mode(offs)
            gc = {"beat_this_bpm": bt.get("bpm"), "rekordbox_bpm": grid.get("bpm"), "downbeat_agreement": round(offs.count(0) / len(offs), 3),
                  "downbeat_shift_beats": phase if offs.count(phase) / len(offs) > 0.6 else 0}

    return {"hits": hits, "builds": builds, "blackouts": blackouts, "impacts": impacts, "risers": risers, "events": events,
            "stabs": stabs, "chords": chords, "chords2": chords2, "keys": keys, "parts": parts, "functions": functions,
            "lines": lines, "chops": chops, "pump": pump, "pan": pan, "preset": preset, "grid_check": gc,
            "summary": {"builds_from_audio": sum(b["source"] != "sections" for b in builds), "builds": len(builds),
                        "blackouts_from_audio": sum(b["source"] == "audio" for b in blackouts), "impacts": len(impacts),
                        "risers": len(risers), "events": len(events), "stabs": len(stabs), "chords": len(chords),
                        "parts": len(parts), "lines": len(lines), "kicks": len(hits["kick"]), "unconfirmed_hits": unconfirmed}}
