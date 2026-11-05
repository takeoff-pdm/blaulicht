// Club light show simulator for Melody Lab (three.js).
//
// What the fixtures do is the look's job (club_looks.mjs: a menu under the club); this file holds the music logic
// and the scene. Below is the default look, Club.
//
// Everything is a pure function of the song time, so seeking, pausing and scrubbing just work:
//   pacing     the Rekordbox beat grid; movement cycles last 1, 2, 4 or 8 beats and only change on
//              8-beat (2-bar) block boundaries, so every animation stays in phase with the beat;
//              patterns don't glide but jump 8 steps per cycle on the note grid (sixteenths,
//              eighths or quarters), flashing on each step, and harder on the kick;
//   colour     one matched pair per song (red + orange, magenta + cyan, ...), picked by the key on
//              the circle of fifths, plus white; which of the two leads swaps per hook / music
//              change and on chord changes (from the clean bass line); melody notes take the
//              second colour, whiter at their onset. No other hues;
//   melody     the clean, beat-synced main line (melody_line.py: vocal or lead per bar) plays
//              across the ceiling: each note lights two lamps of the row its pitch maps to (low
//              near the stage), flashing on the onset and holding at its velocity while it sounds;
//              recurring 2-bar hooks always get the same pattern and colours;
//   speed      how busy the melody is in the current 2-bar block (note onsets per beat);
//   reacting   pulses come from the drums' real hits (no drums, no pulse; rolls flicker), the
//              section level moves with the loudness, the LED ropes meter the stems, and the
//              pattern and palette change at every music change: drops, Rekordbox phrases, and
//              bar lines where the stems change (an instrument comes in or drops out);
//   animation  the ceiling pattern per segment between music changes, picked from its melody
//              (wide range -> rings, stepwise -> waves, ...); builds run towards the stage;
//   darkness   patterns are narrow bands, rings and sparse checkers: at most about a quarter of
//              the ceiling is lit at once, and calm parts leave the room nearly dark;
//   strobes    only on the drop, when the bass hits: sixteenths from its downbeat for up to one
//              bar, as long as the low end holds, and while they fire they are the only light in
//              the room. A type per drop: VP1 blinder, rope spin (each flash 30% of one rope, rope
//              by rope round the hub), rope pairs (two 30% segments on two ropes), and VP1s with
//              rope pairs on the biggest drop; the ropes never flash all at once. Nothing strobes
//              before the drop; builds run the lights towards it and the
//              last half beat is a blackout;
//   smoke      a smoke machine at the stage edge fires a long fill at the start of each breakdown
//              and a short blast on each drop (selectable under the club); its smoke only shows where
//              light hits it, and the haze it leaves makes the beams stronger;
//   timing     the beat grid comes from the server already moved onto the drum hits (grid.py), and
//              each frame shows the moment it reaches the screen (one frame ahead);
//   crowd      a small knee bounce and sway for everyone, about one in six hops in drops.
//   room       10 x 17 x 4.5 m (wide x long x high), stage on a short wall, panel walls, a bar, a door; a
//              ceiling grid of downlights (4 across, 8 along), a spider web of five 5 m frosted LED
//              ropes (100 pixels each) sagging from one ceiling hub, and four VaryTech VP1 LED panels (warm /
//              cold white, the engine's blinder) in a line along the long axis down the middle; the
//              VP1s are the only strobes. The truss moving heads and truss strobes are commented out
//              for now; the phrase pattern drives the ceiling grid. Walk with WASD, drag to look.
import * as THREE from "three";
import { EffectComposer } from "three/addons/postprocessing/EffectComposer.js";
import { RenderPass } from "three/addons/postprocessing/RenderPass.js";
import { UnrealBloomPass } from "three/addons/postprocessing/UnrealBloomPass.js";
import { OutputPass } from "three/addons/postprocessing/OutputPass.js";
import { RectAreaLightUniformsLib } from "three/addons/lights/RectAreaLightUniformsLib.js";
import { LOOK_LIST, makeDesk, frame, hash3 } from "./club_looks.mjs";

RectAreaLightUniformsLib.init();

const lab = window.MelodyLab;
const host = document.getElementById("club");
const hud = document.getElementById("clubhud");
const opts = { strobes: true, smoke: "both", look: "club", dejavu: false, burn: false, countdown: false };
document.getElementById("strobes").addEventListener("change", (e) => { opts.strobes = e.target.checked; });
document.getElementById("smoke").addEventListener("change", (e) => { opts.smoke = e.target.value; });
// the look (club_looks.mjs) and its toggles, remembered per browser. The A/B page has none of these controls and stays on Club.
{
  const keep = (k, v) => { try { localStorage.setItem("melody.club." + k, v); } catch {} };
  const kept = (k) => { try { return localStorage.getItem("melody.club." + k); } catch { return null; } };
  const sel = document.getElementById("look"), about = document.getElementById("lookabout");
  if (sel) {
    sel.innerHTML = LOOK_LIST.map((l) => `<option value="${l.id}" title="${l.about}">${l.name}</option>`).join("");
    const say = () => { const l = LOOK_LIST.find((x) => x.id === sel.value); opts.look = l.id; if (about) about.textContent = l.about; };
    if (LOOK_LIST.some((l) => l.id === kept("look"))) sel.value = kept("look");
    sel.addEventListener("change", () => { say(); keep("look", sel.value); });
    say();
  }
  for (const k of ["dejavu", "burn", "countdown"]) {
    const box = document.getElementById(k);
    if (!box) continue;
    box.checked = kept(k) === "1"; opts[k] = box.checked;
    box.addEventListener("change", () => { opts[k] = box.checked; keep(k, box.checked ? "1" : "0"); });
  }
}
// manual fog: hold the Fog button (or F) to fire the machine at the current song time; while held,
// a 0.5 s smoke event is added every half second, so the smoke stays a function of the song time
// (it comes back when you seek over it; a new song starts without it)
const fogBtn = document.getElementById("fog");
let fogHeld = false;
const setFog = (on) => { fogHeld = on; fogBtn.classList.toggle("firing", on); };
fogBtn.addEventListener("pointerdown", (e) => { e.preventDefault(); setFog(true); });
for (const ev of ["pointerup", "pointerleave", "pointercancel"]) fogBtn.addEventListener(ev, () => setFog(false));
window.addEventListener("keydown", (e) => { if (e.code === "KeyF" && !e.repeat && !(e.target && /INPUT|SELECT|TEXTAREA/.test(e.target.tagName))) setFog(true); });
window.addEventListener("keyup", (e) => { if (e.code === "KeyF") setFog(false); });
window.addEventListener("blur", () => setFog(false));
function fireFog(P, t) {
  const last = P.manualFog;
  if (last && t >= last.t && t - last.t < 0.5) return;
  const ev = { t, kind: "manual", dur: 0.5, amount: 0.35, n: 40, life: 12 };
  P.smoke.push(ev);
  P.smoke.sort((a, b) => a.t - b.t);
  P.manualFog = ev;
}

// ------------------------------------------------------------------------------------------------
// Music helpers
const PCS = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };
function parseKey(s) {
  if (!s) return null;
  const m = /^([A-G])([#b♯♭]?)(m?)/.exec(s.trim());
  if (!m) return null;
  let pc = PCS[m[1]] + (m[2] === "#" || m[2] === "♯" ? 1 : m[2] === "b" || m[2] === "♭" ? -1 : 0);
  pc = (pc + 12) % 12;
  return { pc, minor: m[3] === "m", name: s };
}
// Colour: one matched pair per song, plus white, which can always be used. The pairs go together
// without having to be complementary; neighbours in the list are related, and the key picks one by
// its place on the circle of fifths (minor keys by their relative major; two neighbouring places
// share a pair, so related keys look related).
const PAIRS = [
  { name: "red + orange", primary: 0, secondary: 28 },
  { name: "pink + purple", primary: 330, secondary: 285 },
  { name: "magenta + cyan", primary: 300, secondary: 185 },
  { name: "blue + violet", primary: 228, secondary: 272 },
  { name: "cyan + blue", primary: 185, secondary: 215 },
  { name: "green + teal", primary: 115, secondary: 170 },
];
function keyPalette(key) {
  if (!key) return { ...PAIRS[3], minor: false };
  const majorPc = key.minor ? (key.pc + 3) % 12 : key.pc;
  return { ...PAIRS[Math.floor(((majorPc * 7) % 12) / 2)], minor: key.minor };
}
// a pitch class's distance from the tonic on the circle of fifths (-5..6)
const fifthsFrom = (pc, tonic) => { const d = ((((pc - tonic) * 7) % 12) + 12) % 12; return d > 6 ? d - 12 : d; };
const PC_NAMES = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];

function lowerBound(arr, t, get = (x) => x) {  // first index with get(arr[i]) >= t
  let lo = 0, hi = arr.length;
  while (lo < hi) { const m = (lo + hi) >> 1; if (get(arr[m]) < t) lo = m + 1; else hi = m; }
  return lo;
}
function noteAt(notes, t) {  // the note sounding at t, or null
  if (!notes || !notes.length) return null;
  let i = lowerBound(notes, t, (n) => n[0]) - 1;
  for (let k = 0; k < 3 && i >= 0; k++, i--) if (notes[i][0] <= t && t < notes[i][1]) return notes[i];
  return null;
}

// Envelope follower over a 100 fps uint8 stem loudness track (build_viz_data.py), normalised to
// 0..1 by the song's own range: fast attack, slower release, like a VU meter.
function follower(e, [lo, hi], attack_s, release_s) {
  const out = new Float32Array(e.length), a = 1 - Math.exp(-0.01 / attack_s), r = 1 - Math.exp(-0.01 / release_s);
  let y = 0;
  for (let i = 0; i < e.length; i++) {
    const x = Math.max(0, Math.min(1, (e[i] - lo) / Math.max(1, hi - lo)));
    y += (x > y ? a : r) * (x - y); out[i] = y;
  }
  return out;
}
// Drum hits: the drums stem rising by 5 dB within 20 ms, peak-picked, at least 60 ms apart;
// strength from the hit's level in the song's range (kicks near 1, hats lower).
function drumHits(e, [lo, hi]) {
  const RISE = 5 * 255 / 60, times = [], amps = [];
  let last = -1e9;
  for (let f = 2; f < e.length - 2; f++) {
    const rise = e[f] - e[f - 2];
    if (rise < RISE || rise < e[f + 1] - e[f - 1] || f - last < 6) continue;
    const amp = Math.max(0, Math.min(1, (Math.max(e[f], e[f + 1], e[f + 2]) - lo) / Math.max(1, hi - lo)));
    if (amp < 0.15) continue;
    times.push((f - 1) / 100); amps.push(amp); last = f;
  }
  return { times: Float64Array.from(times), amps: Float32Array.from(amps) };
}
// 0..1: how hard the drums just hit (the last few hits, each decaying over about 70 ms); weighted
// towards the strong hits, so kicks and snares punch and hi-hats only sparkle
function hitPulse(P, t) {
  const { times, amps } = P.hits;
  let v = 0;
  for (let i = lowerBound(times, t + 1e-9) - 1, k = 0; i >= 0 && k < 3; i--, k++) v = Math.max(v, amps[i] ** 1.5 * Math.exp(-(t - times[i]) * 14));
  return v;
}

// Per-song structures, built once.
let P = null;
function prepare(song) {
  const grid = song.beats && song.beats.beats.length > 1 ? song.beats : null;
  const bpm = (grid && grid.bpm) || 120;
  const beats = grid ? Float64Array.from(grid.beats) : Float64Array.from({ length: Math.ceil(song.duration * bpm / 60) + 2 }, (_, i) => i * 60 / bpm);
  const inBar = grid ? Int8Array.from(grid.beat_in_bar) : Int8Array.from(beats, (_, i) => (i % 4) + 1);
  const firstDown = Math.max(0, inBar.indexOf(1));
  const key = parseKey(song.key);
  const show = song.show || { sections: [], drops: [] };
  // the clean, beat-synced main melody and bass lines from the server (melody_line.py); the raw
  // transcriptions only if a song has none
  const melody = song.notes.line && song.notes.line.length ? song.notes.line
    : [...(song.notes.game || []), ...(song.notes.lead || [])].sort((a, b) => a[0] - b[0]);
  const pitches = melody.map((n) => n[2]).sort((a, b) => a - b);
  const pct = (q) => (pitches.length ? pitches[Math.floor(q * (pitches.length - 1))] : 64);
  const M = song.melody || {};
  const P = { song, bpm, beats, inBar, firstDown, key, palette: keyPalette(key), sections: show.sections || [],
    drops: show.drops || [], phrases: song.phrases || [], melCenter: pct(0.5), lineLo: pct(0.1), lineHi: pct(0.9),
    line: melody, bassLine: song.notes.bassline || song.notes.bass || [],
    roots: M.roots || [], rootStarts: (M.roots || []).map((r) => r[0]),
    hooks: M.hooks || { blocks: [], ids: [], count: 0 } };
  P.tonic = key ? key.pc : 0;
  P.beatPos = (t) => beatPos(P, t);
  P.beatTime = (i) => beatTime(P, i);
  // melodic activity per 8-beat block -> movement period (beats per cycle)
  const onsets = melody.map((n) => n[0]).sort((a, b) => a - b);
  P.blockPeriod = [];
  for (let k = 0; ; k++) {
    const a = beatTime(P, firstDown + 8 * k), b = beatTime(P, firstDown + 8 * (k + 1));
    if (a > song.duration) break;
    const n = lowerBound(onsets, b) - lowerBound(onsets, a);
    const density = n / 8;
    P.blockPeriod.push(density > 1.5 ? 1 : density > 0.75 ? 2 : density > 0.3 ? 4 : 8);
  }
  // What the lights react to, all precomputed so the show stays a pure function of the time:
  // drum hits (onsets in the drums stem), loudness followers per stem (fast attack, slow release),
  // and the points where the music changes.
  const ED = song.energyData, EN = song.energyNorm;
  P.hits = drumHits(ED.drums, EN.drums);
  P.env = {};
  for (const k of ["drums", "bass", "other", "vocals"]) P.env[k] = follower(ED[k], EN[k], 0.01, 0.12);
  P.env.mix = follower(ED.mix, EN.mix, 0.01, 0.2);
  // Music changes: the drops, the Rekordbox phrase starts, and the bar lines where the stems
  // change (an instrument comes in or drops out): distance between the stem levels of the two bars
  // before and the two bars after, peaks above 0.2 (about one every 9 bars).
  const downs = Array.from(beats).filter((_, i) => inBar[i] === 1);
  const STEMS = ["drums", "bass", "other", "vocals"];
  const feats = downs.slice(0, -1).map((a, i) => {
    const fa = Math.floor(a * 100), fb = Math.max(fa + 1, Math.floor(downs[i + 1] * 100));
    return STEMS.map((k) => { let s = 0; for (let f = fa; f < fb; f++) s += ED[k][f] || 0; return s / (fb - fa) / 255; });
  });
  const nov = feats.map((_, b) => {
    if (b < 2 || b + 1 >= feats.length) return 0;
    return Math.hypot(...STEMS.map((_, j) => (feats[b][j] + feats[b + 1][j] - feats[b - 2][j] - feats[b - 1][j]) / 2));
  });
  const barDur = 4 * 60 / bpm, strong = [...P.drops.map((d) => d.time), ...P.phrases.map((p) => p.start)];
  const changes = [...strong];
  nov.forEach((v, b) => {
    if (v > 0.2 && v >= Math.max(...nov.slice(Math.max(0, b - 2), b + 3)) && !strong.some((t) => Math.abs(t - downs[b]) < 1.5 * barDur)) changes.push(downs[b]);
  });
  changes.sort((a, b) => a - b);
  P.segStarts = [-1e9];
  for (const t of changes) if (t - P.segStarts[P.segStarts.length - 1] > 0.9 * barDur) P.segStarts.push(t);
  P.musicChanges = new Set(changes.filter((t) => !strong.includes(t)));
  // ceiling pattern from the melody line between two times: wide range -> rings, stepwise -> waves,
  // leaps -> checkers, few notes -> fans or sweeps
  const ORDER = ["wave", "cross", "fan", "circle", "sweep"];
  const animOf = (start, end, i) => {
    const seq = melody.filter((n) => n[0] >= start && n[0] < end).map((n) => n[2]);
    if (seq.length < 4) return ["sweep", "fan"][i % 2];
    const sorted = [...seq].sort((a, b) => a - b);
    const range = sorted[Math.floor(sorted.length * 0.9)] - sorted[Math.floor(sorted.length * 0.1)];
    let steps = 0; for (let j = 1; j < seq.length; j++) steps += Math.abs(seq[j] - seq[j - 1]) <= 2;
    return range > 12 ? "circle" : steps / (seq.length - 1) > 0.65 ? "wave" : range <= 4 ? "fan" : "cross";
  };
  // per segment (between two music changes), never the same pattern twice in a row
  let prevAnim = null;
  P.segAnim = P.segStarts.map((start, i) => {
    let anim = animOf(start, P.segStarts[i + 1] ?? Infinity, i);
    if (anim === prevAnim) anim = ORDER[(ORDER.indexOf(anim) + 1 + (i % 2)) % ORDER.length];
    prevAnim = anim;
    return anim;
  });
  // per hook: the same music gets the same look every time it comes back. Picked from the hook's
  // first appearance; two hooks in a row (call and response) get different patterns.
  P.hookAnim = [];
  const H = P.hooks;
  H.ids.forEach((id, b) => {
    if (id < 0 || P.hookAnim[id]) return;
    let anim = animOf(H.blocks[b], H.blocks[b + 1] ?? Infinity, id);
    const before = b > 0 && H.ids[b - 1] >= 0 ? P.hookAnim[H.ids[b - 1]] : null;
    if (anim === before) anim = ORDER[(ORDER.indexOf(anim) + 1) % ORDER.length];
    P.hookAnim[id] = anim;
  });
  // strobe length per drop: up to one bar, while the low end (drums + bass) stays within 6 dB of
  // the drop's first beat; a drop that stops dead after the hit strobes only as long as it sounds
  const lowDb = (i) => {
    const fa = Math.floor(beatTime(P, i) * 100), fb = Math.max(fa + 1, Math.floor(beatTime(P, i + 1) * 100));
    let p = 0; for (let f = fa; f < fb; f++) p += 10 ** (((ED.drums[f] || 0) / 255 * 60 - 60) / 10) + 10 ** (((ED.bass[f] || 0) / 255 * 60 - 60) / 10);
    return 10 * Math.log10(p / (fb - fa) + 1e-9);
  };
  // strobe type per drop, so drops don't all look alike: the VP1 blinder, a rope spin (each flash a
  // 30% segment of one rope, rope by rope round the hub) or rope pairs (each flash two 30% segments
  // on two different ropes); the ropes never flash all at once;
  // consecutive drops never share a type, and the biggest drop (the longest breakdown before it,
  // 8 bars or more) gets the VP1s and the ropes together
  const TYPES = ["blinder", "spin", "pairs"];
  const longest = Math.max(0, ...P.drops.map((d) => d.breakdown_bars || 0));
  const big = longest >= 8 ? P.drops.findIndex((d) => d.breakdown_bars === longest) : -1;
  P.drops = P.drops.map((d, i) => {
    const i0 = Math.round(beatPos(P, d.time)), ref = lowDb(i0);
    let k = 1;
    while (k < 4 && lowDb(i0 + k) > ref - 6) k++;
    return { ...d, strobeBeats: k, strobeType: i === big ? "all" : TYPES[i % TYPES.length] };
  });
  // smoke machine: a long soft fill at the start of every breakdown of 4 bars or more (so the
  // beams show when the drop comes), and a short punchy blast on every drop's downbeat
  P.smoke = [
    ...P.sections.filter((sec) => sec.kind === "Breakdown" && sec.end - sec.start >= 4 * barDur)
      .map((sec) => ({ t: sec.start, kind: "breakdown", dur: Math.min(6, (sec.end - sec.start) / 2), amount: 0.7, n: 140, life: 14 })),
    ...P.drops.map((d) => ({ t: d.time, kind: "drop", dur: 1.0, amount: 0.45, n: 70, life: 7 })),
  ].sort((a, b) => a.t - b.t);
  applyCues(P, song.cues || {});
  return P;
}

// The show compiler's cues (viz/show_compile.py, from the offline analyzers), where a song has them:
//   pulses    the separated kick and snare drive the pulse (snares a little softer), hi-hats a rope
//             sparkle, all checked against ADTOF's transcription; without DrumSep the drums stem's
//             onsets stay;
//   builds    each drop's build starts where the riser (DSP or effect head) / snare roll really starts, and its progress
//             follows the compiled intensity curve (a roll speeding up climbs faster);
//   blackouts the audio's own pre-drop silence where it has one;
//   accents   impacts / crashes and chord stabs as short flashes, sound events (air horn, siren,
//             whoosh, boom; PANNs and PretrainedSED) with their own looks, a boom also fires the smoke;
//   harmony   chord roots from the chord tracker; structure letters choose the look outside hooks,
//             changing on All-In-One's boundaries and at every drop;
//   stereo    the synths' stereo position per bar leans the uplights and ropes left or right;
//   preset    the song's energy (aggressive / party / danceable tags) speeds patterns up, its calm
//             slows them and softens the level.
const pulseSet = (list, scale) => ({ times: Float64Array.from(list.map((h) => h[0])), amps: Float32Array.from(list.map((h) => Math.min(1, h[1] * scale))) });
const PC = { C: 0, "C#": 1, D: 2, "D#": 3, E: 4, F: 5, "F#": 6, G: 7, "G#": 8, A: 9, "A#": 10, B: 11 };
function applyCues(P, C) {
  P.cues = C;
  const H = C.hits || {};
  if ((H.kick || []).length > 20) {
    const merged = [...H.kick.map((h) => [h[0], h[1]]), ...(H.snare || []).map((h) => [h[0], h[1] * 0.8])].sort((a, b) => a[0] - b[0]);
    P.hits = pulseSet(merged, 1);
    P.hats = pulseSet(H.hh || [], 1);
  }
  const byDrop = new Map((C.builds || []).map((b) => [b.drop, b]));
  const blk = new Map((C.blackouts || []).map((b) => [b.drop, b]));
  P.drops = P.drops.map((d) => {
    const b = byDrop.get(d.time), k = blk.get(d.time);
    return { ...d, ...(b && b.source !== "sections" ? { build_start: b.start, curve: b.curve, build_source: b.source } : {}),
      ...(k && k.source === "audio" ? { blackout_start: k.start } : {}) };
  });
  P.impacts = pulseSet((C.impacts || []).map((i) => [i[0], Math.min(1, i[1] / 20)]), 1);
  P.stabs = pulseSet((C.stabs || []).map((x) => [x[0], Math.min(1, 0.4 + x[1] / 10)]), 1);
  P.events = (C.events || []).map(([a, b, kind, peak]) => ({ a, b, kind, peak }));
  for (const e of P.events) if (e.kind === "boom") P.smoke.push({ t: e.a, kind: "drop", dur: 0.6, amount: 0.3, n: 40, life: 6 });
  P.smoke.sort((a, b) => a.t - b.t);
  if ((C.chords || []).length) {
    const ch = C.chords.filter((c) => c[2] !== "N");
    P.roots = ch.map(([a, b, lab]) => [a, b, PC[lab.replace("m", "")] ?? 0]);
    P.rootStarts = P.roots.map((r) => r[0]);
  }
  P.parts = C.parts || [];
  P.partStarts = P.parts.map((x) => x[0]);
  P.pan = C.pan || [];
  P.panStarts = P.pan.map((x) => x[0]);
  // Déjà Vu: per part, how many beats (whole bars) back its first appearance lies, and how long that one is.
  // Every part that starts on a drop counts as the same music: lighters treat a song's drops alike (decision/notes.md).
  const first = new Map(), beatLen = 60 / P.bpm;
  P.dejaVu = P.parts.map(([a, b, letter]) => {
    const pa = beatPos(P, a), pb = beatPos(P, b), key = P.drops.some((d) => Math.abs(d.time - a) < beatLen) ? "drop" : letter, f = first.get(key);
    if (!f) { first.set(key, { pa, len: pb - pa }); return { pa, shift: 0, len: 0 }; }
    return { pa, shift: 4 * Math.round((pa - f.pa) / 4), len: Math.max(4, 4 * Math.round(f.len / 4)) };
  });
  const pr = C.preset || {};
  P.energyPreset = pr.energy ?? 0.5;
  P.calmPreset = pr.calm ?? 0.2;
}
// the strongest recent pulse of a set (decaying over `decay` per second), 0 without one
function pulseAt(set, t, decay = 10) {
  if (!set || !set.times.length) return 0;
  let v = 0;
  for (let i = lowerBound(set.times, t + 1e-9) - 1, k = 0; i >= 0 && k < 2; i--, k++) v = Math.max(v, set.amps[i] * Math.exp(-(t - set.times[i]) * decay));
  return v;
}
// Déjà Vu: where this moment's look comes from: a part that came before repeats its first appearance, looping if it runs longer
function dejaPos(P, pos) {
  const D = P.dejaVu;
  let lo = 0, hi = D.length;
  while (lo < hi) { const m = (lo + hi) >> 1; if (D[m].pa <= pos) lo = m + 1; else hi = m; }
  const d = D[lo - 1];
  if (!d || !d.shift) return pos;
  return d.pa - d.shift + ((pos - d.pa) % d.len);
}
function beatPos(P, t) {
  const B = P.beats, n = B.length;
  if (t <= B[0]) return (t - B[0]) / (B[1] - B[0]);
  if (t >= B[n - 1]) return n - 1 + (t - B[n - 1]) / (B[n - 1] - B[n - 2]);
  let lo = 0, hi = n - 1;
  while (hi - lo > 1) { const m = (lo + hi) >> 1; if (B[m] <= t) lo = m; else hi = m; }
  return lo + (t - B[lo]) / (B[lo + 1] - B[lo]);
}
function beatTime(P, i) {
  const B = P.beats, n = B.length;
  if (i <= 0) return B[0] + i * (B[1] - B[0]);
  if (i >= n - 1) return B[n - 1] + (i - n + 1) * (B[n - 1] - B[n - 2]);
  const k = Math.floor(i);
  return B[k] + (i - k) * (B[k + 1] - B[k]);
}

// the haze in the room, 0..1: every smoke event pushes it up while the machine fires and it clears
// with a 30 s time constant
const smokeOn = (ev) => ev.kind === "manual" || opts.smoke === "both" || opts.smoke === ev.kind;
function hazeAt(P, t) {
  let h = 0;
  for (const ev of P.smoke) {
    if (ev.t > t) break;
    if (!smokeOn(ev)) continue;
    const a = t - ev.t;
    h += ev.amount * (a < ev.dur ? a / ev.dur : Math.exp(-(a - ev.dur) / 30));
  }
  return Math.min(1, h);
}

const LEVEL = { Intro: 0.4, Groove: 0.65, Breakdown: 0.3, Build: 0.5, Drop: 1.0, Outro: 0.35 };

// The show at time t: every fixture value the scene needs, plus what the HUD shows.
function showAt(P, t) {
  const song = P.song, pos = beatPos(P, t), idx = Math.floor(pos), frac = pos - idx;
  const rel = pos - P.firstDown, bar = Math.floor(rel / 4), beatInBar = ((Math.floor(rel) % 4) + 4) % 4 + 1;
  const beatDur = 60 / P.bpm;
  // the look's own clock: the song's unless Déjà Vu sends this moment back to where the music first played
  const lookPos = opts.dejavu && P.dejaVu ? dejaPos(P, pos) : pos, tL = lookPos === pos ? t : beatTime(P, lookPos), relL = lookPos - P.firstDown;
  const section = P.sections.find((s) => s.start <= t && t < s.end) || null;
  const phraseIdx = P.phrases.findIndex((p) => p.start <= t && t < p.end);
  const phrase = phraseIdx >= 0 ? P.phrases[phraseIdx] : null;
  const next = P.drops.find((d) => d.time > t) || null, prev = [...P.drops].reverse().find((d) => d.time <= t) || null;
  const toDrop = next ? beatPos(P, next.time) - pos : Infinity;
  const buildBeats = next ? Math.min(128, beatPos(P, next.time) - beatPos(P, next.build_start)) : 0;
  const sinceDrop = prev ? pos - beatPos(P, prev.time) : Infinity;
  const inBuild = next && buildBeats >= 2 && toDrop > 0 && toDrop <= buildBeats;
  // the audio's own pre-drop silence where the compiler found one, else the last half beat
  const blackout = inBuild && (next.blackout_start != null ? t >= next.blackout_start : toDrop <= 0.5);
  // the compiled intensity curve where there is one (snare roll / riser), else linear
  let buildProgress = inBuild ? 1 - toDrop / buildBeats : 0;
  if (inBuild && next.curve) {
    const cv = next.curve, j = Math.max(0, Math.min(cv.length - 2, lowerBound(cv, t, (q) => q[0]) - 1));
    const [t0, v0] = cv[j], [t1, v1] = cv[j + 1];
    buildProgress = Math.max(0, Math.min(1, v0 + (v1 - v0) * Math.max(0, Math.min(1, (t - t0) / Math.max(1e-3, t1 - t0)))));
  }
  const kind = inBuild ? "Build" : section ? section.kind : "Groove";

  // the stem sections set the level; Rekordbox's club phrases nudge it (Chorus up, Down down)
  let level = LEVEL[kind] ?? 0.6;
  if (phrase && kind !== "Build") {
    if (phrase.label === "Chorus" && kind !== "Breakdown") level = Math.max(level, 0.85);
    else if (phrase.label === "Down") level = Math.min(level, 0.5);
  }
  if (inBuild) level = 0.45 + 0.5 * buildProgress;
  const baseLevel = level;
  const energetic = kind === "Drop" || level >= 0.85;

  // what the music does right now: stem loudness (followers), how hard the drums just hit
  const f = Math.max(0, Math.round(t * 100));
  const env = (k) => (f < P.env[k].length ? P.env[k][f] : 0);
  const energy = { drums: env("drums"), bass: env("bass"), other: env("other"), vocals: env("vocals"), mix: env("mix") };
  const hit = hitPulse(P, t);
  const hat = pulseAt(P.hats, t, 18), impact = pulseAt(P.impacts, t, 3), stab = pulseAt(P.stabs, t, 9);
  const event = P.events ? P.events.find((e) => e.a <= t && t < e.b + 0.2) || null : null;
  const pj = P.partStarts ? lowerBound(P.partStarts, tL + 1e-9) - 1 : -1, part = pj >= 0 ? P.parts[pj][2].charCodeAt(0) - 65 : -1;
  const pk = P.panStarts ? lowerBound(P.panStarts, t + 1e-9) - 1 : -1, pan = pk >= 0 ? P.pan[pk][1] : 0;
  // the section sets the level, the loudness moves it: quiet moments inside a section dim
  level *= 0.3 + 0.7 * energy.mix;
  // segment: the stretch since the last music change (drop, phrase start, stem change)
  const seg = Math.max(0, lowerBound(P.segStarts, tL + 1e-9) - 1);
  const sinceChange = lookPos - beatPos(P, Math.max(P.segStarts[seg], P.beats[0]));
  // builds: the ceiling chase steps on every drum hit, so a snare roll speeds it up
  let buildSteps = 0;
  if (inBuild) {
    const hitsN = lowerBound(P.hits.times, t) - lowerBound(P.hits.times, next.build_start);
    const beatsN = pos - beatPos(P, next.build_start);
    buildSteps = hitsN >= beatsN / 2 ? hitsN : Math.floor(beatsN * 2);
  }

  // movement: period from the melody's activity in this 2-bar block, scaled by the section
  const block = Math.max(0, Math.floor(relL / 8));
  let period = P.blockPeriod[Math.min(block, P.blockPeriod.length - 1)] ?? 4;
  if (P.energyPreset > 0.6 && period > 1) period /= 2;  // the song's character (style / mood tags)
  else if (P.calmPreset > 0.5 && period < 8) period *= 2;
  if (kind === "Breakdown" || kind === "Intro" || kind === "Outro") period = Math.min(8, period * 2);
  if (kind === "Drop") period = Math.max(1, period / 2);
  const phase = (((relL / period) % 1) + 1) % 1;
  // Patterns don't glide, they step on the note grid: 8 steps per cycle (4 when the cycle is one
  // beat), i.e. sixteenths for 1-2 beat cycles, eighths for 4, quarters for 8. Every step and
  // every beat starts a short flash (about 60 ms and 80 ms), so the lights snap to the music.
  const sub = Math.min(4, Math.max(1, Math.ceil(8 / period))), steps = period * sub;
  const stepPos = relL * sub, stepIdx = Math.floor(stepPos);
  const step = ((stepIdx % steps) + steps) % steps;
  const stepEnv = Math.exp(-(stepPos - stepIdx) * (beatDur / sub) * 16);
  const beatEnv = Math.exp(-frac * beatDur * 12);
  // hook: the melody's 2-bar block; the same hook always gets the same pattern (and colours)
  const H = P.hooks, hb = lowerBound(H.blocks, tL + 1e-9) - 1;
  const hook = hb >= 0 && hb < H.ids.length ? H.ids[hb] : -1;
  let anim = hook >= 0 && P.hookAnim[hook] ? P.hookAnim[hook] : P.segAnim[seg] || "wave";
  if (kind === "Breakdown" || kind === "Intro" || kind === "Outro") anim = anim === "circle" ? "circle" : "wave";
  if (inBuild) anim = "rise";

  // melody line: the note sounding now (its onset flashes, then it holds at its velocity), and
  // the chord root from the bass line, which moves the palette on every chord change
  const li = lowerBound(P.line, t + 1e-9, (n) => n[0]) - 1;
  const melodyNote = li >= 0 && P.line[li][1] > t ? P.line[li] : null;
  const noteEnv = melodyNote ? (melodyNote[3] ?? 0.8) * (0.5 + 0.5 * Math.exp(-(t - melodyNote[0]) * 12)) : 0;
  const vocal = melodyNote && melodyNote[4] === "v" ? melodyNote : null, lead = melodyNote && !vocal ? melodyNote : null;
  const bassNote = noteAt(P.bassLine, t);
  const ri = lowerBound(P.rootStarts, tL + 1e-9) - 1;
  const root = ri >= 0 ? P.roots[ri][2] : null;
  // chord changes swap which colour of the pair leads: roots an odd number of fifths from the tonic
  const rootLead = root == null ? 0 : Math.abs(fifthsFrom(root, P.tonic)) % 2;
  // melody notes take the second colour, whiter right at their onset
  const noteSat = melodyNote ? 0.95 - 0.6 * Math.exp(-(t - melodyNote[0]) * 12) : 0.95;

  // strobe: 0..1 this frame
  let strobe = 0;
  const flash = (perBeat, width_s = 0.035) => ((pos * perBeat) % 1) < Math.min(0.4, width_s / (beatDur / perBeat));
  // Only the drop strobes, from the moment the bass hits: the drop downbeat, where kick and bass
  // come back in (on the grid within 40 ms for 370 of 490 drops; offbeat basslines follow the
  // kick by half a beat). Sixteenths for up to one bar (prepare: strobeBeats), easing off from
  // full power towards the end of the bar. Nothing before it.
  const haze = hazeAt(P, t);
  const strobing = !!prev && sinceDrop >= 0 && sinceDrop < prev.strobeBeats;
  const strobeType = strobing ? prev.strobeType : null, perBeat = 4;
  const strobeIdx = strobing ? Math.floor(sinceDrop * perBeat) : -1;  // which flash of the burst
  if (opts.strobes && strobing && flash(perBeat, 0.045)) strobe = 1 - 0.35 * sinceDrop / 4;

  return { t, pos, idx, frac, rel: relL, lookBar: Math.floor(relL / 4), bar, beatInBar, beatDur, section, kind, phrase, next, prev, toDrop, sinceDrop,
    inBuild, blackout, buildBeats, buildProgress, buildSteps, level, baseLevel, energetic, energy, hit, seg, sinceChange, period, phase,
    sub, steps, step, stepEnv, beatEnv, anim, hook, root, rootLead, noteSat, noteIdx: li, noteEnv, hat, impact, stab, event, part, pan,
    vocal, lead, bassNote, melodyNote, strobing, strobeType, strobeIdx, strobe, haze };
}

// ------------------------------------------------------------------------------------------------
// Scene: a 10 m wide, 17 m long, 4.5 m high club. x runs across the stage (-5..5), z from the stage
// wall (-8.5) to the back wall (8.5), y up. The stage is a 3 m deep, 0.8 m high step at the stage wall.
const renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: "high-performance" });
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.toneMappingExposure = 1.15;  // a touch brighter than neutral (the user found 1.0 a bit dark)
host.appendChild(renderer.domElement);
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x020204);
scene.fog = new THREE.FogExp2(0x040408, 0.035);
const camera = new THREE.PerspectiveCamera(70, 16 / 9, 0.05, 200);

const ROOM = { w: 10, d: 17, h: 4.5, back: -8.5 };
const ROOM_Z = ROOM.back + ROOM.d / 2;
const STAGE_Z = ROOM.back + 3, STAGE_H = 0.8;  // the stage covers z < STAGE_Z
const BACK_Z = ROOM.back + ROOM.d;  // the back wall, with the door
const BAR = { x0: -5, x1: -4.2, z0: 2.0, z1: BACK_Z - 0.6 };  // bar counter along the left wall

// Walking camera: WASD on the floor at eye height (Shift runs), drag to look, wheel steps. Walls,
// the bar and the stage edge (a step up) are respected. Presets: audience, DJ, top (above the room).
const EYE = 1.7;
const walker = { pos: new THREE.Vector3(), yaw: 0, pitch: 0, flying: false, keys: new Set() };
const VIEWS = {
  audience: { pos: [0, EYE, BACK_Z - 0.6], yaw: 0, pitch: 0.1 },
  dj: { pos: [0, STAGE_H + EYE, STAGE_Z - 2.2], yaw: Math.PI, pitch: -0.08 },
  top: { pos: [0, 15, 5.5], yaw: 0, pitch: -1.15, flying: true },
};
function setView(v) { const p = VIEWS[v]; walker.pos.set(...p.pos); walker.yaw = p.yaw; walker.pitch = p.pitch; walker.flying = !!p.flying; }
setView("audience");
const groundAt = (x, z) => (z < STAGE_Z ? STAGE_H : 0);
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
function walk(forward, right) {
  const sin = Math.sin(walker.yaw), cos = Math.cos(walker.yaw), p = walker.pos;  // yaw 0 looks along -z
  p.x = clamp(p.x - sin * forward + cos * right, -ROOM.w / 2 + 0.3, ROOM.w / 2 - 0.3);
  p.z = clamp(p.z - cos * forward - sin * right, ROOM.back + 0.3, ROOM.back + ROOM.d - 0.3);
  if (p.z > BAR.z0 - 0.3 && p.z < BAR.z1 + 0.3) p.x = Math.max(p.x, BAR.x1 + 0.3);
  walker.flying = false;
}
function updateWalker(dt) {
  const k = walker.keys;
  const f = (k.has("KeyW") ? 1 : 0) - (k.has("KeyS") ? 1 : 0), r = (k.has("KeyD") ? 1 : 0) - (k.has("KeyA") ? 1 : 0);
  if (f || r) {
    const speed = (k.has("ShiftLeft") || k.has("ShiftRight") ? 6 : 3) * dt / Math.hypot(f, r);
    walk(f * speed, r * speed);
  }
  if (!walker.flying) walker.pos.y += (groundAt(walker.pos.x, walker.pos.z) + EYE - walker.pos.y) * Math.min(1, dt * 12);
  camera.position.copy(walker.pos);
  camera.rotation.set(walker.pitch, walker.yaw, 0, "YXZ");
}
const typing = (e) => e.target && (e.target.tagName === "INPUT" && !["range", "checkbox"].includes(e.target.type) || e.target.tagName === "SELECT");
window.addEventListener("keydown", (e) => { if (/^Key[WASD]$|^Shift/.test(e.code) && !typing(e)) walker.keys.add(e.code); });
window.addEventListener("keyup", (e) => walker.keys.delete(e.code));
window.addEventListener("blur", () => walker.keys.clear());
{
  const el = renderer.domElement; let drag = null;
  el.addEventListener("pointerdown", (e) => { drag = { x: e.clientX, y: e.clientY }; el.setPointerCapture(e.pointerId); });
  el.addEventListener("pointermove", (e) => {
    if (!drag) return;
    walker.yaw -= (e.clientX - drag.x) * 0.004; walker.pitch = clamp(walker.pitch - (e.clientY - drag.y) * 0.004, -1.45, 1.45);
    drag = { x: e.clientX, y: e.clientY };
  });
  el.addEventListener("pointerup", () => { drag = null; });
  el.addEventListener("wheel", (e) => { e.preventDefault(); walk(-Math.sign(e.deltaY) * 0.6, 0); }, { passive: false });
}
document.getElementById("resetview").addEventListener("click", () => setView("audience"));
document.getElementById("djview").addEventListener("click", () => setView("dj"));
document.getElementById("topview").addEventListener("click", () => setView("top"));
document.getElementById("fullscreen").addEventListener("click", () => {
  if (document.fullscreenElement) document.exitFullscreen(); else host.requestFullscreen();
});

const composer = new EffectComposer(renderer);
composer.addPass(new RenderPass(scene, camera));
const bloom = new UnrealBloomPass(new THREE.Vector2(512, 512), 0.9, 0.5, 0.3);
composer.addPass(bloom);
composer.addPass(new OutputPass());

const ambient = new THREE.HemisphereLight(0x1a1c2a, 0x050505, 0.6); scene.add(ambient);

// room: acoustic-panel walls, polished floor, dark ceiling
const panelTex = (() => {
  const cv = document.createElement("canvas"); cv.width = cv.height = 256; const g = cv.getContext("2d");
  g.fillStyle = "#3a3c45"; g.fillRect(0, 0, 256, 256);
  for (let i = 0; i < 1400; i++) { const v = 48 + Math.random() * 16; g.fillStyle = `rgb(${v},${v + 1},${v + 6})`; g.fillRect(Math.random() * 256, Math.random() * 256, 2, 2); }
  g.fillStyle = "#121318"; g.fillRect(0, 0, 256, 5); g.fillRect(0, 0, 5, 256);  // seams between panels
  const t = new THREE.CanvasTexture(cv); t.colorSpace = THREE.SRGBColorSpace; t.wrapS = t.wrapT = THREE.RepeatWrapping; return t;
})();
const wallMat = (w, h) => { const t = panelTex.clone(); t.repeat.set(w / 1.25, h / 1.25); t.needsUpdate = true; return new THREE.MeshStandardMaterial({ map: t, color: 0x8a8c96, roughness: 0.92, metalness: 0 }); };
const floor = new THREE.Mesh(new THREE.PlaneGeometry(ROOM.w, ROOM.d), new THREE.MeshStandardMaterial({ color: 0x111116, roughness: 0.35, metalness: 0.35 }));
floor.rotation.x = -Math.PI / 2; floor.position.z = ROOM_Z; scene.add(floor);
const back = new THREE.Mesh(new THREE.PlaneGeometry(ROOM.w, ROOM.h), wallMat(ROOM.w, ROOM.h)); back.position.set(0, ROOM.h / 2, ROOM.back); scene.add(back);
const front = new THREE.Mesh(new THREE.PlaneGeometry(ROOM.w, ROOM.h), wallMat(ROOM.w, ROOM.h)); front.rotation.y = Math.PI; front.position.set(0, ROOM.h / 2, ROOM.back + ROOM.d); scene.add(front);
for (const s of [-1, 1]) {
  const w = new THREE.Mesh(new THREE.PlaneGeometry(ROOM.d, ROOM.h), wallMat(ROOM.d, ROOM.h));
  w.rotation.y = -s * Math.PI / 2; w.position.set(s * ROOM.w / 2, ROOM.h / 2, ROOM_Z); scene.add(w);
}
const ceiling = new THREE.Mesh(new THREE.PlaneGeometry(ROOM.w, ROOM.d), new THREE.MeshStandardMaterial({ color: 0x0b0b0e, roughness: 0.95 }));
ceiling.rotation.x = Math.PI / 2; ceiling.position.set(0, ROOM.h, ROOM_Z); scene.add(ceiling);
// cove LEDs along the top of both long walls, washing the walls down (palette colour, see applyShow)
const coves = [];
for (const sx of [-1, 1]) {
  const x = sx * (ROOM.w / 2 - 0.2);
  const led = new THREE.Mesh(new THREE.BoxGeometry(0.04, 0.04, ROOM.d - 1), new THREE.MeshBasicMaterial({ color: 0x000000, toneMapped: false }));
  led.position.set(x, ROOM.h - 0.25, ROOM_Z); scene.add(led);
  const light = new THREE.RectAreaLight(0xffffff, 0, ROOM.d - 1, 0.2);
  light.position.set(x, ROOM.h - 0.25, ROOM_Z); light.lookAt(sx * ROOM.w / 2, ROOM.h - 1.6, ROOM_Z); scene.add(light);
  coves.push({ led, light });
}
// door and exit sign in the back wall, bar along the left wall
const door = new THREE.Mesh(new THREE.BoxGeometry(1.4, 2.3, 0.06), new THREE.MeshStandardMaterial({ color: 0x18191e, roughness: 0.6, metalness: 0.4 }));
door.position.set(3.2, 1.15, BACK_Z - 0.03); scene.add(door);
const exitTex = (() => {
  const cv = document.createElement("canvas"); cv.width = 128; cv.height = 48; const g = cv.getContext("2d");
  g.fillStyle = "#0a8f3c"; g.fillRect(0, 0, 128, 48); g.fillStyle = "#e9fff0"; g.font = "bold 30px sans-serif"; g.textAlign = "center"; g.textBaseline = "middle"; g.fillText("EXIT", 64, 26);
  return new THREE.CanvasTexture(cv);
})();
const exit = new THREE.Mesh(new THREE.PlaneGeometry(0.6, 0.22), new THREE.MeshBasicMaterial({ map: exitTex, toneMapped: false, color: 0x9a9a9a }));
exit.rotation.y = Math.PI; exit.position.set(3.2, 2.6, BACK_Z - 0.04); scene.add(exit);
const bar = new THREE.Mesh(new THREE.BoxGeometry(BAR.x1 - BAR.x0, 1.1, BAR.z1 - BAR.z0), new THREE.MeshStandardMaterial({ color: 0x1d1a18, roughness: 0.5, metalness: 0.2 }));
bar.position.set((BAR.x0 + BAR.x1) / 2, 0.55, (BAR.z0 + BAR.z1) / 2); scene.add(bar);
const BAR_LED = new THREE.Color(1.6, 0.75, 0.3);
const barLed = new THREE.Mesh(new THREE.BoxGeometry(0.03, 0.03, BAR.z1 - BAR.z0), new THREE.MeshBasicMaterial({ color: BAR_LED.clone(), toneMapped: false }));
barLed.position.set(BAR.x1 + 0.02, 1.08, (BAR.z0 + BAR.z1) / 2); scene.add(barLed);
const barLight = new THREE.PointLight(0xffb070, 4, 6, 1.5); barLight.position.set(BAR.x1 + 0.4, 1.6, (BAR.z0 + BAR.z1) / 2); scene.add(barLight);
// stage, booth, DJ, truss
const stage = new THREE.Mesh(new THREE.BoxGeometry(ROOM.w, STAGE_H, STAGE_Z - ROOM.back), new THREE.MeshStandardMaterial({ color: 0x15151a, roughness: 0.7 }));
stage.position.set(0, STAGE_H / 2, (ROOM.back + STAGE_Z) / 2); scene.add(stage);
const booth = new THREE.Mesh(new THREE.BoxGeometry(2.6, 1.1, 0.9), new THREE.MeshStandardMaterial({ color: 0x1c1c22, roughness: 0.5, metalness: 0.4 }));
booth.position.set(0, STAGE_H + 0.55, STAGE_Z - 0.9); scene.add(booth);
const dj = new THREE.Mesh(new THREE.CapsuleGeometry(0.25, 0.9, 4, 8), new THREE.MeshStandardMaterial({ color: 0x2a2a30, roughness: 0.8 }));
dj.position.set(0, STAGE_H + 0.7, STAGE_Z - 1.6); scene.add(dj);
const trussMat = new THREE.MeshStandardMaterial({ color: 0x3a3d44, roughness: 0.4, metalness: 0.8 });
const TRUSS_Y = ROOM.h - 0.6, TRUSS_Z = STAGE_Z - 0.1;
const truss = new THREE.Mesh(new THREE.BoxGeometry(ROOM.w - 0.4, 0.25, 0.25), trussMat); truss.position.set(0, TRUSS_Y, TRUSS_Z); scene.add(truss);
for (const x of [-4.75, 4.75]) { const leg = new THREE.Mesh(new THREE.BoxGeometry(0.25, TRUSS_Y, 0.25), trussMat); leg.position.set(x, TRUSS_Y / 2, TRUSS_Z); scene.add(leg); }

// beams: open cones with a soft additive shader (fake haze)
const beamMat = () => new THREE.ShaderMaterial({
  uniforms: { color: { value: new THREE.Color(1, 1, 1) }, intensity: { value: 0 } },
  vertexShader: `varying vec2 vUv; varying vec3 vN; varying vec3 vV;
    void main() { vUv = uv; vec4 mv = modelViewMatrix * vec4(position, 1.0); vN = normalize(normalMatrix * normal); vV = normalize(-mv.xyz); gl_Position = projectionMatrix * mv; }`,
  fragmentShader: `uniform vec3 color; uniform float intensity; varying vec2 vUv; varying vec3 vN; varying vec3 vV;
    void main() { float along = pow(vUv.y, 1.6); float edge = pow(abs(dot(normalize(vN), normalize(vV))), 1.4);
      gl_FragColor = vec4(color * intensity * along * edge * 0.55, 1.0); }`,
  transparent: true, depthWrite: false, blending: THREE.AdditiveBlending, side: THREE.DoubleSide,
});
const BEAM_LEN = 12;
const coneGeo = new THREE.CylinderGeometry(0.06, Math.tan(THREE.MathUtils.degToRad(6)) * BEAM_LEN, BEAM_LEN, 32, 1, true);
coneGeo.translate(0, -BEAM_LEN / 2, 0); coneGeo.rotateX(-Math.PI / 2);  // apex at the origin, along +Z

// Moving heads on the truss: disabled for now (the user asked to comment them out).
const movers = [];
/*
for (let i = 0; i < 6; i++) {
  const x = -4 + i * 1.6;
  const head = new THREE.Group(); head.position.set(x, TRUSS_Y - 0.3, TRUSS_Z); scene.add(head);
  const body = new THREE.Mesh(new THREE.CylinderGeometry(0.18, 0.22, 0.4, 12), trussMat); body.position.y = 0.15; head.add(body);
  const aim = new THREE.Group(); head.add(aim);
  const lens = new THREE.Mesh(new THREE.CircleGeometry(0.12, 16), new THREE.MeshBasicMaterial({ color: 0xffffff, toneMapped: false }));
  lens.position.z = 0.02; aim.add(lens);
  const beam = new THREE.Mesh(coneGeo, beamMat()); aim.add(beam);
  const spot = new THREE.SpotLight(0xffffff, 0, 30, THREE.MathUtils.degToRad(8), 0.5, 1.2);
  spot.position.copy(head.position); scene.add(spot); scene.add(spot.target);
  movers.push({ x, head, aim, lens, beam, spot });
}
*/

// LED ropes: five flexible 5 m LED tubes strung like a tangled spider web round a hub on the
// ceiling over the dance floor. Each is fixed to the ceiling at three points and hangs between
// them in two catenary loops of different depth, so the loops sit at different heights and ropes
// cross each other seen from below (layout: WEB_ROPES). Each rope has 100 pixels (5 cm apart)
// behind a frosted diffuser: the pixels are a 100 x 1 texture per rope, sampled with linear
// filtering plus a 5-tap blur along the rope (the diffuser spreads each LED over about its
// neighbours), so single LEDs never show: soft light in a milky tube, slightly desaturated like
// real frosted glass, with a faint glassy body when dark. Pixel 0 is at (or next to) the hub, so
// patterns can radiate from the web's centre. Each rope also lights the room below its deepest
// loop with a point light in its average colour.
const TUBE_LEN = 5, TUBE_PIX = 100, TUBE_R = 0.028;
const WEB_HUB = new THREE.Vector3(0, ROOM.h - 0.03, 0);  // on the centre line, between two VP1s
// per rope: three ceiling anchors (x, z; the first at or next to the hub, so pixel 0 is the web's
// centre) and the sag of the first loop (m; the second loop gets the rest of the 5 m). Roughly
// mirror-symmetric about the centre line but messy: two mirrored pairs that cross in an X, one
// towards the stage and one towards the back, with slightly different anchors and sags left and
// right, and one rope across the hub. Five spans cross seen from below, every loop sags 0.34-0.75 m
// (lowest point 3.72 m), every anchor clears the VP1s and the downlight cans.
const WEB_ROPES = [
  [[0, 0], [-0.9, -1.05], [1.2, -2.55], 0.7],          // towards the stage, left then across to the right
  [[0.05, 0.08], [0.95, -1.15], [-1.25, -2.55], 0.55],  // its mirror image, crossing it
  [[-2.1, 0.65], [0.3, -0.3], [1.85, 0.5], 0.75],       // across the hub, left to right
  [[0.06, -0.05], [1.0, 1.35], [-1.2, 2.9], 0.6],       // towards the back, right then across to the left
  [[0, 0], [-1.05, 1.25], [1.25, 2.85], 0.45],          // its mirror image, crossing it
];
// a rope hanging between two anchors at the same height: catenary y = c cosh(x / c), with c solved
// so the arc is the given length (or, with sagLen, the length that gives a sag)
class Rope extends THREE.Curve {
  constructor(a, b, len) {
    super(); this.a = a; this.b = b; this.d = a.distanceTo(b);
    let lo = 0.01, hi = 1e4;
    for (let i = 0; i < 100; i++) { const c = (lo + hi) / 2; if (2 * c * Math.sinh(this.d / (2 * c)) > len) lo = c; else hi = c; }
    this.c = (lo + hi) / 2; this.sag = this.c * (Math.cosh(this.d / (2 * this.c)) - 1);
  }
  static sagLen(d, sag) {
    let lo = 1e-3, hi = 1e5;
    for (let i = 0; i < 100; i++) { const c = (lo + hi) / 2; if (c * (Math.cosh(d / (2 * c)) - 1) > sag) lo = c; else hi = c; }
    const c = (lo + hi) / 2; return 2 * c * Math.sinh(d / (2 * c));
  }
  getPoint(t, out = new THREE.Vector3()) {
    const x = (t - 0.5) * this.d, c = this.c;
    return out.copy(this.a).lerp(this.b, t).setY(this.a.y + c * Math.cosh(x / c) - c * Math.cosh(this.d / (2 * c)));
  }
}
const tubeMat = (tex) => new THREE.ShaderMaterial({
  uniforms: { pix: { value: tex }, texel: { value: 1 / TUBE_PIX } },
  vertexShader: `varying vec2 vUv; varying vec3 vN; varying vec3 vV;
    void main() { vUv = uv; vec4 mv = modelViewMatrix * vec4(position, 1.0); vN = normalize(normalMatrix * normal); vV = normalize(-mv.xyz); gl_Position = projectionMatrix * mv; }`,
  fragmentShader: `uniform sampler2D pix; uniform float texel; varying vec2 vUv; varying vec3 vN; varying vec3 vV;
    vec3 at(float u) { return texture2D(pix, vec2(clamp(u, 0.5 * texel, 1.0 - 0.5 * texel), 0.5)).rgb; }
    void main() {
      float u = vUv.x;  // TubeGeometry: x runs along the rope (by arc length), y around it
      vec3 c = 0.36 * at(u) + 0.22 * (at(u - texel) + at(u + texel)) + 0.1 * (at(u - 2.0 * texel) + at(u + 2.0 * texel));
      float facing = abs(dot(normalize(vN), normalize(vV)));
      float body = mix(0.6, 1.0, pow(facing, 0.5));                      // even across, softer at the silhouette
      vec3 milky = mix(c, vec3(dot(c, vec3(0.2126, 0.7152, 0.0722))), 0.15);  // frosting takes a little saturation
      vec3 glass = vec3(0.035, 0.038, 0.045) * (0.5 + pow(1.0 - facing, 2.0));  // unlit frosted glass, brighter rim
      gl_FragColor = vec4(glass + milky * body * 2.4, 1.0);
    }`,
});
const mountGeo = new THREE.CylinderGeometry(0.05, 0.05, 0.04, 16);
{ const hub = new THREE.Mesh(new THREE.CylinderGeometry(0.11, 0.11, 0.05, 20), trussMat); hub.position.copy(WEB_HUB).setY(ROOM.h - 0.025); scene.add(hub); }
const tubes = WEB_ROPES.map(([[ax, az], [mx, mz], [ox, oz], s1], k) => {
  const start = new THREE.Vector3(ax, WEB_HUB.y, az), mid = new THREE.Vector3(mx, WEB_HUB.y, mz), outer = new THREE.Vector3(ox, WEB_HUB.y, oz);
  const l1 = Rope.sagLen(start.distanceTo(mid), s1);
  const path = new THREE.CurvePath();
  path.add(new Rope(start, mid, l1)); path.add(new Rope(mid, outer, TUBE_LEN - l1));
  const data = new Uint16Array(TUBE_PIX * 4);
  const tex = new THREE.DataTexture(data, TUBE_PIX, 1, THREE.RGBAFormat, THREE.HalfFloatType);
  tex.magFilter = tex.minFilter = THREE.LinearFilter; tex.needsUpdate = true;
  scene.add(new THREE.Mesh(new THREE.TubeGeometry(path, 200, TUBE_R, 12, false), tubeMat(tex)));
  for (const p of [start, mid, outer]) { const m = new THREE.Mesh(mountGeo, trussMat); m.position.copy(p).setY(ROOM.h - 0.02); scene.add(m); }
  // its light on the room: a point light just below the deepest point (area lights per rope cost
  // a third of the frame rate)
  let low = null;
  for (let i = 0; i <= 60; i++) { const q = path.getPointAt(i / 60); if (!low || q.y < low.y) low = q; }
  const light = new THREE.PointLight(0xffffff, 0, 8, 1.6);
  light.position.copy(low).add(new THREE.Vector3(0, -0.15, 0));
  scene.add(light);
  return { k, tex, data, light, path, low: low.y };
});

// uplights along the stage wall: additive glows + two coloured spill lights
const glowTex = (() => {
  const c = document.createElement("canvas"); c.width = c.height = 128; const g = c.getContext("2d");
  const grd = g.createRadialGradient(64, 64, 0, 64, 64, 64); grd.addColorStop(0, "rgba(255,255,255,1)"); grd.addColorStop(0.35, "rgba(255,255,255,0.35)"); grd.addColorStop(1, "rgba(255,255,255,0)");
  g.fillStyle = grd; g.fillRect(0, 0, 128, 128); return new THREE.CanvasTexture(c);
})();
const ups = [];
for (const x of [-4.4, -1.47, 1.47, 4.4]) {
  const sp = new THREE.Sprite(new THREE.SpriteMaterial({ map: glowTex, color: 0xffffff, blending: THREE.AdditiveBlending, depthWrite: false, transparent: true, opacity: 0 }));
  sp.position.set(x, STAGE_H + 1.5, ROOM.back + 0.1); sp.scale.set(1.6, 3.6, 1); scene.add(sp);
  ups.push(sp);
}
const spill = [new THREE.PointLight(0xffffff, 0, 12, 1.5), new THREE.PointLight(0xffffff, 0, 12, 1.5)];
spill[0].position.set(-3, 2.4, ROOM.back + 0.7); spill[1].position.set(3, 2.4, ROOM.back + 0.7); spill.forEach((l) => scene.add(l));

// bass strip along the stage edge
const strip = new THREE.Mesh(new THREE.BoxGeometry(ROOM.w - 0.4, 0.06, 0.05), new THREE.MeshBasicMaterial({ color: 0x000000, toneMapped: false }));
strip.position.set(0, STAGE_H - 0.04, STAGE_Z + 0.03); scene.add(strip);
const stripLight = new THREE.PointLight(0xffffff, 0, 8, 1.6); stripLight.position.set(0, 0.4, STAGE_Z + 0.6); scene.add(stripLight);

// Strobes on the truss: replaced by the VP1 panels below, disabled for now.
const strobes = [];
/*
for (const x of [-3.6, -1.2, 1.2, 3.6]) {
  const s = new THREE.Mesh(new THREE.BoxGeometry(0.7, 0.22, 0.12), new THREE.MeshBasicMaterial({ color: 0x000000, toneMapped: false }));
  s.position.set(x, TRUSS_Y + 0.25, TRUSS_Z + 0.1); scene.add(s); strobes.push(s);
}
*/
const flashLight = new THREE.PointLight(0xffffff, 0, 30, 1.0); flashLight.position.set(0, ROOM.h - 0.6, ROOM_Z); scene.add(flashLight);

// Smoke machine at the front edge of the stage, below the DJ booth, aimed over the crowd. Its smoke
// is a pool of soft particles computed from the song time like everything else: each one leaves
// the nozzle with the jet's speed and slows down (drag), then within a few seconds drifts to its own
// spot spread evenly over the room, growing and thinning, so the fog becomes an even haze instead of
// a dense cloud (manual fog: hold the Fog button or F). Like real smoke in a
// dark club it only shows where light hits it: additive, in the room's current light colour (white
// in a strobe flash). The haze it leaves (hazeAt) makes the downlight beams stronger.
const SMOKE_MAX = 1500;
const NOZZLE = new THREE.Vector3(0, STAGE_H + 0.2, STAGE_Z - 0.05), JET = new THREE.Vector3(0, 0.3, 1).normalize();
{
  const body = new THREE.Mesh(new THREE.BoxGeometry(0.55, 0.28, 0.4), trussMat); body.position.set(0, STAGE_H + 0.14, STAGE_Z - 0.3); scene.add(body);
  const nozzle = new THREE.Mesh(new THREE.CylinderGeometry(0.05, 0.06, 0.16, 12), trussMat); nozzle.rotation.x = Math.PI / 2 - 0.3; nozzle.position.copy(NOZZLE); scene.add(nozzle);
}
const smokeTex = (() => {  // a soft, slightly lumpy puff, so particles don't read as discs
  const c = document.createElement("canvas"); c.width = c.height = 64; const g = c.getContext("2d");
  let seed = 3; const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
  for (let i = 0; i < 7; i++) {
    const x = 20 + rnd() * 24, y = 20 + rnd() * 24, r = 14 + rnd() * 16;
    const grd = g.createRadialGradient(x, y, 0, x, y, r); grd.addColorStop(0, "rgba(255,255,255,0.35)"); grd.addColorStop(1, "rgba(255,255,255,0)");
    g.fillStyle = grd; g.fillRect(0, 0, 64, 64);
  }
  return new THREE.CanvasTexture(c);
})();
const smokeGeo = new THREE.BufferGeometry();
smokeGeo.setAttribute("position", new THREE.BufferAttribute(new Float32Array(SMOKE_MAX * 3), 3));
smokeGeo.setAttribute("size", new THREE.BufferAttribute(new Float32Array(SMOKE_MAX), 1));
smokeGeo.setAttribute("alpha", new THREE.BufferAttribute(new Float32Array(SMOKE_MAX), 1));
const smokeMat = new THREE.ShaderMaterial({
  uniforms: { color: { value: new THREE.Color(0, 0, 0) }, map: { value: smokeTex }, scale: { value: 400 } },
  vertexShader: `attribute float size; attribute float alpha; uniform float scale; varying float vA;
    void main() { vA = alpha; vec4 mv = modelViewMatrix * vec4(position, 1.0); gl_PointSize = size * scale / max(0.1, -mv.z); gl_Position = projectionMatrix * mv; }`,
  fragmentShader: `uniform vec3 color; uniform sampler2D map; varying float vA;
    void main() { float a = texture2D(map, gl_PointCoord).a * vA; gl_FragColor = vec4(color * a, 1.0); }`,
  transparent: true, depthWrite: false, blending: THREE.AdditiveBlending,
});
const smoke = new THREE.Points(smokeGeo, smokeMat);
smoke.frustumCulled = false;
scene.add(smoke);
function updateSmoke(S, light) {
  const pos = smokeGeo.attributes.position.array, size = smokeGeo.attributes.size.array, alpha = smokeGeo.attributes.alpha.array;
  let n = 0;
  P.smoke.forEach((ev, e) => {
    if (!smokeOn(ev) || S.t < ev.t || S.t > ev.t + ev.dur + ev.life) return;
    const v0 = ev.kind === "drop" ? 7 : ev.kind === "manual" ? 5 : 4, peak = ev.kind === "drop" ? 0.34 : ev.kind === "manual" ? 0.3 : 0.24;
    for (let j = 0; j < ev.n && n < SMOKE_MAX; j++) {
      const age = S.t - (ev.t + ev.dur * (j + 0.5) / ev.n);
      if (age < 0 || age > ev.life) continue;
      const r1 = hash3(e, j, 1), r2 = hash3(e, j, 2), r3 = hash3(e, j, 3);
      // 1. the jet: out of the nozzle, slowing down with drag
      const yaw = (r1 - 0.5) * 0.7, pitch = (r2 - 0.5) * 0.4, speed = v0 * (0.75 + 0.5 * r3);
      const dist = speed * 0.55 * (1 - Math.exp(-age / 0.55));
      const jx = NOZZLE.x + Math.sin(yaw) * dist, jy = NOZZLE.y + (JET.y + pitch) * dist, jz = NOZZLE.z + JET.z * Math.cos(yaw) * dist;
      // 2. spreading: from about a second on, each particle drifts to its own spot spread evenly
      // over the room (above the stage edge, below the ceiling), most of the way there in ~2.5 s,
      // growing and thinning as it goes, so the fog fills the room as an even haze
      const tx = -ROOM.w / 2 + 0.6 + (ROOM.w - 1.2) * hash3(e, j, 6);
      const tz = STAGE_Z + 0.3 + (BACK_Z - STAGE_Z - 0.9) * hash3(e, j, 7);
      const ty = 0.6 + (ROOM.h - 1.0) * hash3(e, j, 8);
      const spread = age < 0.8 ? 0 : 1 - Math.exp(-(age - 0.8) / 2.5);
      const x = clamp(jx + (tx - jx) * spread, -ROOM.w / 2 + 0.4, ROOM.w / 2 - 0.4);
      const y = Math.min(jy + (ty - jy) * spread, ROOM.h - 0.4);
      const z = Math.min(jz + (tz - jz) * spread, BACK_Z - 0.6);
      pos[3 * n] = x; pos[3 * n + 1] = y; pos[3 * n + 2] = z;
      size[n] = 0.35 + 0.6 * Math.min(age, 1.5) + 2.6 * spread;
      alpha[n] = Math.min(1, age / 0.15) * (1 - age / ev.life) ** 1.6 * peak * (1 - 0.7 * spread);
      n++;
    }
  });
  smokeGeo.setDrawRange(0, n);
  smokeGeo.attributes.position.needsUpdate = smokeGeo.attributes.size.needsUpdate = smokeGeo.attributes.alpha.needsUpdate = true;
  smokeMat.uniforms.color.value.copy(light);
}

// Four VaryTech VP1 LED panels on the ceiling, in a line along the long (z) axis down the middle
// of the room, facing down. Like the engine's fixture: dimmer + strobe + warm and cold white.
const VP1_LEN = 1.3, VP1_WID = 0.5;  // long side along the room
const ledTex = (() => {
  const cv = document.createElement("canvas"); cv.width = 100; cv.height = 260; const g = cv.getContext("2d");
  g.fillStyle = "#000"; g.fillRect(0, 0, cv.width, cv.height);
  for (let y = 0; y < 13; y++) for (let x = 0; x < 5; x++) {
    const grd = g.createRadialGradient(10 + x * 20, 10 + y * 20, 0, 10 + x * 20, 10 + y * 20, 9);
    grd.addColorStop(0, "#fff"); grd.addColorStop(0.6, "#bbb"); grd.addColorStop(1, "#000");
    g.fillStyle = grd; g.fillRect(x * 20, y * 20, 20, 20);
  }
  const t = new THREE.CanvasTexture(cv); t.colorSpace = THREE.SRGBColorSpace; return t;
})();
const COLD = new THREE.Color(0xdde8ff), WARM = new THREE.Color(0xffc98a);
const vp1 = [];
for (const z of [-4.5, -1.5, 1.5, 4.5]) {
  const body = new THREE.Mesh(new THREE.BoxGeometry(VP1_WID + 0.1, 0.12, VP1_LEN + 0.1), trussMat);
  body.position.set(0, ROOM.h - 0.08, z); scene.add(body);
  const face = new THREE.Mesh(new THREE.PlaneGeometry(VP1_WID, VP1_LEN), new THREE.MeshBasicMaterial({ map: ledTex, color: 0x000000, toneMapped: false }));
  face.rotation.x = Math.PI / 2; face.position.set(0, ROOM.h - 0.15, z); scene.add(face);
  const light = new THREE.RectAreaLight(0xffffff, 0, VP1_WID, VP1_LEN);
  light.position.set(0, ROOM.h - 0.16, z); light.lookAt(0, 0, z); scene.add(light);
  vp1.push({ z, face, light });
}

// front spot on the DJ (vocals), from the ceiling
const djSpot = new THREE.SpotLight(0xffe2b8, 0, 20, THREE.MathUtils.degToRad(10), 0.6, 1.2);
djSpot.position.set(0, ROOM.h - 0.2, STAGE_Z + 0.2); djSpot.target.position.copy(dj.position); scene.add(djSpot); scene.add(djSpot.target);

// ceiling grid of downlights over the dance floor: 4 across, 8 along the room (row 0 nearest the
// stage), leaving the centre line for the VP1s
const CEIL_COLS = 4, CEIL_ROWS = 8, CEIL_LEN = ROOM.h - 0.25, CEIL_X = [-3.7, -1.6, 1.6, 3.7];
const ceilCone = new THREE.CylinderGeometry(0.04, Math.tan(THREE.MathUtils.degToRad(6)) * CEIL_LEN, CEIL_LEN, 24, 1, true);
ceilCone.translate(0, -CEIL_LEN / 2, 0); ceilCone.rotateX(-Math.PI / 2);
const ceil = [];
for (let r = 0; r < CEIL_ROWS; r++) for (let c = 0; c < CEIL_COLS; c++) {
  const x = CEIL_X[c], z = STAGE_Z + 0.9 + r * 1.8;  // 8 rows from just past the stage edge to the back
  const can = new THREE.Mesh(new THREE.CylinderGeometry(0.13, 0.13, 0.22, 12), trussMat); can.position.set(x, ROOM.h - 0.11, z); scene.add(can);
  const aim = new THREE.Group(); aim.position.set(x, ROOM.h - 0.24, z); scene.add(aim);
  const lens = new THREE.Mesh(new THREE.CircleGeometry(0.09, 16), new THREE.MeshBasicMaterial({ color: 0x000000, toneMapped: false }));
  lens.position.z = 0.01; aim.add(lens);
  const beam = new THREE.Mesh(ceilCone, beamMat()); aim.add(beam);
  const pool = new THREE.Mesh(new THREE.CircleGeometry(1.0, 32), new THREE.MeshBasicMaterial({ map: glowTex, color: 0x000000, transparent: true, opacity: 0, blending: THREE.AdditiveBlending, depthWrite: false }));
  pool.rotation.x = -Math.PI / 2; pool.position.set(x, 0.02, z); scene.add(pool);
  ceil.push({ r, c, x, aim, lens, beam, pool });
}

// crowd on the dance floor (not behind the bar)
const CROWD = 170;
const crowd = new THREE.InstancedMesh(new THREE.CapsuleGeometry(0.2, 0.95, 4, 8), new THREE.MeshStandardMaterial({ color: 0x1b1b22, roughness: 0.7, metalness: 0.1 }), CROWD);
const people = [];
{
  let seed = 7; const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
  while (people.length < CROWD) {
    const x = -4.4 + rnd() * 8.8, z = STAGE_Z + 0.5 + rnd() * (BACK_Z - STAGE_Z - 1.2);
    if (x < BAR.x1 + 0.4 && z > BAR.z0 - 0.4 && z < BAR.z1 + 0.4) continue;
    people.push({ x, z, h: 0.92 + rnd() * 0.16, jitter: (rnd() - 0.5) * 0.16, energy: 0.6 + rnd() * 0.6, hopper: rnd() < 0.17, phase: rnd() * 2 * Math.PI });
  }
}
crowd.frustumCulled = false;
scene.add(crowd);

// ------------------------------------------------------------------------------------------------
// Apply the show
const col = new THREE.Color(), white = new THREE.Color(1, 1, 1), tmp = new THREE.Vector3(), dummy = new THREE.Object3D();
const hsl = (h, s, l, out = col) => out.setHSL(((h % 360) + 360) % 360 / 360, s, l);
const deg = THREE.MathUtils.degToRad;
const STROBE_NAMES = { blinder: "VP1 blinder", spin: "rope spin", pairs: "rope pairs", all: "VP1 + rope pairs" };
const toHalf = THREE.DataUtils.toHalfFloat, HALF_ONE = toHalf(1), smokeCol = new THREE.Color(), smokeTmp = new THREE.Color();

function moverAim(S, i) {
  const n = 8, u = (i - (n - 1) / 2) / ((n - 1) / 2), ph = 2 * Math.PI * S.phase, odd = i % 2 ? 1 : -1;
  let pan = 0, tilt = 40;
  switch (S.anim) {
    case "fan": pan = 38 * u * (0.55 + 0.45 * Math.sin(ph)); tilt = 38 + 14 * Math.sin(ph); break;
    case "cross": pan = odd * 34 * Math.sin(ph); tilt = 42 + 10 * Math.cos(ph); break;
    case "wave": pan = 26 * Math.sin(ph + i * Math.PI / 4); tilt = 36 + 18 * Math.sin(ph + i * Math.PI / 4 + Math.PI / 2); break;
    case "circle": pan = 24 * Math.cos(ph * odd + i * 0.4); tilt = 42 + 16 * Math.sin(ph * odd + i * 0.4); break;
    case "sweep": pan = 46 * Math.sin(ph); tilt = 44; break;
    case "rise": pan = -u * 8 * (1 - S.buildProgress) + 6 * Math.sin(ph) * (1 - S.buildProgress); tilt = 40 + 75 * S.buildProgress; break;
  }
  if (S.melodyNote && S.anim !== "rise") tilt += Math.max(-1, Math.min(1, (S.melodyNote[2] - P.melCenter) / 12)) * 10;
  return [deg(pan), deg(tilt)];
}

// What the fixtures do comes from the look (club_looks.mjs, which also owns the strobes and the blackout);
// this only draws its desk onto the scene.
const desk = makeDesk(), roofFlash = new THREE.Color().copy(COLD).lerp(WARM, 0.3), FLASH = [roofFlash.r, roofFlash.g, roofFlash.b];
function applyShow(S) {
  // the song's colour pair (plus white): which of the two leads swaps per hook (the same hook
  // always gets the same colours) or, outside hooks, per music change, and on chord changes
  const base = P.palette, look = (S.hook >= 0 ? S.hook : S.part >= 0 ? S.part : S.seg) + S.rootLead;
  const pal = look % 2 ? { ...base, primary: base.secondary, secondary: base.primary } : base;
  const D = frame(P, S, desk, { look: opts.look, dejavu: opts.dejavu, burn: opts.burn, countdown: opts.countdown, pal, flash: FLASH });
  // during the blackout and while the drop strobes, the VP1s are the only light in the room
  const dark = D.dark;
  ambient.intensity = dark ? 0 : 0.6 * D.ambient;
  barLight.intensity = dark ? 0 : 4; barLed.material.color.copy(BAR_LED).multiplyScalar(dark ? 0 : 1);
  // LED ropes: pixels -> half-float texture, and the tube's light: its average colour
  for (const T of tubes) {
    const px = D.ropes[T.k];
    let sr = 0, sg = 0, sb = 0;
    for (let i = 0; i < TUBE_PIX; i++) {
      const r = Math.min(2, px[3 * i]), g = Math.min(2, px[3 * i + 1]), b = Math.min(2, px[3 * i + 2]);
      T.data[4 * i] = toHalf(r); T.data[4 * i + 1] = toHalf(g); T.data[4 * i + 2] = toHalf(b); T.data[4 * i + 3] = HALF_ONE;
      sr += r; sg += g; sb += b;
    }
    T.tex.needsUpdate = true;
    const m = Math.max(sr, sg, sb) / TUBE_PIX;
    if (m > 1e-4) T.light.color.setRGB(sr / TUBE_PIX / m, sg / TUBE_PIX / m, sb / TUBE_PIX / m);
    T.light.intensity = m * 40;
  }
  ups.forEach((u, i) => { const d = D.ups[i]; hsl(d.h, d.s, 0.55, u.material.color); u.material.opacity = dark ? 0 : Math.min(1, d.b); });
  spill.forEach((l, i) => { const d = D.spill[i]; hsl(d.h, d.s, 0.5, l.color); l.intensity = dark ? 0 : 22 * d.b; });
  // bass strip
  hsl(D.strip.h, 1, 0.5, strip.material.color); strip.material.color.multiplyScalar(dark ? 0 : D.strip.b * 3);
  hsl(D.strip.h, 1, 0.5, stripLight.color); stripLight.intensity = dark ? 0 : D.strip.b * 25;
  // ceiling grid (4 across, 8 along the room, row 0 at the stage)
  ceil.forEach((L, i) => {
    const d = D.ceil[i], b = dark ? 0 : d.b, sway = dark ? 0 : d.sway;
    hsl(d.h, d.s, 0.55);
    L.beam.material.uniforms.color.value.copy(col); L.beam.material.uniforms.intensity.value = b * 0.45 * (0.6 + 1.4 * S.haze);
    L.lens.material.color.copy(col).multiplyScalar(dark ? 0 : D.lens + b * 3);
    L.pool.material.color.copy(col); L.pool.material.opacity = Math.min(1, b * 0.6);
    tmp.set(Math.sin(deg(sway)), -Math.cos(deg(sway)), 0.0001);
    L.aim.lookAt(L.aim.position.clone().add(tmp));
    L.pool.position.x = L.x + Math.tan(deg(sway)) * CEIL_LEN;
  });
  // cove wall wash
  coves.forEach((cv, i) => {
    const d = D.cove[i];
    hsl(d.h, d.s, 0.55);
    cv.led.material.color.copy(col).multiplyScalar(dark ? 0 : 0.04 + d.b);
    cv.light.color.copy(col); cv.light.intensity = dark ? 0 : d.b * 3.5;
  });
  // VP1 panels: the blinder strobe. Cold white flashes; the drop's burst mixes in the warm white.
  const warm = S.strobing ? 0.45 : 0.1, vp = D.vp;
  col.copy(COLD).lerp(WARM, warm);
  vp1.forEach((p) => {
    p.face.material.color.copy(col).multiplyScalar(0.03 + vp * 3.5);
    p.light.color.copy(col); p.light.intensity = vp * 50;
  });
  flashLight.color.copy(col); flashLight.intensity = vp * 80;
  // DJ / vocal spot
  djSpot.intensity = dark ? 0 : 220 * D.spot;
  // smoke: lit by the room, in the look's colour at its level (pulsing with the drums in Club),
  // white in a strobe flash, invisible in a blackout
  if (dark) smokeCol.setRGB(0, 0, 0);
  else hsl(D.smoke.h, D.smoke.s, 0.5, smokeCol).multiplyScalar(D.smoke.b);
  if (S.strobe > 0) smokeCol.add(smokeTmp.copy(COLD).multiplyScalar(0.9 * S.strobe));
  updateSmoke(S, smokeCol);
  // crowd: everyone bounces a little in the knees (down on the beat) and sways over two beats;
  // only about one in six hops, and only in drops
  const drive = 0.3 + 0.7 * S.energy.drums;
  const bob = (S.kind === "Drop" ? 0.04 : S.energetic ? 0.03 : S.level > 0.4 ? 0.018 : 0.006) * drive;
  const hop = (S.kind === "Drop" ? 0.12 : S.energetic ? 0.05 : 0) * drive;
  people.forEach((p, i) => {
    const fr = (((S.frac + p.jitter) % 1) + 1) % 1, up = Math.sin(Math.PI * fr);
    const y = bob * p.energy * up + (p.hopper ? hop * Math.pow(up, 0.7) : 0);
    const sway = Math.sin(Math.PI * (S.rel + p.jitter) + p.phase) * (0.015 + 0.5 * bob);
    dummy.position.set(p.x + sway, 0.7 * p.h + y, p.z); dummy.rotation.set(0, 0, sway * 1.2);
    dummy.scale.set(1, p.h, 1); dummy.updateMatrix();
    crowd.setMatrixAt(i, dummy.matrix);
  });
  crowd.instanceMatrix.needsUpdate = true;
}

// ------------------------------------------------------------------------------------------------
// HUD + loop
const fmtBeats = (b) => b >= 8 ? `${Math.ceil(b / 4)} bars` : `${Math.ceil(b)} beats`;
function renderHud(S) {
  const key = P.key ? `${P.key.name}` : "key ?";
  const drop = S.blackout ? `<b class="hot">BLACKOUT</b>` : S.strobing ? `<b class="hot">DROP · ${STROBE_NAMES[S.strobeType]}</b>`
    : S.next ? `next drop in ${fmtBeats(S.toDrop)}${S.inBuild && S.next.build_source ? ` · build from the ${S.next.build_source}` : ""}` : "no drop ahead";
  hud.innerHTML = `<div class="row"><span class="chip k-${S.kind}">${S.kind}</span>${S.phrase ? `<span class="chip phrase">${S.phrase.label}</span>` : ""}
      <span>bar ${S.bar + 1}.${S.beatInBar}</span></div>
    <div class="row"><span>${P.bpm.toFixed(1)} BPM</span><span>${key}</span><span class="sw" style="background:hsl(${P.palette.primary} 90% 55%)"></span><span class="sw" style="background:hsl(${P.palette.secondary} 90% 55%)"></span><span>${P.palette.name}</span></div>
    <div class="row"><span><b>${desk.info}</b>${opts.dejavu ? " · déjà vu" : ""}${opts.burn ? " · slow burn" : ""}${opts.countdown ? " · countdown" : ""}</span></div>
    <div class="row"><span>${[S.hook >= 0 ? `hook ${String.fromCharCode(65 + S.hook)}` : "", S.root != null ? `root ${PC_NAMES[S.root]}` : "", opts.look === "club" ? `ceiling: ${desk.anim || S.anim} · ${S.period} beat${S.period > 1 ? "s" : ""}/cycle in ${["", "quarters", "eighths", "", "sixteenths"][S.sub]}` : ""].filter(Boolean).join(" · ")}${S.strobe > 0 ? ` · <b class="hot">${S.strobeType === "blinder" ? "VP1" : S.strobeType === "all" ? "VP1 + ropes" : "ropes"} ${Math.round(S.strobe * 100)}%</b>` : ""}</span></div>
    <div class="row"><span>${drop}</span>${S.event ? `<b>${S.event.kind}</b>` : ""}${S.haze > 0.02 ? `<span>haze ${Math.round(S.haze * 100)}%</span>` : ""}</div>`;
}

// upcoming cues: phrase changes, sections, builds, blackouts, drops, fill-ins
const cuesEl = document.getElementById("cues");
const fmtT = (t) => `${Math.floor(t / 60)}:${(t % 60).toFixed(1).padStart(4, "0")}`;
function renderCues(S) {
  const ev = [];
  for (const ph of P.phrases) {
    if (ph.start > S.t) ev.push([ph.start, `${ph.label} phrase`, "phrase"]);
    if (ph.fill != null && ph.fill > S.t && ph.fill < ph.end) ev.push([ph.fill, "fill-in", "fill"]);
  }
  for (const sec of P.sections) if (sec.start > S.t && sec.kind !== "Build" && sec.kind !== "Drop") ev.push([sec.start, `${sec.kind} section`, "sec"]);
  for (const d of P.drops) {
    if (d.build_start > S.t && d.build_start < d.time) ev.push([d.build_start, "build", "build"]);
    const bo = beatTime(P, beatPos(P, d.time) - 0.5);
    if (bo > S.t) ev.push([bo, "blackout", "black"]);
    if (d.time > S.t) ev.push([d.time, `DROP: bass hits, ${STROBE_NAMES[d.strobeType]} ${d.strobeBeats} beat${d.strobeBeats > 1 ? "s" : ""}`, "drop"]);
  }
  P.segStarts.forEach((t, i) => { if (t > S.t && P.musicChanges.has(t)) ev.push([t, `music change: ${P.segAnim[i]}`, "phrase"]); });
  for (const e of P.events || []) if (e.a > S.t && e.kind !== "crowd" && e.kind !== "noise") ev.push([e.a, `sound: ${e.kind}`, "fill"]);
  for (const sm of P.smoke) if (sm.t > S.t && smokeOn(sm)) ev.push([sm.t + 0.001, sm.kind === "drop" ? "smoke: drop blast" : sm.kind === "weather" ? "smoke: noise" : "smoke: breakdown fill", "sec"]);
  ev.sort((a, b) => a[0] - b[0]);
  const rows = ev.slice(0, 9).map(([t, what, cls]) => {
    const beats = beatPos(P, t) - S.pos;
    return `<tr class="${cls}"><td>${fmtT(t)}</td><td>${beats >= 8 ? `${(beats / 4).toFixed(1)} bars` : `${beats.toFixed(1)} beats`}</td><td>${what}</td></tr>`;
  });
  cuesEl.innerHTML = rows.length ? `<table>${rows.join("")}</table>` : `<div class="none">Nothing ahead.</div>`;
}

// 16:9 always: the canvas is letterboxed inside the host (which is 16:9 except in fullscreen)
function resize() {
  const W = host.clientWidth, H = host.clientHeight;
  if (!W || !H) return;
  const w = Math.min(W, Math.round(H * 16 / 9)), h = Math.round(w * 9 / 16);
  renderer.setSize(w, h, false); composer.setSize(w, h); bloom.setSize(w, h);
  smokeMat.uniforms.scale.value = h * renderer.getPixelRatio() / (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)));
  Object.assign(renderer.domElement.style, { width: `${w}px`, height: `${h}px`, left: `${(W - w) / 2}px`, top: `${(H - h) / 2}px` });
  camera.aspect = 16 / 9; camera.updateProjectionMatrix();
}
new ResizeObserver(resize).observe(host);
resize();

let lastSongId = null, lastHud = 0, lastFrame = 0, frameDt = 1 / 60;
function loop(now) {
  requestAnimationFrame(loop);
  const dt = Math.min(0.1, (now - (lastFrame || now)) / 1000); lastFrame = now;
  if (dt > 0) frameDt += 0.05 * (dt - frameDt);
  updateWalker(dt);
  const song = lab.state.song;
  if (song && song.id !== lastSongId) { P = prepare(song); lastSongId = song.id; }
  if (song && P) {
    if (fogHeld) fireFog(P, lab.analysisTime());
    // the frame drawn now reaches the screen about one frame later: show that moment
    const S = showAt(P, lab.analysisTime() + (lab.state.playing ? frameDt : 0));
    applyShow(S);
    if (now - lastHud > 80) { renderHud(S); renderCues(S); lastHud = now; }
  }
  composer.render();
}
requestAnimationFrame(loop);
window.MelodyLab.club = { showAt: (t, p = P) => p && showAt(p, t), prepare, walker, setView,
  // for checks from the browser console: every fixture value of a frame
  debug: { applyShow, desk, get P() { return P; }, set P(v) { P = v; }, opts,
    fixtures: { ambient, barLight, barLed, tubes, ups, spill, strip, stripLight, ceil, coves, vp1, flashLight, djSpot, smokeMat } } };
