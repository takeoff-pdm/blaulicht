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
const COMPILED = path.join(HERE, "../../data/melody/compiled");  // pipeline/song.py
const { prepare, showAt, beatPos, opts } = core;
const { frame, makeDesk, LOOK_LIST, burnCount, club2Plan, ROPES, ROPE_PIX } = looks;

function load(id) {
  const doc = JSON.parse(fs.readFileSync(path.join(COMPILED, `${id}.json`)));
  for (const v of Object.values(doc.notes)) v.sort((a, b) => a[0] - b[0]);
  doc.energyData = {}; doc.energyNorm = {};
  for (const [k, v] of Object.entries(doc.energy)) {
    const e = new Uint8Array(Buffer.from(v, "base64"));
    doc.energyData[k] = e;
    const sorted = Array.from(e).sort((a, b) => a - b);
    doc.energyNorm[k] = [sorted[Math.floor(sorted.length * 0.2)], sorted[Math.floor(sorted.length * 0.995)]];
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

// Human Lighter against each map's raw events: what the state rules say should be on the desk right after, and a while after, every
// event, worked out here from the events alone. Frames in a blackout or strobe, and events with another one of their group too soon after, are skipped.
function replay(song) {
  const R = { maps: 0, events: 0, checked: 0, wrong_level: 0, wrong_colour: 0, after_checked: 0, wrong_after: 0, spins: 0, spin_same: 0, zooms: 0, zoom_same: 0, speeds: 0, speed_wrong: 0, per_minute: [], skipped_dark: 0 };
  const maps = (song.lightshow || {}).maps || [];
  const near = (a, b, tol = 0.03) => Math.abs(a - b) <= tol;
  maps.forEach((m, mi) => {
    Object.assign(opts, { dejavu: false, burn: false, countdown: false, look: "human" });
    const P = prepare(song), D = makeDesk(), base = P.palette;
    const at = (t) => { const S = showAt(P, t); frame(P, S, D, { look: "human", map: mi, pal: base, flash: [1, 1, 1] }); return !D.dark; };
    const rope = (k) => { let v = 0; const px = D.ropes[k]; for (let i = 0; i < px.length; i++) v = Math.max(v, px[i]); return v; };
    const lamp = () => { let v = 0; for (const L of D.ceil) v = Math.max(v, L.b); return v; };
    const level = { 0: () => D.ups[0].b / 0.9, 1: lamp, 4: () => D.strip.b / 0.9 };
    const hue = { 0: () => D.ups[0].h, 1: () => D.ceil[0].h, 4: () => D.strip.h };
    const boosts = m.events.filter((e) => e[1] === 5), boostAt = (t) => { let on = false; for (const e of boosts) { if (e[0] > t) break; on = e[2] === 1; } return on; };
    const byGroup = {}; for (const e of m.events) (byGroup[e[1]] = byGroup[e[1]] || []).push(e);
    const inCover = (t) => t >= m.cover[0] && t <= m.cover[1];
    R.maps++;
    const lights = m.events.filter((e) => e[1] <= 4 && e[1] !== 5);
    R.per_minute.push(lights.length / Math.max(1 / 60, (m.cover[1] - m.cover[0]) / 60));
    for (const ty of [0, 1, 2, 3, 4]) {
      const list = byGroup[ty] || [];
      list.forEach((e, i) => {
        R.events++;
        const t = e[0], v = e[2], next = i + 1 < list.length ? list[i + 1][0] : Infinity;
        if (next - t < 0.01 || !inCover(t + 0.002)) return;
        if (!at(t + 0.002)) { R.skipped_dark++; return; }
        R.checked++;
        const kind = v === 0 ? 0 : ((v - 1) % 4) + 1, peak = kind === 2 || kind === 3;
        if (ty === 2 || ty === 3) {  // lasers: lit or not, on their own ropes
          const lit = rope(ty === 2 ? 1 : 0);
          if ((v === 0) !== (lit < 1e-6)) R.wrong_level++;
        } else {
          const got = level[ty]();
          const want = kind === 0 ? 0 : peak ? 1 : 0.55;
          if (!near(got, want, peak ? 0.04 : 0.001)) R.wrong_level++;
          if (kind !== 0) {
            const wantHue = (v >= 5) !== boostAt(t + 0.002) ? base.secondary : base.primary;
            if (hue[ty]() !== wantHue) R.wrong_colour++;
          }
          // then: a flash settles to on, a fade is gone after its 1.5 s and a quarter left halfway
          if (kind === 2 && next - t > 0.8 && inCover(t + 0.75) && at(t + 0.75)) { R.after_checked++; if (!near(level[ty](), 0.55, 0.03)) R.wrong_after++; }
          if (kind === 3 && next - t > 1.6 && inCover(t + 1.55)) {
            if (at(t + 0.75)) { R.after_checked++; if (!near(level[ty](), 0.25, 0.01)) R.wrong_after++; }
            if (at(t + 1.55)) { R.after_checked++; if (level[ty]() !== 0) R.wrong_after++; }
          }
        }
      });
    }
    // ring spin and zoom must change which lamps are lit, while the rings stay on
    const litSet = () => D.ceil.filter((L) => L.b > 1e-6).map((L) => L.r * 4 + L.c).join();
    for (const [ty, key, same] of [[8, "spins", "spin_same"], [9, "zooms", "zoom_same"]]) {
      for (const e of byGroup[ty] || []) {
        // the rings must be lit on both sides of it, so no ring event right there, and the last one left them on or still fading
        const ring = byGroup[1] || [], j = ring.findIndex((x) => x[0] > e[0] - 0.02);
        if (j > -1 && ring[j][0] <= e[0] + 0.01) continue;
        const lastRing = ring[(j < 0 ? ring.length : j) - 1];
        if (!lastRing || lastRing[2] === 0 || !inCover(e[0])) continue;
        if ([3, 7].includes(lastRing[2]) && e[0] - lastRing[0] > 1.4) continue;
        if ((byGroup[ty] || []).some((x) => x !== e && Math.abs(x[0] - e[0]) < 0.015)) continue;  // two at once may cancel out
        if (!at(e[0] - 0.01)) continue;
        const a = litSet();
        if (!at(e[0] + 0.002)) continue;
        R[key]++;
        if (a === litSet()) R[same]++;
      }
    }
    // laser speed: over 0.1 s the 30% band moves by speed x 0.12 rope lengths a second, and not at all at speed 0
    for (const e of (byGroup[12] || [])) {
      const lights2 = byGroup[2] || [], lastLeft = [...lights2].reverse().find((x) => x[0] <= e[0]);
      const nextEv = (byGroup[12] || []).find((x) => x[0] > e[0] + 1e-6), nextL = lights2.find((x) => x[0] > e[0]);
      if (!lastLeft || ![1, 5].includes(lastLeft[2]) || !inCover(e[0]) || (nextEv && nextEv[0] < e[0] + 0.2) || (nextL && nextL[0] < e[0] + 0.2)) continue;
      const first = () => { const px = D.ropes[1]; for (let i = 0; i < 100; i++) if (px[3 * i] + px[3 * i + 1] + px[3 * i + 2] > 0.2) return i; return -1; };
      if (!at(e[0] + 0.02)) continue;
      const a = first();
      if (!at(e[0] + 0.12)) continue;
      const b = first();
      if (a <= 1 || b <= 1 || a > 65 || b > 65) continue;  // the band wraps round the rope's ends
      R.speeds++;
      if (Math.abs((b - a) / 100 - e[2] * 0.12 * 0.1) > 0.021) R.speed_wrong++;
    }
  });
  Object.assign(opts, { dejavu: false, burn: false, countdown: false, look: "club" });
  return R;
}

// Club 2's claims, per 16th of the song, against Club's: each fixture does one job, and looks hold through a section.
function club2Claims(song) {
  const R = { sections: {}, ceilRises: 0, ceilOnLead: 0, upRises: 0, upOnDrum: 0, ropeRises: 0, ropeOnSecond: 0, hold: {}, flips: {}, groups: {}, follow: {} };
  Object.assign(opts, { dejavu: false, burn: false, countdown: false, look: "club2" });
  const P0 = prepare(song), plan = club2Plan(P0), near = (a, t, w = 0.045) => { let l = 0, h = a.length; while (l < h) { const m = (l + h) >> 1; if (a[m] < t - w) l = m + 1; else h = m; } return l < a.length && a[l] <= t + w; };
  const on = (part) => (plan.parts[part] || []).map((h) => h.t);
  const series = { vocal: on("vocal"), stabs: on("stabs"), lead: on("lead"), drums: on("drums") };
  const H = (song.cues || {}).hits || {}, kicks = (H.kick || []).map((h) => h[0]).sort((a, b) => a - b), snares = (H.snare || []).map((h) => h[0]).sort((a, b) => a - b);
  const drum = (t) => near(kicks, t) || near(snares, t);
  for (const s of plan.sections) { const b = (P0.beatPos(s.b) - P0.beatPos(s.a)) / 4; R.sections[s.lead] = (R.sections[s.lead] || 0) + b; }
  const B = P0.beats, grid = []; for (let i = 0; i + 1 < B.length; i++) for (let k = 0; k < 4; k++) grid.push(B[i] + (B[i + 1] - B[i]) * k / 4);
  for (const look of ["club2", "club"]) {
    Object.assign(opts, { look });
    const P = prepare(song), D = makeDesk();
    const pal = (S) => { const b = P.palette, n = (S.hook >= 0 ? S.hook : S.part >= 0 ? S.part : S.seg) + S.rootLead; return n % 2 ? { ...b, primary: b.secondary, secondary: b.primary } : b; };
    const read = (t) => {
      const S = showAt(P, t); frame(P, S, D, { look, pal: pal(S), flash: [1, 1, 1] });
      let ce = 0, ro = 0; const litHue = new Set(); const lit = [];
      for (const L of D.ceil) { ce = Math.max(ce, L.b); lit.push(L.b >= 0.1); if (L.b >= 0.1) litHue.add(L.h); }
      for (const px of D.ropes) for (let i = 0; i < px.length; i += 3) ro = Math.max(ro, px[i], px[i + 1], px[i + 2]);
      return { S, dark: D.dark, ce, ro, up: Math.max(D.ups[0].b, D.ups[1].b, D.ups[2].b, D.ups[3].b), st: D.strip.b, co: D.cove[0].b, hue: [...litHue].sort().join(), upHue: D.ups[1].h, lit };
    };
    let flips = 0, groups = 0, cells = 0, hueChanges = 0, prevLit = null, prevHue = null, prevSec = null;
    const seen = new Set();
    for (const t of grid) {
      if (t > song.duration) break;
      const a = read(t - 0.02), b = read(t + 0.04), sec = leadAt2(plan, t + 0.04);
      if (a.dark || b.dark) { prevLit = null; continue; }
      cells++;
      if (prevLit) for (let i = 0; i < b.lit.length; i++) if (b.lit[i] !== prevLit[i]) flips++;
      prevLit = b.lit;
      groups += ["ce", "ro", "up", "st", "co"].filter((k) => b[k] - a[k] > 0.05).length;
      // a colour change on the ceiling inside a section (builds have their own whitening, the same hue)
      const hue = b.hue;
      if (hue) { if (prevHue && prevHue !== hue && prevSec === sec && !b.S.inBuild) hueChanges++; prevHue = hue; prevSec = sec; }
      if (look === "club2" && !b.S.inBuild && !a.S.inBuild) {
        if (b.ce - a.ce > 0.1) { R.ceilRises++; if (near(series[sec.lead], t)) R.ceilOnLead++; }
        if (b.up - a.up > 0.1) { R.upRises++; if (drum(t)) R.upOnDrum++; }
        if (sec.second && b.ro - a.ro > 0.1) { R.ropeRises++; if (near(series[sec.second], t)) R.ropeOnSecond++; }
      }
    }
    const minutes = Math.max(1 / 60, cells * (60 / P.bpm / 4) / 60);
    R.hold[look] = hueChanges / minutes; R.flips[look] = flips / minutes; R.groups[look] = groups / Math.max(1, cells);
  }
  Object.assign(opts, { dejavu: false, burn: false, countdown: false, look: "club" });
  return R;
}
const leadAt2 = (plan, t) => { let i = 0; while (i + 1 < plan.sections.length && plan.sections[i + 1].a <= t + 1e-9) i++; return plan.sections[i]; };

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
  const rs = claimsAll.map((c) => c.replay).filter((r) => r && r.maps), rsum = (k) => rs.reduce((a, r) => a + r[k], 0);
  console.log("human replay: maps", rsum("maps"), "events", rsum("events"), "checked", rsum("checked"), "(in a blackout or strobe", rsum("skipped_dark") + ")", "wrong level", rsum("wrong_level"), "wrong colour", rsum("wrong_colour"), "| later state checks", rsum("after_checked"), "wrong", rsum("wrong_after"));
  console.log("  ring spins", rsum("spins"), "no change", rsum("spin_same"), "| zooms", rsum("zooms"), "no change", rsum("zoom_same"), "| laser speeds", rsum("speeds"), "wrong", rsum("speed_wrong"), "| light events a minute, median map", median(rs.flatMap((r) => r.per_minute)));
  const showIds = new Set(rows.filter((r) => r.combo === "human").map((r) => r.id));
  if (showIds.size) {
    console.log("on the", showIds.size, "songs with a map:  look, lampP95, lamp>25%, black%, depth, kickCorr, clubCorr, drop/breakdown");
    for (const c of combos.filter((x) => !x.includes("+"))) {
      const R = rows.filter((r) => r.combo === c && showIds.has(r.id));
      if (!R.length) continue;
      const ratio = median(R.map((r) => (r.byKind.Drop && r.byKind.Breakdown ? r.byKind.Drop / Math.max(1e-3, r.byKind.Breakdown) : null)));
      console.log(" ", c.padEnd(10), median(R.map((r) => r.lampP95)).toFixed(2), (100 * mean(R.map((r) => r.lampOver25))).toFixed(1).padStart(6), (100 * mean(R.map((r) => r.blackShare))).toFixed(1).padStart(6), median(R.map((r) => r.depth)).toFixed(3).padStart(7), median(R.map((r) => r.kickCorr)).toFixed(2).padStart(6), (median(R.map((r) => r.clubCorr)) ?? 0).toFixed(2).padStart(6), (ratio ?? 0).toFixed(1).padStart(6));
    }
  }
  const cs = claimsAll.map((c) => c.club2).filter(Boolean), csum = (k) => cs.reduce((a, r) => a + r[k], 0), leads = {};
  for (const c of cs) for (const [k, v] of Object.entries(c.sections)) leads[k] = (leads[k] || 0) + v;
  const allBars = Object.values(leads).reduce((a, b) => a + b, 0) || 1;
  console.log("club 2: what leads, share of bars:", Object.entries(leads).sort((a, b) => b[1] - a[1]).map(([k, v]) => `${k} ${(100 * v / allBars).toFixed(0)}%`).join(", "));
  console.log("  ceiling rises on the lead part", csum("ceilRises") ? (100 * csum("ceilOnLead") / csum("ceilRises")).toFixed(1) + "%" : "-", "of", csum("ceilRises"), "| uplight rises on a kick or snare", csum("upRises") ? (100 * csum("upOnDrum") / csum("upRises")).toFixed(1) + "%" : "-", "of", csum("upRises"), "| rope rises on the second part", csum("ropeRises") ? (100 * csum("ropeOnSecond") / csum("ropeRises")).toFixed(1) + "%" : "-", "of", csum("ropeRises"));
  console.log("  median per song, club 2 vs club: ceiling colour changes inside a section per minute", median(cs.map((c) => c.hold.club2)).toFixed(1), "vs", median(cs.map((c) => c.hold.club)).toFixed(1), "| lamps switching per minute", median(cs.map((c) => c.flips.club2)).toFixed(0), "vs", median(cs.map((c) => c.flips.club)).toFixed(0), "| fixture groups rising per 16th", median(cs.map((c) => c.groups.club2)).toFixed(2), "vs", median(cs.map((c) => c.groups.club)).toFixed(2));
  console.log("no analysis at all: crashes", sum((c) => c.bare.crashes), "non-finite frames", sum((c) => c.bare.nan));
  console.log("countdown: builds", sum((c) => c.countdown.builds), "row came back", sum((c) => c.countdown.row_came_back));
  process.exit(0);
}

const index = JSON.parse(fs.readFileSync(path.join(COMPILED, "index.json"))).filter((s) => s.genre === "electronic" && s.has_audio && s.has_beats);
const all = index.map((s) => s.id).sort();
const [si, sn] = opt("--shard", "0/1").split("/").map(Number);
const nSongs = +opt("--songs", all.length), dt = 1 / +opt("--fps", 25);
const onlyShows = args.includes("--only-shows");
const ids = all.filter((id) => !onlyShows || fs.existsSync(path.join(COMPILED, "../viz/lightshows", `${id}.json`))).filter((_, i, a) => i % Math.ceil(a.length / nSongs) === 0).slice(0, nSongs).filter((_, i) => i % sn === si);
const COMBOS = [...LOOK_LIST.map((l) => [l.id, l.id, {}]), ["club+dejavu", "club", { dejavu: true }], ["club+burn", "club", { burn: true }], ["club+countdown", "club", { countdown: true }],
  ...LOOK_LIST.slice(1).map((l) => [`${l.id}+all`, l.id, { dejavu: true, burn: true, countdown: true }])];
const rows = [], claimRows = [];
for (const id of ids) {
  const song = load(id);
  let base = null;
  for (const [combo, look, tog] of COMBOS) {
    if (look === "human" && !song.lightshow) continue;
    const { out } = run(song, look, tog, dt, base);
    if (combo === "club") base = out;
    const { series, ...rest } = out;
    rows.push({ id, combo, ...rest });
  }
  claimRows.push({ id, ...claims(song, dt), bare: bare(song), replay: replay(song), club2: club2Claims(song) });
}
const result = { rows, claims: claimRows };
const file = opt("--out", null);
if (file) fs.writeFileSync(file, JSON.stringify(result));
else console.log(JSON.stringify(result).slice(0, 2000));
console.error("done", ids.length, "songs");
