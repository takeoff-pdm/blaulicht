"""Style and mood of the song, overall and second by second.

Essentia's Discogs-EffNet embeddings (16 kHz mono, one 1280-d vector per ~1 s patch) with its
classifier heads: 400 Discogs styles ("Electronic---Hard Techno", ...), the moods aggressive,
happy, party, relaxed, sad, and danceability. The models are CC BY-NC-SA 4.0 (MTG; non-commercial
unless licensed from MTG), so this analyzer is for research; a commercial build would retrain small
heads on permissively licensed embeddings. Kept: the song's 10 most likely styles (mean over the
song), per second the probability of its 8 top styles and of each mood (uint8 tracks at 1 fps),
and the song means. The compiler can pick a show preset per song from these and change it where a
section's style shifts.
"""

import json

import numpy as np

from analyze import common

NAME, VERSION = "tags", 1
DIR = common.ROOT / "models/essentia"
MOODS = {"aggressive": ("mood_aggressive", 0), "happy": ("mood_happy", 0), "party": ("mood_party", 1),
         "relaxed": ("mood_relaxed", 1), "sad": ("mood_sad", 1), "danceable": ("danceability", 0)}


def setup():
    import essentia
    import essentia.standard as es

    essentia.log.warningActive = False
    emb = es.TensorflowPredictEffnetDiscogs(graphFilename=str(DIR / "discogs-effnet-bs64-1.pb"), output="PartitionedCall:1")
    genre = es.TensorflowPredict2D(graphFilename=str(DIR / "genre_discogs400-discogs-effnet-1.pb"),
                                   input="serving_default_model_Placeholder", output="PartitionedCall:0")
    heads = {k: (es.TensorflowPredict2D(graphFilename=str(DIR / f"{m}-discogs-effnet-1.pb"), output="model/Softmax"), col)
             for k, (m, col) in MOODS.items()}
    styles = json.loads((DIR / "genre_discogs400-discogs-effnet-1.json").read_text())["classes"]
    return es, emb, genre, heads, styles


def analyse(song, state):
    es, emb, genre, heads, styles = state
    audio = common.decode(song.audio, 16000, 1)[0]
    e = emb(audio)
    g = np.asarray(genre(e))
    mean = g.mean(0)
    top = np.argsort(-mean)
    out = {"fps": round(16000 / (256 * 62), 4), "styles": [[styles[j], round(float(mean[j]), 3)] for j in top[:10]],
           "style_tracks": {styles[j]: common.track(g[:, j]) for j in top[:8]}, "moods": {}, "mood_tracks": {}}
    for k, (head, col) in heads.items():
        p = np.asarray(head(e))[:, col]
        out["moods"][k] = round(float(p.mean()), 3)
        out["mood_tracks"][k] = common.track(p)
    return out
