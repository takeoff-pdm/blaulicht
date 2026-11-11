"""Run the lyrics analyzer on one song with a Core ML backend and compare with a reference lyrics.json.

    MELODY_WHISPER_BACKEND=coreml2 LYRICS_COREML_MODEL=<models folder> MELODY_ROOT=<root with stems_vocals/> \
    HF_HUB_OFFLINE=1 python test_lyrics.py <song id> <audio> [<reference lyrics.json> ...]
"""
import difflib
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))


def words_cmp(a, b):
    norm = lambda w: "".join(c for c in w.lower() if c.isalnum())
    x, y = [norm(w[2]) for w in a["words"]], [norm(w[2]) for w in b["words"]]
    sm = difflib.SequenceMatcher(None, x, y, autojunk=False)
    offs = [b["words"][j + k][0] - a["words"][i + k][0] for i, j, n in sm.get_matching_blocks() for k in range(n)]
    return f"words {len(x)} vs {len(y)}, text match {sm.ratio():.3f}, timing within 50 ms {sum(abs(o) <= 0.05 for o in offs)}/{len(offs)}"


def main():
    from analyze import common, lyrics

    sid, audio = sys.argv[1], Path(sys.argv[2])
    song = common.Song(sid, audio.stem, audio, str(audio))
    t0 = time.perf_counter()
    model = lyrics.setup()
    t1 = time.perf_counter()
    result = lyrics.analyse(song, model)
    t2 = time.perf_counter()
    Path("lyrics_out.json").write_text(json.dumps(result))
    print(f"load {t1 - t0:.1f} s, lyrics step {t2 - t1:.1f} s")
    print("  stats:", {k: round(v, 2) if isinstance(v, float) else v for k, v in getattr(model.model, "stats", {}).items()})
    for ref in sys.argv[3:]:
        print(f"  vs {ref}:", words_cmp(json.loads(Path(ref).read_text()), result))
    print("  summary:", result["summary"], "language", result["language"])


if __name__ == "__main__":
    main()
