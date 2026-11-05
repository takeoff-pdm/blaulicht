"""Lyrics with word timestamps, and vocal chops.

faster-whisper (CTranslate2; code and Whisper weights MIT) large-v3 in float16 on the Mel-RoFormer
vocals (`stems_vocals/<id>.npy` from the vocals analyzer), 16 kHz. Instead of Whisper's own voice
detection, the vocal regions of the separated stem are cut into chunks of at most 28 s (the
approach that worked best for sung lyrics in arXiv 2506.15514); each chunk is transcribed with the
song's language (detected once on its longest vocal stretch, limited to English or German unless
another language is very clear), no conditioning on earlier text, word timestamps on. Lines that
look like hallucinations are dropped: no-speech probability over 0.6 with a mean log-probability
under -1, a compression ratio over 2.4, a mean word probability under 0.45, or one of Whisper's
phantom subtitle lines ("Thank you for watching", "Untertitel ...") on non-speech. Vocal chops:
vocal regions that yield no word with probability 0.4 or more, or are shorter than 1 s. Output words [[start, end, word, p]], lines
[[start, end, text]], chops [[start, end]], language.
"""

import numpy as np
from scipy.signal import resample_poly

from analyze import common
from analyze.vocals import OUT as VOCALS, regions

NAME, VERSION = "lyrics", 2
GPU = True
CHUNK = 28.0
# Whisper's well-known phantom lines on non-speech (from its YouTube subtitle training data)
PHANTOM = ("thank you for watching", "thanks for watching", "thank you", "we'll be right back", "see you next time",
           "we'll see you next time", "please subscribe", "subscribe", "bye", "you", "untertitel", "untertitelung",
           "vielen dank", "danke fürs zuschauen", "bis zum nächsten mal", "copyright", "amara.org")


def _phantom(text):
    t = "".join(c for c in text.lower() if c.isalnum() or c in " '").strip()
    return not t or any(t == p or (p in t and len(t) <= len(p) + 12) for p in PHANTOM)


def setup():
    from faster_whisper import WhisperModel

    return WhisperModel("large-v3", device="cuda", compute_type="float16")


def analyse(song, model):
    p = VOCALS / f"{song.id}.npy"
    if not p.exists():
        raise FileNotFoundError("no separated vocals yet (run the vocals analyzer first)")
    v = np.load(p).astype(np.float32)
    reg, _ = regions(v)
    x = resample_poly(v, 160, 441).astype(np.float32)
    sr = 16000
    chunks = []
    for a, b in reg:
        while b - a > 0.3:
            chunks.append((a, min(b, a + CHUNK)))
            a += CHUNK
    if not chunks:
        return {"language": None, "words": [], "lines": [], "chops": [], "summary": {"words": 0, "lines": 0, "chops": 0}}
    la, lb = max(chunks, key=lambda c: c[1] - c[0])
    _, info = model.transcribe(x[int(la * sr): int(lb * sr)], language=None, beam_size=1, vad_filter=False)
    probs = dict(info.all_language_probs or [])
    lang = info.language if info.language_probability > 0.8 else max(("en", "de"), key=lambda k: probs.get(k, 0))
    words, lines = [], []
    for a, b in chunks:
        segs, _ = model.transcribe(x[int(a * sr): int(b * sr)], language=lang, beam_size=5, temperature=0.0,
                                   condition_on_previous_text=False, word_timestamps=True, vad_filter=False)
        for s in segs:
            ws = s.words or []
            mean_p = sum(w.probability for w in ws) / len(ws) if ws else 0.0
            if (s.no_speech_prob > 0.6 and s.avg_logprob < -1.0) or s.compression_ratio > 2.4 or mean_p < 0.45 or _phantom(s.text):
                continue
            lines.append([round(a + s.start, 2), round(a + s.end, 2), s.text.strip()])
            for w in ws:
                words.append([round(a + w.start, 2), round(a + w.end, 2), w.word.strip(), round(float(w.probability), 2)])
    chops = []
    for a, b in reg:
        sure = [w for w in words if w[0] < b and w[1] > a and w[3] >= 0.4]
        if b - a < 1.0 or not sure:
            chops.append([a, b])
    return {"language": lang, "language_probability": round(float(info.language_probability), 2), "words": words,
            "lines": lines, "chops": chops, "summary": {"words": len(words), "lines": len(lines), "chops": len(chops)}}
