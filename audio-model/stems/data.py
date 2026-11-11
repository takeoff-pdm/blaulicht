"""Training data: teacher / ground-truth clips stored by teacher.py.

Every clip is (4, n) float16 mono: mix, drums, bass, vocals; other is the
residual. Samples are random crops, half of them remixed from stems of
four different clips with random per-stem gains (Demucs-style remixing).
"""

import hashlib
import math
import resource
from collections import OrderedDict
from pathlib import Path

import numpy as np
import torch

SR = 44_100

# Standard MUSDB18 validation tracks (from the musdb package).
MUSDB_VALID = {
    "Actions - One Minute Smile", "Clara Berry And Wooldog - Waltz For My Victims",
    "Johnny Lokke - Promises & Lies", "Patrick Talbot - A Reason To Leave",
    "Triviul - Angelsaint", "Alexander Ross - Goodbye Bolero", "Fergessen - Nos Palpitants",
    "Leaf - Summerghost", "Skelpolu - Human Mistakes", "Young Griffo - Pennies",
    "ANiMAL - Rockshow", "James May - On The Line", "Meaxic - Take A Step",
    "Traffic Experiment - Sirens",
}


def read_index(root: Path) -> dict[str, str]:
    """clip id -> source path, over all index shards in `root`."""
    out = {}
    for idx in root.glob("index.*.tsv"):
        for line in idx.read_text().splitlines():
            cid, src = line.split("\t", 1)
            if (root / f"{cid}.npy").exists():
                out[cid] = src
    return out


def edm_split(src: str) -> str:
    """Deterministic split of the dance tracks by path hash: 1/8 test, 1/8 valid."""
    h = int(hashlib.sha1(Path(src).name.encode()).hexdigest(), 16) % 8
    return {0: "test", 1: "valid"}.get(h, "train")


def clips(root: Path, split: str | None = None, kind: str = "plain") -> list[Path]:
    idx = read_index(root)
    keep = []
    for cid, src in sorted(idx.items()):
        if kind == "musdb":
            s = "valid" if Path(src).name in MUSDB_VALID else "train"
        elif kind == "edm":
            s = edm_split(src)
        else:
            s = "train"
        if split is None or s == split:
            keep.append(root / f"{cid}.npy")
    return keep


class Mixes(torch.utils.data.IterableDataset):
    def __init__(self, groups: list[tuple[list[Path], float]], seconds: float = 6.0,
                 remix: float = 0.5, seed: int = 0):
        self.groups = [(files, w) for files, w in groups if files]
        w = np.array([g[1] for g in self.groups], float)
        self.weights = w / w.sum()
        self.seg = int(seconds * SR)
        self.remix = remix
        self.seed = seed
        self._cache: OrderedDict[Path, np.ndarray] = OrderedDict()

    MAX_OPEN = 256  # every cached memmap holds a file descriptor

    def _load(self, path: Path) -> np.ndarray:
        arr = self._cache.get(path)
        if arr is None:
            arr = self._cache[path] = np.load(path, mmap_mode="r")
            if len(self._cache) > self.MAX_OPEN:
                self._cache.popitem(last=False)
        else:
            self._cache.move_to_end(path)
        return arr

    def _crop(self, rng, path: Path) -> np.ndarray:
        arr = self._load(path)
        start = rng.integers(0, max(1, arr.shape[1] - self.seg))
        x = np.asarray(arr[:, start : start + self.seg], np.float32)
        if x.shape[1] < self.seg:
            x = np.pad(x, ((0, 0), (0, self.seg - x.shape[1])))
        mix, d, b, v = x
        return np.stack([d, b, mix - d - b - v, v])  # (4, seg) stems

    def _pick(self, rng) -> Path:
        files = self.groups[rng.choice(len(self.groups), p=self.weights)][0]
        return files[rng.integers(len(files))]

    def __iter__(self):
        soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
        resource.setrlimit(resource.RLIMIT_NOFILE, (min(hard, max(soft, 8192)), hard))
        info = torch.utils.data.get_worker_info()
        rng = np.random.default_rng((self.seed, info.id if info else 0, torch.initial_seed() % 2**31))
        while True:
            if rng.random() < self.remix:
                stems = np.stack([self._crop(rng, self._pick(rng))[s] for s in range(4)])
            else:
                stems = self._crop(rng, self._pick(rng))
            gains = 10 ** (rng.uniform(-6, 3, size=(4, 1)) / 20)
            gains *= 10 ** (rng.uniform(-10, 4) / 20)
            stems = stems * gains
            if rng.random() < 0.5:
                stems = -stems
            mix = stems.sum(0)
            rms = math.sqrt(float((mix**2).mean()))
            if rms < 1e-3:
                continue
            peak = np.abs(mix).max()
            if peak > 1.0:
                stems, mix = stems / peak, mix / peak
            yield torch.from_numpy(mix.astype(np.float32)), torch.from_numpy(stems.astype(np.float32))
