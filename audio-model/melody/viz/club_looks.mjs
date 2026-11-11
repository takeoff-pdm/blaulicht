// The club's looks: what the room listens to. No three.js and no DOM in here, so every look can be checked
// under node (decision/check_looks.mjs). A look writes the frame's fixture values onto a plain desk; club.js
// only draws the desk.
//
// frame() handles the blackout and the strobes before any look runs, so no look can reach them: strobes stay
// on the drop, for up to a bar, as the only light in the room. Looks use the song's colour pair plus white, and
// stay within the "few lights" rule, except Breathing Room and Stained Glass, which need wide areas at low level.
//
// The desk, in the units club.js expects:
//   ceil[r * 4 + c]  {b 0..1, h degrees, s 0..1, sway degrees}, row 0 at the stage, column 0 on the left
//   ropes[k]         Float32Array of 100 x rgb, pixel 0 at the hub
//   ups[i], spill[i] {b, h, s}, left to right along the stage wall
//   strip {b, h}, cove[i] {b, h, s}, spot 0..1 (warm, on the DJ), vp 0..1, smoke {h, s, b}
//   ambient (scale), lens (lamp glass when off), info (the HUD's line)

export const CEIL_ROWS = 8, CEIL_COLS = 4, ROPES = 5, ROPE_PIX = 100;

export const LOOK_LIST = [
  { id: "club", name: "Club", about: "Everything at once: patterns from the melody, pulses from the drums, stems on the ropes." },
  { id: "club2", name: "Club 2", about: "Club without the clutter: one job per fixture. The ceiling and walls play whichever part leads the section, the ropes the next one, kick and snare stay on the uplights, bass on the floor strip. Patterns and colours hold through a section." },
  { id: "mixer", name: "Mixing Desk", about: "Each stem owns its fixtures: drums the ceiling, bass the floor strip and uplights, synths the ropes, vocals the DJ spot and walls." },
  { id: "score", name: "Score", about: "A piano roll on the ceiling and ropes: notes travel towards the stage and land as they sound. The bass line runs on one rope and the floor strip." },
  { id: "shock", name: "Shockwave", about: "Each hit is a wave: kicks roll from the stage to the back, snares ring out from the rope hub, chord stabs ring in." },
  { id: "breath", name: "Breathing Room", about: "Pulses inverted: the room glows between kicks and ducks on each one, by the track's measured sidechain depth." },
  { id: "kick", name: "Kick Room", about: "Drums only, one colour plus white. No drums, no light." },
  { id: "glass", name: "Stained Glass", about: "Slow colour fields, no pulses: each chord root owns an area of the ceiling, minor or major picks the colour, far from the key goes white." },
  { id: "spot", name: "Spotlight", about: "The voice leads: the room dims just before someone sings, a warm spot and a rope glow follow them, and it returns when they stop." },
  { id: "weather", name: "Weather", about: "No beat: brightness lifts the light from floor to ceiling, noise brings haze and whiter light, the stereo position moves it sideways." },
  { id: "human", name: "Human Lighter", about: "A person's lightshow for this song, from a community Beat Saber map, replayed on this rig: back lasers on the stage wall, rings on the ceiling, lasers on the ropes, track lights on the walls and floor strip. A reference to compare the other looks with. Songs without a map stay on Club." },
  { id: "director", name: "Director", about: "Picks a look per section from the analysis: a home look from the song's style for drops and grooves, Shockwave and Countdown in builds, Spotlight, Stained Glass or Weather in breakdowns." },
];

// ---- helpers
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
const clamp01 = (v) => clamp(v, 0, 1);
const near = (d, w) => Math.max(0, 1 - Math.abs(d) / w);  // 1 at d = 0, 0 from |d| >= w
const odd = (n) => ((n % 2) + 2) % 2;
const sq = (x) => x * x;
export const hash3 = (a, b, c) => { let h = (a * 374761393 + b * 668265263 + c * 2147483647) | 0; h = Math.imul(h ^ (h >>> 13), 1274126177); return ((h ^ (h >>> 16)) >>> 0) / 4294967296; };
const fifthsFrom = (pc, tonic) => { const d = ((((pc - tonic) * 7) % 12) + 12) % 12; return d > 6 ? d - 12 : d; };
const PC = { C: 0, "C#": 1, D: 2, "D#": 3, E: 4, F: 5, "F#": 6, G: 7, "G#": 8, A: 9, "A#": 10, B: 11 };

function lowerBound(arr, t, get = (x) => x) {
  let lo = 0, hi = arr.length;
  while (lo < hi) { const m = (lo + hi) >> 1; if (get(arr[m]) < t) lo = m + 1; else hi = m; }
  return lo;
}
function pulseAt(set, t, decay = 10) {  // the strongest recent pulse of a set
  if (!set || !set.times.length) return 0;
  let v = 0;
  for (let i = lowerBound(set.times, t + 1e-9) - 1, k = 0; i >= 0 && k < 2; i--, k++) v = Math.max(v, set.amps[i] * Math.exp(-(t - set.times[i]) * decay));
  return v;
}
function recent(set, t, window, out) {  // [time, amp] of the hits in the last `window` seconds, newest first (8 at most)
  out.length = 0;
  if (!set) return out;
  for (let i = lowerBound(set.times, t + 1e-9) - 1; i >= 0 && out.length < 8 && t - set.times[i] <= window; i--) out.push(set.times[i], set.amps[i]);
  return out;
}
// three's Color.setHSL in the working colour space, so the rope pixels come out the same as a three.js colour
const hue2rgb = (p, q, t) => { if (t < 0) t += 1; if (t > 1) t -= 1; if (t < 1 / 6) return p + (q - p) * 6 * t; if (t < 1 / 2) return q; if (t < 2 / 3) return p + (q - p) * 6 * (2 / 3 - t); return p; };
function hslRGB(h, s, l, out) {
  const H = ((h % 360) + 360) % 360 / 360;
  s = clamp01(s); l = clamp01(l);
  if (s === 0) { out[0] = out[1] = out[2] = l; return out; }
  const p = l <= 0.5 ? l * (1 + s) : l + s - l * s, q = 2 * l - p;
  out[0] = hue2rgb(q, p, H + 1 / 3); out[1] = hue2rgb(q, p, H); out[2] = hue2rgb(q, p, H - 1 / 3);
  return out;
}
const RGB = [0, 0, 0];
function addPx(px, i, rgb, g) { px[3 * i] += rgb[0] * g; px[3 * i + 1] += rgb[1] * g; px[3 * i + 2] += rgb[2] * g; }
function ropeRange(px, rgb, x0, x1, g) {  // a lit stretch of a rope with soft ends
  const a = Math.max(0, Math.floor(x0 * ROPE_PIX) - 1), b = Math.min(ROPE_PIX - 1, Math.ceil(x1 * ROPE_PIX) + 1);
  for (let i = a; i <= b; i++) {
    const x = (i + 0.5) / ROPE_PIX, e = Math.min(1, Math.max(0, Math.min(x - x0, x1 - x) / 0.02 + 0.5));
    if (e > 0) addPx(px, i, rgb, g * e);
  }
}
function ropeBlob(px, rgb, x, width, g) {  // a soft blob
  const a = Math.max(0, Math.floor((x - 3 * width) * ROPE_PIX)), b = Math.min(ROPE_PIX - 1, Math.ceil((x + 3 * width) * ROPE_PIX));
  for (let i = a; i <= b; i++) addPx(px, i, rgb, g * Math.exp(-sq((i + 0.5) / ROPE_PIX - x) / (2 * width * width)));
}
function setLamp(D, r, c, b, h, s) {  // the brightest wins
  const L = D.ceil[r * CEIL_COLS + c];
  if (b > L.b) { L.b = b; L.h = h; L.s = s; }
}
const envAt = (P, k, t) => { const e = P.env[k], f = Math.round(t * 100); return f >= 0 && f < e.length ? e[f] : 0; };
function smooth(a, tau) {  // one-pole forwards and backwards: no lag, because the whole song is known
  const out = Float32Array.from(a), k = 1 / Math.max(1, tau);
  for (let i = 1; i < out.length; i++) out[i] = out[i - 1] + k * (out[i] - out[i - 1]);
  for (let i = out.length - 2; i >= 0; i--) out[i] = out[i + 1] + k * (out[i] - out[i + 1]);
  return out;
}
const fnv = (s) => { let h = 2166136261; for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); } return h >>> 0; };
const decode = (b64) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
function normTrack(u8, tau) {  // a 0..255 track smoothed, then 0..1 between its own 5th and 95th percentile, so each song uses the whole range
  const s = smooth(Float32Array.from(u8), tau), sorted = Array.from(s).sort((a, b) => a - b), lo = sorted[Math.floor(sorted.length * 0.05)], hi = sorted[Math.floor(sorted.length * 0.95)];
  return s.map((v) => clamp01((v - lo) / Math.max(1e-3, hi - lo)));
}

// ---- what the looks share, built once per song
function shared(P) {
  if (P.lookData) return P.lookData;
  const C = P.song.cues || {}, H = C.hits || {}, pieces = ((P.song.features || {}).drumparts || {}).pieces || {};
  const set = (list, min = 0) => { const l = list.filter((h) => h[1] >= min); return { times: Float64Array.from(l, (h) => h[0]), amps: Float32Array.from(l, (h) => Math.min(1, h[1])) }; };
  const empty = set([]);
  P.lookData = {
    kick: (H.kick || []).length > 20 ? set(H.kick) : P.hits, snare: set(H.snare || []), crash: set(H.crash || []),
    toms: set((pieces.toms || {}).hits || [], 0.6), ride: set((pieces.ride || {}).hits || [], 0.6),
    hats: P.hats || empty, stabs: P.stabs || empty, impacts: P.impacts || empty, slowMix: smooth(P.env.mix, 200), a: [], b: [], c: [],
  };
  return P.lookData;
}
const slowAt = (X, t) => { const f = Math.round(t * 100); return f >= 0 && f < X.slowMix.length ? X.slowMix[f] : 0; };

// ---- Club: the original show, everything at once
function drawClub(P, S, A, D) {
  const { pal, accent, sat, kick: kickPulse, swap, anim } = A;
  const TUBE_STEM = [["bass", pal.primary, sat], ["drums", pal.secondary, sat], ["other", pal.primary, sat], ["vocals", pal.secondary, sat], ["mix", 0, 0]];  // the mix in white
  const env = Math.min(1, 0.15 + 0.45 * S.stepEnv + 0.6 * kickPulse), ph = S.step / S.steps;
  const melTube = S.melodyNote ? ((S.noteIdx % ROPES) + ROPES) % ROPES : -1;
  const melX = S.melodyNote ? clamp((S.melodyNote[2] - P.lineLo) / Math.max(1, P.lineHi - P.lineLo), 0.06, 0.94) : 0;
  // LED rope web, x = 0 at the hub .. 1 at the outer anchor:
  //   builds    light closes in from the outer anchors towards the hub with the build's progress,
  //             whiter towards the drop, flashing on every drum hit (snare rolls flicker faster);
  //   drops     the hook's / segment's pattern on the note grid, like the ceiling: sweep = a ring
  //             running out from the hub through the web, wave = the same with each spoke a fifth
  //             later (a spiral), cross = every fourth 5 cm block lit and stepping, circle = a ring
  //             closing in on the hub, fan = one spoke after the other round the hub; flashing on
  //             each step and on the kick;
  //   grooves   one stem per spoke (bass, drums, other, vocals, mix): a bar out from the hub as
  //             long as the stem is loud, so you see which instruments play;
  //   calm      a faint slow gradient in the palette;
  //   melody    (all but builds) each note of the line is a soft blob on the next spoke round the
  //             hub, as far out as its pitch is high, flashing on the onset and holding while it
  //             sounds.
  for (let k = 0; k < ROPES; k++) {
    const px = D.ropes[k];
    let hue = k % 2 ? pal.secondary : pal.primary, s = sat;
    if (S.inBuild) s = 1 - 0.7 * S.buildProgress;
    if (S.energetic && anim === "wave") hue = accent;
    hslRGB(hue, s, 0.5, RGB);
    const stem = TUBE_STEM[k];
    if (S.kind === "Groove" && !S.inBuild) hslRGB(stem[1], stem[2], 0.5, RGB);
    for (let i = 0; i < ROPE_PIX; i++) {
      const x = (i + 0.5) / ROPE_PIX;
      let b = 0;
      if (S.inBuild) {
        b = x > 1 - S.buildProgress ? 0.12 + 0.55 * kickPulse * (0.4 + 0.6 * S.buildProgress) : 0;
      } else if (S.energetic) {
        let on;
        switch (anim) {
          case "wave": { let d = (((x - ph - k / ROPES) % 1) + 1) % 1; d = Math.min(d, 1 - d); on = near(d, 0.07); break; }
          case "cross": on = (Math.floor(x * 20) + k + S.step) % 4 === 0 ? 1 : 0; break;
          case "circle": on = near(1 - x - ph, 0.08); break;  // closing in on the hub
          case "fan": on = k === S.step % ROPES ? 0.8 : 0; break;
          default: on = near(x - ph, 0.07); break;  // sweep
        }
        b = on * env;
      } else if (S.kind === "Groove") {
        const lvl = S.energy[stem[0]] || 0, d = x;
        b = d < lvl ? (lvl - d < 0.06 ? 0.25 + 0.5 * kickPulse : 0.05) : 0;
      } else {
        b = 0.01 + 0.025 * S.level * (0.5 + 0.5 * Math.sin(2 * Math.PI * (S.rel / 8) + x * 3 + k * 1.3));
      }
      if (b > 0) { px[3 * i] = RGB[0] * b; px[3 * i + 1] = RGB[1] * b; px[3 * i + 2] = RGB[2] * b; }
    }
    const E = S.event;
    if (E && !S.inBuild) {  // sound events: their own rope looks, on top
      const prog = (S.t - E.a) / Math.max(0.2, E.b - E.a), sixteenth = Math.floor(S.rel * 4);
      hslRGB(E.kind === "siren" ? pal.primary : pal.secondary, sat, 0.5, RGB);
      for (let i = 0; i < ROPE_PIX; i++) {
        const x = (i + 0.5) / ROPE_PIX;
        let g = 0;
        if (E.kind === "siren") g = k === ((Math.floor(S.rel * ROPES) % ROPES) + ROPES) % ROPES ? 0.7 : 0;  // spinning round the hub
        else if (E.kind === "whoosh") g = 0.9 * near(x - prog, 0.1);  // a ring rushing out from the hub
        else if (E.kind === "airhorn") g = hash3(sixteenth, k, 3) < 0.4 && Math.abs(x - hash3(sixteenth, k, 4)) < 0.1 ? 0.9 : 0;  // blasts
        else if (E.kind === "boom") g = 0.8 * Math.exp(-(S.t - E.a) * 3);
        if (g > 0) addPx(px, i, RGB, g);
      }
    }
    if (S.energetic && S.hat > 0.05) {  // hi-hats: a faint white sparkle
      const tick = Math.floor(S.t * 16);
      for (let i = 0; i < ROPE_PIX; i += 3) if (hash3(tick, k, i) < 0.05) { const g = 0.35 * S.hat; px[3 * i] += g; px[3 * i + 1] += g; px[3 * i + 2] += g; }
    }
    if (k === melTube && !S.inBuild) {  // the melody blob on top
      hslRGB(accent, S.noteSat, 0.5, RGB);
      const amp = S.noteEnv * (S.energetic ? 0.9 : 1);
      for (let i = 0; i < ROPE_PIX; i++) {
        const g = amp * Math.exp(-(((i + 0.5) / ROPE_PIX - melX) ** 2) / (2 * 0.035 ** 2));
        if (g > 0.003) addPx(px, i, RGB, g);
      }
    }
  }
  // uplights: in energetic parts two of the four take turns per beat and flash on the drum hits;
  // calm parts breathe slowly at a lower level
  D.ups.forEach((u, i) => {
    let b = S.energetic ? (odd(i) === odd(S.idx) ? 1 : 0.05) * S.level * (0.04 + 0.96 * kickPulse) * (0.4 + 0.6 * S.energy.drums)
      : S.level * 0.55 * (0.55 + 0.45 * Math.sin(2 * Math.PI * (S.rel / 8) + i));
    b *= Math.max(0.3, 1 + 0.7 * S.pan * (i < 2 ? -1 : 1));  // lean towards where the synths sit
    b = Math.max(b, S.impact * 0.9);
    u.b = Math.min(1, b); u.h = (i % 2 === 0) !== swap ? pal.secondary : pal.primary; u.s = sat * (1 - 0.6 * S.impact);
  });
  D.spill.forEach((l, i) => { l.h = i ? pal.secondary : pal.primary; l.s = sat; l.b = S.level * (S.energetic ? 0.08 + 0.92 * kickPulse : 0.6); });
  // bass strip
  D.strip.h = pal.primary; D.strip.b = S.energy.bass * (0.4 + 0.6 * S.level);
  // ceiling grid, at most a quarter lit at once: builds send one row from the back towards the stage,
  // a step per drum hit (snare rolls speed it up); drops play the segment's pattern (picked from its
  // melody, new at every music change), jumping one step per note-grid step (no gliding) and flashing on
  // each step and harder on the kick; grooves jump half a row away from the stage on every beat; calm
  // parts light two lights of the melody's pitch row, brighter on each new note
  const gx = (CEIL_COLS - 1) / 2, gz = (CEIL_ROWS - 1) / 2;
  // the melody plays across the room: each note of the line lights two lamps of the row its pitch
  // maps to (the line's low notes near the stage, high ones at the back), alternating sides per
  // note, flashing on the onset and holding at the note's velocity for as long as it sounds
  const mn = S.melodyNote;
  const noteRow = mn ? clamp(Math.round((mn[2] - P.lineLo) / Math.max(1, P.lineHi - P.lineLo) * (CEIL_ROWS - 1)), 0, CEIL_ROWS - 1) : -1;
  const noteCols = odd(S.noteIdx) ? [0, 2] : [1, 3];
  const k = S.step, rowAt = Math.floor(k * CEIL_ROWS / S.steps), flip = odd(k) ? 1 : -1;
  for (const L of D.ceil) {
    const { r, c } = L, u = c / (CEIL_COLS - 1);
    let b = 0, h = pal.primary, sway = 0, ls = sat;
    if (S.inBuild) {
      const row = CEIL_ROWS - 1 - (S.buildSteps % CEIL_ROWS);
      b = r === row ? (0.3 + 0.55 * S.buildProgress) * (0.5 + 0.5 * kickPulse) : 0.01;
    } else if (S.energetic) {
      let on;
      switch (anim) {
        case "wave": on = (((r - rowAt - c) % CEIL_ROWS) + CEIL_ROWS) % CEIL_ROWS === 0 ? 1 : 0; sway = 6 * flip; break;  // a diagonal
        case "cross": on = (r + c + k) % 4 === 0 ? 1 : 0; sway = (odd(r + c) ? 1 : -1) * 8 * flip; break;  // sparse checker
        case "circle": on = near(Math.hypot((c - gx) / gx, (r - gz) / gz) - (k / S.steps) * 1.5, 0.2); break;  // ring
        case "fan": on = Math.round(Math.abs(r - gz) - 0.5) === Math.floor(k * 4 / S.steps) ? 1 : 0; sway = (u - 0.5) * 16 * (k / S.steps); break;  // rows outwards from the middle
        default: on = r === rowAt ? 1 : 0; break;  // sweep: one row stepping along the room
      }
      b = on * Math.min(1, 0.12 + 0.45 * S.stepEnv + 0.6 * kickPulse) + 0.01; h = r % 2 ? pal.secondary : pal.primary;
    } else if (S.kind === "Groove") {
      const row = ((S.idx % CEIL_ROWS) + CEIL_ROWS) % CEIL_ROWS;
      b = r === row && odd(c + S.idx) === 0 ? Math.min(1, 0.12 + 0.45 * S.beatEnv + 0.5 * kickPulse) : 0.01; h = S.lookBar % 2 ? pal.secondary : pal.primary;
    } else {
      b = 0.008 + 0.025 * S.level * (0.5 + 0.5 * Math.sin(2 * Math.PI * (S.rel / 8) + r * 0.8 + c));
    }
    if (!S.inBuild && r === noteRow && noteCols.includes(c)) {
      const nb = S.noteEnv * (S.energetic ? 0.9 : 0.75);
      if (nb > b) { b = nb; h = accent; ls = S.noteSat; }
    }
    L.b = b; L.h = h; L.s = ls; L.sway = sway;
  }
  // cove wall wash: breathing over 2 bars in calm parts, drum hits in drops, and a swell over the
  // first beat after every music change
  let cove = S.energetic ? 0.08 + 0.62 * kickPulse : 0.3 + 0.15 * Math.sin(2 * Math.PI * S.rel / 8);
  if (S.sinceChange >= 0 && S.sinceChange < 1) cove += 0.6 * (1 - S.sinceChange);
  cove += 0.8 * S.impact;
  D.cove.forEach((cv, i) => { cv.h = i ? pal.secondary : pal.primary; cv.s = sat; cv.b = cove * S.level; });
  // the DJ / vocal spot
  D.spot = (0.1 + 0.9 * S.energy.vocals) * (S.energetic ? 0.5 : 1);
}

// ---- Mixing Desk: each stem owns its fixtures for the whole song, in fixed colours. No sections: a breakdown
// goes dark because the instruments left, and a drop is big because they came back.
function drawMixer(P, S, A, D, X) {
  const t = S.t, base = P.palette, sat = A.sat, en = S.energy;
  // drums, the ceiling: the kick in the middle, snares either side, hats hopping along the back, crashes at the front
  const kick = pulseAt(X.kick, t, 12), snare = pulseAt(X.snare, t, 14), crash = pulseAt(X.crash, t, 3.5), hat = pulseAt(X.hats, t, 22);
  const drums = 0.45 + 0.55 * en.drums;
  const hatN = lowerBound(X.hats.times, t + 1e-9), hatLamp = (hatN * 3) % 8, snareN = lowerBound(X.snare.times, t + 1e-9);
  for (let r = 0; r < CEIL_ROWS; r++) for (let c = 0; c < CEIL_COLS; c++) {
    if ((c === 1 || c === 2) && r >= 2 && r <= 5) setLamp(D, r, c, kick * drums, base.primary, 0.15);
    if ((c === 0 || c === 3) && (r === 3 || r === 4) && (r + (c ? 1 : 0) + snareN) % 2 === 0) setLamp(D, r, c, snare * drums * 0.9, base.secondary, 0.3);
    if (r >= 6 && (r - 6) * 4 + c === hatLamp) setLamp(D, r, c, hat * drums * 0.4, base.primary, 0);
    if (r === 0) setLamp(D, r, c, crash * 0.8, base.primary, 0);
  }
  // bass, the floor: the strip and the uplights
  D.strip.h = base.primary; D.strip.b = en.bass;
  D.ups.forEach((u) => { u.h = base.primary; u.s = sat; u.b = en.bass * 0.85; });
  D.spill.forEach((l) => { l.h = base.primary; l.s = sat; l.b = en.bass * 0.5; });
  // synths, the ropes: the stem's level out from the hub, each rope a little later than the last, leaning with the stereo position
  const stab = pulseAt(X.stabs, t, 9), SIDE = [1, -1, 1, -1, 1];
  hslRGB(base.secondary, sat, 0.5, RGB);
  for (let k = 0; k < ROPES; k++) {
    const len = envAt(P, "other", t - k * 0.06) * clamp(0.85 + 0.6 * S.pan * SIDE[k], 0.3, 1.2);
    if (len > 0.02) {
      ropeRange(D.ropes[k], RGB, 0, len, 0.04 + 0.32 * stab);
      ropeRange(D.ropes[k], RGB, Math.max(0, len - 0.06), len, 0.55);
    }
  }
  if (S.lead && S.melodyNote) {
    const x = clamp((S.melodyNote[2] - P.lineLo) / Math.max(1, P.lineHi - P.lineLo), 0.06, 0.94);
    hslRGB(base.secondary, S.noteSat, 0.5, RGB);
    ropeBlob(D.ropes[((S.noteIdx % ROPES) + ROPES) % ROPES], RGB, x, 0.035, S.noteEnv * 0.8);
  }
  // vocals, the DJ spot and the walls
  D.spot = en.vocals;
  D.cove.forEach((cv) => { cv.h = base.secondary; cv.s = 0.35; cv.b = en.vocals * 0.55; });
  D.smoke.b = 0.07 + 0.3 * en.mix;
  D.lens = 0.1;
}

// ---- Score: a piano roll. Notes enter at the back and land on the stage row as they sound, moving a row per
// eighth, on the grid. The ropes show the same with the hub as now and the outer anchor a bar ahead.
const FLY = 3.5;  // beats a note spends crossing the seven rows
function scoreData(P, X) {
  if (X.score) return X.score;
  const lo = P.lineLo, hi = Math.max(P.lineLo + 1, P.lineHi);
  const band = (p, n) => clamp(Math.floor((p - lo) / (hi - lo + 1e-3) * n), 0, n - 1);
  const notes = (list, map) => list.map((n) => ({ on: P.beatPos(n[0]), off: Math.max(P.beatPos(n[1]), P.beatPos(n[0]) + 0.2), t: n[0], vel: n[3] ?? 0.8, ...map(n) })).sort((a, b) => a.on - b.on);
  X.score = { line: notes(P.line, (n) => ({ col: band(n[2], CEIL_COLS), rope: band(n[2], ROPES - 1) })), bass: notes(P.bassLine, () => ({ rope: ROPES - 1 })) };
  return X.score;
}
function drawScore(P, S, A, D, X) {
  const Y = scoreData(P, X), pos = S.pos, t = S.t, posQ = Math.floor(pos * 4) / 4, { pal, sat, accent } = A;
  const gain = 0.45 + 0.55 * S.baseLevel;  // the notes are as busy in a breakdown as in a drop, so the section sets how hard they hit
  const kick = pulseAt(X.kick, t, 10);
  hslRGB(accent, sat, 0.5, RGB);
  const melRGB = [RGB[0], RGB[1], RGB[2]], bassRGB = hslRGB(pal.primary, sat, 0.5, [0, 0, 0]);
  for (let i = Math.max(0, lowerBound(Y.line, pos - 16, (n) => n.on)); i < Y.line.length; i++) {
    const n = Y.line[i], d = n.on - pos;
    if (d > 4) break;
    if (n.off <= pos) continue;
    const onset = d <= 0, flash = onset ? gain * n.vel * (0.5 + 0.5 * Math.exp(-(t - n.t) * 12)) : 0;
    if (onset) {  // landed: the stage row and that column's uplight
      setLamp(D, 0, n.col, flash, accent, S.noteSat);
      const u = D.ups[n.col]; if (flash > u.b) { u.b = flash; u.h = accent; u.s = sat; }
    } else if (d <= FLY) {
      const row = Math.ceil(d * 2);
      setLamp(D, row, n.col, gain * (0.12 + 0.33 * (1 - row / 7)), accent, sat);
    }
    // the rope: the note's whole length, a bar of the future; bright as it sounds
    const x0 = Math.max(0, (n.on - posQ) / 4), x1 = Math.min(1, (n.off - posQ) / 4);
    if (x1 > x0) ropeRange(D.ropes[n.rope], melRGB, x0, Math.max(x1, x0 + 0.03), onset ? flash : 0.16 * gain);
  }
  for (let i = Math.max(0, lowerBound(Y.bass, pos - 16, (n) => n.on)); i < Y.bass.length; i++) {
    const n = Y.bass[i], d = n.on - pos;
    if (d > 4) break;
    if (n.off <= pos) continue;
    const onset = d <= 0, flash = onset ? gain * n.vel * (0.45 + 0.55 * Math.exp(-(t - n.t) * 10)) : 0;
    if (onset && flash > D.strip.b) D.strip.b = flash;
    const x0 = Math.max(0, (n.on - posQ) / 4), x1 = Math.min(1, (n.off - posQ) / 4);
    if (x1 > x0) ropeRange(D.ropes[n.rope], bassRGB, x0, Math.max(x1, x0 + 0.03), onset ? flash * 0.8 : 0.12 * gain);
  }
  D.strip.h = pal.primary;
  // the kick stays as a quiet reference on the walls, so the room keeps its pulse
  const lvl = 0.4 + 0.6 * S.level;
  D.cove.forEach((cv, i) => { cv.h = i ? pal.secondary : pal.primary; cv.s = sat; cv.b = (0.04 + 0.22 * kick) * lvl; });
  D.spill.forEach((l, i) => { l.h = i ? pal.secondary : pal.primary; l.s = sat; l.b = 0.14 * kick * lvl; });
  D.spot = S.vocal ? 0.4 + 0.6 * S.energy.vocals : 0;
  D.lens = 0.1;
}

// ---- Shockwave: every hit is a wave, so the light fades over distance rather than time. A kick crosses the
// room in one beat; a snare rings out from the rope hub; hats make a small ring; stabs ring in from the anchors.
function drawShock(P, S, A, D, X) {
  const t = S.t, beat = S.beatDur, d = beat / 8, { pal, sat } = A, gain = 0.45 + 0.55 * S.level;
  const white = S.inBuild ? S.buildProgress : 0;
  for (let r = 0; r < CEIL_ROWS; r++) {
    const wave = pulseAt(X.kick, t - r * d, 24) * (1 - 0.55 * r / (CEIL_ROWS - 1)) * gain;
    const back = Math.max(pulseAt(X.crash, t - (CEIL_ROWS - 1 - r) * d * 1.5, 7) * 0.9, pulseAt(X.impacts, t - (CEIL_ROWS - 1 - r) * d * 1.5, 7) * 0.7);
    for (let c = 0; c < CEIL_COLS; c++) {  // every other lamp, a zigzag, so even two waves in the room stay around a quarter of the ceiling
      if ((r + c) % 2) continue;
      setLamp(D, r, c, wave, pal.primary, sat * (1 - 0.7 * white));
      if (back > 0.03) setLamp(D, r, c, back, pal.primary, 0);
    }
  }
  const sn = recent(X.snare, t, beat * 1.2, X.a), hh = recent(X.hats, t, beat * 0.5, X.b), st = recent(X.stabs, t, beat, X.c);
  const snareRGB = hslRGB(pal.secondary, sat * (1 - 0.7 * white), 0.5, [0, 0, 0]), stabRGB = hslRGB(pal.primary, sat, 0.5, [0, 0, 0]);
  for (let k = 0; k < ROPES; k++) {
    const px = D.ropes[k], off = k * 0.012;
    for (let i = 0; i < ROPE_PIX; i++) {
      const x = (i + 0.5) / ROPE_PIX;
      let a = 0, b = 0, c = 0;
      for (let j = 0; j < sn.length; j += 2) { const q = t - sn[j] - (x + off) * beat * 0.75; if (q >= 0) a = Math.max(a, sn[j + 1] * Math.exp(-q * 22)); }
      if (x < 0.35) for (let j = 0; j < hh.length; j += 2) { const q = t - hh[j] - x * beat * 0.3; if (q >= 0) b = Math.max(b, hh[j + 1] * Math.exp(-q * 30) * (1 - x / 0.35)); }
      for (let j = 0; j < st.length; j += 2) { const q = t - st[j] - (1 - x) * beat * 0.5; if (q >= 0) c = Math.max(c, st[j + 1] * Math.exp(-q * 18)); }
      if (a > 0.004) addPx(px, i, snareRGB, a * gain);
      if (c > 0.004) addPx(px, i, stabRGB, c * 0.4 * gain);
      if (b > 0.004) { const g = b * 0.3; px[3 * i] += g; px[3 * i + 1] += g; px[3 * i + 2] += g; }
    }
  }
  // the source and the echo: uplights on the stage wall hit first, the walls catch it late
  const kick = pulseAt(X.kick, t, 10), impact = pulseAt(X.impacts, t, 3), crash = pulseAt(X.crash, t, 3);
  D.ups.forEach((u, i) => { u.h = i % 2 ? pal.secondary : pal.primary; u.s = sat; u.b = kick * 0.8 * gain; });
  D.spill.forEach((l, i) => { l.h = i ? pal.secondary : pal.primary; l.s = sat; l.b = kick * 0.6 * gain; });
  D.strip.h = pal.primary; D.strip.b = Math.max(S.energy.bass * 0.5, kick * 0.6) * gain;
  D.cove.forEach((cv, i) => { cv.h = i ? pal.secondary : pal.primary; cv.s = crash > 0.1 ? 0.2 : sat; cv.b = 0.02 + 0.6 * crash + 0.5 * impact + 0.12 * pulseAt(X.kick, t - beat * 0.9, 6) * gain; });
  D.spot = S.energy.vocals * 0.4;
  D.lens = 0.1;
}

// ---- Breathing Room: the room is on, and the kick takes it away. The depth is the track's measured sidechain.
function pumpData(P, X) {
  if (X.pump) return X.pump;
  const list = (P.song.cues || {}).pump || [];
  X.pump = list.length > 8 ? { times: Float64Array.from(list, (p) => p[0]), depth: Float32Array.from(list, (p) => p[1]) }
    : { times: X.kick.times, depth: Float32Array.from(X.kick.times, () => 0.7) };
  return X.pump;
}
function duckAt(Y, t, tau) {
  const i = lowerBound(Y.times, t + 1e-9) - 1;
  if (i < 0) return 1;
  return 1 - (0.3 + 0.7 * Math.min(1, Y.depth[i])) * Math.exp(-(t - Y.times[i]) / tau);
}
function drawBreath(P, S, A, D, X) {
  const Y = pumpData(P, X), t = S.t, base = P.palette, sat = A.sat, duck = duckAt(Y, t, S.beatDur * 0.28);
  const G = (0.1 + 0.6 * S.baseLevel) * (0.35 + 0.65 * slowAt(X, t)), v = G * duck, shift = Math.floor(S.lookBar / 4);
  for (const L of D.ceil) { L.b = v * 0.3; L.h = (((L.r >> 1) + shift) % 2) ? base.secondary : base.primary; L.s = sat; }
  for (let k = 0; k < ROPES; k++) {
    hslRGB(k % 2 ? base.secondary : base.primary, sat, 0.5, RGB);
    const px = D.ropes[k];
    for (let i = 0; i < ROPE_PIX; i++) addPx(px, i, RGB, v * 1.4 * (0.14 + 0.11 * Math.sin(2 * Math.PI * ((i + 0.5) / ROPE_PIX * 1.2 - S.rel / 8) + k * 1.3)));
  }
  D.ups.forEach((u, i) => { u.h = i % 2 ? base.secondary : base.primary; u.s = sat; u.b = v * 0.8; });
  D.spill.forEach((l, i) => { l.h = i ? base.secondary : base.primary; l.s = sat; l.b = v * 0.7; });
  D.cove.forEach((cv, i) => { cv.h = i ? base.secondary : base.primary; cv.s = sat; cv.b = v; });
  D.strip.h = base.primary; D.strip.b = S.energy.bass * duck * 0.6;
  D.spot = S.energy.vocals * 0.8;
  D.smoke.b = 0.05 + 0.4 * v;
  D.lens = 0.1;
}

// ---- Kick Room: drums only, one colour plus white. No melody, no colour changes, and no drums means no light.
function drawKick(P, S, A, D, X) {
  const t = S.t, h = P.palette.primary, sat = A.sat;
  const kick = pulseAt(X.kick, t, 14), snare = pulseAt(X.snare, t, 16), hat = pulseAt(X.hats, t, 24), ride = pulseAt(X.ride, t, 12), crash = pulseAt(X.crash, t, 4), toms = pulseAt(X.toms, t, 10);
  const hatN = lowerBound(X.hats.times, t + 1e-9), hatRow = 1 + (hatN * 3) % 6, hatCol = hatN % 2 ? 0 : 3, snareN = lowerBound(X.snare.times, t + 1e-9);
  for (let r = 0; r < CEIL_ROWS; r++) for (let c = 0; c < CEIL_COLS; c++) {
    if ((c === 1 || c === 2) && r >= 2 && r <= 5) setLamp(D, r, c, kick, h, sat);
    if ((c === 0 || c === 3) && (r === 0 || r === CEIL_ROWS - 1) && ((r ? 1 : 0) + (c ? 1 : 0) + snareN) % 2 === 0) setLamp(D, r, c, snare * 0.9, h, 0);
    if (r === hatRow && c === hatCol) setLamp(D, r, c, hat * 0.4, h, 0);
    if (r === CEIL_ROWS - 1 && (c === 1 || c === 2)) setLamp(D, r, c, ride * 0.35, h, 0);
    if (r === 0 && (c === 1 || c === 2)) setLamp(D, r, c, crash * 0.8, h, 0);
  }
  // the ropes only speak for the snare and toms: 30% stretches, never a whole rope
  const white = [1, 1, 1], colour = hslRGB(h, sat, 0.5, [0, 0, 0]);
  if (snare > 0.02) {
    const n = lowerBound(X.snare.times, t + 1e-9), a = Math.floor(hash3(n, 1, 5) * ROPES), b = (a + 1 + Math.floor(hash3(n, 2, 5) * (ROPES - 1))) % ROPES;
    ropeRange(D.ropes[a], white, ...span30(hash3(n, 3, 5)), snare * 0.8);
    ropeRange(D.ropes[b], white, ...span30(hash3(n, 4, 5)), snare * 0.8);
  }
  if (toms > 0.02) {
    const n = lowerBound(X.toms.times, t + 1e-9);
    ropeRange(D.ropes[n % ROPES], colour, ...span30(hash3(n, 6, 5)), toms * 0.7);
  }
  D.ups.forEach((u) => { u.h = h; u.s = sat; u.b = kick * 0.9; });
  D.spill.forEach((l) => { l.h = h; l.s = sat; l.b = kick * 0.8; });
  D.strip.h = h; D.strip.b = kick * 0.7;
  const impact = pulseAt(X.impacts, t, 3);
  D.cove.forEach((cv) => { cv.h = h; cv.s = 0; cv.b = 0.6 * crash + 0.5 * impact; });
  D.spot = 0; D.ambient = 0.35; D.lens = 0.05;
  D.smoke = { h, s: sat, b: 0.03 + 0.35 * kick };
}
const span30 = (u) => { const a = u * 0.7; return [a, a + 0.3]; };

// ---- Stained Glass: harmony. The twelve roots sit on a ring over the ceiling in circle-of-fifths order, the
// key's home at the stage. Each chord lights its area; minor takes the first colour, major the second, and
// chords far from the key go whiter. It fades over a beat at every change, and it never pulses.
function glassData(P, X, A) {
  if (X.glass) return X.glass;
  const raw = ((P.song.cues || {}).chords || []).filter((c) => c[2] !== "N").map(([a, b, lab]) => ({ a, b, root: PC[lab.replace("m", "")] ?? 0, minor: /m$/.test(lab) }));
  const list = raw.length ? raw : (P.roots || []).map(([a, b, root]) => ({ a, b, root, minor: !!P.palette.minor }));
  if (!list.length) list.push({ a: 0, b: Infinity, root: P.tonic, minor: !!P.palette.minor });
  const base = P.palette;
  for (const c of list) {
    const f = fifthsFrom(c.root, P.tonic), th = (f * 30 - 90) * Math.PI / 180;
    c.th = th; c.cx = 1.5 + 1.5 * Math.cos(th); c.cz = 3.5 + 3.5 * Math.sin(th);
    c.h = c.minor ? base.primary : base.secondary; c.s = A.sat * (1 - 0.09 * Math.abs(f));
  }
  X.glass = { list, starts: list.map((c) => c.a) };
  return X.glass;
}
const glassField = (c, r, col) => Math.max(0, Math.exp(-(sq(col - c.cx) / 2.4 + sq(r - c.cz) / 5)) - 0.25) / 0.75;
function drawGlass(P, S, A, D, X) {
  const Y = glassData(P, X, A), t = S.t, i = Math.max(0, lowerBound(Y.starts, t + 1e-9) - 1), cur = Y.list[i], prev = Y.list[Math.max(0, i - 1)];
  const fade = i === 0 ? 1 : clamp01((t - cur.a) / S.beatDur), lvl = (0.4 + 0.6 * S.baseLevel) * (0.45 + 0.55 * slowAt(X, t));  // calm parts are where this look lives, so they stay well lit
  for (const L of D.ceil) {
    const a = fade * glassField(cur, L.r, L.c), p = (1 - fade) * glassField(prev, L.r, L.c);
    L.b = (a + p) * lvl * 0.85;
    const c = a >= p ? cur : prev; L.h = c.h; L.s = c.s;
  }
  // the ropes glow in the current chord's colour, strongest on the rope nearest its place on the ring
  const dip = 1 - 0.7 * (1 - fade);
  hslRGB(cur.h, cur.s, 0.5, RGB);
  const ROPE_TH = [-1.9, -1.2, 0, 1.2, 1.9];
  for (let k = 0; k < ROPES; k++) {
    const w = 0.35 + 0.65 * sq(Math.cos(cur.th - ROPE_TH[k])), px = D.ropes[k];
    for (let j = 0; j < ROPE_PIX; j++) {
      const x = (j + 0.5) / ROPE_PIX;
      addPx(px, j, RGB, lvl * 0.6 * w * dip * Math.sin(Math.PI * x) * (0.75 + 0.25 * Math.sin(x * 7 + S.rel * 0.2 + k)));
    }
  }
  D.ups.forEach((u) => { u.h = cur.h; u.s = cur.s; u.b = lvl * 0.7 * dip; });
  D.spill.forEach((l) => { l.h = cur.h; l.s = cur.s; l.b = lvl * 0.6 * dip; });
  D.cove.forEach((cv, j) => { cv.h = cur.h; cv.s = cur.s; cv.b = lvl * 0.9 * dip * (j ? 0.6 + 0.4 * cur.cx / 3 : 1 - 0.4 * cur.cx / 3); });
  D.strip.h = cur.h; D.strip.b = 0;
  D.spot = S.energy.vocals * 0.5;
  D.smoke = { h: cur.h, s: cur.s, b: 0.05 + 0.25 * lvl };
  D.lens = 0.1;
}

// ---- Spotlight: the Club show steps back for the voice. The voice track is smoothed both ways, so the room
// starts to dim before the singer comes in. It reads when a voice sounds, not what it says (Whisper invents text).
function voiceData(P, X) {
  if (X.voice) return X.voice;
  const F = P.song.features || {}, C = P.song.cues || {}, n = P.env.mix.length, v = new Float32Array(n);
  let regions = (F.vocals || {}).regions || [];
  if (!regions.length) regions = C.lines || [];
  for (const r of regions) for (let f = Math.max(0, Math.round(r[0] * 100)), e = Math.min(n, Math.round(r[1] * 100)); f < e; f++) v[f] = 1;
  X.voice = smooth(v, 30);
  return X.voice;
}
const voiceAt = (P, X, t) => { const Y = voiceData(P, X), f = Math.round(t * 100); return f >= 0 && f < Y.length ? Y[f] : 0; };
function drawSpot(P, S, A, D, X) {
  drawClub(P, S, A, D);
  const t = S.t, v = voiceAt(P, X, t), dim = 1 - v * (S.energetic ? 0.55 : 0.85), voc = S.energy.vocals, { pal, accent } = A;
  for (const L of D.ceil) L.b *= dim;
  for (const px of D.ropes) for (let i = 0; i < px.length; i++) px[i] *= dim;
  for (const u of D.ups) u.b *= dim;
  for (const l of D.spill) l.b *= dim;
  for (const cv of D.cove) cv.b *= dim;
  D.strip.b *= dim;
  D.spot = Math.max(D.spot * dim, v * (0.35 + 0.65 * voc));
  // the crowd's own light under the singer, and the line on a rope, as far out as it's high
  setLamp(D, 0, 1, v * (0.15 + 0.45 * voc), pal.primary, 0.12);
  setLamp(D, 0, 2, v * (0.15 + 0.45 * voc), pal.primary, 0.12);
  if (S.vocal) {
    hslRGB(accent, 0.25, 0.5, RGB);
    const x = clamp((S.vocal[2] - P.lineLo) / Math.max(1, P.lineHi - P.lineLo), 0.06, 0.94);
    ropeBlob(D.ropes[((S.noteIdx % ROPES) + ROPES) % ROPES], RGB, x, 0.05, S.noteEnv);
  }
  // vocal chops flicker on a small group of lamps
  const chops = (P.song.cues || {}).chops || [], ci = lowerBound(chops, t + 1e-9, (c) => c[0]) - 1;
  if (ci >= 0 && t < chops[ci][1]) {
    const on = Math.floor(S.pos * 8) % 2 === 0 ? 0.7 : 0.08, c = ci % 2 ? 1 : 2;
    setLamp(D, 3, c, on, pal.primary, 0); setLamp(D, 4, 3 - c, on, pal.primary, 0);
  }
}

// ---- Weather: timbre, in real time, not in beats. Bright sounds lift the light from the floor to the ceiling, noisy
// ones whiten it and fire the smoke machine, and the synths' stereo position moves it across the room.
function weatherData(P, X) {
  if (X.weather) return X.weather;
  const T = (P.song.features || {}).timbre;
  const fps = T ? T.fps : 20;
  const bright = T && T.brightness && T.brightness.mix ? normTrack(decode(T.brightness.mix), 1.5 * fps) : null;
  const noise = T && T.noisiness && T.noisiness.mix ? normTrack(decode(T.noisiness.mix), 1.5 * fps) : null;
  const smoke = [];
  if (noise) {
    let last = -1e9;
    for (let i = 1; i < noise.length; i++) if (noise[i] >= 0.6 && noise[i - 1] < 0.6 && i / fps - last > 8) { last = i / fps; smoke.push({ t: last, kind: "weather", dur: 2.5, amount: 0.3, n: 60, life: 14 }); }
  }
  X.weather = { fps, bright, noise, smoke };
  return X.weather;
}
function panAt(P, t) {
  const n = (P.pan || []).length;
  if (!n) return 0;
  const j = Math.max(0, lowerBound(P.panStarts, t + 1e-9) - 1), a = P.pan[j], b = P.pan[Math.min(n - 1, j + 1)];
  return a[1] + (b[1] - a[1]) * clamp01((t - a[0]) / Math.max(1e-3, b[0] - a[0]));
}
function drawWeather(P, S, A, D, X) {
  const Y = weatherData(P, X), t = S.t, f = Math.round(t * Y.fps), base = P.palette, sat = A.sat;
  const at = (a, d) => (a && f >= 0 && f < a.length ? a[f] : d);
  const B = at(Y.bright, 0.5), N = at(Y.noise, 0.3), M = 0.25 + 0.75 * slowAt(X, t), pan = panAt(P, t);
  const floorW = clamp01(1.4 - 2.2 * B), wallW = clamp01(1.2 - 2 * Math.abs(B - 0.5)), ceilW = clamp01(2.2 * B - 0.8), s = sat * (1 - 0.75 * N);
  const side = (x) => clamp(1 + pan * x * 1.2, 0.15, 1.6);
  for (const L of D.ceil) {
    const cloud = 0.5 + 0.5 * Math.sin(0.9 * L.r + 1.7 * L.c + t * 1.1) * Math.sin(0.6 * L.r - 1.1 * L.c - t * 0.7 + 2);
    L.b = ceilW * M * side((L.c - 1.5) / 1.5) * 0.65 * Math.max(0, cloud - 0.6) / 0.4; L.h = base.secondary; L.s = s;
  }
  hslRGB(base.secondary, s, 0.5, RGB);
  const SIDE = [0.4, -0.4, 0.6, -0.4, 0.4];
  for (let k = 0; k < ROPES; k++) {
    const px = D.ropes[k], w = (0.1 + 0.9 * ceilW) * M * 0.45 * side(SIDE[k]);
    for (let i = 0; i < ROPE_PIX; i++) { const x = (i + 0.5) / ROPE_PIX, c = 0.5 + 0.5 * Math.sin(x * 5 + k * 2 + t * 1.3); addPx(px, i, RGB, w * c * c); }
  }
  const UP_X = [-1, -0.33, 0.33, 1];
  D.ups.forEach((u, i) => { u.h = base.primary; u.s = s; u.b = floorW * M * 0.9 * side(UP_X[i]); });
  D.spill.forEach((l, i) => { l.h = base.primary; l.s = s; l.b = floorW * M * 0.7 * side(i ? 1 : -1); });
  D.strip.h = base.primary; D.strip.b = floorW * M * 0.7;
  D.cove.forEach((cv, i) => { cv.h = i ? base.secondary : base.primary; cv.s = s; cv.b = wallW * M * 0.8 * side(i ? 1 : -1); });
  D.spot = 0; D.lens = 0.08;
  D.smoke = { h: base.primary, s, b: 0.05 + 0.35 * M };
}

// ---- Club 2: Club with the clutter taken out. One job per fixture, and nothing changes its mind inside a section:
//   ceiling  only the part that leads this section (vocal, synth stabs or synth line), or the kick if nothing melodic is there
//   walls    the same part, slower, so the room's overall light moves with it; and crashes and impacts
//   ropes    the next part, quietly, as a 30% stretch per note
//   uplights kick on the inner pair, snare on the outer pair, kept below the lead
//   strip    the bass level
//   DJ spot  the vocal
// What leads: the part with enough notes and the steadiest rhythm from bar to bar. Loudness can't tell: on Lean On the vocal is
// at least as loud as the synth in every section, but the synth's stabs repeat most steadily, and one lighter (map 29a78) follows those.
// Tuned by eye on a few songs: it's a heuristic, and no ground truth says which part leads.
const PART_NAME = { vocal: "vocal", stabs: "synth stabs", lead: "synth", drums: "drums" };
function hookScore(P, list, a, b) {
  const pa = P.beatPos(a), bars = (P.beatPos(b) - pa) / 4, n = Math.floor(bars);
  const on = list.filter((h) => h.t >= a && h.t < b);
  if (bars < 1 || !on.length) return 0;
  const grid = Array.from({ length: Math.max(1, n) }, () => new Array(16).fill(0));
  for (const h of on) { const x = (P.beatPos(h.t) - pa) / 4, i = Math.floor(x); if (i < n) grid[i][Math.round((x - i) * 16) % 16] = 1; }
  let sum = 0, pairs = 0;
  for (let i = 1; i < n; i++) {
    let d = 0, u = 0, v = 0;
    for (let k = 0; k < 16; k++) { d += grid[i][k] * grid[i - 1][k]; u += grid[i][k]; v += grid[i - 1][k]; }
    if (u && v) { sum += d / Math.sqrt(u * v); pairs++; }
  }
  return Math.min(1, on.length / bars / 3) * (0.3 + 0.7 * (pairs ? sum / pairs : 0));
}
function leadPlan(P, X) {
  if (X.leadPlan) return X.leadPlan;
  const notes = P.song.notes || {}, C = P.song.cues || {};
  const snap = (t) => { const p = P.beatPos(t), q = Math.round(p * 4) / 4; return Math.abs(p - q) * 60 / P.bpm <= 0.04 ? P.beatTime(q) : t; };
  const pitched = (list) => (list || []).map((n) => ({ t: snap(n[0]), off: n[1], p: n[2], v: n[3] ?? 0.8 })).sort((a, b) => a.t - b.t);
  const parts = {
    vocal: pitched(notes.game), lead: pitched(notes.lead),
    stabs: (C.stabs || []).map((s) => ({ t: s[0], off: s[0], p: null, v: Math.min(1, 0.45 + s[1] / 10) })).sort((a, b) => a.t - b.t),
    drums: Array.from(X.kick.times, (t, i) => ({ t, off: t, p: null, v: X.kick.amps[i] })),
  };
  const range = {};
  for (const k of ["vocal", "lead"]) { const s = parts[k].map((n) => n.p).sort((a, b) => a - b); range[k] = s.length ? [s[Math.floor(s.length * 0.1)], Math.max(s[Math.floor(s.length * 0.9)], s[Math.floor(s.length * 0.1)] + 1)] : [60, 72]; }
  const secs = (P.sections || []).length ? P.sections : [{ start: 0, end: P.song.duration, kind: "Groove" }];
  const seed = fnv(P.song.id || ""), kinds = ["Intro", "Groove", "Breakdown", "Build", "Drop", "Outro"];
  let prev = null;
  const sections = secs.map((s) => {
    const short = (P.beatPos(s.end) - P.beatPos(s.start)) / 4 < 2;
    const ranked = ["vocal", "stabs", "lead"].map((k) => [k, hookScore(P, parts[k], s.start, s.end)]).sort((a, b) => b[1] - a[1]);
    const lead = short && prev ? prev.lead : ranked[0][1] >= 0.12 ? ranked[0][0] : "drums";
    const second = short && prev ? prev.second : lead !== "drums" && ranked[1][1] >= 0.15 ? ranked[1][0] : null;
    prev = { a: s.start, b: s.end, kind: s.kind, lead, second, scores: Object.fromEntries(ranked), shape: (seed + kinds.indexOf(s.kind) * 7) % 3 };
    return prev;
  });
  X.leadPlan = { parts, range, sections, starts: sections.map((s) => s.a) };
  return X.leadPlan;
}
export const club2Plan = (P) => leadPlan(P, shared(P));
const leadAt = (Y, t) => Y.sections[Math.max(0, lowerBound(Y.starts, t + 1e-9) - 1)];
function drawClub2(P, S, A, D, X) {
  const Y = leadPlan(P, X), t = S.t, base = P.palette, sat = A.sat, sec = leadAt(Y, t), level = 0.3 + 0.7 * S.baseLevel;
  const high = S.kind === "Drop" || S.kind === "Groove" || S.inBuild, main = high ? base.primary : base.secondary, under = high ? base.secondary : base.primary;
  D.note = S.inBuild ? "build" : `ceiling: ${PART_NAME[sec.lead]}${sec.second ? ` · ropes: ${PART_NAME[sec.second]}` : ""}`;
  // ceiling, walls and ropes
  const p = S.buildProgress;
  let wall = S.inBuild ? 0.05 + 0.3 * p : 0;
  if (S.inBuild) {  // one row walking to the stage, a step per drum hit; the ropes close in on the hub
    const row = CEIL_ROWS - 1 - (S.buildSteps % CEIL_ROWS), pulse = Math.max(S.hit, 0);
    for (let c = 0; c < CEIL_COLS; c++) setLamp(D, row, c, (0.3 + 0.55 * p) * (0.5 + 0.5 * pulse), main, sat * (1 - 0.7 * p));
    hslRGB(under, sat * (1 - 0.7 * p), 0.5, RGB);
    for (const px of D.ropes) ropeRange(px, RGB, 1 - p, 1, 0.1 + 0.5 * pulse * (0.4 + 0.6 * p));
  } else {
    const list = Y.parts[sec.lead], last = lowerBound(list, t + 1e-9, (h) => h.t) - 1, first = lowerBound(list, sec.a, (h) => h.t);
    const barLen = 240 / P.bpm, held = last >= 0 && t - list[last].t < 2 * barLen;  // lights stay as the last hit left them, like a person's "on": the room doesn't go out between hits
    if (held) wall = list[last].v * (0.35 + 0.65 * Math.exp(-(t - list[last].t) * 2.5));
    for (let i = last, k = 0; i >= 0 && k < 4; i--, k++) {
      const h = list[i], age = t - h.t, env = Math.exp(-age * 6), b = Math.min(1, 1.25 * level * h.v * Math.max(env, h.off > t || (k === 0 && held) ? (high ? 0.5 : 0.35) : 0));
      if (b < 0.02) continue;
      const s = sat * (1 - 0.6 * env);
      if (h.p != null) {  // a note: its row is its pitch; a full row in a drop, and two lamps taking turns when calm
        const [lo, hi] = Y.range[sec.lead], row = clamp(Math.round((h.p - lo) / (hi - lo) * (CEIL_ROWS - 1)), 0, CEIL_ROWS - 1), side = i % 2;
        if (high) for (let c = 0; c < CEIL_COLS; c++) setLamp(D, row, c, b, main, s);
        else { setLamp(D, row, side ? 0 : 1, b, main, s); setLamp(D, row, side ? 2 : 3, b, main, s); }
      } else {  // a stab or kick has no pitch: it steps along the section's shape, a full row in a drop, the inner lamps when calm
        const n = i - first, cols = high && sec.shape !== 2 ? [0, 1, 2, 3] : [1, 2], tri = Math.abs(((n % 14) + 14) % 14 - 7);
        const rows = sec.shape === 0 ? [((n % CEIL_ROWS) + CEIL_ROWS) % CEIL_ROWS] : sec.shape === 1 ? [tri] : [3 - ((n % 4) + 4) % 4, 4 + ((n % 4) + 4) % 4];
        for (const r of rows) for (const c of cols) setLamp(D, r, c, b, main, s);
      }
    }
    if (sec.second) {  // the next part: a 30% stretch per note, each on the next rope
      const l2 = Y.parts[sec.second], j = lowerBound(l2, t + 1e-9, (h) => h.t) - 1;
      hslRGB(under, sat, 0.5, RGB);
      for (let i = j, k = 0; i >= 0 && k < 2; i--, k++) {
        const h = l2[i], age = t - h.t, g = 0.5 * level * h.v * Math.max(Math.exp(-age * 8), h.off > t ? 0.3 : 0);
        if (g < 0.01) continue;
        const u = h.p != null ? clamp((h.p - Y.range[sec.second][0]) / (Y.range[sec.second][1] - Y.range[sec.second][0]), 0, 1) * 0.7 : hash3(i, 5, 11) * 0.7;
        ropeRange(D.ropes[i % ROPES], RGB, u, u + 0.3, g);
      }
    }
  }
  // the drums on the uplights and spill, kept below the lead so they don't take over the room again
  const kick = pulseAt(X.kick, t, 12), snare = pulseAt(X.snare, t, 14), crash = pulseAt(X.crash, t, 3.5), impact = pulseAt(X.impacts, t, 3);
  D.ups.forEach((u, i) => { const inner = i === 1 || i === 2; u.h = under; u.s = inner ? sat : 0.3; u.b = (0.15 + (inner ? kick * 0.4 : snare * 0.35)) * level; });
  D.spill.forEach((l) => { l.h = under; l.s = sat; l.b = (0.12 + kick * 0.25) * level; });
  D.strip.h = under; D.strip.b = S.energy.bass * 0.6 * level;
  D.cove.forEach((c) => { c.h = main; c.s = sat * (1 - 0.7 * Math.min(1, 2 * crash + impact)); c.b = wall * level + 0.6 * crash + 0.5 * impact; });
  D.spot = S.energy.vocals * (high ? 0.6 : 1);
  D.lens = 0.1;
  D.smoke = { h: main, s: sat, b: 0.05 + 0.3 * level * Math.max(kick, 0.3) };
}

// ---- Human Lighter: a community map's lighting events, replayed. The game's state rules: a group stays as its last event left it.
// Value 0 off; 1, 5 on; 2, 6 flash, then on; 3, 7 flash, then fade out; 4, 8 transition, held as on. 1-4 are one colour, 5-8 the
// other, and a boost event swaps them; here they're the song's pair. The flash and fade times are approximate, from memory of the game.
// Nothing here looks at the music: if it lines up, that's the mapper's doing. Toggles that reshape a show don't apply to it.
const HUMAN_ON = 0.55, HUMAN_FLASH_S = 0.6, HUMAN_FADE_S = 1.5, LASER_RATE = 0.12, LASER_BAND = 0.3;
function humanData(P, X, which) {
  const maps = (P.song.lightshow || {}).maps || [], i = Math.max(0, Math.min(which || 0, maps.length - 1));
  X.human = X.human || [];
  if (X.human[i]) return X.human[i];
  const m = maps[i], group = (ty) => { const e = m.events.filter((x) => x[1] === ty); return { t: Float64Array.from(e, (x) => x[0]), v: Int32Array.from(e, (x) => x[2]) }; };
  // each laser speed event starts the lasers from a new place, as in the game, and sets how fast they run
  const speed = (ty, side) => { const g = group(ty); return { ...g, p: Float64Array.from(g.t, (_, k) => hash3(k, side, 9)) }; };
  X.human[i] = { map: m, back: group(0), ring: group(1), left: group(2), right: group(3), center: group(4), boost: group(5), spin: group(8).t, zoom: group(9).t, leftSpeed: speed(12, 1), rightSpeed: speed(13, 2) };
  return X.human[i];
}
function humanLight(g, t) {
  const i = lowerBound(g.t, t + 1e-9) - 1;
  if (i < 0) return { b: 0, w: 0, red: false };
  const v = g.v[i], age = t - g.t[i], red = v >= 5, kind = v === 0 ? 0 : ((v - 1) % 4) + 1;  // 1 on, 2 flash, 3 fade, 4 transition
  const flash = Math.exp(-age / (HUMAN_FLASH_S / 3));
  if (kind === 0) return { b: 0, w: 0, red };
  if (kind === 2) return { b: HUMAN_ON + (1 - HUMAN_ON) * flash, w: flash, red };
  if (kind === 3) { const a = age / HUMAN_FADE_S; return a >= 1 ? { b: 0, w: 0, red } : { b: (1 - a) * (1 - a), w: flash, red }; }
  return { b: HUMAN_ON, w: 0, red };
}
function laserBand(px, rgb, spd, t, k, g) {  // a 30% stretch of the rope, running at the laser's speed
  const i = lowerBound(spd.t, t + 1e-9) - 1, x = i >= 0 ? spd.p[i] + spd.v[i] * LASER_RATE * (t - spd.t[i]) : 0.5, a = (((x + k * 0.37) % 1) + 1) % 1;
  ropeRange(px, rgb, a, Math.min(1, a + LASER_BAND), g);
  if (a + LASER_BAND > 1) ropeRange(px, rgb, 0, a + LASER_BAND - 1, g);
}
function drawHuman(P, S, A, D, X) {
  const Y = humanData(P, X, A.map), t = S.t, base = P.palette, sat = A.sat;
  D.ambient = 0.6; D.lens = 0.08; D.spot = 0; D.smoke.b = 0.05;
  if (t < Y.map.cover[0] || t > Y.map.cover[1]) return;  // no one lit this part of our version of the song
  const bi = lowerBound(Y.boost.t, t + 1e-9) - 1, boost = bi >= 0 && Y.boost.v[bi] === 1;
  const hue = (l) => (l.red !== boost ? base.secondary : base.primary), satOf = (l) => sat * (1 - 0.65 * l.w);
  const back = humanLight(Y.back, t), ring = humanLight(Y.ring, t), left = humanLight(Y.left, t), right = humanLight(Y.right, t), center = humanLight(Y.center, t);
  D.ups.forEach((u) => { u.b = back.b * 0.9; u.h = hue(back); u.s = satOf(back); });
  D.spill.forEach((l) => { l.b = back.b * 0.7; l.h = hue(back); l.s = satOf(back); });
  // rings: every other row, which ring spin steps along; ring zoom moves them between the inner and the outer lamps. A quarter of the ceiling.
  const spin = lowerBound(Y.spin, t + 1e-9), zoom = lowerBound(Y.zoom, t + 1e-9);
  for (const L of D.ceil) {
    L.h = hue(ring); L.s = satOf(ring);
    if ((L.r + spin) % 2 === 0 && (zoom % 2 ? L.c === 0 || L.c === 3 : L.c === 1 || L.c === 2)) { L.b = ring.b; L.sway = (spin % 2 ? 1 : -1) * 6; }
  }
  const leftRGB = hslRGB(hue(left), satOf(left), 0.5, [0, 0, 0]), rightRGB = hslRGB(hue(right), satOf(right), 0.5, [0, 0, 0]);
  if (left.b > 0) { laserBand(D.ropes[1], leftRGB, Y.leftSpeed, t, 1, left.b); laserBand(D.ropes[3], leftRGB, Y.leftSpeed, t, 3, left.b); laserBand(D.ropes[2], leftRGB, Y.leftSpeed, t, 2, left.b * 0.5); }
  if (right.b > 0) { laserBand(D.ropes[0], rightRGB, Y.rightSpeed, t, 0, right.b); laserBand(D.ropes[4], rightRGB, Y.rightSpeed, t, 4, right.b); laserBand(D.ropes[2], rightRGB, Y.rightSpeed, t, 2, right.b * 0.5); }
  D.strip.h = hue(center); D.strip.b = center.b * 0.9;
  D.cove.forEach((c) => { c.h = hue(center); c.s = satOf(center); c.b = center.b * 0.8; });
  D.smoke = { h: hue(back), s: sat, b: 0.05 + 0.3 * Math.max(back.b, ring.b, center.b) };
}
const fmtTime = (t) => `${Math.floor(t / 60)}:${String(Math.floor(t % 60)).padStart(2, "0")}`;
function humanInfo(P, t, which) {
  const maps = P.song.lightshow.maps, m = maps[Math.max(0, Math.min(which || 0, maps.length - 1))];
  const where = t < m.cover[0] ? ` · the map starts at ${fmtTime(m.cover[0])}` : t > m.cover[1] ? " · the map has ended" : "";
  return `Human Lighter: ${m.name} · beatsaver.com/maps/${m.map}${where}`;
}

// ---- the looks by id, and which fixture groups Slow Burn hands them first
const ORDER = {
  club: ["ceil", "ropes", "ups", "cove", "strip", "spot"], club2: ["ceil", "ups", "ropes", "strip", "cove", "spot"], mixer: ["ceil", "strip", "ropes", "ups", "cove", "spot"], score: ["ceil", "ropes", "ups", "strip", "cove", "spot"],
  shock: ["ceil", "ropes", "ups", "strip", "cove", "spot"], breath: ["cove", "ups", "ropes", "ceil", "strip", "spot"], kick: ["ceil", "ups", "strip", "ropes", "cove", "spot"],
  glass: ["ceil", "cove", "ropes", "ups", "spot", "strip"], spot: ["spot", "ceil", "ropes", "cove", "ups", "strip"], weather: ["ups", "cove", "ropes", "ceil", "strip", "spot"],
};
const DRAW = { club: drawClub, club2: drawClub2, mixer: drawMixer, score: drawScore, shock: drawShock, breath: drawBreath, kick: drawKick, glass: drawGlass, spot: drawSpot, weather: drawWeather, human: drawHuman };
const NAME = Object.fromEntries(LOOK_LIST.map((l) => [l.id, l.name]));

// ---- Director: a look per section, from the analysis. The song's style gives the home look for its drops.
function homeLook(P) {
  const style = (((P.song.cues || {}).preset || {}).style || "").toLowerCase();
  if (/techno|minimal/.test(style)) return "kick";
  if (/drum n bass|dubstep|breakbeat|hardstyle|hardcore|donk|halftime|trap|bassline|electro|big room/.test(style)) return "shock";
  if (/trance|hands up|goa|italodance|eurodance|progressive/.test(style)) return "score";
  if (/house|disco|garage/.test(style)) return "breath";
  return "mixer";
}
function directorPlan(P, X) {
  if (X.plan) return X.plan;
  const home = homeLook(P), voice = voiceData(P, X);
  const mean = (a, s) => { let v = 0, n = 0; for (let f = Math.round(s.start * 100), e = Math.min(a.length, Math.round(s.end * 100)); f < e; f += 25) { v += a[f]; n++; } return n ? v / n : 0; };
  const chords = ((P.song.cues || {}).chords || []).map((c) => c[0]);
  const spans = (P.sections || []).map((s) => {
    const minutes = Math.max(1 / 60, (s.end - s.start) / 60), vo = mean(voice, s), drums = mean(P.env.drums, s), other = mean(P.env.other, s);
    const changes = chords.filter((t) => t >= s.start && t < s.end).length / minutes;
    let look;
    switch (s.kind) {
      case "Drop": look = home; break;
      case "Build": look = "shock"; break;
      case "Groove": look = home; break;  // the drop's music just below its level: the same treatment, without the strobes
      case "Breakdown": look = vo >= 0.5 ? "spot" : changes >= 6 ? "glass" : "weather"; break;
      default: look = vo >= 0.5 ? "spot" : drums > 0.35 && other < 0.25 ? "kick" : "weather";
    }
    return { a: s.start, b: s.end, look };
  });
  X.plan = { home, spans, starts: spans.map((s) => s.a) };
  return X.plan;
}
export function directedLook(P, t, inBuild) {
  const plan = directorPlan(P, shared(P));
  if (inBuild) return "shock";
  const i = lowerBound(plan.starts, t + 1e-9) - 1;
  return i >= 0 && t < plan.spans[i].b ? plan.spans[i].look : plan.home;
}

// ---- toggles
function biggestDrop(P) {  // the longest breakdown before it; of equals, the later
  let best = -1, score = -1;
  P.drops.forEach((d, i) => { const v = d.breakdown_bars || 0; if (v >= score) { best = i; score = v; } });
  return best;
}
// Déjà Vu gives all of a song's drops one strobe type, its own: on the Beat Saber maps a lighter's treatment repeats over a
// song's drops whether or not the music has the same letter (.78, different letters .77; other songs .25). Slow Burn keeps
// the VP1s and the full bar for the biggest drop. Both only change P.drops.
function applyToggles(P, o) {
  const key = `${o.dejavu ? 1 : 0}${o.burn ? 1 : 0}`;
  if (P.toggleKey === key) return;
  P.toggleKey = key;
  for (const d of P.drops) { d.baseType = d.baseType ?? d.strobeType; d.baseBeats = d.baseBeats ?? d.strobeBeats; d.strobeType = d.baseType; d.strobeBeats = d.baseBeats; }
  if (o.dejavu) {
    const TYPES = ["blinder", "spin", "pairs"], type = TYPES[fnv(P.song.id || "") % TYPES.length];
    for (const d of P.drops) d.strobeType = type;
  }
  if (o.burn) {
    const big = biggestDrop(P);
    P.drops.forEach((d, i) => {
      if (i === big) { d.strobeType = "all"; return; }
      if (d.strobeType === "blinder" || d.strobeType === "all") d.strobeType = i % 2 ? "pairs" : "spin";
      d.strobeBeats = Math.min(d.strobeBeats, 2);
    });
  }
}
// Weather's smoke only joins the machine's plan while its look can run, so other looks keep their own smoke exactly
function syncWeatherSmoke(P, look) {
  if (P.smokeLook === look) return;
  P.smokeLook = look;
  let add = [];
  if (look === "weather" || look === "director") {
    const X = shared(P), Y = weatherData(P, X);
    add = look === "weather" ? Y.smoke : Y.smoke.filter((ev) => directedLook(P, ev.t, false) === "weather");
  }
  P.smoke = P.smoke.filter((ev) => ev.kind !== "weather").concat(add).sort((a, b) => a.t - b.t);
}

// Slow Burn: the rig is rationed over the song. One fixture group in the intro, another with each section, all of
// them only on the biggest drop, then they retire one by one. Ramps over a bar, except the jump on that drop.
function burnPlan(P) {
  if (P.burnPlan) return P.burnPlan;
  const N = 6, big = biggestDrop(P), tBig = big >= 0 ? P.drops[big].time : P.song.duration * 0.66, bar = 4 * 60 / P.bpm;
  const starts = [...new Set((P.sections || []).map((s) => s.start))].filter((t) => t > 0).sort((a, b) => a - b);
  const before = starts.filter((t) => t < tBig - 1e-3), after = starts.filter((t) => t > tBig + bar);
  const key = [[0, 1, false]];
  before.forEach((t, i) => key.push([t, 1 + Math.floor((N - 2) * (i + 1) / before.length), false]));
  key.push([tBig, N, true]);
  after.forEach((t, i) => key.push([t, Math.max(2, N - 1 - i), false]));
  P.burnPlan = { key, starts: key.map((k) => k[0]), bar };
  return P.burnPlan;
}
export function burnCount(P, t) {
  const B = burnPlan(P), i = Math.max(0, lowerBound(B.starts, t + 1e-9) - 1), k = B.key[i];
  if (i === 0 || k[2]) return k[1];
  const prev = B.key[i - 1][1];
  return prev + (k[1] - prev) * clamp01((t - k[0]) / B.bar);
}
function applyBurn(P, S, D, order) {
  const n = burnCount(P, S.t), g = (name) => clamp01(n - order.indexOf(name));
  const ce = g("ceil"), ro = g("ropes"), up = g("ups"), co = g("cove"), st = g("strip"), sp = g("spot");
  for (const L of D.ceil) L.b *= ce;
  if (ro < 1) for (const px of D.ropes) for (let i = 0; i < px.length; i++) px[i] *= ro;
  for (const u of D.ups) u.b *= up;
  for (const l of D.spill) l.b *= up;
  for (const c of D.cove) c.b *= co;
  D.strip.b *= st; D.spot *= sp;
}

// Countdown: a build as a visible timer. A spine of lamps runs back from the stage and goes out a row at a time, the
// last bar counts its beats on the front row, and the ropes fill from the outer anchors towards the hub.
function applyCountdown(S, A, D) {
  if (!S.inBuild) return;
  const span = Math.min(S.buildBeats, 32);
  if (S.toDrop > span) return;
  const { pal, sat, kick } = A, p = 1 - S.toDrop / span, s = sat * (1 - 0.7 * p);
  const barsLeft = Math.ceil(S.toDrop / 4), rows = barsLeft <= 1 ? 1 : Math.min(CEIL_ROWS, Math.ceil(CEIL_ROWS * barsLeft / Math.ceil(span / 4)));
  const beatsLeft = Math.ceil(S.toDrop);
  for (const L of D.ceil) { L.b = 0; L.h = pal.primary; L.s = s; L.sway = 0; }
  if (barsLeft <= 1) for (let c = 0; c < Math.min(CEIL_COLS, beatsLeft); c++) { const L = D.ceil[c]; L.b = 0.45 + 0.5 * kick; }
  else for (let r = 0; r < rows; r++) {
    if (r === rows - 1) { D.ceil[r * CEIL_COLS + 1].b = 0.25 + 0.6 * kick; D.ceil[r * CEIL_COLS + 2].b = 0.25 + 0.6 * kick; }
    else D.ceil[r * CEIL_COLS + (r % 2 ? 1 : 2)].b = 0.14;
  }
  hslRGB(pal.primary, s, 0.5, RGB);
  for (const px of D.ropes) {
    px.fill(0);
    ropeRange(px, RGB, 1 - p, 1, 0.1 + 0.55 * kick * (0.4 + 0.6 * p));
  }
}

// ---- the frame
export function makeDesk() {
  const lamp = () => ({ b: 0, h: 0, s: 1 });
  return {
    ceil: Array.from({ length: CEIL_ROWS * CEIL_COLS }, (_, i) => ({ ...lamp(), r: Math.floor(i / CEIL_COLS), c: i % CEIL_COLS, sway: 0 })),
    ropes: Array.from({ length: ROPES }, () => new Float32Array(ROPE_PIX * 3)),
    ups: Array.from({ length: 4 }, lamp), spill: Array.from({ length: 2 }, lamp), cove: Array.from({ length: 2 }, lamp),
    strip: { b: 0, h: 0 }, spot: 0, vp: 0, ambient: 1, lens: 0.2, smoke: { h: 0, s: 1, b: 0 }, dark: false, look: "club", anim: "", info: "",
  };
}
function clearDesk(D, pal, sat) {
  for (const L of D.ceil) { L.b = 0; L.h = pal.primary; L.s = sat; L.sway = 0; }
  for (const px of D.ropes) px.fill(0);
  D.ups.forEach((u, i) => { u.b = 0; u.h = i % 2 ? pal.secondary : pal.primary; u.s = sat; });
  D.spill.forEach((l, i) => { l.b = 0; l.h = i ? pal.secondary : pal.primary; l.s = sat; });
  D.cove.forEach((c, i) => { c.b = 0; c.h = i ? pal.secondary : pal.primary; c.s = sat; });
  D.strip.b = 0; D.strip.h = pal.primary; D.spot = 0; D.vp = 0; D.ambient = 1; D.lens = 0.2;
}
const CLUB_ANIMS = ["wave", "cross", "fan", "circle", "sweep"];

// One frame. o: { look, dejavu, burn, countdown, pal (the frame's colour pair, leader first), flash (the rope strobe's rgb) }
export function frame(P, S, D, o) {
  const sat = o.pal.minor ? 1 : 0.9, kick = Math.max(S.hit, 0.8 * S.stab);
  clearDesk(D, o.pal, sat);
  applyToggles(P, o);
  syncWeatherSmoke(P, o.look);
  D.dark = S.blackout || S.strobing;
  D.look = o.look; D.info = NAME[o.look] || "";
  if (D.dark) {
    // the strobes, on top of a dark room. VP1s: types blinder and all. Ropes: one 30% stretch per flash round the hub
    // (spin), or two on different ropes (pairs, and with the VP1s); never all at once.
    D.vp = S.strobeType === "blinder" || S.strobeType === "all" ? S.strobe : 0;
    D.smoke.h = o.pal.primary; D.smoke.s = sat; D.smoke.b = 0;
    if (S.strobing && S.strobe > 0 && S.strobeType !== "blinder") {
      const f = S.strobeIdx, flash = [];
      if (S.strobeType === "spin") flash.push([f % ROPES, ((f * 0.37) % 1) * 0.7]);
      else {
        const a = Math.floor(hash3(f, 1, 7) * ROPES), b = (a + 1 + Math.floor(hash3(f, 2, 7) * (ROPES - 1))) % ROPES;
        flash.push([a, hash3(f, 3, 7) * 0.7], [b, hash3(f, 4, 7) * 0.7]);
      }
      const r = o.flash[0] * 1.6 * S.strobe, g = o.flash[1] * 1.6 * S.strobe, bl = o.flash[2] * 1.6 * S.strobe;
      for (const [rope, start] of flash) {
        const px = D.ropes[rope];
        for (let i = 0; i < ROPE_PIX; i++) {
          const x = (i + 0.5) / ROPE_PIX, e = Math.min(1, Math.max(0, Math.min(x - start, start + 0.3 - x) / 0.02 + 0.5));
          if (e > 0) { px[3 * i] += r * e; px[3 * i + 1] += g * e; px[3 * i + 2] += bl * e; }
        }
      }
    }
    return D;
  }
  const director = o.look === "director", hasShow = ((P.song.lightshow || {}).maps || []).length > 0;
  const id = director ? directedLook(P, S.t, S.inBuild) : o.look === "human" && !hasShow ? "club" : o.look;
  const look = DRAW[id] ? id : "club", X = look === "club" ? null : shared(P);
  const anim = o.dejavu && CLUB_ANIMS.includes(S.anim) ? CLUB_ANIMS[(CLUB_ANIMS.indexOf(S.anim) + fnv(P.song.id || "")) % CLUB_ANIMS.length] : S.anim;
  const A = { pal: o.pal, accent: o.pal.secondary, sat, kick, swap: S.energetic && S.lookBar % 2 === 1, anim, map: o.map || 0 };
  D.anim = anim;
  D.smoke.h = o.pal.primary; D.smoke.s = sat; D.smoke.b = 0.07 + 0.3 * S.level * (0.35 + 0.65 * kick);
  DRAW[look](P, S, A, D, X);
  if (o.burn && look !== "human") applyBurn(P, S, D, director ? ORDER.club : ORDER[look]);
  if ((o.countdown || director) && look !== "human") applyCountdown(S, A, D);
  D.info = director ? `Director: ${NAME[look]}` : look === "human" ? humanInfo(P, S.t, o.map) : o.look === "human" ? "Club: this song has no Beat Saber map" : look === "club2" ? `Club 2 · ${D.note}` : NAME[look];
  return D;
}
