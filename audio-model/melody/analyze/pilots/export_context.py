"""On the Mac: copy what the pilots compare against from the viewer's read-only API to
`pilots/context/<id>.json`; `sync.sh push` then takes it to ae-charlie, where `cues.parts` etc. don't exist
(`pipeline/show_compile.py` makes them).

    python3 audio-model/melody/analyze/pilots/export_context.py [id ...]
"""

import json
import sys
import urllib.request
from pathlib import Path

API = "http://127.0.0.1:8766/api"
HERE = Path(__file__).parent
SONGS = ["58e556af6855", "fe0b3c607b1b", "8a0e3b75f4a1", "dcb7a7d3e846", "6a67050f048d"]


def trim(d: dict) -> dict:
    f, c = d.get("features", {}), d.get("cues", {})
    return {
        "id": d["id"], "name": d["name"], "duration": d["duration"], "key": d.get("key"),
        "bpm": d.get("beats", {}).get("bpm"), "beats": d.get("beats", {}).get("beats"),
        "parts": c.get("parts"), "lines": c.get("lines"), "functions": c.get("functions"),
        "drops": d.get("show", {}).get("drops"), "sections": d.get("show", {}).get("sections"),
        "vocal_regions": f.get("vocals", {}).get("regions"), "vocal_share": f.get("vocals", {}).get("share"),
        "mt3_summary": f.get("mt3", {}).get("summary"), "mt3_active": f.get("mt3", {}).get("active"),
        "styles": (f.get("tags", {}).get("styles") or [])[:6],
        "energy": d.get("energy"), "fps": d.get("fps"),
    }


def main():
    (HERE / "context").mkdir(exist_ok=True)
    for sid in sys.argv[1:] or SONGS:
        with urllib.request.urlopen(f"{API}/analysis/{sid}") as r:
            doc = trim(json.load(r))
        (HERE / "context" / f"{sid}.json").write_text(json.dumps(doc, separators=(",", ":"), ensure_ascii=False))
        print(sid, doc["name"], f'{doc["duration"]:.0f} s', len(doc["parts"] or []), "parts", len(doc["drops"] or []), "drops",
              len(doc["sections"] or []), "sections")


if __name__ == "__main__":
    main()
