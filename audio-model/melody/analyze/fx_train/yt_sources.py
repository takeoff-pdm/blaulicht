"""Isolated effects from YouTube, weakly labelled by search query and title.

    python -m analyze.fx_train.yt_sources search     # metadata only, writes candidates.json
    python -m analyze.fx_train.yt_sources download   # audio of the chosen videos, one at a time

Limits we keep to: no login, no cookies, no proxies, no bot-check workarounds. One request at a time
with sleeps. On "confirm you're not a bot" or a 429 the job stops and says so rather than
pressing on; a single 403 on one video's data is skipped, a second in a row or a fourth overall
stops it. Audio stays on ae-charlie in models/fx_head/sources/ and is never redistributed; the
manifest (video id, title, channel, licence, class, duration) goes to analyze/fx_train/yt_manifest.json
so it's possible to show exactly what was used. Creative Commons searches come first, then uploads
whose titles say free or no copyright.
"""

import json
import re
import subprocess
import sys
import time
import urllib.parse
from pathlib import Path

from analyze import common

SRC = common.ROOT / "models/fx_head/sources"
AUDIO = SRC / "audio"
CANDS = SRC / "candidates.json"
MANIFEST = Path(__file__).parent / "yt_manifest.json"
YT = str(common.ROOT / ".venv-fx/bin/yt-dlp")
CC = "EgIwAQ%253D%253D"  # YouTube's own "Creative Commons" search filter
MAX_DUR, MIN_DUR = 900, 2
MAX_BYTES = 10 * 1024 ** 3
SLEEP = ["--sleep-requests", "2", "--sleep-interval", "3", "--max-sleep-interval", "7"]
BLOCK = re.compile(r"confirm you.?re not a bot|sign in to confirm|http error 429|too many requests", re.I)
FORBIDDEN = re.compile(r"http error 403", re.I)

QUERIES = {
    "riser": ["riser sound effect", "edm riser sound effect", "uplifter sound effect", "white noise riser sound effect",
              "noise sweep sound effect", "build up riser sfx", "free edm riser samples", "synth riser sound effect"],
    "downlifter": ["downlifter sound effect", "edm downlifter", "downsweep sound effect", "sub drop sound effect",
                   "down sweep sfx", "free downlifter samples", "808 sub drop sample", "noise down sweep sound effect"],
    "impact": ["impact sound effect", "edm impact sound effect", "boom impact sound effect", "sub boom sound effect",
               "cinematic impact sound effect", "free impact samples", "bass boom sound effect", "low impact hit sfx"],
    "horn": ["air horn sound effect", "dj air horn sound effect", "reggae air horn sample", "air horn sfx free",
             "dubstep air horn sample", "mlg air horn sound effect", "airhorn sound effect no copyright"],
    "siren": ["dub siren sound effect", "dj siren sound effect", "rave siren sample", "siren sound effect",
              "siren sfx free", "reggae dub siren", "police siren sound effect no copyright", "air raid siren sound effect"],
    "revcym": ["reverse cymbal sound effect", "reverse crash cymbal sound effect", "reversed cymbal sample",
               "reverse cymbal sfx free", "cymbal swell reverse", "reverse crash sound effect"],
}
# a title must name its class, and no other class
TERMS = {
    "riser": r"riser|uplift|up ?lifter|sweep up|up ?sweep|build ?up|swell",
    "downlifter": r"downlift|down ?lifter|down ?sweep|sweep down|sub ?drop|808 drop",
    "impact": r"impact|\bboom\b|\bbooms\b|sub hit|bass hit|\bhit\b|\bhits\b",
    "horn": r"air ?horn",
    "siren": r"siren",
    "revcym": r"revers\w* (crash|cymbal)|(crash|cymbal) revers\w*|cymbal swell",
}
EXCLUDE = re.compile(r"\b(tutorial|how to|how i|make|making|remix|mix\b|mixes|song|reaction|top \d+ songs|full album|lyrics|"
                     r"live set|podcast|lesson|guide|explained|course|serum|ableton|fl studio|logic pro|vst|plugin|synthesi[sz]|"
                     r"sound design|compilation of songs|ringtone|1 hour|10 hours?|ten hours?|loop for|gameplay|asmr)\b", re.I)
FREE = re.compile(r"\b(free|no copyright|copyright free|royalty|non copyright|nocopyright)\b", re.I)


def _other_classes(cls, title):
    return [c for c, t in TERMS.items() if c != cls and re.search(t, title, re.I)]


def _search(url):
    r = subprocess.run([YT, "--flat-playlist", "--dump-json", "--playlist-end", "60", "--sleep-requests", "3", url],
                       capture_output=True, text=True, timeout=300)
    if BLOCK.search(r.stderr):
        raise SystemExit(f"YouTube blocked the search, stopping: {r.stderr[-400:]}")
    out = []
    for line in r.stdout.splitlines():
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    return out


def search():
    SRC.mkdir(parents=True, exist_ok=True)
    found = {}  # video id -> entry; first class wins, Creative Commons queries first
    for cc in (True, False):
        for cls, queries in QUERIES.items():
            for q in queries:
                base = "https://www.youtube.com/results?search_query=" + urllib.parse.quote_plus(q)
                url = base + "&sp=" + CC if cc else base
                rows = _search(url)
                kept = 0
                for e in rows:
                    vid, title, dur = e.get("id"), e.get("title") or "", e.get("duration") or 0
                    if vid in found or not (MIN_DUR <= dur <= MAX_DUR):
                        continue
                    if EXCLUDE.search(title) or not re.search(TERMS[cls], title, re.I) or _other_classes(cls, title):
                        continue
                    found[vid] = {"id": vid, "title": title, "channel": e.get("channel") or e.get("uploader") or "",
                                  "channel_id": e.get("channel_id") or "", "duration": dur, "class": cls, "query": q,
                                  "cc_search": cc, "free_title": bool(FREE.search(title))}
                    kept += 1
                print(f"{'cc ' if cc else '   '}{cls:11s} {q:44s} {len(rows):3d} results, {kept:3d} new", flush=True)
                CANDS.write_text(json.dumps(found, ensure_ascii=False, indent=0))
                time.sleep(4)
    return found


# Titles that say song, tutorial or game, whatever else they say. The search words alone let through
# "Genshin Impact" skill clips, "Type Beat" songs and sound-design lessons.
NOT_FX = re.compile(r"genshin|type beat|official|music video|tutorial|\btips\b|technique|cartoon|drum break|the power of|simpler|"
                    r"sylenth|bitwig|c[oó]mo hacer|free music|background music|techno music|\bbgm\b|gaming|digital echo|succubus|flashpoint|"
                    r"glass|foley|fight|metal breakdowns|smash hit|vine boom|cinema sins|truck|freight|firetruck|violin|jeopardy|theme|"
                    r"recording junk|vocal samples|\blive\b|prank|quickie|quick tip|mixing tips|strawberry fields|what does|late riser|car bass|applause|"
                    r"\bdance\b(?! sound)|\bcover\b|illuminati|apple daily|\bipl\b|stadium|nakakatakot", re.I)
CLUB = re.compile(r"\b(edm|dj|club|rave|techno|trance|dub|dubsiren|reggae|dancehall|dubstep|bigroom|sample|samples|pack|house|hardstyle)\b", re.I)
REAL_WORLD = re.compile(r"police|ambulance|tornado|air raid|war\b|emergency|missile|nuclear|bomb|alarm", re.I)
PER_CLASS = 40


# ... and a title has to say it's an effect: a song called "Downlifter" or "Siren" doesn't
SAYS_FX = re.compile(r"sound|sfx|\bfx\b|sample|\bpack\b|effect|dub ?siren|sirena|airhorn|air horn", re.I)
LIMIT = {"horn": 300, "siren": 300}  # ten minutes of one police car adds nothing


# a spoken or sung watermark after each sound: seen on the audit sheets as the same blob ending every cut
WATERMARKED = re.compile(r"motion array|alex koltsov|marko duca", re.I)


def wanted(e):
    t, ch = e["title"], e.get("channel") or ""
    return (not NOT_FX.search(t) and SAYS_FX.search(t) is not None and not ch.endswith("- Topic") and not WATERMARKED.search(ch)
            and not re.search(r"audio ?jungle|auidiojungle", t, re.I)
            and e["duration"] <= LIMIT.get(e["class"], MAX_DUR))


def choose(found):
    """Per class: videos already fetched count first, then club-flavoured titles, each side Creative
    Commons or free-to-use before the rest. At most four per channel, so one channel's voice-over or
    watermark can't fill a class, and real-world sirens are capped so police recordings don't crowd
    out dub and rave sirens. Only classes short of candidates reach for uploads that don't say free."""
    out = []
    have = {p.stem for p in AUDIO.glob("*.json")}
    for cls in QUERIES:
        rows = [e for e in found.values() if e["class"] == cls and wanted(e)]
        free = [e for e in rows if e["cc_search"] or e["free_title"]]
        if len(free) < PER_CLASS:
            free = rows
        free.sort(key=lambda e: (e["id"] not in have, not CLUB.search(e["title"]), not (e["cc_search"] or e["free_title"]),
                                 not e["cc_search"], -min(e["duration"], 240)))
        per, chosen, real = {}, [], 0
        for e in free:
            ch = e["channel_id"] or e["channel"]
            if per.get(ch, 0) >= 4 or (cls == "siren" and REAL_WORLD.search(e["title"]) and real >= 12):
                continue
            per[ch] = per.get(ch, 0) + 1
            real += bool(REAL_WORLD.search(e["title"]))
            chosen.append(e)
            if len(chosen) >= PER_CLASS:
                break
        out += chosen
        print(f"{cls}: {len(rows)} candidates, {len(chosen)} chosen, {sum(e['duration'] for e in chosen) / 60:.1f} min, "
              f"{sum(e['cc_search'] for e in chosen)} from the Creative Commons search, "
              f"{sum(not (e['cc_search'] or e['free_title']) for e in chosen)} not saying free", flush=True)
    return out


def drop_unwanted():
    """Downloads an earlier, looser filter let in: tutorials, songs, game clips."""
    for p in list(AUDIO.glob("*.json")):
        e = json.loads(p.read_text())
        if not wanted(e):
            print("dropping", e["id"], e["title"][:60], flush=True)
            for f in AUDIO.glob(e["id"] + ".*"):
                f.unlink()


def _write_manifest():
    rows = []
    for p in sorted(AUDIO.glob("*.json")):
        rows.append(json.loads(p.read_text()))
    MANIFEST.write_text(json.dumps(rows, ensure_ascii=False, indent=0))
    # also next to the audio, so it survives a code push from the Mac
    (SRC / "yt_manifest.json").write_text(MANIFEST.read_text())
    return rows


def download():
    AUDIO.mkdir(parents=True, exist_ok=True)
    drop_unwanted()
    chosen = choose(json.loads(CANDS.read_text()))
    done, in_a_row, total_403 = 0, 0, 0
    for i, e in enumerate(chosen):
        side = AUDIO / f"{e['id']}.json"
        if side.exists():
            continue
        if sum(p.stat().st_size for p in AUDIO.iterdir()) > MAX_BYTES:
            print("10 GB reached, stopping", flush=True)
            break
        r = subprocess.run([YT, "--no-playlist", "--no-cookies-from-browser", "-f", "bestaudio[ext=m4a]/bestaudio", "--match-filter", f"duration<{MAX_DUR}",
                            "--max-filesize", "80M", *SLEEP, "-o", str(AUDIO / "%(id)s.%(ext)s"), "--print-json", "--no-simulate",
                            f"https://www.youtube.com/watch?v={e['id']}"], capture_output=True, text=True, timeout=900)
        if BLOCK.search(r.stderr):
            print(f"YouTube blocked the download of {e['id']}, stopping after {done} downloads: {r.stderr[-400:]}", flush=True)
            _write_manifest()
            raise SystemExit(3)
        if FORBIDDEN.search(r.stderr):  # one video's data URL can 403; twice running, or often, is a block
            in_a_row, total_403 = in_a_row + 1, total_403 + 1
            print(f"[{i + 1}/{len(chosen)}] {e['id']} {e['title'][:50]}: HTTP 403 ({in_a_row} in a row, {total_403} total)", flush=True)
            if in_a_row >= 2 or total_403 >= 4:
                print(f"repeated 403s, treating this as a block and stopping after {done} downloads", flush=True)
                _write_manifest()
                raise SystemExit(3)
            time.sleep(20)
            continue
        in_a_row = 0
        info = None
        for line in r.stdout.splitlines():
            if line.startswith("{"):
                info = json.loads(line)
        if info is None or not list(AUDIO.glob(e["id"] + ".*")):
            print(f"[{i + 1}/{len(chosen)}] {e['id']} {e['title'][:50]}: failed: {r.stderr[-160:].strip()}", flush=True)
            continue
        side.write_text(json.dumps({**e, "licence": info.get("license") or "", "channel_id": info.get("channel_id") or e["channel_id"],
                                    "duration": info.get("duration") or e["duration"], "upload_date": info.get("upload_date") or "",
                                    "url": f"https://www.youtube.com/watch?v={e['id']}"}, ensure_ascii=False))
        done += 1
        print(f"[{i + 1}/{len(chosen)}] {e['class']:10s} {e['id']} {e['duration']:4d} s {info.get('license') or '-':.24s} {e['title'][:60]}", flush=True)
        if done % 20 == 0:
            _write_manifest()
        time.sleep(3)
    rows = _write_manifest()
    print(f"done: {len(rows)} videos, {sum(p.stat().st_size for p in AUDIO.iterdir()) / 1e6:.0f} MB", flush=True)


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "search":
        choose(search())
    elif cmd == "download":
        download()
    else:
        sys.exit(__doc__)
