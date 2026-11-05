"use strict";
// Headless runs of the show logic (core.cjs, the pure part of club.js) under node.
//   node harness.cjs structure                 -> structure/<id>.json: what the rules chose and the features per segment/drop
//   node harness.cjs check <variant> [ids...]  -> checks/<variant>.json: hard-rule checks and descriptive numbers per song
//   node harness.cjs bars <variant> [ids...]   -> bars/<id>.<variant>.json: a per-bar table of the show and the music
// A variant is a folder plans/<variant>/<id>.json; "rules" means no plan at all.
const fs = require("fs");
const path = require("path");
const { prepare, showAt, beatPos, PAIRS } = require("./core.cjs");

const HERE = __dirname, CACHE = path.join(HERE, "cache");

function load(id, plan) {
  const doc = JSON.parse(fs.readFileSync(path.join(CACHE, `${id}.json`)));
  for (const v of Object.values(doc.notes)) v.sort((a, b) => a[0] - b[0]);
  doc.energyData = {}; doc.energyNorm = {};
  for (const [k, v] of Object.entries(doc.energy)) {
    const e = new Uint8Array(Buffer.from(v, "base64"));
    doc.energyData[k] = e;
    const sorted = Array.from(e).sort((a, b) => a - b);
    doc.energyNorm[k] = [sorted[Math.floor(sorted.length * 0.2)], sorted[Math.floor(sorted.length * 0.995)]];
  }
  doc.plan = plan || null;
  return doc;
}
const ids = () => fs.readdirSync(CACHE).filter((f) => f.endsWith(".json")).map((f) => f.slice(0, -5)).sort();
const mean = (a) => (a.length ? a.reduce((x, y) => x + y, 0) / a.length : 0);

// features per segment and drop, sampled per sixteenth, for the plan generators
function structure(id) {
  const song = load(id), P = prepare(song), dur = song.duration;
  const segEnd = (i) => (i + 1 < P.segStarts.length ? P.segStarts[i + 1] : dur);
  const beatDur = 60 / P.bpm;
  const segs = P.segStarts.map((start, i) => {
    const a = Math.max(0, start), b = Math.min(dur, segEnd(i));
    const stats = { n: 0, energetic: 0, mix: 0, drums: 0, bass: 0, other: 0, vocals: 0, period: 0 };
    const kinds = {}, parts = {}, hooks = {};
    for (let t = a + 0.01; t < b; t += beatDur / 4) {
      const S = showAt(P, t);
      stats.n++; stats.energetic += S.energetic ? 1 : 0; stats.period += S.period;
      for (const k of ["mix", "drums", "bass", "other", "vocals"]) stats[k] += S.energy[k];
      kinds[S.kind] = (kinds[S.kind] || 0) + 1; parts[S.part] = (parts[S.part] || 0) + 1; if (S.hook >= 0) hooks[S.hook] = (hooks[S.hook] || 0) + 1;
    }
    const top = (o) => Object.entries(o).sort((x, y) => y[1] - x[1])[0]?.[0] ?? null;
    const n = Math.max(1, stats.n);
    const count = (set) => (set && set.times ? set.times.filter((x) => x >= a && x < b).length : 0);
    const notes = P.line.filter((x) => x[0] >= a && x[0] < b);
    const seq = notes.map((x) => x[2]), sorted = [...seq].sort((x, y) => x - y);
    let stepwise = 0; for (let j = 1; j < seq.length; j++) stepwise += Math.abs(seq[j] - seq[j - 1]) <= 2;
    const bars = (b - a) / (4 * beatDur);
    return { i, start: round(start), end: round(b), bars: round(bars), kind: top(kinds), part: top(parts) === null ? -1 : +top(parts), hook: top(hooks) === null ? -1 : +top(hooks),
      energetic: round(stats.energetic / n), mix: round(stats.mix / n), drums: round(stats.drums / n), bass: round(stats.bass / n), other: round(stats.other / n), vocals: round(stats.vocals / n),
      hits_per_bar: round(count(P.hits) / Math.max(0.25, bars)), hats_per_bar: round(count(P.hats) / Math.max(0.25, bars)), stabs_per_bar: round(count(P.stabs) / Math.max(0.25, bars)),
      notes_per_bar: round(notes.length / Math.max(0.25, bars)), pitch_range: seq.length >= 4 ? sorted[Math.floor(seq.length * 0.9)] - sorted[Math.floor(seq.length * 0.1)] : 0,
      stepwise: seq.length >= 2 ? round(stepwise / (seq.length - 1)) : 0, period: round(stats.period / n), rule_anim: P.segAnim[i], is_drop: P.drops.some((d) => Math.abs(d.time - start) < 0.01) };
  });
  const ED = song.energyData;
  const low = (a, b) => { const fa = Math.floor(a * 100), fb = Math.max(fa + 1, Math.floor(b * 100)); let p = 0; for (let f = fa; f < fb; f++) p += 10 ** (((ED.drums[f] || 0) / 255 * 60 - 60) / 10) + 10 ** (((ED.bass[f] || 0) / 255 * 60 - 60) / 10); return 10 * Math.log10(p / (fb - fa) + 1e-9); };
  const drops = P.drops.map((d, i) => ({ i, time: d.time, breakdown_bars: d.breakdown_bars || 0, build_bars: round((d.time - d.build_start) / (4 * beatDur)), build_source: d.build_source || "sections",
    audio_blackout: d.blackout_start != null, rule_type: d.strobeType, rule_beats: d.strobeBeats, low_jump_db: round(low(d.time, d.time + 4 * beatDur) - low(d.time - 8 * beatDur, d.time - beatDur)), part: (showAt(P, d.time + 0.01).part),
    snare_rate: (() => { const sn = (P.cues.hits || {}).snare || []; return sn.filter((h) => h[0] > d.time - 4 * beatDur && h[0] < d.time).length; })() }));
  return { id, name: song.name, duration: dur, bpm: P.bpm, key: song.key, rule_palette: PAIRS.findIndex((p) => p.name === P.palette.name), segStarts: P.segStarts, hooks: P.hookAnim.map((a, i) => [i, a]).filter((x) => x[1]),
    preset: P.cues.preset || {}, tags: song.tags, segs, drops };
}
const round = (x) => Math.round(x * 1000) / 1000;

// per-song hard-rule checks at 50 fps, and the numbers the report needs
function check(id, plan) {
  const song = load(id, plan), P = prepare(song), dt = 0.02, dur = song.duration, beatDur = 60 / P.bpm;
  const V = { strobe_outside_window: 0, strobe_over_a_bar: 0, strobe_not_dark: 0, bad_period: 0, step_off_grid: 0, palette: 0, bad_type: 0, blackout_during_strobe_missing: 0 };
  const palName = P.palette.name;
  if (!PAIRS.some((p) => p.name === palName)) V.palette++;
  let prevStep = null, stepN = 0, strobeFrames = 0;
  // proxy brightness of the ceiling and pulses, for a beat-lock number comparable to the Skip-BART pilot
  const proxy = [];
  for (let t = 0; t < dur; t += dt) {
    const S = showAt(P, t);
    if (S.strobe > 0) {
      strobeFrames++;
      const d = S.prev;
      if (!d || t < d.time - 1e-6 || S.sinceDrop >= d.strobeBeats + 1e-6) V.strobe_outside_window++;
      if (S.sinceDrop >= 4 + 1e-6) V.strobe_over_a_bar++;
      if (!S.strobing) V.strobe_not_dark++;  // applyShow: dark = blackout || strobing -> every other light off
      if (!["blinder", "spin", "pairs", "all"].includes(S.strobeType)) V.bad_type++;
    }
    if (![1, 2, 4, 8].includes(S.period)) V.bad_period++;
    const stepAbs = Math.floor(S.rel * S.sub);
    if (prevStep !== null && stepAbs !== prevStep.abs && S.sub === prevStep.sub) {
      stepN++;
      // the first frame after a step: how far past the grid point (in seconds); a pure step on the note grid is under one frame
      const localBeat = P.beats[Math.min(P.beats.length - 1, S.idx + 1)] - P.beats[Math.max(0, S.idx)] || beatDur;
      const off = (S.rel * S.sub - stepAbs) / S.sub * localBeat;
      if (off > dt + 1e-3) V.step_off_grid++;
    }
    prevStep = { abs: stepAbs, sub: S.sub };
    const b = S.strobe > 0 ? S.strobe : S.blackout ? 0 : S.level * Math.min(1, 0.12 + 0.45 * S.stepEnv + 0.6 * Math.max(S.hit, 0.8 * S.stab));
    proxy.push(b);
  }
  // phase lock exactly as in the Skip-BART pilot (analyze/pilots/skip_bart.py, locking): the proxy minus a 2 s
  // moving average, |mean(d exp(i 2 pi beat position x detune))| / std(d), against detuned tempi
  const N = proxy.length, win = Math.round(2 / dt) + 1, half = (win - 1) / 2, d = new Float64Array(N);
  let run = 0; const cum = new Float64Array(N + 1);
  for (let i = 0; i < N; i++) { run += proxy[i]; cum[i + 1] = run; }
  for (let i = 0; i < N; i++) { const lo = Math.max(0, i - half), hi = Math.min(N, i + half + 1); d[i] = proxy[i] - (cum[hi] - cum[lo]) / (hi - lo); }
  const dm = mean(Array.from(d)), sd = Math.sqrt(mean(Array.from(d, (x) => (x - dm) ** 2))) + 1e-12;
  const strength = (f) => { let re = 0, im = 0, n = 0; for (let i = 0; i < N; i++) { const t = i * dt; if (t < P.beats[0] || t > P.beats[P.beats.length - 1]) continue; const ph = 2 * Math.PI * beatPos(P, t) * f; re += d[i] * Math.cos(ph); im += d[i] * Math.sin(ph); n++; } return Math.hypot(re, im) / Math.max(1, n) / sd; };
  const detuned = [0.8, 0.85, 0.9, 0.93, 0.96, 1.04, 1.07, 1.1, 1.15, 1.2].map(strength);
  return { id, violations: V, strobe_frames: strobeFrames, steps: stepN, palette: palName, phase_lock: round(strength(1)), phase_lock_detuned: round(mean(detuned)),
    drops: P.drops.map((d) => ({ type: d.strobeType, beats: d.strobeBeats, rate: d.strobeRate || 4 })) };
}

// per-bar table of the show and the music, for the comparison with human lighting (bs_compare.py)
function bars(id, plan) {
  const song = load(id, plan), P = prepare(song), dur = song.duration;
  const downs = Array.from(P.beats).filter((_, i) => P.inBar[i] === 1);
  const out = [];
  const count = (set, a, b) => (set && set.times ? set.times.filter((x) => x >= a && x < b).length : 0);
  for (let i = 0; i + 1 < downs.length; i++) {
    const a = downs[i], b = downs[i + 1];
    if (b > dur) break;
    const n = 16, kinds = {};
    const q = { level: 0, bright: 0, drums: 0, bass: 0, other: 0, vocals: 0, mix: 0, energetic: 0, build: 0 };
    let part = -1, hook = -1, stepChanges = 0, prev = null;
    for (let k = 0; k < n; k++) {
      const t = a + (b - a) * (k + 0.5) / n, S = showAt(P, t);
      const bright = S.strobe > 0 ? S.strobe : S.blackout ? 0 : S.level * Math.min(1, 0.12 + 0.45 * S.stepEnv + 0.6 * Math.max(S.hit, 0.8 * S.stab));
      q.level += S.level / n; q.bright += bright / n; q.energetic += (S.energetic ? 1 : 0) / n; q.build += (S.inBuild ? 1 : 0) / n;
      for (const x of ["drums", "bass", "other", "vocals", "mix"]) q[x] += S.energy[x] / n;
      kinds[S.kind] = (kinds[S.kind] || 0) + 1; part = S.part; hook = S.hook;
      const key = S.step + ":" + S.period; if (prev !== null && key !== prev) stepChanges++; prev = key;
    }
    const next = P.drops.find((d) => d.time >= a), last = [...P.drops].reverse().find((d) => d.time < b);
    const segStart = P.segStarts.some((x) => x >= a - 0.02 && x < b - 0.02);
    const dropHere = P.drops.some((d) => d.time >= a - 0.05 && d.time < b - 0.05);
    const barDur = b - a;
    const notes = P.line.filter((x) => x[0] >= a && x[0] < b).length;
    const out1 = { t0: round(a), t1: round(b), kind: Object.entries(kinds).sort((x, y) => y[1] - x[1])[0][0], part, hook, seg_start: segStart, drop: dropHere, hits: count(P.hits, a, b), hats: count(P.hats, a, b), stabs: count(P.stabs, a, b), impacts: count(P.impacts, a, b), notes,
      since_drop: last ? round((a - last.time) / barDur) : 999, to_drop: next ? round((next.time - a) / barDur) : 999, steps: stepChanges };
    for (const [k, v] of Object.entries(q)) out1[k] = round(v);
    out.push(out1);
  }
  return out;
}

// the rhythm of the show after each drop: per 16th of the bar, summed over bars 2-4 (bar 1 is the strobe), the flash a frame after the
// step and the kick pulse alone; for the comparison with what human lighters do in the same bars (bs_rhythm.py)
function rhythm(id, plan) {
  const song = load(id, plan), P = prepare(song);
  const downs = Array.from(P.beats).filter((_, i) => P.inBar[i] === 1);
  const out = [];
  for (const d of P.drops) {
    let i = downs.findIndex((x) => Math.abs(x - d.time) < 0.1);
    if (i < 0) continue;
    if (i + 4 >= downs.length) continue;
    const show = new Array(16).fill(0), kick = new Array(16).fill(0);
    for (let b = i + 1; b < i + 4; b++) for (let k = 0; k < 16; k++) {
      const t = downs[b] + (downs[b + 1] - downs[b]) * k / 16 + 0.015, S = showAt(P, t);
      if (S.strobe > 0) continue;
      show[k] += 0.45 * S.stepEnv * (S.energetic ? 1 : 0.3) + 0.6 * Math.max(S.hit, 0.8 * S.stab);
      kick[k] += S.hit;
    }
    out.push({ t0: downs[i], t1: downs[i + 4], bar: i, show, kick });
  }
  return out;
}

const [cmd, ...rest] = process.argv.slice(2);
if (cmd === "structure") {
  const out = path.join(HERE, "structure"); fs.mkdirSync(out, { recursive: true });
  const list = rest.length ? rest : ids();
  for (const id of list) fs.writeFileSync(path.join(out, `${id}.json`), JSON.stringify(structure(id)));
  console.log("structure:", list.length, "songs");
} else if (cmd === "bars") {
  const out = path.join(HERE, "bars"); fs.mkdirSync(out, { recursive: true });
  const [variant, ...only] = rest;
  const list = only.length ? only : ids();
  for (const id of list) {
    const planFile = path.join(HERE, "plans", variant, `${id}.json`);
    fs.writeFileSync(path.join(out, `${id}.${variant}.json`), JSON.stringify(bars(id, variant === "rules" ? null : JSON.parse(fs.readFileSync(planFile)))));
  }
  console.log("bars:", variant, list.length);
} else if (cmd === "rhythm") {
  const out = path.join(HERE, "bars"); fs.mkdirSync(out, { recursive: true });
  const [variant, ...only] = rest;
  const list = only.length ? only : ids();
  for (const id of list) {
    const planFile = path.join(HERE, "plans", variant, `${id}.json`);
    if (variant !== "rules" && !fs.existsSync(planFile)) continue;
    fs.writeFileSync(path.join(out, `${id}.${variant}.rhythm.json`), JSON.stringify(rhythm(id, variant === "rules" ? null : JSON.parse(fs.readFileSync(planFile)))));
  }
  console.log("rhythm:", variant);
} else if (cmd === "check") {
  const [variant, ...only] = rest;
  const dir = path.join(HERE, "plans", variant);
  const list = only.length ? only : variant === "rules" ? ids() : fs.readdirSync(dir).filter((f) => f.endsWith(".json")).map((f) => f.slice(0, -5)).sort();
  const res = [];
  for (const id of list) res.push(check(id, variant === "rules" ? null : JSON.parse(fs.readFileSync(path.join(dir, `${id}.json`)))));
  fs.mkdirSync(path.join(HERE, "checks"), { recursive: true });
  fs.writeFileSync(path.join(HERE, "checks", `${variant}.json`), JSON.stringify(res));
  const tot = {}; for (const r of res) for (const [k, v] of Object.entries(r.violations)) tot[k] = (tot[k] || 0) + v;
  const med = (a) => [...a].sort((x, y) => x - y)[Math.floor(a.length / 2)];
  console.log(variant, "songs", res.length, "violations", JSON.stringify(tot), "steps", res.reduce((s, r) => s + r.steps, 0), "strobe frames", res.reduce((s, r) => s + r.strobe_frames, 0),
    "phase lock median", med(res.map((r) => r.phase_lock)), "detuned", med(res.map((r) => r.phase_lock_detuned)));
} else {
  console.error("usage: node harness.cjs structure | check <variant> [ids]");
  process.exit(2);
}
