"""A contact sheet of generated effects, to look at: python -m analyze.fx_train.smoke_effects <out dir>"""

import sys
import time
from pathlib import Path

import numpy as np

from analyze.fx_train import effects as fx
from analyze.fx_train.backbone import mel_power
from analyze.fx_train.cuts import _img, _png

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
rng = np.random.default_rng(3)
rows, t0 = [], time.time()
n = 0
for kind in fx.CLASSES:
    line = []
    for i in range(7):
        e = fx.make(kind, rng, 60 / 128)
        assert np.isfinite(e.audio).all() and len(e.audio) > 0 and e.b > e.a, (kind, e.family)
        line.append(np.pad(_img(mel_power(e.audio), h=72, w=120), 2, constant_values=255))
        n += 1
    rows.append(np.concatenate(line, axis=1))
line = []
for i in range(7):
    e = fx.distractor(rng, 60 / 128)
    assert np.isfinite(e.audio).all()
    line.append(np.pad(_img(mel_power(e.audio), h=72, w=120), 2, constant_values=255))
rows.append(np.concatenate(line, axis=1))
_png(out / "generated.png", np.concatenate(rows))
print(f"{n} effects, {(time.time() - t0) / n * 1000:.0f} ms each; rows are", fx.CLASSES, "and distractors")
