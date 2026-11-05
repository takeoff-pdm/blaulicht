"""Download the matched Beat Saber maps' lighting and align it to our audio.

For each match (bs/matches.json): fetch the zip, read Info.dat and the difficulty with the most lighting
events, convert beats to seconds through the map's BPM changes, then find the time offset between the map's
audio and our mp3 by cross-correlating onset envelopes. Keeps events in seconds of OUR audio plus the alignment
score in bs/maps/<song id>_<map id>.json; the map's audio is not kept. Needs numpy and ffmpeg.

    python audio-model/melody/decision/bs_fetch.py [--jobs 4] [--max-per-song 3]
"""

import argparse
import concurrent.futures as cf
import io
import json
import os
import subprocess
import tempfile
import time
import urllib.request
import zipfile
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
OUT = HERE / "bs" / "maps"
UA = {"User-Agent": "blaulicht-research/0.1 (decision experiment)"}
SR, FPS = 11025, 100
LIGHT_TYPES = {0: "back", 1: "ring", 2: "left", 3: "right", 4: "center", 6: "extra1", 7: "extra2", 10: "extra3", 11: "extra4"}  # 8 ring spin, 9 zoom, 12-13 laser speed, 5 boost


def decode(path_or_bytes):
    """Mono float PCM through ffmpeg, from a file path or bytes."""
    args = ["ffmpeg", "-v", "error", "-nostdin", "-i", path_or_bytes if isinstance(path_or_bytes, str) else "pipe:0", "-ac", "1", "-ar", str(SR), "-f", "f32le", "pipe:1"]
    r = subprocess.run(args, input=None if isinstance(path_or_bytes, str) else path_or_bytes, capture_output=True, timeout=300)
    if r.returncode != 0:
        raise RuntimeError("ffmpeg: " + r.stderr[-200:].decode(errors="replace"))
    return np.frombuffer(r.stdout, dtype=np.float32)


def onset_env(x):
    hop = SR // FPS
    n = len(x) // hop
    if n < 100:
        raise RuntimeError("audio too short")
    # a cheap log-energy flux per band: 3 bands from differences of a moving sum
    frames = x[: n * hop].reshape(n, hop)
    e = np.log1p(100 * (frames**2).sum(1))
    d = np.diff(e, prepend=e[0])
    d[d < 0] = 0
    d -= np.convolve(d, np.ones(101) / 101, mode="same")
    return d / (np.std(d) + 1e-9)


def align(a, b):
    """Offset s such that t in the map's audio (a) = t + s in ours (b), with the normalised correlation peak."""
    n = len(a) + len(b)
    nfft = 1 << int(np.ceil(np.log2(n)))
    c = np.fft.irfft(np.fft.rfft(b, nfft) * np.conj(np.fft.rfft(a, nfft)), nfft)
    c = np.concatenate([c[-len(a):], c[: len(b)]])
    i = int(np.argmax(c))
    lag = i - len(a)
    ov = max(1, min(len(a), len(b)))
    # sharpness: the peak against the correlation's own spread, and the overlap-normalised value
    return lag / FPS, float(c[i] / ov), float((c[i] - np.median(c)) / (np.std(c) + 1e-9))


def beats_to_seconds(bpm, changes):
    """A function from map beats to seconds of the map's audio, with BPM changes [(beat, bpm), ...]."""
    changes = sorted(changes)
    knots, t, cur_b, cur_bpm = [(0.0, 0.0, bpm)], 0.0, 0.0, bpm
    for b, new in changes:
        if b <= cur_b or new <= 0:
            if b == 0 and new > 0:
                cur_bpm = new
                knots = [(0.0, 0.0, new)]
            continue
        t += (b - cur_b) * 60 / cur_bpm
        cur_b, cur_bpm = b, new
        knots.append((b, t, new))

    def conv(beat):
        k = next((k for k in reversed(knots) if k[0] <= beat), knots[0])
        return k[1] + (beat - k[0]) * 60 / k[2]

    return conv


def parse_events(diff, ver_bpm):
    """Basic lighting events as (beat, group, value, boost?), from the v2 or v3 difficulty format; also the v3 group events count."""
    ev, changes = [], []
    if "_events" in diff or "_version" in diff:
        for e in diff.get("_events", []):
            ev.append((e.get("_time", 0.0), e.get("_type", -1), e.get("_value", 0)))
        for c in (diff.get("_customData") or {}).get("_BPMChanges", []) or (diff.get("_customData") or {}).get("_bpmChanges", []) or []:
            changes.append((c.get("_time", 0.0), c.get("_BPM", c.get("_bpm", 0))))
        for e in diff.get("_events", []):
            if e.get("_type") == 100 and e.get("_floatValue"):  # official BPM change event in v2.5
                changes.append((e["_time"], e["_floatValue"]))
    else:
        for e in diff.get("basicBeatmapEvents", []):
            ev.append((e.get("b", 0.0), e.get("et", -1), e.get("i", 0)))
        for c in diff.get("bpmEvents", []):
            changes.append((c.get("b", 0.0), c.get("m", 0)))
    groups = len(diff.get("lightColorEventBoxGroups", []) or []) + len(diff.get("lightRotationEventBoxGroups", []) or [])
    return ev, changes, groups


def read_zip(blob):
    z = zipfile.ZipFile(io.BytesIO(blob))
    names = {n.lower(): n for n in z.namelist()}
    info = json.loads(z.read(names["info.dat"]).decode("utf-8-sig"))
    bpm = info.get("_beatsPerMinute") or (info.get("audio") or {}).get("bpm")
    offset = info.get("_songTimeOffset", 0) or 0
    best = None
    sets = info.get("_difficultyBeatmapSets") or []
    fnames = [d["_beatmapFilename"] for s in sets for d in s.get("_difficultyBeatmaps", [])] or [d.get("beatmapDataFilename") for d in info.get("difficultyBeatmaps", [])]
    for fn in fnames:
        if not fn or fn.lower() not in names:
            continue
        try:
            diff = json.loads(z.read(names[fn.lower()]).decode("utf-8-sig"))
        except Exception:
            continue
        ev, changes, groups = parse_events(diff, bpm)
        if best is None or len(ev) > len(best[0]):
            best = (ev, changes, groups, fn)
    song = info.get("_songFilename") or (info.get("audio") or {}).get("songFilename")
    audio = z.read(names[song.lower()]) if song and song.lower() in names else None
    return bpm, offset, best, audio


def one(args):
    sid, song_path, m = args
    out = OUT / f"{sid}_{m['id']}.json"
    if out.exists():
        return sid, m["id"], "cached"
    try:
        req = urllib.request.Request(m["downloadURL"], headers=UA)
        with urllib.request.urlopen(req, timeout=60) as r:
            blob = r.read()
        time.sleep(0.5)
        bpm, offset, best, audio = read_zip(blob)
        if best is None or audio is None or not bpm:
            return sid, m["id"], "no events or audio"
        ev, changes, groups, fn = best
        with tempfile.NamedTemporaryFile(suffix=".egg", delete=True) as tf:
            tf.write(audio)
            tf.flush()
            a = onset_env(decode(tf.name))
        b = onset_env(decode(song_path))
        lag, peak, sharp = align(a, b)
        conv = beats_to_seconds(float(bpm), changes)
        events = [[round(conv(t) + lag - (offset or 0) / 1000 * 0, 3), ty, val] for t, ty, val in ev if ty not in (100,)]
        events.sort()
        doc = {"song": sid, "map": m["id"], "name": m["song"], "author": m["author"], "curated": m["curated"], "ranked": m["ranked"], "verified": m["verified"], "score": m["score"], "bpm": bpm, "bpm_changes": len(changes),
               "lag": lag, "peak": round(peak, 3), "sharp": round(sharp, 1), "map_audio_s": round(len(a) / FPS, 1), "our_audio_s": round(len(b) / FPS, 1), "group_events": groups, "difficulty": fn, "events": events}
        tmp = out.with_suffix(".tmp")
        tmp.write_text(json.dumps(doc, separators=(",", ":")))
        os.replace(tmp, out)
        return sid, m["id"], f"events {len(events)} lag {lag:.2f} peak {peak:.2f} sharp {sharp:.1f}"
    except Exception as e:  # keep going over the list; report what failed
        return sid, m["id"], f"error {type(e).__name__}: {str(e)[:120]}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--max-per-song", type=int, default=3)
    args = ap.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    matches = json.loads((HERE / "bs/matches.json").read_text())
    songs = {s["id"]: s for s in json.loads((HERE.parent.parent / "data/melody/viz/songs.json").read_text())}
    work = [(sid, songs[sid]["path"], m) for sid, v in matches.items() for m in v["maps"][: args.max_per_song] if os.path.exists(songs[sid]["path"])]
    print(len(work), "maps to fetch", flush=True)
    with cf.ThreadPoolExecutor(args.jobs) as ex:
        for i, (sid, mid, res) in enumerate(ex.map(one, work), 1):
            print(i, sid, mid, res, flush=True)


if __name__ == "__main__":
    main()
