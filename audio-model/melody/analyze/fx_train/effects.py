"""Procedural club-music effects, with wide variety, in code.

Everything is mono float32 at FX_SR. make(kind, rng, beat) returns an Effect: the audio, the sample
that goes on the musical grid (`hit`: a riser's peak, an impact's onset), and the span the label
covers (`a`, `b`). Reverb tails and the like stay in the audio but outside the label; mixtures.py
leaves them out of the loss.
"""

from dataclasses import dataclass

import numpy as np
from scipy.signal import fftconvolve, istft, stft

FX_SR = 32000
CLASSES = ["riser", "downlifter", "impact", "horn", "siren", "revcym"]


@dataclass
class Effect:
    kind: str
    family: str
    audio: np.ndarray
    hit: int  # the sample that lands on the grid
    align: str  # "end": the label ends at hit; "start": the label starts at hit
    a: int  # label span
    b: int
    source: str = "gen"
    source_id: str = ""


# ---- building blocks

def noise(n, rng, color=None):
    """White, pink or brown noise, or something between, by spectral tilt."""
    slope = {"white": 0.0, "pink": 0.5, "brown": 1.0}.get(color, None) if color else rng.uniform(0, 1)
    X = np.fft.rfft(rng.standard_normal(n))
    f = np.maximum(np.fft.rfftfreq(n, 1 / FX_SR), 10.0)
    x = np.fft.irfft(X * f ** -slope, n)
    return (x / (x.std() + 1e-9)).astype(np.float32)


def curve(n, rng, lo=0.6, hi=3.5):
    """0..1 monotonic over n samples: exponent curve, slow-then-fast mostly."""
    return np.linspace(0, 1, n, dtype=np.float32) ** rng.uniform(lo, hi)


def glide(a, b, c):
    """c 0..1 to a frequency moving from a to b on a log scale."""
    return a * (b / a) ** c


def tv_band(x, f_lo, f_hi, res=0.0, n_fft=1024, hop=256):
    """Time-varying band pass by STFT mask: f_lo, f_hi are per-sample cutoffs in Hz (None: open).
    res adds a resonant peak at the moving cutoff."""
    f, _, Z = stft(x, fs=FX_SR, nperseg=n_fft, noverlap=n_fft - hop, boundary="zeros", padded=True)
    fr = Z.shape[1]
    t = np.minimum((np.arange(fr) * hop).astype(int), len(x) - 1)
    lf = np.log2(np.maximum(f, 10.0))[:, None]
    m = np.ones_like(Z.real)
    roll = 1.3  # octaves over which the mask rolls off
    if f_hi is not None:
        c = np.log2(np.maximum(f_hi[t], 10.0))[None, :]
        m *= np.clip(1 - (lf - c) / roll, 0, 1) ** 2
        if res > 0:
            m *= 1 + res * np.exp(-((lf - c) / 0.18) ** 2)
    if f_lo is not None:
        c = np.log2(np.maximum(f_lo[t], 10.0))[None, :]
        m *= np.clip(1 + (lf - c) / roll, 0, 1) ** 2
        if res > 0 and f_hi is None:
            m *= 1 + res * np.exp(-((lf - c) / 0.18) ** 2)
    _, y = istft(Z * m, fs=FX_SR, nperseg=n_fft, noverlap=n_fft - hop, boundary=True)
    y = y[: len(x)]
    return np.pad(y, (0, len(x) - len(y))).astype(np.float32)


def osc(freq, shape="saw", rng=None, phase=None):
    p = np.cumsum(freq) / FX_SR + (rng.uniform() if phase is None and rng is not None else (phase or 0.0))
    if shape == "sine":
        return np.sin(2 * np.pi * p).astype(np.float32)
    if shape == "tri":
        return (2 * np.abs(2 * (p % 1) - 1) - 1).astype(np.float32)
    if shape == "square":
        return np.sign(np.sin(2 * np.pi * p)).astype(np.float32)
    return (2 * (p % 1) - 1).astype(np.float32)


def stack(freq, rng, voices=None, cents=None, shape=None):
    """Detuned unison: a supersaw when wide."""
    voices = voices or int(rng.integers(1, 8))
    cents = cents if cents is not None else rng.uniform(0, 40)
    shape = shape or rng.choice(["saw", "saw", "square", "tri", "sine"])
    out = np.zeros_like(freq, dtype=np.float32)
    for d in np.linspace(-cents, cents, voices) if voices > 1 else [0.0]:
        out += osc(freq * 2 ** (d / 1200), shape, rng)
    return out / np.sqrt(voices)


def reverb(x, rng, t60=None, wet=None):
    """A synthetic room: exponentially decaying filtered noise. Returns a longer signal."""
    t60 = t60 if t60 is not None else float(rng.uniform(0.3, 4.0))
    wet = wet if wet is not None else float(rng.uniform(0.15, 0.7))
    n = int(t60 * FX_SR)
    ir = noise(n, rng, "white") * np.exp(-6.9 * np.arange(n) / n)
    bright = glide(1500.0, 12000.0, rng.uniform())
    ir = tv_band(ir, None, np.full(n, bright, np.float32))
    ir[: int(0.004 * FX_SR)] = 0
    ir /= np.sqrt((ir ** 2).sum()) + 1e-9
    w = fftconvolve(x, ir)
    y = np.zeros(len(w), np.float32)
    y[: len(x)] = x * np.sqrt(1 - wet)
    return (y + np.sqrt(wet) * w).astype(np.float32)


def delay(x, beat, rng):
    d = int(beat * rng.choice([0.25, 0.5, 0.75, 1.0]) * FX_SR)
    fb, reps = rng.uniform(0.25, 0.65), int(rng.integers(2, 6))
    y = np.zeros(len(x) + d * reps, np.float32)
    for k in range(reps + 1):
        y[k * d: k * d + len(x)] += x * fb ** k
    return y


def chorus(x, rng):
    n = len(x)
    t = np.arange(n) / FX_SR
    d = (0.008 + 0.006 * np.sin(2 * np.pi * rng.uniform(0.1, 2.0) * t)) * FX_SR
    idx = np.clip(np.arange(n) - d, 0, n - 1)
    i = idx.astype(int)
    return 0.7 * x + 0.7 * (x[i] + (idx - i) * (x[np.minimum(i + 1, n - 1)] - x[i]))


def phaser(x, rng):
    """A moving comb: the swooshy sound on noise."""
    n = len(x)
    t = np.arange(n) / FX_SR
    d = (0.0015 + 0.0012 * np.sin(2 * np.pi * rng.uniform(0.1, 1.5) * t)) * FX_SR
    idx = np.clip(np.arange(n) - d, 0, n - 1).astype(int)
    return 0.7 * x + 0.7 * x[idx]


def drive(x, rng):
    g = 10 ** (rng.uniform(3, 18) / 20)
    return np.tanh(x / (np.abs(x).max() + 1e-9) * g).astype(np.float32)


def crush(x, rng):
    k = int(rng.integers(2, 8))
    q = 2 ** int(rng.integers(5, 10))
    y = np.repeat(x[::k], k)[: len(x)]
    return (np.round(y * q) / q).astype(np.float32)


def eq(x, rng):
    """Random tilt and a bell, in the frequency domain."""
    X = np.fft.rfft(x)
    f = np.maximum(np.fft.rfftfreq(len(x), 1 / FX_SR), 20.0)
    lf = np.log2(f / 1000)
    g = rng.uniform(-3, 3) * lf + rng.uniform(-9, 9) * np.exp(-((lf - rng.uniform(-3, 3)) / rng.uniform(0.4, 1.5)) ** 2)
    return np.fft.irfft(X * 10 ** (g / 20), len(x)).astype(np.float32)


def gate(x, beat, rng, accel=None):
    """Tremolo in note values: a trance gate, or accelerating towards the end."""
    n = len(x)
    accel = rng.uniform() < 0.5 if accel is None else accel
    c = np.linspace(0, 1, n)
    if accel:
        rates = 2.0 ** np.round(np.interp(c, [0, 1], [rng.integers(0, 2), rng.integers(2, 5)]))  # per beat
    else:
        rates = np.full(n, 2.0 ** int(rng.integers(0, 4)))
    ph = np.cumsum(rates / (beat * FX_SR)) % 1
    depth, shape = rng.uniform(0.4, 1.0), rng.uniform(0.2, 0.8)
    return (x * (1 - depth * (ph > shape))).astype(np.float32)


def sidechain(x, beat, rng, offset=0):
    """Ducked on every beat, as a compressor keyed by the kick would."""
    n = len(x)
    period = int(beat * FX_SR)
    ph = (np.arange(n) + offset) % period
    depth, tau = rng.uniform(0.3, 0.95), rng.uniform(0.08, 0.45) * beat * FX_SR
    return (x * (1 - depth * np.exp(-ph / tau))).astype(np.float32)


def fade(x, ms=4):
    k = min(int(ms * FX_SR / 1000), len(x) // 2)
    if k > 0:
        x = x.copy()
        x[:k] *= np.linspace(0, 1, k)
        x[-k:] *= np.linspace(1, 0, k)
    return x


def chain(x, beat, rng, wet=True):
    """A random FX chain; the length may grow by a tail."""
    if rng.uniform() < 0.6:
        x = eq(x, rng)
    if rng.uniform() < 0.2:
        x = drive(x, rng)
    if rng.uniform() < 0.05:
        x = crush(x, rng)
    if rng.uniform() < 0.2:
        x = chorus(x, rng)
    if rng.uniform() < 0.15:
        x = phaser(x, rng)
    if wet and rng.uniform() < 0.25:
        x = delay(x, beat, rng)
    if wet and rng.uniform() < 0.5:
        x = reverb(x, rng)
    return x


def norm(x):
    return (x / (np.abs(x).max() + 1e-9)).astype(np.float32)


# ---- rising sounds

def _level(n, rng, lo_db=None):
    """A rising level curve in dB: not always from silence."""
    lo = lo_db if lo_db is not None else rng.uniform(-45, -6)
    return (10 ** (lo * (1 - curve(n, rng)) / 20)).astype(np.float32)


def rise_noise(n, beat, rng):
    x = noise(n, rng)
    c = curve(n, rng)
    mode = rng.choice(["lp", "bp", "hp"])
    a = glide(200.0, 3000.0, rng.uniform())
    b = glide(5000.0, 15500.0, rng.uniform())
    f = glide(a, b, c)
    if mode == "lp":
        x = tv_band(x, None, f, res=rng.choice([0, 0, 2, 6]))
    elif mode == "hp":
        x = tv_band(x, f * rng.uniform(0.2, 0.6), None, res=rng.choice([0, 2]))
    else:
        x = tv_band(x, f / rng.uniform(1.2, 3.0), f * rng.uniform(1.2, 3.0), res=rng.choice([0, 0, 2, 6]))
    return x * _level(n, rng), mode


def rise_pitch(n, beat, rng):
    c = curve(n, rng, 0.5, 2.5)
    a = glide(50.0, 500.0, rng.uniform())
    f = glide(a, a * 2 ** rng.uniform(1.0, 5.0), c)
    if rng.uniform() < 0.3:  # vibrato that speeds up
        rate = glide(2.0, 16.0, c)
        f = f * 2 ** (rng.uniform(0.05, 0.5) / 12 * np.sin(2 * np.pi * np.cumsum(rate) / FX_SR))
    x = stack(f, rng)
    if rng.uniform() < 0.6:
        x = tv_band(x, None, np.minimum(f * rng.uniform(2, 10) * (0.3 + c), 15500.0), res=rng.choice([0, 2]))
    return x * _level(n, rng, rng.uniform(-30, -3)), "pitch"


def rise_chord(n, beat, rng):
    """A detuned chord: pitch climbing or just the filter opening, with a swelling level."""
    c = curve(n, rng)
    root = glide(60.0, 400.0, rng.uniform())
    ratios = [1, *rng.choice([1.2, 1.25, 1.333, 1.5, 2.0, 3.0], size=int(rng.integers(1, 4)), replace=False)]
    octs = rng.choice([0.0, 0.0, 1.0, 2.0, 3.0])
    f = glide(root, root * 2 ** octs, c) if octs else np.full(n, root, np.float32)
    x = sum(stack(f * r, rng, voices=int(rng.integers(2, 7)), cents=rng.uniform(5, 40), shape="saw") for r in ratios)
    cut = glide(float(rng.uniform(250, 900)), float(rng.uniform(5000, 15000)), c)
    x = tv_band(x, None, cut, res=rng.choice([0, 3, 6]))
    return x * _level(n, rng, rng.uniform(-30, -6)), "chord"


def rise_sine(n, beat, rng):
    c = curve(n, rng, 0.8, 3.0)
    f = glide(float(rng.uniform(150, 900)), float(rng.uniform(2000, 8000)), c)
    x = osc(f, rng.choice(["sine", "tri"]), rng)
    return x * _level(n, rng, rng.uniform(-25, -3)), "sine"


def rise_pulse(n, beat, rng):
    """Repeated notes speeding up, climbing in pitch or filter."""
    x, t = np.zeros(n, np.float32), 0
    c = curve(n, rng, 0.8, 2.0)
    base = glide(100.0, 800.0, rng.uniform())
    octs = rng.uniform(0, 3)
    shape = rng.choice(["saw", "square", "tri", "noise"])
    while t < n:
        p = c[t]
        div = 2.0 ** np.round(np.interp(p, [0, 1], [rng.choice([0, 1]), rng.choice([2, 3, 4])]))
        step = max(int(beat * FX_SR / div), 64)
        m = min(step, n - t)
        f = np.full(m, base * 2 ** (octs * p), np.float32)
        note = noise(m, rng) if shape == "noise" else stack(f, rng, voices=3, cents=12, shape=str(shape))
        note *= np.exp(-np.arange(m) / (0.5 * step))
        x[t: t + m] += note * (0.15 + 0.85 * p)
        t += step
    return tv_band(x, None, glide(800.0, 15000.0, c)), "pulse"


def rise_swell(n, beat, rng):
    """A reversed reverb on a chord or noise burst: the backward swell."""
    src = np.zeros(max(n, int(0.3 * FX_SR)), np.float32)
    m = int(rng.uniform(0.05, 0.4) * FX_SR)
    f = np.full(m, glide(100.0, 1000.0, rng.uniform()), np.float32)
    src[:m] = (stack(f, rng) if rng.uniform() < 0.5 else noise(m, rng)) * np.hanning(m)
    t60 = n / FX_SR * rng.uniform(0.9, 1.4)
    y = reverb(src[:m], rng, t60=t60, wet=1.0)[:n]
    y = np.pad(y, (0, n - len(y)))[::-1]
    return np.asarray(y, np.float32), "swell"


def rise_shepard(n, beat, rng):
    """Octave-stacked rising tones, each fading in and out: climbs without running out."""
    k = int(rng.integers(3, 7))
    span = rng.uniform(1.0, 3.5)
    ctr = np.linspace(0, 1, n, dtype=np.float32)
    base = rng.uniform(60, 160)
    x = np.zeros(n, np.float32)
    for j in range(k):
        pos = (j / k + span * ctr / k) % 1
        f = base * 2 ** (pos * k)
        x += osc(f, "sine", rng) * np.sin(np.pi * pos) ** 2
    return x * _level(n, rng, rng.uniform(-25, -3)), "shepard"


RISERS = [(rise_noise, 5), (rise_pitch, 3), (rise_chord, 3), (rise_sine, 1), (rise_pulse, 2), (rise_swell, 2), (rise_shepard, 1)]


def _pick(table, rng):
    fns, w = zip(*table)
    return fns[rng.choice(len(fns), p=np.array(w) / sum(w))]


def riser(rng, beat, bars=None):
    bars = bars or float(rng.choice([1, 2, 4, 8, 16], p=[0.12, 0.25, 0.35, 0.2, 0.08]))
    n = int(min(bars * 4 * beat, 42.0) * FX_SR)
    fn = _pick(RISERS, rng)
    x, fam = fn(n, beat, rng)
    if rng.uniform() < 0.3:
        x = gate(x, beat, rng)
    if rng.uniform() < 0.3:
        x = sidechain(x, beat, rng)
    x = fade(x)
    y = chain(x, beat, rng)
    if rng.uniform() < 0.25:  # some are cut dead on the bar line
        y = y[:n]
    return Effect("riser", "riser_" + fam, norm(y), hit=n, align="end", a=0, b=n)


# ---- falling sounds

def down_noise(n, beat, rng):
    x, _ = rise_noise(n, beat, rng)
    return x[::-1].copy(), "noise"


def down_pitch(n, beat, rng):
    x, _ = rise_pitch(n, beat, rng)
    return x[::-1].copy(), "pitch"


def down_sub(n, beat, rng):
    c = curve(n, rng, 0.3, 1.2)
    f = glide(float(rng.uniform(70, 300)), float(rng.uniform(22, 45)), c)
    x = osc(f, "sine", rng)
    if rng.uniform() < 0.4:
        x = np.tanh(x * rng.uniform(1.5, 5))
    env = np.exp(-rng.uniform(0.5, 4) * np.linspace(0, 1, n)) * np.minimum(1, np.arange(n) / 200)
    return (x * env).astype(np.float32), "subdrop"


def down_sine(n, beat, rng):
    x, _ = rise_sine(n, beat, rng)
    return x[::-1].copy(), "sine"


def down_chord(n, beat, rng):
    x, _ = rise_chord(n, beat, rng)
    return x[::-1].copy(), "chord"


DOWNS = [(down_noise, 5), (down_pitch, 2), (down_sub, 3), (down_sine, 1), (down_chord, 1)]


def downlifter(rng, beat, bars=None):
    bars = bars or float(rng.choice([0.5, 1, 2, 4], p=[0.2, 0.35, 0.3, 0.15]))
    n = max(int(bars * 4 * beat * FX_SR), int(0.5 * FX_SR))
    n = min(n, int(20 * FX_SR))
    x, fam = _pick(DOWNS, rng)(n, beat, rng)
    if rng.uniform() < 0.15:
        x = sidechain(x, beat, rng)
    y = chain(fade(x), beat, rng)
    return Effect("downlifter", "down_" + fam, norm(y), hit=0, align="start", a=0, b=n)


# ---- impacts

def _sub_boom(n, rng):
    c = np.minimum(np.arange(n) / (rng.uniform(0.03, 0.25) * FX_SR), 1.0)
    f = glide(float(rng.uniform(90, 350)), float(rng.uniform(28, 60)), c ** 0.5)
    env = np.exp(-np.arange(n) / (rng.uniform(0.15, 1.2) * FX_SR))
    x = osc(f, "sine", rng) * env
    return np.tanh(x * rng.uniform(1, 4)).astype(np.float32)


def _burst(n, rng):
    x = noise(n, rng)
    env = np.exp(-np.arange(n) / (rng.uniform(0.04, 0.6) * FX_SR))
    cut = glide(float(rng.uniform(3000, 15000)), float(rng.uniform(150, 1500)), np.minimum(np.arange(n) / (rng.uniform(0.1, 1.0) * FX_SR), 1.0))
    return tv_band(x * env.astype(np.float32), None, cut)


def _metal(n, rng):
    """Inharmonic partials: clang, gong, plate."""
    base = glide(120.0, 900.0, rng.uniform())
    k = int(rng.integers(4, 14))
    x = np.zeros(n, np.float32)
    t = np.arange(n) / FX_SR
    for _ in range(k):
        f = base * rng.uniform(1, 9)
        x += (np.sin(2 * np.pi * f * t + rng.uniform(0, 6.28)) * np.exp(-t / rng.uniform(0.05, 1.5))).astype(np.float32)
    return x / np.sqrt(k)


def cymbal(n, rng, tau=None):
    """A synthetic crash: noise and many high partials, a fast attack and a long decay."""
    t = np.arange(n) / FX_SR
    tau = tau if tau is not None else rng.uniform(0.25, 1.4)
    x = noise(n, rng, "white") * rng.uniform(0.5, 1.0)
    for _ in range(int(rng.integers(10, 40))):
        f = rng.uniform(1500, 12000)
        x = x + 0.08 * np.sin(2 * np.pi * f * t + rng.uniform(0, 6.28)).astype(np.float32)
    x = tv_band(x.astype(np.float32), np.full(n, rng.uniform(600, 3000), np.float32), None)
    return (x * np.exp(-t / tau)).astype(np.float32)


def impact(rng, beat):
    n = int(rng.uniform(0.6, 5.0) * FX_SR)
    layers = rng.choice(["boom", "boom+burst", "boom+burst+cym", "boom+cym", "burst", "metal", "boom+metal", "cym+burst", "explosion", "snare"],
                        p=[0.16, 0.16, 0.18, 0.12, 0.05, 0.06, 0.08, 0.07, 0.07, 0.05])
    x = np.zeros(n, np.float32)
    if "boom" in layers:
        x += _sub_boom(n, rng) * rng.uniform(0.6, 1.0)
    if "burst" in layers:
        x += _burst(n, rng) * rng.uniform(0.3, 1.0)
    if "metal" in layers:
        x += _metal(n, rng) * rng.uniform(0.3, 1.0)
    if "cym" in layers:
        x += cymbal(n, rng) * rng.uniform(0.3, 1.0)
    if layers == "explosion":
        x = tv_band(noise(n, rng, "brown"), None, glide(6000.0, 150.0, curve(n, rng, 0.2, 0.6))) * np.exp(-np.arange(n) / (rng.uniform(0.3, 1.5) * FX_SR)).astype(np.float32)
    if layers == "snare":
        m = int(0.25 * FX_SR)
        s = noise(m, rng) * np.exp(-np.arange(m) / (0.05 * FX_SR)) + 0.8 * osc(np.full(m, rng.uniform(150, 260), np.float32), "tri", rng) * np.exp(-np.arange(m) / (0.04 * FX_SR))
        x[:m] += s.astype(np.float32)
        x = reverb(x[:m + 2000], rng, t60=rng.uniform(0.8, 3.5), wet=rng.uniform(0.4, 0.8))
    if rng.uniform() < 0.3:
        x = sidechain(x, beat, rng) if rng.uniform() < 0.2 else np.tanh(x * rng.uniform(1, 3))
    y = chain(np.asarray(x, np.float32), beat, rng)
    env = np.abs(y)
    ref = max(env[: int(0.1 * FX_SR)].max(), 1e-6)
    block = int(0.02 * FX_SR)
    e = np.array([env[i: i + block].max() for i in range(0, min(len(env), int(1.2 * FX_SR)), block)])
    below = np.nonzero(e < ref * 0.3)[0]
    b = int(np.clip(((below[0] + 1) * block) if len(below) else 0.6 * FX_SR, 0.16 * FX_SR, 0.8 * FX_SR))
    return Effect("impact", "impact_" + str(layers), norm(fade(y, 1)), hit=0, align="start", a=0, b=b)


# ---- horns and sirens

def _blast(n, rng, base, big):
    """One horn blast: a stack of slightly inharmonic reedy partials, rough, slightly falling at the end."""
    c = np.linspace(0, 1, n, dtype=np.float32)
    droop = 2 ** (-rng.uniform(0, 1.2) * np.maximum(c - 0.7, 0) / 0.3 / 12)
    bend = 2 ** (-rng.uniform(0, 1.0) * np.exp(-c * n / (0.04 * FX_SR)) / 12)
    x = np.zeros(n, np.float32)
    k = int(rng.integers(2, 6)) if not big else int(rng.integers(3, 8))
    ratios = rng.uniform(0.985, 1.015, k) * rng.choice([1, 1, 1.25, 1.5, 2, 2.5, 3], k)
    for r in ratios:
        x += osc(base * r * droop * bend * np.ones(n, np.float32), rng.choice(["saw", "square"]), rng) / np.sqrt(k)
    rough = 1 + rng.uniform(0, 0.5) * np.sin(2 * np.pi * rng.uniform(20, 70) * np.arange(n) / FX_SR)
    env = np.minimum(np.arange(n) / (0.012 * FX_SR), 1) * np.minimum((n - np.arange(n)) / (0.03 * FX_SR), 1)
    return np.tanh(x * rough * env * rng.uniform(1, 3)).astype(np.float32)


def horn(rng, beat):
    """Air horn, stacks of them, a foghorn braam, a brass stab."""
    fam = rng.choice(["air", "air", "air", "fog", "stab"])
    base = {"air": rng.uniform(280, 620), "fog": rng.uniform(70, 180), "stab": rng.uniform(160, 500)}[str(fam)]
    pat = {"single": [(0, 2)], "two": [(0, 0.5), (1, 1.5)], "reggae": [(0, 0.25), (0.5, 0.25), (1, 0.25), (1.5, 1.5)],
           "triplet": [(0, 0.5), (0.75, 0.5), (1.5, 1.5)], "long": [(0, 4)], "dub": [(0, 0.5), (1, 0.5), (2, 0.5), (3, 1)]}
    p = pat[rng.choice(list(pat))]
    n = int((max(a + b for a, b in p)) * beat * FX_SR) + 1
    x = np.zeros(n, np.float32)
    for a, b in p:
        m = int(b * beat * FX_SR * rng.uniform(0.85, 1.0))
        s = int(a * beat * FX_SR)
        x[s: s + m] += _blast(m, rng, base, fam == "fog")[: n - s]
    x = tv_band(x, np.full(n, 60.0, np.float32), np.full(n, rng.uniform(3000, 11000), np.float32))
    y = chain(x, beat, rng)
    return Effect("horn", "horn_" + str(fam), norm(fade(y)), hit=0, align="start", a=0, b=n)


def siren(rng, beat):
    """Wail, yelp, hi-lo, dub siren, rave siren."""
    fam = str(rng.choice(["wail", "yelp", "hilo", "dub", "rave"]))
    bars = float(rng.choice([0.5, 1, 2, 4], p=[0.15, 0.35, 0.35, 0.15]))
    n = max(int(bars * 4 * beat * FX_SR), int(0.8 * FX_SR))
    t = np.arange(n) / FX_SR
    base = glide(300.0, 1500.0, rng.uniform())
    depth = rng.uniform(0.5, 1.6)
    if fam == "wail":
        f = base * 2 ** (depth * np.sin(2 * np.pi * rng.uniform(0.15, 1.2) * t))
    elif fam == "yelp":
        f = base * 2 ** (depth * ((t * rng.uniform(3, 9)) % 1))
    elif fam == "hilo":
        f = base * 2 ** (depth * np.sign(np.sin(2 * np.pi * rng.uniform(0.5, 2.5) * t)))
    elif fam == "dub":
        f = base * 2 ** (depth * ((t * rng.uniform(2, 12)) % 1 - 0.5)) * 2 ** (rng.uniform(-1.5, 1.5) * t / t[-1])
    else:
        f = base * 2 ** (depth * np.sin(2 * np.pi * rng.uniform(1, 8) * t)) * 2 ** (rng.uniform(0, 1.5) * t / t[-1])
    x = stack(np.asarray(f, np.float32), rng, voices=int(rng.integers(1, 4)), cents=rng.uniform(0, 25), shape=rng.choice(["sine", "tri", "square", "saw"]))
    x = tv_band(x, None, np.full(n, rng.uniform(3000, 14000), np.float32))
    x = x * np.minimum(np.arange(n) / (0.02 * FX_SR), 1) * np.minimum((n - np.arange(n)) / (0.05 * FX_SR), 1)
    if fam == "dub" and rng.uniform() < 0.7:
        x = delay(x.astype(np.float32), beat, rng)
    y = chain(np.asarray(x, np.float32), beat, rng)
    return Effect("siren", "siren_" + fam, norm(y), hit=0, align="start", a=0, b=n)


def revcym(rng, beat):
    dur = float(np.clip(rng.choice([0.5, 1, 2, 4, 8]) * beat * rng.uniform(0.9, 1.1), 0.25, 6.0))
    n = int(dur * FX_SR)
    x = cymbal(n, rng, tau=dur * rng.uniform(0.12, 0.4))[::-1].copy()  # a real swell, however short
    y = x
    if rng.uniform() < 0.5:
        y = eq(y, rng)
    if rng.uniform() < 0.3:
        y = np.concatenate([y, cymbal(int(0.3 * FX_SR), rng)[: int(0.3 * FX_SR)] * rng.uniform(0.2, 0.6)])  # the hit it leads into
    return Effect("revcym", "revcym", norm(fade(y, 1)), hit=n, align="end", a=0, b=n)


# ---- things that are not effects: without these the head hears "any strange layer"

def d_snare(rng, beat):
    m = int(0.25 * FX_SR)
    s = noise(m, rng) * np.exp(-np.arange(m) / (rng.uniform(0.02, 0.09) * FX_SR)) + osc(np.full(m, rng.uniform(150, 260), np.float32), "tri", rng) * np.exp(-np.arange(m) / (0.04 * FX_SR))
    return np.asarray(s, np.float32), "snare"


def d_roll(rng, beat):
    """A drum roll speeding up: it belongs to builds but isn't a sweep."""
    bars = float(rng.choice([0.5, 1, 2, 4]))
    n = int(bars * 4 * beat * FX_SR)
    x, t = np.zeros(n, np.float32), 0
    while t < n:
        p = t / n
        div = 2.0 ** np.round(np.interp(p, [0, 1], [rng.choice([1, 2]), rng.choice([3, 4])]))
        step = int(beat * FX_SR / div)
        s, _ = d_snare(rng, beat)
        m = min(len(s), n - t)
        x[t: t + m] += s[:m] * (0.4 + 0.6 * p)
        t += step
    return x, "roll"


def d_pad(rng, beat):
    """A steady pad or noise bed: no rise, no fall."""
    n = int(float(rng.choice([1, 2, 4])) * 4 * beat * FX_SR)
    if rng.uniform() < 0.5:
        x = noise(n, rng)
        x = tv_band(x, np.full(n, rng.uniform(300, 3000), np.float32), np.full(n, rng.uniform(4000, 12000), np.float32))
    else:
        f = np.full(n, glide(80.0, 600.0, rng.uniform()), np.float32)
        x = stack(f, rng, voices=5, cents=20)
        x = tv_band(x, None, np.full(n, rng.uniform(800, 6000), np.float32))
    return fade(x, 200), "pad"


def d_wobble(rng, beat):
    """A filter going up and down: bass wobble or an LFO lead, not one sweep."""
    n = int(float(rng.choice([1, 2, 4])) * 4 * beat * FX_SR)
    f0 = np.full(n, glide(50.0, 300.0, rng.uniform()), np.float32)
    x = stack(f0, rng, voices=3, cents=15, shape="saw")
    t = np.arange(n) / FX_SR
    lfo = 0.5 + 0.5 * np.sin(2 * np.pi * rng.choice([0.25, 0.5, 1, 2, 4]) / beat * t)
    return tv_band(x, None, glide(200.0, 8000.0, lfo.astype(np.float32)), res=rng.choice([0, 3, 6])), "wobble"


def d_arp(rng, beat):
    """Notes going up and down: a melody climbing isn't a riser."""
    n = int(float(rng.choice([1, 2, 4])) * 4 * beat * FX_SR)
    step = int(beat / float(rng.choice([2, 4])) * FX_SR)
    root = glide(150.0, 700.0, rng.uniform())
    scale = rng.choice([0, 2, 3, 5, 7, 10, 12, 14], size=int(rng.integers(3, 7)))
    x = np.zeros(n, np.float32)
    shape = str(rng.choice(["saw", "square", "tri"]))
    for i, t in enumerate(range(0, n, step)):
        m = min(step, n - t)
        f = np.full(m, root * 2 ** (int(scale[i % len(scale)]) / 12), np.float32)
        x[t: t + m] += stack(f, rng, voices=2, cents=8, shape=shape) * np.exp(-np.arange(m) / (0.4 * step))
    return tv_band(x, None, np.full(n, rng.uniform(1500, 9000), np.float32)), "arp"


DISTRACTORS = [d_snare, d_roll, d_pad, d_wobble, d_arp]


def distractor(rng, beat):
    x, fam = DISTRACTORS[int(rng.integers(len(DISTRACTORS)))](rng, beat)
    return Effect("none", "none_" + fam, norm(chain(np.asarray(x, np.float32), beat, rng)), hit=0, align="start", a=0, b=0)


MAKERS = {"riser": riser, "downlifter": downlifter, "impact": impact, "horn": horn, "siren": siren, "revcym": revcym}


def make(kind, rng, beat):
    return MAKERS[kind](rng, beat)
