"""Evaluate students, the teacher and oracles.

    python evaluate.py --ckpt runs/la50/best.pt [--ckpt ...] [--teacher htdemucs_ft] [--oracle]

MUSDB18-HQ test (50 tracks, stereo ground truth):
  uSDR  mean over tracks of 10 log10(|s|^2 / |s - s_hat|^2) on the whole
        track (MDX challenge "SDR").
  cSDR' median over tracks of the median over 1 s chunks of the same ratio.
        No BSS-eval distortion filter, so it is not museval's number; it is
        usually a little lower.
Dance test tracks (held out, teacher stems as reference, mono):
  uSDR against the teacher, and the Pearson correlation of each stem's
  100 frame/s log-energy envelope with the teacher's (what a light show
  would consume).
Writes results JSON next to the first checkpoint (or --out).
"""

import argparse
import json
from pathlib import Path

import numpy as np
import soundfile as sf
import torch

from data import clips
from student import SOURCES, Student

DATA = Path("/localhome/sokrates/stems/data")
SR = 44_100


def sdr_parts(ref: np.ndarray, est: np.ndarray) -> tuple[float, float]:
    """ref/est (C, n) -> (uSDR, chunk-median SDR) in dB."""
    num = (ref**2).sum() + 1e-8
    den = ((ref - est) ** 2).sum() + 1e-8
    u = 10 * np.log10(num / den)
    n = ref.shape[-1] // SR
    r = ref[:, : n * SR].reshape(ref.shape[0], n, SR)
    e = est[:, : n * SR].reshape(ref.shape[0], n, SR)
    rn = (r**2).sum((0, 2))
    keep = rn > 1e-6  # museval skips silent reference chunks
    c = 10 * np.log10((rn + 1e-8) / (((r - e) ** 2).sum((0, 2)) + 1e-8))
    return float(u), float(np.median(c[keep])) if keep.any() else float("nan")


def envelope(x: np.ndarray, hop: int = 441) -> np.ndarray:
    n = x.shape[-1] // hop
    e = (x[..., : n * hop].reshape(-1, n, hop) ** 2).mean((0, 2))
    return 10 * np.log10(e + 1e-8)


def env_metrics(mix: np.ndarray, ref: np.ndarray, est: np.ndarray) -> tuple[float, float]:
    """Light-oriented agreement of one stem: Pearson correlation of 100 fps
    log-energy envelopes floored at mix - 30 dB (leakage far below the mix is
    irrelevant to a light show), and F1 of "stem active" frames (stem
    envelope within 12 dB of the mix)."""
    m = envelope(mix)
    r = np.maximum(envelope(ref), m - 30)
    e = np.maximum(envelope(est), m - 30)
    corr = float(np.corrcoef(r, e)[0, 1]) if r.std() > 1e-6 and e.std() > 1e-6 else float("nan")
    ra, ea = r > m - 12, e > m - 12
    tp = (ra & ea).sum()
    f1 = float(2 * tp / (ra.sum() + ea.sum())) if ra.sum() + ea.sum() else float("nan")
    return corr, f1


def summarize(per_track: list[dict]) -> dict:
    out = {}
    for metric in per_track[0]:
        vals = np.array([t[metric] for t in per_track], float)  # (tracks, 4)
        agg = np.nanmedian if metric.startswith("csdr") else np.nanmean
        out[metric] = {s: round(float(agg(vals[:, i])), 3) for i, s in enumerate(SOURCES)}
        out[metric]["avg"] = round(float(np.mean(list(out[metric].values()))), 3)
    return out


class StudentSep:
    def __init__(self, ckpt: Path, device):
        c = torch.load(ckpt, map_location="cpu", weights_only=False)
        from student import StudentConfig
        cfg = StudentConfig(**{k: tuple(v) if isinstance(v, list) else v for k, v in c["config"].items()})
        self.model = Student(cfg).to(device).eval()
        self.model.load_state_dict(c["model"])
        self.device = device
        self.name = f"{ckpt.parent.name}/{ckpt.stem}"

    @torch.no_grad()
    def __call__(self, mix: np.ndarray) -> np.ndarray:
        """(C, n) -> (4, C, n). Long tracks in 60 s windows with enough
        context on both sides that the result equals one long pass."""
        x = torch.from_numpy(mix).float().to(self.device)
        n, win, ctx = x.shape[-1], 60 * SR, 10 * SR
        out = torch.zeros(4, *x.shape, device=self.device)
        for s in range(0, n, win):
            a, b = max(0, s - ctx), min(n, s + win + ctx)
            with torch.autocast("cuda", torch.bfloat16):
                est, _ = self.model(x[None, :, a:b])
            out[..., s : min(n, s + win)] = est[0, ..., s - a : s - a + min(win, n - s)].float()
        return out.cpu().numpy()


class TeacherSep:
    def __init__(self, name: str, device):
        from demucs.pretrained import get_model
        self.model = get_model(name).to(device).eval()
        self.device = device
        self.name = f"teacher/{name}"

    @torch.no_grad()
    def __call__(self, mix: np.ndarray) -> np.ndarray:
        from demucs.apply import apply_model
        x = torch.from_numpy(mix).float().to(self.device)
        if x.shape[0] == 1:
            x = x.repeat(2, 1)
        ref = x.mean(0)
        m, s = ref.mean(), ref.std() + 1e-8
        est = apply_model(self.model, ((x - m) / s)[None], device=self.device, split=True,
                          overlap=0.25, progress=False)[0] * s + m
        if mix.shape[0] == 1:
            est = est.mean(1, keepdim=True)
        return est.cpu().numpy()


class DspSep:
    """Strawman without learning: HPSS (median filters, 17 frames / 17 bins)
    plus bands. drums = percussive; bass = harmonic below 150 Hz; vocals =
    half the harmonic energy in 250-4000 Hz; other = the rest."""

    name = "baseline/dsp_hpss_bands"

    def __init__(self, device):
        from student import StudentConfig
        self.m = Student(StudentConfig()).to(device)
        self.device = device

    @staticmethod
    def _median(x: torch.Tensor, k: int, dim: int) -> torch.Tensor:
        x = x.movedim(dim, -1)
        x = torch.nn.functional.pad(x[None], (k // 2, k // 2), mode="reflect")[0]
        return x.unfold(-1, k, 1).median(-1).values.movedim(-1, dim)

    @torch.no_grad()
    def __call__(self, mix: np.ndarray) -> np.ndarray:
        X = self.m.stft(torch.from_numpy(mix).float().to(self.device))  # (C, bins, T)
        P = X.abs().mean(0)
        H = self._median(P, 17, 1) ** 2
        Pc = self._median(P, 17, 0) ** 2
        harm = H / (H + Pc + 1e-10)
        hz = torch.arange(P.shape[0], device=self.device) * SR / self.m.cfg.n_fft
        low = (hz < 150).float()[:, None]
        mid = ((hz >= 250) & (hz < 4000)).float()[:, None]
        drums, bass, vocals = 1 - harm, harm * low, 0.5 * harm * mid
        other = 1 - drums - bass - vocals
        masks = torch.stack([drums, bass, other, vocals])[:, None]
        return self.m.istft(masks * X[None], mix.shape[-1]).cpu().numpy()


def oracle_irm(mix: np.ndarray, stems: np.ndarray, mono_mask: bool, device) -> np.ndarray:
    """Ideal ratio mask with the student's STFT: the ceiling of a real
    mask on the mixture (mono_mask: one mask from the channel mean, as the
    student does)."""
    from student import StudentConfig
    m = Student(StudentConfig()).to(device)
    S = m.stft(torch.from_numpy(stems).float().to(device)).abs()  # (4, C, bins, T)
    X = m.stft(torch.from_numpy(mix).float().to(device))
    if mono_mask:
        S = S.mean(1, keepdim=True)
    mask = S / (S.sum(0, keepdim=True) + 1e-8)
    return m.istft(mask * X[None], mix.shape[-1]).cpu().numpy()


def musdb_tracks(limit: int | None):
    root = DATA / "musdb18hq" / "test"
    for t in sorted(root.iterdir())[:limit]:
        stems = np.stack([sf.read(t / f"{s}.wav", dtype="float32")[0].T for s in SOURCES])
        yield t.name, stems.sum(0), stems  # mixture == sum of stems in MUSDB


def edm_tracks():
    for p in clips(DATA / "teacher" / "edm", "test", "edm"):
        mix, d, b, v = np.load(p).astype(np.float32)
        yield p.stem, mix[None], np.stack([d, b, mix - d - b - v, v])[:, None]


def run(sep, device, limit) -> dict:
    res = {}
    rows = []
    for name, mix, stems in (musdb_tracks(limit) if (DATA / "musdb18hq" / "test").exists() else []):
        est = sep(mix)
        parts = [sdr_parts(stems[i], est[i]) for i in range(4)]
        rows.append({"usdr": [p[0] for p in parts], "csdr": [p[1] for p in parts]})
    if rows:
        res["musdb_test"] = summarize(rows)
    if not isinstance(sep, TeacherSep):
        rows = []
        for name, mix, stems in edm_tracks():
            est = sep(mix)
            env = [env_metrics(mix, stems[i], est[i]) for i in range(4)]
            rows.append({
                "usdr_vs_teacher": [sdr_parts(stems[i], est[i])[0] for i in range(4)],
                "env_corr": [e[0] for e in env],
                "active_f1": [e[1] for e in env],
            })
        res["edm_test"] = summarize(rows)
    return res


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ckpt", type=Path, action="append", default=[])
    ap.add_argument("--teacher")
    ap.add_argument("--oracle", action="store_true")
    ap.add_argument("--dsp", action="store_true")
    ap.add_argument("--limit", type=int)
    ap.add_argument("--out", type=Path)
    args = ap.parse_args()
    device = torch.device("cuda")
    results = {}
    for c in args.ckpt:
        sep = StudentSep(c, device)
        results[sep.name] = run(sep, device, args.limit)
        print(sep.name, json.dumps(results[sep.name]), flush=True)
    if args.teacher:
        sep = TeacherSep(args.teacher, device)
        results[sep.name] = run(sep, device, args.limit)
        print(sep.name, json.dumps(results[sep.name]), flush=True)
    if args.dsp:
        sep = DspSep(device)
        results[sep.name] = run(sep, device, args.limit)
        print(sep.name, json.dumps(results[sep.name]), flush=True)
    if args.oracle:
        for mono in (True, False):
            rows = []
            for name, mix, stems in musdb_tracks(args.limit):
                est = oracle_irm(mix, stems, mono, device)
                parts = [sdr_parts(stems[i], est[i]) for i in range(4)]
                rows.append({"usdr": [p[0] for p in parts], "csdr": [p[1] for p in parts]})
            key = f"oracle/irm_{'mono' if mono else 'stereo'}_mask"
            results[key] = {"musdb_test": summarize(rows)}
            print(key, json.dumps(results[key]), flush=True)
    out = args.out or (args.ckpt[0].parent / "eval.json" if args.ckpt else Path("eval.json"))
    old = json.loads(out.read_text()) if out.exists() else {}
    out.write_text(json.dumps(old | results, indent=1))
    print("wrote", out)


if __name__ == "__main__":
    main()
