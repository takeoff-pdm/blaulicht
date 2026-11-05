// Every look and toggle, under node, on the desk club.js draws (viz/club_looks.mjs and core.cjs from build_club.py).
//
//   node audio-model/melody/decision/check_looks.mjs [--songs N] [--fps F] [--shard i/n] [--out file.json]
//   node audio-model/melody/decision/check_looks.mjs --report a.json b.json ...
//
// Hard rules, counted per frame: no NaN; strobes only in [drop, drop + its length), up to a bar, and then nothing else lit;
// a rope flash is at most two ropes and 30% of each; every fixture in the song's colour pair or white. The rest is
// descriptive: how many lamps are lit, how dark the room is, whether the light is locked to the beat, whether it goes
// with or against the kick, and how far each look is from Club. Nobody has judged by eye whether any of it looks good.

import fs from "fs";
import path from "path";
import { createRequire } from "module";
import { fileURLToPath } from "url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const core = require("./core.cjs");
const looks = await import("../viz/club_looks.mjs");
const FEATURES = path.join(HERE, "../../data/melody/features");
const { prepare, showAt, beatPos, opts } = core;
const { frame, makeDesk, LOOK_LIST, burnCount, ROPES, ROPE_PIX } = looks;

function load(id) {
  const doc = JSON.parse(fs.readFileSync(path.join(HERE, "cache", `${id}.json`)));
  for (const v of Object.values(doc.notes)) v.sort((a, b) => a[0] - b[0]);
  doc.energyData = {}; doc.energyNorm = {};
  for (const [k, v] of Object.entries(doc.energy)) {
    const e = new Uint8Array(Buffer.from(v, "base64"));
    doc.energyData[k] = e;
    const sorted = Array.from(e).sort((a, b) => a - b);
    doc.energyNorm[k] = [sorted[Math.floor(sorted.length * 0.2)], sorted[Math.floor(sorted.length * 0.995)]];
  }
  doc.features = {};
  for (const name of ["timbre", "vocals", "drumparts"]) {
    const p = path.join(FEATURES, name, `${id}.json`);
    if (fs.existsSync(p)) doc.features[name] = JSON.parse(fs.readFileSync(p));
  }
  return doc;
}
const lit = (D) => {  // total light per fixture group, 0..1-ish
  let ce = 0; for (const L of D.ceil) ce += L.b; ce /= D.ceil.length;
  let ro = 0; for (const px of D.ropes) for (let i = 0; i < px.length; i += 3) ro += Math.max(px[i], px[i + 1], px[i + 2]); ro /= ROPES * ROPE_PIX;
  const up = (D.ups[0].b + D.ups[1].b + D.ups[2].b + D.ups[3].b) / 4, sp = (D.spill[0].b + D.spill[1].b) / 2, co = (D.cove[0].b + D.cove[1].b) / 2;
  return { ce, ro, up, sp, co, st: D.strip.b, dj: D.spot };
};
const hueOf = (r, g, b) => { const M = Math.max(r, g, b), m = Math.min(r, g, b), d = M - m; if (d <= 0) return { h: 0, s: 0 }; const s = d / M; let h = M === r ? ((g - b) / d) % 6 : M === g ? (b - r) / d + 2 : (r - g) / d + 4; return { h: (h * 60 + 360) % 360, s }; };
const hueDist = (a, b) => { const d = Math.abs(a - b) % 360; return d > 180 ? 360 - d : d; };
const mean = (a) => (a.length ? a.reduce((x, y) => x + y, 0) / a.length : 0);
const corr = (a, b) => { const ma = mean(a), mb = mean(b); let n = 0, x = 0, y = 0; for (let i = 0; i < a.length; i++) { n += (a[i] - ma) * (b[i] - mb); x += (a[i] - ma) ** 2; y += (b[i] - mb) ** 2; } return x > 0 && y > 0 ? n / Math.sqrt(x * y) : 0; };
const median = (a) => { const s = [...a].filter((x) => x != null && Number.isFinite(x)).sort((x, y) => x - y); return s.length ? s[Math.floor(s.length / 2)] : null; };
const pct = (a, q) => { const s = [...a].sort((x, y) => x - y); return s.length ? s[Math.min(s.length - 1, Math.floor(q * s.length))] : 0; };
const round = (x, n = 3) => (x == null ? null : Math.round(x * 10 ** n) / 10 ** n);

function detrend(a, dt) {  // minus a 1 s moving average: what moves within the beat, not the song's loudness
  const half = Math.round(0.5 / dt), cum = new Float64Array(a.length + 1), out = new Float64Array(a.length);
  for (let i = 0; i < a.length; i++) cum[i + 1] = cum[i] + a[i];
  for (let i = 0; i < a.length; i++) { const lo = Math.max(0, i - half), hi = Math.min(a.length, i + half + 1); out[i] = a[i] - (cum[hi] - cum[lo]) / (hi - lo); }
  return out;
}
// the same beat-lock number as the Skip-BART pilot and harness.cjs: the signal minus a 2 s moving average, at the beat against detuned tempi
function beatLock(P, series, dt) {
  const N = series.length, half = Math.round(1 / dt), cum = new Float64Array(N + 1);
  for (let i = 0; i < N; i++) cum[i + 1] = cum[i] + series[i];
  const d = new Float64Array(N);
  for (let i = 0; i < N; i++) { const lo = Math.max(0, i - half), hi = Math.min(N, i + half + 1); d[i] = series[i] - (cum[hi] - cum[lo]) / (hi - lo); }
  const dm = mean(d), sd = Math.sqrt(mean(Array.from(d, (x) => (x - dm) ** 2))) + 1e-12;
  const pos = new Float64Array(N);
  for (let i = 0; i < N; i++) pos[i] = beatPos(P, i * dt);
  const strength = (f) => { let re = 0, im = 0; for (let i = 0; i < N; i++) { const ph = 2 * Math.PI * pos[i] * f; re += d[i] * Math.cos(ph); im += d[i] * Math.sin(ph); } return Math.hypot(re, im) / N / sd; };
  // lock is a ratio, so a nearly flat signal can score high: depth is how much of the light level actually moves with the beat
  const lock = strength(1), level = mean(series);
  return { lock, detuned: mean([0.85, 0.93, 1.07, 1.15].map(strength)), depth: level > 0 ? lock * sd / level : 0 };
}

function run(song, look, tog, dt, base) {
  Object.assign(opts, { dejavu: !!tog.dejavu, burn: !!tog.burn, countdown: !!tog.countdown, look });
  const P = prepare(song), D = makeDesk(), dur = song.duration, FLASH = [0.8, 0.8, 0.85];
  const V = { nan: 0, strobe_window: 0, strobe_over_bar: 0, strobe_not_dark: 0, dark_not_black: 0, rope_flash: 0, palette_lamp: 0, palette_rope: 0, vp_outside: 0 };
  const series = [], lampShare = [], kind = {}, kick = [], t0 = process.hrtime.bigint();
  let frames = 0, litFrames = 0, ropePx = 0, ropeOff = 0, blackFrames = 0, nonDark = 0;
  const ok = (h, s, b, pal) => !(b > 0.02) || s < 0.05 || hueDist(h, pal.primary) < 1 || hueDist(h, pal.secondary) < 1;
  let timeInLook = {};
  for (let t = 0; t < dur; t += dt) {
    const S = showAt(P, t), base_ = P.palette, n = (S.hook >= 0 ? S.hook : S.part >= 0 ? S.part : S.seg) + S.rootLead;
    const pal = n % 2 ? { ...base_, primary: base_.secondary, secondary: base_.primary } : base_;
    frame(P, S, D, { look, dejavu: tog.dejavu, burn: tog.burn, countdown: tog.countdown, pal, flash: FLASH });
    frames++;
    const L = lit(D), total = L.ce + L.ro + L.up + 0.5 * L.sp + L.co + 0.5 * L.st + 0.5 * L.dj;
    series.push(D.dark && series.length ? series[series.length - 1] : total); kick.push(Math.max(S.hit, 0));  // strobes aren't the look: hold through them
    for (const v of [L.ce, L.ro, L.up, L.sp, L.co, L.st, L.dj]) if (!Number.isFinite(v) || v < -1e-6) V.nan++;
    if (S.strobe > 0) {
      const d = S.prev;
      if (!d || t < d.time - 1e-6 || S.sinceDrop >= d.strobeBeats + 1e-6) V.strobe_window++;
      if (S.sinceDrop >= 4 + 1e-6) V.strobe_over_bar++;
      if (!D.dark) V.strobe_not_dark++;
    }
    if (D.vp > 0 && !(S.strobing && S.strobe > 0)) V.vp_outside++;
    if (D.dark) {
      if (L.ce + L.up + L.sp + L.co + L.st + L.dj > 0) V.dark_not_black++;
      let ropes = 0, most = 0;
      for (const px of D.ropes) { let c = 0; for (let i = 0; i < px.length; i += 3) if (px[i] > 0.001) c++; if (c) ropes++; most = Math.max(most, c); }
      if (ropes > 2 || most > 34 || (!S.strobing && ropes)) V.rope_flash++;
    } else {
      nonDark++;
      if (total < 0.01) blackFrames++;
      let on = 0;
      for (const l of D.ceil) { if (l.b >= 0.1) on++; if (!ok(l.h, l.s, l.b, pal)) V.palette_lamp++; }
      for (const u of D.ups) if (!ok(u.h, u.s, u.b, pal)) V.palette_lamp++;
      for (const u of D.spill) if (!ok(u.h, u.s, u.b, pal)) V.palette_lamp++;
      for (const u of D.cove) if (!ok(u.h, u.s, u.b, pal)) V.palette_lamp++;
      if (!ok(D.strip.h, 1, D.strip.b, pal)) V.palette_lamp++;
      lampShare.push(on / D.ceil.length);
      for (const px of D.ropes) for (let i = 0; i < px.length; i += 3) {
        const M = Math.max(px[i], px[i + 1], px[i + 2]);
        if (M < 0.03) continue;
        ropePx++;
        const { h, s } = hueOf(px[i], px[i + 1], px[i + 2]);
        if (s > 0.25 && hueDist(h, pal.primary) > 25 && hueDist(h, pal.secondary) > 25) ropeOff++;
      }
      (kind[S.kind] = kind[S.kind] || []).push(total);
      if (D.info && D.info.startsWith("Director")) timeInLook[D.info] = (timeInLook[D.info] || 0) + dt;
    }
  }
  const ms = Number(process.hrtime.bigint() - t0) / 1e6 / frames;
  V.palette_rope = 0;
  const out = { violations: V, ms: round(ms, 4), frames, ropeOffShare: ropePx ? ropeOff / ropePx : 0, lampP95: pct(lampShare, 0.95), lampMean: mean(lampShare), lampOver25: mean(lampShare.map((x) => (x > 0.26 ? 1 : 0))),
    blackShare: nonDark ? blackFrames / nonDark : 0, kickCorr: corr(detrend(series, dt), detrend(kick, dt)), byKind: Object.fromEntries(Object.entries(kind).map(([k, v]) => [k, mean(v)])), timeInLook, weather: P.smoke.filter((e) => e.kind === "weather").length };
  const bl = beatLock(P, series, dt); out.lock = bl.lock; out.detuned = bl.detuned; out.depth = bl.depth;
  if (base) out.clubCorr = corr(series, base.series);
  out.series = series;
  return { out, P };
}

// what each look claims to do, on the desk
function claims(song, dt) {
  const R = {};
  const palOf = (P, S) => { const b = P.palette, n = (S.hook >= 0 ? S.hook : S.part >= 0 ? S.part : S.seg) + S.rootLead; return n % 2 ? { ...b, primary: b.secondary, secondary: b.primary } : b; };
  const FLASH = [1, 1, 1];
  const at = (P, look, t, D, tog = {}) => { const S = showAt(P, t); return { S, D: frame(P, S, D, { look, pal: palOf(P, S), flash: FLASH, ...tog }) }; };
  Object.assign(opts, { dejavu: false, burn: false, countdown: false, look: "score" });
  // Score: half a beat before its onset a note is on the row next to the stage, on the onset on the stage row
  {
    const P = prepare(song), D = makeDesk();
    let n = 0, row1 = 0, row0 = 0, far = 0;
    const lo = P.lineLo, hi = Math.max(P.lineLo + 1, P.lineHi);
    for (const note of P.line) {
      const S = showAt(P, note[0] + 0.002);
      if (S.strobing || S.blackout || S.pos < 8) continue;
      const col = Math.min(3, Math.floor((note[2] - lo) / (hi - lo + 1e-3) * 4)), col_ = Math.max(0, col);
      const before = showAt(P, note[0] - 0.25 * S.beatDur);
      if (before.strobing || before.blackout) continue;
      n++;
      frame(P, before, D, { look: "score", pal: palOf(P, before), flash: FLASH });
      if (D.ceil[1 * 4 + col_].b > 0.05) row1++;
      const b4 = showAt(P, note[0] - 2.9 * S.beatDur);
      if (!b4.strobing && !b4.blackout) { frame(P, b4, D, { look: "score", pal: palOf(P, b4), flash: FLASH }); if (D.ceil[6 * 4 + col_].b > 0.05) far++; }
      frame(P, S, D, { look: "score", pal: palOf(P, S), flash: FLASH });
      if (D.ceil[0 + col_].b > 0.05) row0++;
    }
    R.score = { notes: n, on_row1_quarter_beat_before: n ? row1 / n : null, on_row6_3_beats_before: n ? far / n : null, on_stage_row_at_onset: n ? row0 / n : null };
  }
  // Shockwave: after a kick with nothing around it, the back row peaks later than the front one by about 7/8 of a beat
  {
    const P = prepare(song), D = makeDesk();
    const kicks = ((song.cues || {}).hits || {}).kick || [];
    const delays = [], beat = 60 / P.bpm, backWaves = [...(((song.cues || {}).hits || {}).crash || []).map((h) => h[0]), ...((song.cues || {}).impacts || []).map((h) => h[0])];
    for (let i = 1; i < kicks.length - 1; i += 7) {
      const k = kicks[i][0];
      if (kicks[i + 1][0] - k < beat * 0.9 || k - kicks[i - 1][0] < beat * 0.95) continue;  // alone: an earlier kick's wave would reach the back first
      if (backWaves.some((x) => x > k - 2 * beat && x < k + 1.2 * beat)) continue;  // and so would a crash or impact, which starts from the back
      if (showAt(P, k).strobing || showAt(P, k + beat).strobing) continue;
      let t0 = -1, t7 = -1, b0 = 0, b7 = 0;
      for (let x = 0; x < beat * 1.1; x += 0.002) {
        const { D: Dd, S } = at(P, "shock", k + x, D);
        if (S.blackout) continue;
        if (Dd.ceil[0].b > b0) { b0 = Dd.ceil[0].b; t0 = x; }
        if (Dd.ceil[29].b > b7) { b7 = Dd.ceil[29].b; t7 = x; }  // row 7, column 1: the zigzag leaves column 0 off
      }
      if (t0 >= 0 && t7 >= 0) delays.push((t7 - t0) / beat);
      if (delays.length >= 12) break;
    }
    R.shock = { kicks: delays.length, back_row_delay_beats: median(delays), back_waves_per_minute: backWaves.length / (song.duration / 60) };
  }
  // Déjà Vu: a repeat of a part (and every drop) gets the same pattern at the same place, and the song one strobe type
  {
    Object.assign(opts, { dejavu: true });
    const P = prepare(song), D = makeDesk();
    frame(P, showAt(P, 0.01), D, { look: "club", dejavu: true, pal: P.palette, flash: FLASH });
    const first = new Map();
    let same = 0, n = 0, repeat = 0, whole = 0;
    const beatLen = 60 / P.bpm;
    (P.parts || []).forEach(([a, b, letter]) => {
      whole += b - a;
      const key = P.drops.some((d) => Math.abs(d.time - a) < beatLen) ? "drop" : letter, f = first.get(key);
      if (!f) { first.set(key, [a, b]); return; }
      repeat += b - a;
      const pa = beatPos(P, a), pf = beatPos(P, f[0]), shift = 4 * Math.round((pa - pf) / 4), len = Math.min(beatPos(P, b) - pa, beatPos(P, f[1]) - pf);
      for (let x = 0.5; x < len; x += 2.13) {
        const u = showAt(P, core.beatTime(P, pa + x)), v = showAt(P, core.beatTime(P, pa - shift + x));
        if (u.kind !== v.kind || u.inBuild || v.inBuild) continue;
        n++;
        if (u.anim === v.anim && u.step === v.step && u.period === v.period && u.rootLead === v.rootLead) same++;
      }
    });
    const types = new Set(P.drops.map((d) => d.strobeType));
    R.dejavu = { repeat_share: whole ? repeat / whole : 0, compared: n, same_pattern: n ? same / n : null, strobe_types: types.size, drops: P.drops.length, type: [...types][0] ?? null };
    Object.assign(opts, { dejavu: false });
  }
  // Slow Burn: one group at the start, never all before the biggest drop, all on it; strobes rationed; undone when off
  {
    Object.assign(opts, { burn: true });
    const P = prepare(song), D = makeDesk(), keep = P.drops.map((d) => [d.strobeType, d.strobeBeats]);
    frame(P, showAt(P, 0.01), D, { look: "club", burn: true, pal: P.palette, flash: FLASH });
    const best = P.drops.reduce((m, d, i) => ((d.breakdown_bars || 0) >= (P.drops[m] ? P.drops[m].breakdown_bars || 0 : -1) ? i : m), 0), big = P.drops[best];
    let early = 0, allBefore = 0, typeOk = true;
    for (let t = 0; t < song.duration; t += 0.5) { const n = burnCount(P, t); if (big && t < big.time - 0.01 && n > 5.001) allBefore++; }
    if (burnCount(P, 0.01) !== 1) early++;
    P.drops.forEach((d, i) => { if (i === best ? d.strobeType !== "all" : d.strobeType === "all" || d.strobeType === "blinder" || d.strobeBeats > 2) typeOk = false; });
    const atBig = big ? burnCount(P, big.time + 0.01) : 6;
    frame(P, showAt(P, 0.02), D, { look: "club", burn: false, pal: P.palette, flash: FLASH });
    const undone = P.drops.every((d, i) => d.strobeType === keep[i][0] && d.strobeBeats === keep[i][1]);
    R.burn = { start_not_one: early, all_before_biggest: allBefore, count_on_biggest: atBig, strobes_rationed: typeOk, undone_when_off: undone };
    Object.assign(opts, { burn: false });
  }
  // Countdown: the furthest lit row only comes closer to the stage during a build
  {
    const P = prepare(song), D = makeDesk();
    let builds = 0, backwards = 0;
    for (const d of P.drops) {
      let last = 99, seen = false;
      for (let t = d.build_start; t < d.time; t += 0.05) {
        const S = showAt(P, t);
        if (!S.inBuild || S.blackout || S.strobing) continue;
        if (S.toDrop > Math.min(S.buildBeats, 32)) continue;
        frame(P, S, D, { look: "club", countdown: true, pal: palOf(P, S), flash: FLASH });
        let far = -1; for (const L of D.ceil) if (L.b > 0.01) far = Math.max(far, L.r);
        if (far > last) backwards++;
        last = far; seen = true;
      }
      if (seen) builds++;
    }
    R.countdown = { builds, row_came_back: backwards };
  }
  return R;
}

// a song the analysis gave nothing to: no cues, no features, no notes. Every look must still run and stay finite
function bare(song) {
  const empty = { ...song, cues: {}, features: {}, melody: {}, notes: {}, phrases: [] };
  let crashes = 0, nan = 0;
  for (const { id } of LOOK_LIST) {
    try {
      Object.assign(opts, { dejavu: true, burn: true, countdown: true, look: id });
      const P = prepare(empty), D = makeDesk();
      for (let t = 0; t < song.duration; t += 0.5) {
        const S = showAt(P, t);
        frame(P, S, D, { look: id, dejavu: true, burn: true, countdown: true, pal: P.palette, flash: [1, 1, 1] });
        const L = lit(D);
        if (![L.ce, L.ro, L.up, L.sp, L.co, L.st, L.dj].every(Number.isFinite)) nan++;
      }
    } catch (e) { crashes++; if (crashes === 1) console.error(id, e.stack); }
  }
  Object.assign(opts, { dejavu: false, burn: false, countdown: false, look: "club" });
  return { crashes, nan };
}

const args = process.argv.slice(2);
const opt = (n, d) => { const i = args.indexOf(n); return i >= 0 ? args[i + 1] : d; };

if (args[0] === "--report") {
  const parts = args.slice(1).map((f) => JSON.parse(fs.readFileSync(f)));
  const rows = parts.flatMap((p) => p.rows), claimsAll = parts.flatMap((p) => p.claims);
  const combos = [...new Set(rows.map((r) => r.combo))];
  const tot = {};
  console.log("songs", new Set(rows.map((r) => r.id)).size);
  console.log("combo".padEnd(26), "viol ropeOff% lampP95 lamp>25% black% lock  detuned depth kickCorr clubCorr  drop/breakdown ms/frame");
  for (const c of combos) {
    const R = rows.filter((r) => r.combo === c), v = {};
    for (const r of R) for (const [k, x] of Object.entries(r.violations)) { v[k] = (v[k] || 0) + x; tot[k] = (tot[k] || 0) + x; }
    const sum = Object.values(v).reduce((a, b) => a + b, 0);
    const ratio = median(R.map((r) => (r.byKind.Drop && r.byKind.Breakdown ? r.byKind.Drop / Math.max(1e-3, r.byKind.Breakdown) : null)));
    console.log(c.padEnd(26), String(sum).padStart(4), (100 * mean(R.map((r) => r.ropeOffShare))).toFixed(1).padStart(7), median(R.map((r) => r.lampP95)).toFixed(2).padStart(7), (100 * mean(R.map((r) => r.lampOver25))).toFixed(1).padStart(8), (100 * mean(R.map((r) => r.blackShare))).toFixed(1).padStart(6), median(R.map((r) => r.lock)).toFixed(3).padStart(6), median(R.map((r) => r.detuned)).toFixed(3).padStart(7), median(R.map((r) => r.depth)).toFixed(3).padStart(6), median(R.map((r) => r.kickCorr)).toFixed(2).padStart(8), (median(R.map((r) => r.clubCorr)) ?? 0).toFixed(2).padStart(8), (ratio ?? 0).toFixed(1).padStart(9), median(R.map((r) => r.ms)).toFixed(3).padStart(10));
  }
  console.log("violations", JSON.stringify(tot));
  const t = {}; for (const r of rows.filter((x) => x.combo === "director")) for (const [k, v] of Object.entries(r.timeInLook)) t[k] = (t[k] || 0) + v;
  const all = Object.values(t).reduce((a, b) => a + b, 0) || 1;
  console.log("director time", Object.entries(t).sort((a, b) => b[1] - a[1]).map(([k, v]) => `${k.replace("Director: ", "")} ${(100 * v / all).toFixed(0)}%`).join(", "));
  const m = (f) => median(claimsAll.map(f));
  const sum = (f) => claimsAll.reduce((a, c) => a + (f(c) || 0), 0);
  console.log("score: notes", sum((c) => c.score.notes), "row 1 a quarter beat before", m((c) => c.score.on_row1_quarter_beat_before), "row 6 three beats before", m((c) => c.score.on_row6_3_beats_before), "stage row on the onset", m((c) => c.score.on_stage_row_at_onset));
  console.log("shock: kicks", sum((c) => c.shock.kicks), "back row later by (beats)", m((c) => c.shock.back_row_delay_beats), "waves from the back per minute", m((c) => c.shock.back_waves_per_minute));
  console.log("weather: smoke events per song", median(rows.filter((r) => r.combo === "weather").map((r) => r.weather)), "max", Math.max(...rows.filter((r) => r.combo === "weather").map((r) => r.weather)));
  const kinds = {}; for (const c of claimsAll) if (c.dejavu.type) kinds[c.dejavu.type] = (kinds[c.dejavu.type] || 0) + 1;
  console.log("dejavu: repeat share of song", m((c) => c.dejavu.repeat_share), "compared", sum((c) => c.dejavu.compared), "same pattern (median song)", m((c) => c.dejavu.same_pattern), "pooled", sum((c) => (c.dejavu.same_pattern ?? 0) * c.dejavu.compared) / Math.max(1, sum((c) => c.dejavu.compared)), "songs with over one strobe type", claimsAll.filter((c) => c.dejavu.strobe_types > 1).length, "types across songs", JSON.stringify(kinds));
  console.log("burn: bad start", sum((c) => c.burn.start_not_one), "all before biggest", sum((c) => c.burn.all_before_biggest), "count on biggest", m((c) => c.burn.count_on_biggest), "rationed", claimsAll.filter((c) => c.burn.strobes_rationed).length, "of", claimsAll.length, "undone", claimsAll.filter((c) => c.burn.undone_when_off).length);
  console.log("no analysis at all: crashes", sum((c) => c.bare.crashes), "non-finite frames", sum((c) => c.bare.nan));
  console.log("countdown: builds", sum((c) => c.countdown.builds), "row came back", sum((c) => c.countdown.row_came_back));
  process.exit(0);
}

const all = fs.readdirSync(path.join(HERE, "cache")).filter((f) => f.endsWith(".json")).map((f) => f.slice(0, -5)).sort();
const [si, sn] = opt("--shard", "0/1").split("/").map(Number);
const nSongs = +opt("--songs", all.length), dt = 1 / +opt("--fps", 25);
const ids = all.filter((_, i) => i % Math.ceil(all.length / nSongs) === 0).slice(0, nSongs).filter((_, i) => i % sn === si);
const COMBOS = [...LOOK_LIST.map((l) => [l.id, l.id, {}]), ["club+dejavu", "club", { dejavu: true }], ["club+burn", "club", { burn: true }], ["club+countdown", "club", { countdown: true }],
  ...LOOK_LIST.slice(1).map((l) => [`${l.id}+all`, l.id, { dejavu: true, burn: true, countdown: true }])];
const rows = [], claimRows = [];
for (const id of ids) {
  const song = load(id);
  let base = null;
  for (const [combo, look, tog] of COMBOS) {
    const { out } = run(song, look, tog, dt, base);
    if (combo === "club") base = out;
    const { series, ...rest } = out;
    rows.push({ id, combo, ...rest });
  }
  claimRows.push({ id, ...claims(song, dt), bare: bare(song) });
}
const result = { rows, claims: claimRows };
const file = opt("--out", null);
if (file) fs.writeFileSync(file, JSON.stringify(result));
else console.log(JSON.stringify(result).slice(0, 2000));
console.error("done", ids.length, "songs");
