"""Which of our electronic songs have human-made light shows on BeatSaver?

Beat Saber custom maps carry hand-placed lighting events (on / flash / fade / off per light group, in
beats, two colours). This asks BeatSaver's public search API about each song, one request a second,
and writes every plausible map with lighting to bs/matches.json. Standard library only.

    python3 audio-model/melody/decision/bs_match.py
"""

import json
import re
import sys
import time
import unicodedata
import urllib.parse
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
DATA = HERE.parent.parent / "data/melody/viz"
OUT = HERE / "bs"
UA = {"User-Agent": "blaulicht-research/0.1 (decision experiment)"}


def norm(s):
    s = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode().lower()
    s = re.sub(r"\((?:feat|ft|extended|original|radio|club|official|edit|mix|remix|vip)[^)]*\)|\[[^\]]*\]", " ", s)
    s = re.sub(r"\b(feat|ft)\.?\b.*", " ", s)
    return " ".join(re.sub(r"[^a-z0-9 ]+", " ", s).split())


def split_name(name):
    name = re.sub(r"^\s*\d+[.)]\s*", "", name)  # "02. Artist - Title"
    parts = re.split(r"\s+[-–—]\s+", name, maxsplit=1)
    return (parts[0], parts[1]) if len(parts) == 2 else ("", parts[0])


def words(s):
    return set(norm(s).split())


def score(title, artist, doc):
    md = doc["metadata"]
    # title words must be there in the map's song name; the artist anywhere in its names
    tw = words(title)
    if not tw:
        return 0.0
    have = words(md.get("songName", "") + " " + md.get("songSubName", ""))
    t = len(tw & have) / len(tw)
    aw = words(artist)
    ha = words(md.get("songAuthorName", "") + " " + md.get("songName", "") + " " + md.get("songSubName", ""))
    a = len(aw & ha) / len(aw) if aw else 0.5
    return t * (0.4 + 0.6 * a)


def search(q):
    url = "https://api.beatsaver.com/search/text/0?" + urllib.parse.urlencode({"q": q, "sortOrder": "Relevance"})  # no automapper filter: automapper=false returned only automapped maps
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=20) as r:
        return json.load(r).get("docs", [])


def main():
    songs = json.loads((DATA / "songs.json").read_text())
    genres = json.loads((DATA / "genres.json").read_text())
    electronic = [s for s in songs if genres.get(s["id"], {}).get("genre") == "electronic"]
    OUT.mkdir(exist_ok=True)
    out = {}
    for i, s in enumerate(electronic):
        artist, title = split_name(s["name"])
        q = f"{norm(artist).split(' ')[0] if artist else ''} {norm(title)}".strip()
        try:
            docs = search(q)
        except Exception as e:  # one bad query is not worth stopping for
            print("error", s["name"], e, file=sys.stderr, flush=True)
            docs = []
        hits = []
        for d in docs:
            sc = score(title, artist, d)
            if sc < 0.7 or d.get("automapper"):
                continue
            v = d["versions"][0]
            ev = max((x.get("events", 0) for x in v["diffs"]), default=0)
            if ev < 200:
                continue
            hits.append({"id": d["id"], "song": d["metadata"]["songName"], "author": d["metadata"]["songAuthorName"],
                         "bpm": d["metadata"]["bpm"], "duration": d["metadata"]["duration"], "events": ev,
                         "score": d["stats"]["score"], "upvotes": d["stats"].get("upvotes"), "ranked": d.get("ranked"), "curated": d.get("curator") is not None,
                         "verified": (d.get("uploader") or {}).get("verifiedMapper"), "downloadURL": v["downloadURL"], "match": round(sc, 2)})
        hits = [h for h in hits if abs(h["duration"] - s["duration"]) <= max(45, 0.35 * s["duration"])]
        hits.sort(key=lambda h: (-(h["curated"] or h["ranked"] or False), -h["score"]))
        out[s["id"]] = {"name": s["name"], "duration": s["duration"], "maps": hits[:4]}
        if (i + 1) % 20 == 0:
            print(i + 1, sum(bool(v["maps"]) for v in out.values()), flush=True)
            (OUT / "matches.json").write_text(json.dumps(out, indent=1))
        time.sleep(1.0)
    (OUT / "matches.json").write_text(json.dumps(out, indent=1))
    n = sum(bool(v["maps"]) for v in out.values())
    print(f"{n} of {len(out)} electronic songs have at least one map with lighting; "
          f"{sum(len(v['maps']) >= 2 for v in out.values())} have two or more", flush=True)


if __name__ == "__main__":
    main()
