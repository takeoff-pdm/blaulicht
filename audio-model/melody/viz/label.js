"use strict";
// Melody Lab's labelling mode: correct a song's sections, keyboard first, in about a minute, and save
// them as ground truth (audio-model/data/melody/labels/<id>.json, see serve.py).
//
// A classic script loaded after viewer.html's inline one, so it uses that script's globals (state, seek,
// play, view, fit, canv, drawBeats, fmt, $ ...). viewer.html calls in through window.MelodyLab.label.
// All times are analysis time, the time of the beat grid and the analyzers: what is under the playhead
// is what is being heard (analysisTime() takes off the audio offset and the output latency).

const LABEL_KEYS = [
  { label: "intro", code: "KeyI", key: "I", color: "#7d8bab" },
  { label: "buildup", code: "KeyU", key: "U", color: "#f0b020" },
  { label: "drop", code: "KeyP", key: "P", color: "#ff3b5c" },
  { label: "breakdown", code: "KeyK", key: "K", color: "#8a74ff" },
  { label: "outro", code: "KeyO", key: "O", color: "#6a7290" },
  { label: "silence", code: "KeyL", key: "L", color: "#39404e" },
];
const FX_KEYS = [
  { kind: "riser", code: "KeyR", key: "R", color: "#f0b020" },
  { kind: "downlifter", code: "KeyT", key: "T", color: "#4fc3f7" },
  { kind: "airhorn", code: "KeyH", key: "H", color: "#ff8a65" },
  { kind: "siren", code: "KeyN", key: "N", color: "#ff5b76" },
  { kind: "other", code: "KeyV", key: "V", color: "#9aa4b5" },
  { kind: "impact", code: "KeyM", key: "M", color: "#ffffff" },  // a point, the others a span
];
const LABEL_COLOR = Object.fromEntries(LABEL_KEYS.map((k) => [k.label, k.color]));
const FX_COLOR = Object.fromEntries(FX_KEYS.map((k) => [k.kind, k.color]));
const PREROLL_BARS = 2;   // an audition starts this long before the boundary
const MIN_GAP = 0.05;     // the shortest section, s
const SAVE_DELAY = 400;   // autosave this long after the last edit, ms
const RETRY_DELAY = 3000; // try a failed save again after this, ms
const REACTION = 0.1;     // a key pressed on hearing something comes this late, s
const r3 = (t) => Math.round(t * 1000) / 1000;

function gridOf(doc) {
  const beats = doc.beats?.beats || [], inBar = doc.beats?.beat_in_bar || [];
  const downs = beats.filter((_, i) => inBar[i] === 1);
  const barLen = downs.length > 1 ? (downs[downs.length - 1] - downs[0]) / (downs.length - 1) : 2;
  const beatLen = beats.length > 1 ? (beats[beats.length - 1] - beats[0]) / (beats.length - 1) : 0.5;
  return { beats, downs, barLen, beatLen };
}
function nearestOn(list, t, tol) {  // the line of a sorted list nearest to t, or t if none is within tol
  let lo = 0, hi = list.length;
  while (lo < hi) { const m = (lo + hi) >> 1; if (list[m] < t) lo = m + 1; else hi = m; }
  let best = t, bd = tol;
  for (const j of [lo - 1, lo]) if (j >= 0 && j < list.length && Math.abs(list[j] - t) <= bd) { best = list[j]; bd = Math.abs(list[j] - t); }
  return best;
}

// ---------------------------------------------------------------------------------------------
// Suggestions: where a song starts from, before a human has said anything

const RULE_TO_LABEL = { Intro: "intro", Build: "buildup", Drop: "drop", Breakdown: "breakdown", Outro: "outro",
  Groove: "drop" };  // kick and bass running, just below the drop's level: mostly the rest of a drop
const AIO_TO_LABEL = { intro: "intro", outro: "outro", break: "breakdown", bridge: "breakdown", verse: "breakdown",
  chorus: "drop", inst: "drop", solo: "drop" };
const SILENT = 30;           // mix loudness (0..255 is -60..0 dBFS) below which a frame counts as silent
const SILENT_EDGE = 1;       // silence at either end of a song counts from this long, s
const SILENT_MIDDLE = 1.5;   // inside a song from this long, s
const ALIGN_BARS = 1;        // a rule boundary this close to an All-In-One one moves onto it

function carve(secs, a, b, label, via) {  // the sections with [a, b] replaced by one new section
  const out = [];
  for (const s of secs) {
    if (s.end <= a || s.start >= b) { out.push(s); continue; }
    if (s.start < a) out.push({ ...s, end: a });
    if (s.end > b) out.push({ ...s, start: b });
  }
  out.push({ start: a, end: b, label, via });
  return out.sort((x, y) => x.start - y.start);
}
function silentRuns(doc) {  // [[start, end]] in seconds, from the mix loudness
  const fps = doc.fps || 100, mix = doc.energyData?.mix || Uint8Array.from(atob(doc.energy?.mix || ""), (c) => c.charCodeAt(0));
  const runs = [];
  let from = -1;
  for (let f = 0; f <= mix.length; f++) {
    const quiet = f < mix.length && mix[f] < SILENT;
    if (quiet && from < 0) from = f;
    if (!quiet && from >= 0) { runs.push([from / fps, f / fps]); from = -1; }
  }
  return runs;
}
function suggestSections(doc) {
  const { downs, barLen } = gridOf(doc), dur = doc.duration;
  const functions = doc.cues?.functions || [], aioBounds = [...functions, ...(doc.cues?.parts || [])].flatMap((s) => [s[0], s[1]]).sort((a, b) => a - b);
  // the section rule (sections.py), unless it found no drop: then its one long intro says nothing
  let secs = (doc.show?.sections || []).map((s) => ({ start: s.start, end: s.end, label: RULE_TO_LABEL[s.kind] || "drop", via: "rule", rule: s.kind }));
  if (!(doc.show?.drops || []).length) secs = functions.map(([a, b, f]) => ({ start: a, end: b, label: AIO_TO_LABEL[f] || "drop", via: "allinone" }));
  if (!secs.length) secs = [{ start: 0, end: dur, label: "intro", via: "none" }];

  if ((doc.show?.drops || []).length) {
    // All-In-One's intro outvotes a rule build or drop inside it: kick and bass arriving early is no drop yet
    const intro = functions.find(([, , f]) => f === "intro");
    if (intro) for (const s of secs) {
      const inside = Math.max(0, Math.min(s.end, intro[1]) - Math.max(s.start, intro[0]));
      if (inside >= 0.8 * (s.end - s.start) && s.label !== "intro") { s.label = "intro"; s.via = "allinone"; }
    }
    // The rule's boundaries follow the low end, All-In-One's the phrases: the kick stopping for a fill
    // one bar early isn't where the section ends (on the 175 songs this moves 67% of them onto a 4-bar
    // line, against 57%). Drops, and the builds counted back from them, stay.
    for (let i = 1; i < secs.length; i++) {
      if (secs[i].rule === "Drop" || secs[i].rule === "Build") continue;
      const t = nearestOn(aioBounds, secs[i].start, ALIGN_BARS * barLen + 0.01);
      if (t !== secs[i].start && t > secs[i - 1].start + barLen && t < secs[i].end - barLen) { secs[i - 1].end = secs[i].start = t; secs[i].via = "rule+allinone"; }
    }
    // an outro or a break the rule can't see, because the kick keeps going
    const last = secs[secs.length - 1], outro = functions.find(([, , f]) => f === "outro");
    if (outro && last.label !== "outro" && outro[0] > last.start + 4 * barLen && outro[0] < last.end) secs = carve(secs, outro[0], last.end, "outro", "allinone");
    for (const [a, b, f] of functions) {
      if (f !== "break" && f !== "bridge") continue;
      const host = secs.find((s) => s.label === "drop" && a >= s.start + 2 * barLen && b <= s.end - 2 * barLen);
      if (host) secs = carve(secs, a, b, "breakdown", "allinone");
    }
  }
  // the whole song, on bar lines, same labels merged
  secs[0].start = 0;
  secs[secs.length - 1].end = dur;
  for (let i = 1; i < secs.length; i++) secs[i - 1].end = secs[i].start = nearestOn(downs, secs[i].start, barLen);
  secs = secs.filter((s) => s.end - s.start > MIN_GAP);
  // silence, from the audio: not on bar lines at the song's ends, where it starts or stops with the audio
  for (const [a, b] of silentRuns(doc)) {
    const head = a < 0.05, tail = b > dur - 0.05;
    if (b - a < (head || tail ? SILENT_EDGE : SILENT_MIDDLE)) continue;
    const from = head ? 0 : tail ? a : nearestOn(downs, a, barLen), to = tail ? dur : head ? b : nearestOn(downs, b, barLen);
    if (to - from > MIN_GAP) secs = carve(secs, from, to, "silence", "energy");
  }
  const merged = [];
  for (const s of secs) {
    if (s.end - s.start <= MIN_GAP) continue;
    const p = merged[merged.length - 1];
    if (!p) { merged.push({ ...s, start: 0 }); continue; }
    if (p.label === s.label) { p.end = s.end; if (s.via !== p.via) p.via = "rule+allinone"; if (s.rule) p.rule = p.rule ? `${p.rule}+${s.rule}` : s.rule; continue; }
    p.end = s.start;
    merged.push({ ...s });
  }
  merged[merged.length - 1].end = dur;
  return { source: "sections.py rule + All-In-One functions/parts + mix silence",
    sections: merged.map((s) => ({ ...s, start: r3(s.start), end: r3(s.end) })) };
}

// ---------------------------------------------------------------------------------------------
// Editing

const L = {
  on: false, loaded: null,     // loaded: {id, doc} of the song in the viewer
  id: null, name: "", grid: null, sections: null, fx: [], suggested: null, status: null,
  undo: [], redo: [],
  sel: null,                   // the boundary last jumped to, made or grabbed: see activeBoundary
  splitAt: null,               // a split just made: label keys mean the section after it
  open: null,                  // an fx span being marked: {start, kind}
  heard: new Set(), lastT: 0,  // boundaries (ms) the playhead has played across, or the user has placed
  token: 0, busy: false, armed: 0, drag: null, queue: null, saved: null, savedZoom: null,
};
const ms = (t) => Math.round(t * 1000);
const head = () => Math.max(0, analysisTime());
const seekTo = (t) => seek(t + state.offset);  // analysis time to audio time
const bounds = () => L.sections.slice(1).map((s) => s.start);
const unheard = () => bounds().filter((b) => !L.heard.has(ms(b))).length;
const sectionIndexAt = (t) => { let i = 0; while (i + 1 < L.sections.length && L.sections[i + 1].start <= t + 0.002) i++; return i; };

function currentIndex() {  // the section the label keys act on
  const t = head();
  const i = sectionIndexAt(t), next = L.sections[i + 1];
  // a split made just before its bar line: the new section is the one meant, though it hasn't started
  return next && next.start === L.splitAt && t >= L.splitAt - L.grid.barLen ? i + 1 : i;
}
function selectedBoundary() {  // the selection, while it is still one of the two around the playhead
  if (L.sel == null) return null;
  const t = head(), bs = bounds();
  if (!bs.includes(L.sel) || Math.abs(L.sel - t) > 20) return null;
  const before = bs.filter((b) => b <= t + 0.002).pop(), after = bs.find((b) => b > t + 0.002);
  return L.sel === before || L.sel === after ? L.sel : null;
}
function activeBoundary() {  // what delete and nudge act on: highlighted in the lane. Never one off screen.
  const sel = selectedBoundary();
  if (sel != null) return sel;
  const t = head();
  let best = null, bd = Math.max(state.window, 16);
  for (const b of bounds()) if (Math.abs(b - t) < bd) { best = b; bd = Math.abs(b - t); }
  return best;
}

function snapTime(t, mode) {
  const { beats, downs, barLen, beatLen } = L.grid;
  if (mode === "bar") return r3(nearestOn(downs, t, barLen));
  if (mode === "beat") return r3(nearestOn(beats, t, beatLen));
  return r3(t);
}
function gridStep(t, dir, mode) {  // the next bar or beat line from t
  const { beats, downs, barLen, beatLen } = L.grid, list = mode === "bar" ? downs : beats;
  const found = dir > 0 ? list.find((x) => x > t + 0.02) : [...list].reverse().find((x) => x < t - 0.02);
  return r3(found ?? t + dir * (mode === "bar" ? barLen : beatLen));
}

function hint(text, keep = 2600) {
  const el = $("lhint");
  el.textContent = text;
  clearTimeout(hint.timer);
  hint.timer = setTimeout(() => { el.textContent = ""; }, keep);
}
const snapshot = () => JSON.stringify([L.sections, L.fx]);
function touched() {
  if (!L.status) L.status = "partial";
  queueSave();
}
function edit(fn) {
  const before = snapshot();
  fn();
  L.undo.push(before);
  if (L.undo.length > 200) L.undo.shift();
  L.redo = [];
  touched();
}
function undoRedo(from, to, name) {
  if (!from.length) return hint(`Nothing to ${name}`);
  to.push(snapshot());
  [L.sections, L.fx] = JSON.parse(from.pop());
  L.splitAt = null;
  touched();
  hint(name === "undo" ? "Undone" : "Redone");
}

function setLabel(label) {
  const i = currentIndex(), s = L.sections[i];
  if (s.label !== label) edit(() => { s.label = label; });
  hint(`${fmt(s.start)}: ${label}`);
}
function split(mode) {
  const t = snapTime(head(), mode), i = sectionIndexAt(t), s = L.sections[i];
  if (t - s.start < MIN_GAP || s.end - t < MIN_GAP) return hint("There is a boundary here already");
  edit(() => L.sections.splice(i, 1, { ...s, end: t }, { start: t, end: s.end, label: s.label }));
  L.sel = L.splitAt = t;
  L.heard.add(ms(t));
  hint(`Split at ${fmt(t)}${mode === "bar" ? " (bar)" : mode === "beat" ? " (beat)" : ""}`);
}
function deleteBoundary() {
  const b = activeBoundary();
  if (b == null) return hint("No boundary near the playhead");
  const i = L.sections.findIndex((s) => s.start === b);
  edit(() => { L.sections[i - 1].end = L.sections[i].end; L.sections.splice(i, 1); });
  L.sel = L.splitAt = null;
  hint(`Deleted the boundary at ${fmt(b)}`);
}
function moveBoundary(i, t) {
  L.sections[i - 1].end = L.sections[i].start = t;
  if (L.splitAt === L.sel) L.splitAt = t;
  L.sel = t;
  L.heard.add(ms(t));
}
function nudge(dir, mode) {
  const b = activeBoundary();
  if (b == null) return hint("No boundary near the playhead");
  const i = L.sections.findIndex((s) => s.start === b), t = gridStep(b, dir, mode);
  if (t <= L.sections[i - 1].start + MIN_GAP || t >= L.sections[i].end - MIN_GAP) return hint("The next boundary is in the way");
  L.sel = b;
  edit(() => moveBoundary(i, t));
  hint(`Boundary moved to ${fmt(t)}`);
}

function audition(b) {  // play into a boundary, from a little before it
  seekTo(Math.max(0, b - PREROLL_BARS * L.grid.barLen));
  if (!state.playing) play();
}
function jump(dir, preroll) {
  const t = head(), sel = selectedBoundary(), bs = bounds();
  const ref = sel == null ? t : dir > 0 ? Math.max(t, sel) : Math.min(t, sel);
  const b = dir > 0 ? bs.find((x) => x > ref + 0.01) : [...bs].reverse().find((x) => x < ref - 0.01);
  L.splitAt = null;
  if (b == null) {
    if (dir > 0) return hint("That was the last boundary");
    L.sel = null;
    return seekTo(0);
  }
  L.sel = b;
  if (preroll) audition(b); else seekTo(b);
}

// On the nearest beat, unless Alt says free: a key pressed in time with the music lands within a
// beat of the hit, but rarely on it, and the grid is the better guess than the finger.
function fxTime(free) {
  return free ? r3(head()) : snapTime(head() - (state.playing ? REACTION : 0), "beat");
}
function fxKey(kind, free) {
  const t = fxTime(free);
  if (kind === "impact") {
    edit(() => L.fx.push({ start: t, end: t, kind }));
    return hint(`impact at ${fmt(t)}`);
  }
  const open = L.open?.kind;
  closeOpenFx(t);
  if (open === kind) return;  // the same key again ended it; another one ended it and starts its own
  L.open = { start: t, kind };
  hint(`${kind} from ${fmt(t)}: ${FX_KEYS.find((k) => k.kind === kind).key} again ends it, Esc cancels`);
}
function closeOpenFx(t) {
  if (!L.open) return;
  t ??= fxTime(false);
  const { start, kind } = L.open;
  L.open = null;
  if (t > start) {
    edit(() => L.fx.push({ start, end: t, kind }));
    hint(`${kind} ${fmt(start)} to ${fmt(t)}`);
  } else hint(`${kind} dropped: it ended before it started`);
}
function deleteFx() {
  const t = head();
  let best = -1, bd = 3;
  L.fx.forEach((f, i) => { const d = t < f.start ? f.start - t : t > f.end ? t - f.end : 0; if (d < bd) { best = i; bd = d; } });
  if (best < 0) return hint("No fx mark near the playhead");
  const f = L.fx[best];
  edit(() => L.fx.splice(best, 1));
  hint(`Removed the ${f.kind} at ${fmt(f.start)}`);
}

// ---------------------------------------------------------------------------------------------
// Saving: one request at a time, oldest song first, a failed one kept and tried again

const pendingSaves = new Map();  // id -> the payload still to reach the server
let saveTimer = null, retryTimer = null, flushing = null;

function payload() {
  return { id: L.id, name: L.name, version: 1, status: L.status, labeller: "human", sections: L.sections, fx: L.fx, suggested: L.suggested };
}
function queueSave() {
  pendingSaves.set(L.id, JSON.stringify(payload()));
  clearTimeout(saveTimer);
  saveTimer = setTimeout(flushSaves, SAVE_DELAY);
}
function saveWarning(text) {
  $("lwarn").textContent = text;
  $("lwarn").classList.toggle("show", !!text);
}
async function drainSaves() {
  while (pendingSaves.size) {
    const [id, body] = pendingSaves.entries().next().value;
    try {
      const res = await fetch(`/api/labels/${id}`, { method: "POST", headers: { "Content-Type": "application/json" }, body });
      if (!res.ok) throw new Error((await res.json().catch(() => ({}))).error || `HTTP ${res.status}`);
      if (pendingSaves.get(id) === body) pendingSaves.delete(id);  // else edited meanwhile: save that too
      if (id === L.id) L.saved = new Date();
      saveWarning("");
    } catch (e) {
      saveWarning(`NOT SAVED (${e.message}): trying again, keep this tab open`);
      retryTimer = setTimeout(flushSaves, RETRY_DELAY);
      break;
    }
  }
}
function flushSaves() {
  clearTimeout(saveTimer);
  clearTimeout(retryTimer);
  if (!flushing) flushing = drainSaves().finally(() => { flushing = null; });
  return flushing;
}
window.addEventListener("pagehide", () => {  // leaving within the autosave delay, or with a save failing
  for (const [id, body] of pendingSaves) navigator.sendBeacon(`/api/labels/${id}`, new Blob([body], { type: "application/json" }));
});

// ---------------------------------------------------------------------------------------------
// Songs and the queue

async function refreshQueue() {
  try {
    const res = await fetch("/api/labels/queue");
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    L.queue = await res.json();
  } catch (e) {
    hint(`Could not read the queue: ${e.message}`);
  }
}
function nextInQueue(id, after) {  // after: skip to the first song behind this one, not the first of all
  const q = L.queue; if (!q) return null;
  const from = after ? q.order.indexOf(id) + 1 : 0;
  const rotated = [...q.order.slice(from), ...q.order.slice(0, from)];
  return rotated.find((x) => x !== id && q.status[x] !== "done") ?? null;
}
function goToSong(id) {
  $("song").value = id;
  loadSong(id);
}
async function openSong(id, doc) {
  const token = ++L.token;
  Object.assign(L, { id, name: doc.name, grid: gridOf(doc), sections: null, fx: [], suggested: null, status: null, undo: [], redo: [],
    sel: null, splitAt: null, open: null, heard: new Set(), lastT: 0, armed: 0, drag: null, saved: null });
  let stored;
  try {
    if (pendingSaves.has(id)) stored = JSON.parse(pendingSaves.get(id));  // not on the server yet
    else {
      const res = await fetch(`/api/labels/${id}`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      stored = await res.json();
    }
  } catch (e) {  // never start over from the suggestion: the next edit would overwrite the song's labels
    saveWarning(`Could not read this song's labels (${e.message}): trying again`);
    setTimeout(() => { if (token === L.token) openSong(id, doc); }, RETRY_DELAY);
    return;
  }
  if (token !== L.token) return;
  saveWarning("");
  if (stored) {
    Object.assign(L, { sections: stored.sections, fx: stored.fx, suggested: stored.suggested, status: stored.status, saved: stored.updated ? new Date(stored.updated) : null });
    if (stored.status === "done") for (const b of bounds()) L.heard.add(ms(b));
  } else {  // nothing is written until the first edit: an untouched suggestion is no one's decision
    L.suggested = suggestSections(doc);
    L.sections = L.suggested.sections.map(({ start, end, label }) => ({ start, end, label }));
  }
}
async function doneNext() {
  if (L.busy) return;
  const n = unheard();
  if (n && L.status !== "done" && performance.now() > L.armed) {  // never let a stray Enter turn a suggestion into ground truth
    L.armed = performance.now() + 5000;
    return hint(`${n} boundar${n === 1 ? "y" : "ies"} not heard yet: Enter again to finish anyway`, 5000);
  }
  L.armed = 0;
  closeOpenFx();
  const id = L.id;
  L.status = "done";
  pendingSaves.set(id, JSON.stringify(payload()));
  L.busy = true;
  await flushSaves();
  L.busy = false;
  if (pendingSaves.has(id)) return hint("Staying on this song until it is saved", 5000);
  await refreshQueue();
  const next = nextInQueue(id, false);
  if (next) goToSong(next); else hint("That was the last song of the queue", 8000);
}
async function later() {  // the next song without finishing this one, which stays in the queue
  closeOpenFx();
  await flushSaves();
  await refreshQueue();
  const next = nextInQueue(L.id, true);
  if (next) goToSong(next); else hint("No other song left in the queue");
}

function setMode(on) {
  L.on = on;
  document.body.classList.toggle("labelling", on);
  $("labelmode").classList.toggle("on", on);
  const hash = new URLSearchParams(location.hash.slice(1));
  if (on) hash.set("label", "1"); else hash.delete("label");
  history.replaceState(null, "", "#" + hash);
  const zoom = $("zoom");
  if (on) {
    if (+zoom.value < 16) { L.savedZoom = +zoom.value; zoom.value = 16; zoom.dispatchEvent(new Event("input")); }
    refreshQueue().then(() => {  // no song picked yet: go to where the queue stands
      if (state.songs.length && !state.loadToken && L.queue?.next) goToSong(L.queue.next);
    });
    if (L.loaded && L.loaded.id !== L.id) openSong(L.loaded.id, L.loaded.doc);
  } else {
    closeOpenFx();
    flushSaves();
    saveWarning("");
    if (L.savedZoom != null) { zoom.value = L.savedZoom; zoom.dispatchEvent(new Event("input")); L.savedZoom = null; }
  }
}

// ---------------------------------------------------------------------------------------------
// Keys

window.addEventListener("keydown", (e) => {
  if (!L.on || !state.song || !L.sections) return;
  if (e.target.tagName === "INPUT" && e.target.type !== "range" && e.target.type !== "checkbox") return;
  if (e.target.tagName === "SELECT") return;
  if (e.metaKey || e.ctrlKey) {  // leave the browser's own shortcuts alone
    if (e.key.toLowerCase() === "z") { e.preventDefault(); if (e.shiftKey) undoRedo(L.redo, L.undo, "redo"); else undoRedo(L.undo, L.redo, "undo"); }
    return;
  }
  const c = e.code, sectionKey = LABEL_KEYS.find((k) => k.code === c), fx = FX_KEYS.find((k) => k.code === c);
  if (c !== "Enter") L.armed = 0;
  if (c === "Comma" || c === "Period") { e.preventDefault(); return nudge(c === "Period" ? 1 : -1, e.shiftKey ? "beat" : "bar"); }
  if (e.repeat) return;
  if (sectionKey) setLabel(sectionKey.label);
  else if (fx) fxKey(fx.kind, e.altKey);
  else if (c === "KeyQ") jump(-1, !e.shiftKey);
  else if (c === "KeyE") jump(1, !e.shiftKey);
  else if (c === "KeyG") { const b = activeBoundary(); if (b != null) { L.sel = b; audition(b); } else hint("No boundary near the playhead"); }
  else if (c === "KeyC") split(e.altKey ? "free" : e.shiftKey ? "beat" : "bar");
  else if (c === "KeyX") { if (e.shiftKey) deleteFx(); else deleteBoundary(); }
  else if (c === "Escape") { if (L.open) { L.open = null; hint("Mark cancelled"); } }
  else if (c === "Enter") { e.preventDefault(); if (e.shiftKey) later(); else doneNext(); }
});

// ---------------------------------------------------------------------------------------------
// Mouse: drag a boundary, click anywhere else to bring that moment under the playhead

const laneCanvas = $("labels");
const laneX = (ev) => ev.clientX - laneCanvas.getBoundingClientRect().left;
function laneTime(x) {
  const w = laneCanvas.clientWidth;
  return view(w).t0 + (x - LABEL_W) / (w - LABEL_W) * state.window;
}
function boundaryAtX(x) {
  const v = view(laneCanvas.clientWidth);
  let best = null, bd = 7;
  for (const b of bounds()) { const d = Math.abs(v.x(b) - x); if (d < bd) { best = b; bd = d; } }
  return best;
}
laneCanvas.addEventListener("pointerdown", (ev) => {
  if (!L.sections || !state.song) return;
  const b = boundaryAtX(laneX(ev));
  if (b == null) return seekTo(laneTime(laneX(ev)));
  laneCanvas.setPointerCapture(ev.pointerId);
  L.sel = b;
  L.drag = { i: L.sections.findIndex((s) => s.start === b), before: snapshot(), moved: false };
});
laneCanvas.addEventListener("pointermove", (ev) => {
  if (!L.sections) return;
  if (!L.drag) { laneCanvas.style.cursor = boundaryAtX(laneX(ev)) != null ? "ew-resize" : "pointer"; return; }
  const { i } = L.drag, lo = L.sections[i - 1].start + MIN_GAP, hi = L.sections[i].end - MIN_GAP;
  const t = Math.max(lo, Math.min(hi, snapTime(laneTime(laneX(ev)), ev.altKey ? "free" : ev.shiftKey ? "beat" : "bar")));
  if (t === L.sections[i].start) return;
  moveBoundary(i, r3(t));
  L.drag.moved = true;
});
function endDrag() {
  if (L.drag?.moved) { L.undo.push(L.drag.before); L.redo = []; touched(); }
  L.drag = null;
}
laneCanvas.addEventListener("pointerup", endDrag);
laneCanvas.addEventListener("pointercancel", endDrag);

// ---------------------------------------------------------------------------------------------
// Drawing: the lane, the boundaries on the other timelines, the whole song on the overview

function drawLabelLane() {
  const { g, w, h } = fit(laneCanvas);
  g.fillStyle = "#1c2029"; g.fillRect(0, 0, w, h);
  g.fillStyle = "#5aa9ff"; g.font = "600 10px sans-serif"; g.textBaseline = "middle"; g.fillText("LABEL", 6, 20);
  if (!state.song || !L.sections) return;
  g.fillStyle = "#8a93a3"; g.font = "11px sans-serif"; g.fillText("fx", 6, h - 11);
  const v = view(w), secH = h - 22, fxY = secH + 2, fxH = 18, cur = currentIndex(), active = activeBoundary();
  g.save(); g.beginPath(); g.rect(LABEL_W, 0, w - LABEL_W, h); g.clip();
  L.sections.forEach((s, i) => {
    if (s.end < v.t0 || s.start > v.t1) return;
    const x0 = Math.max(LABEL_W, v.x(s.start)), x1 = Math.min(w, v.x(s.end));
    g.fillStyle = LABEL_COLOR[s.label]; g.globalAlpha = i === cur ? 1 : 0.55; g.fillRect(x0, 0, x1 - x0, secH); g.globalAlpha = 1;
    if (x1 - x0 > 40) {
      const bars = Math.round((s.end - s.start) / L.grid.barLen);
      g.fillStyle = s.label === "silence" ? "#d3d9e4" : "#0b0d12"; g.font = "700 12px sans-serif"; g.textBaseline = "middle";
      g.fillText(s.label.toUpperCase(), x0 + 7, secH / 2 - 7, x1 - x0 - 10);
      g.font = "10px sans-serif"; g.fillText(`${bars} bar${bars === 1 ? "" : "s"}`, x0 + 7, secH / 2 + 8, x1 - x0 - 10);
    }
  });
  drawBeats(g, v, 0, secH, false);
  for (const b of bounds()) {  // dashed until heard or placed, the active one white and wide
    if (b < v.t0 || b > v.t1) continue;
    const x = Math.round(v.x(b)), on = b === active, heard = L.heard.has(ms(b));
    g.fillStyle = "#0b0d12"; g.fillRect(x - (on ? 2 : 1.5), 0, on ? 4 : 3, secH);
    g.fillStyle = on ? "#ffffff" : heard ? "#d3d9e4" : "#f0b020";
    if (heard || on) g.fillRect(x - (on ? 1 : 0.5), 0, on ? 2 : 1, secH);
    else for (let y = 0; y < secH; y += 7) g.fillRect(x - 0.5, y, 1, 4);
    if (on) { g.beginPath(); g.moveTo(x - 6, 0); g.lineTo(x + 6, 0); g.lineTo(x, 8); g.closePath(); g.fill(); }
  }
  for (const f of L.fx) {
    if (f.end < v.t0 || f.start > v.t1) continue;
    const x0 = v.x(f.start), x1 = v.x(f.end);
    g.fillStyle = FX_COLOR[f.kind];
    if (f.end === f.start) { g.beginPath(); g.moveTo(x0, fxY + 2); g.lineTo(x0 + 6, fxY + fxH / 2); g.lineTo(x0, fxY + fxH - 2); g.lineTo(x0 - 6, fxY + fxH / 2); g.closePath(); g.fill(); }
    else g.fillRect(x0, fxY + 2, Math.max(2, x1 - x0), fxH - 4);
    if (x1 - x0 > 34 || f.end === f.start) { g.fillStyle = f.end === f.start ? "#d3d9e4" : "#0b0d12"; g.font = "600 10px sans-serif"; g.textBaseline = "middle"; g.fillText(f.kind, f.end === f.start ? x0 + 9 : x0 + 4, fxY + fxH / 2 + 0.5); }
  }
  if (L.open) {  // being marked: from where it started to the playhead
    const x0 = v.x(L.open.start), x1 = v.x(head());
    g.fillStyle = FX_COLOR[L.open.kind]; g.globalAlpha = 0.45 + 0.3 * Math.sin(performance.now() / 150);
    g.fillRect(Math.min(x0, x1), fxY + 2, Math.max(2, Math.abs(x1 - x0)), fxH - 4); g.globalAlpha = 1;
    g.fillStyle = "#0b0d12"; g.font = "600 10px sans-serif"; g.textBaseline = "middle"; g.fillText(L.open.kind, x0 + 4, fxY + fxH / 2 + 0.5);
  }
  g.restore();
  g.fillStyle = "#ffffff"; g.globalAlpha = 0.85; g.fillRect(Math.round(v.head), 0, 1.5, h); g.globalAlpha = 1;
}
function drawBoundaryLines(c) {  // on the other timelines, to compare a boundary with what the analyzers saw
  const g = c.getContext("2d"), w = c.clientWidth, h = c.clientHeight, v = view(w);
  g.save(); g.beginPath(); g.rect(LABEL_W, 0, w - LABEL_W, h); g.clip();
  for (const s of L.sections.slice(1)) {
    if (s.start < v.t0 || s.start > v.t1) continue;
    g.fillStyle = LABEL_COLOR[s.label]; g.globalAlpha = 0.8; g.fillRect(Math.round(v.x(s.start)) - 0.5, 0, 1.5, h);
  }
  g.restore();
}
function drawOverviewStrip() {  // the whole song at a glance, along the bottom of the overview (click to seek)
  const c = canv.overview, g = c.getContext("2d"), w = c.clientWidth, h = c.clientHeight, dur = state.song.duration, sh = 18, y = h - sh;
  g.fillStyle = "#0e1014"; g.fillRect(0, y - 1, w, sh + 1);
  L.sections.forEach((s, i) => {
    const x0 = s.start / dur * w, x1 = s.end / dur * w;
    g.fillStyle = LABEL_COLOR[s.label]; g.fillRect(x0, y, Math.max(1, x1 - x0 - 1), sh);
    if (x1 - x0 > 46) { g.fillStyle = s.label === "silence" ? "#d3d9e4" : "#0b0d12"; g.font = "600 10px sans-serif"; g.textBaseline = "middle"; g.fillText(s.label, x0 + 4, y + sh / 2 + 0.5, x1 - x0 - 6); }
  });
  for (const f of L.fx) { g.fillStyle = FX_COLOR[f.kind]; g.fillRect(f.start / dur * w - 0.5, y - 5, Math.max(1.5, (f.end - f.start) / dur * w), 4); }
  g.fillStyle = "#fff"; g.fillRect(head() / dur * w, y, 1.5, sh);
}
function setText(id, text) { const el = $(id); if (el.textContent !== text) el.textContent = text; }
function renderBar() {
  const q = L.queue;
  if (q) {
    const of = q.done < q.target ? q.target : q.total, at = q.order.indexOf(L.id);
    setText("lprog", `${q.done} of ${of} done${at >= 0 ? ` · queue #${at + 1}` : L.id ? " · not in the queue" : ""}`);
  }
  if (!L.sections) return setText("lstate", "");
  const n = unheard();
  setText("lstate", [L.status || "untouched", n ? `${n} unheard` : "", L.saved ? `saved ${L.saved.toLocaleTimeString()}` : ""].filter(Boolean).join(" · "));
}
function labelFrame() {
  if (!L.on) return;
  renderBar();
  if (!state.song || !L.sections) { drawLabelLane(); return; }
  const t = analysisTime();
  if (state.playing && t > L.lastT && t - L.lastT < 1) for (const b of bounds()) if (b > L.lastT && b <= t) L.heard.add(ms(b));
  L.lastT = t;
  drawLabelLane();
  for (const c of [canv.roll, canv.lanes, canv.feat]) drawBoundaryLines(c);
  drawOverviewStrip();
}

// ---------------------------------------------------------------------------------------------
// The legend, the top bar button, and what viewer.html calls

{
  const kbd = (k, color) => `<span class="kbd${color ? " c" : ""}"${color ? ` style="background:${color}${color === "#39404e" ? ";color:#d3d9e4" : ""}"` : ""}>${k}</span>`;
  const group = (title, items) => `<span class="g"><b>${title}</b> ${items.join(" ")}</span>`;
  $("llegend").innerHTML = [
    group("Section", LABEL_KEYS.map((k) => `${kbd(k.key, k.color)} ${k.label}`)),
    group("Go", [`${kbd("Q")}${kbd("E")} prev/next boundary, heard from 2 bars before (Shift: no lead-in)`, `${kbd("G")} again`]),
    group("Edit", [`${kbd("C")} split on a bar (Shift beat, Alt free)`, `${kbd("X")} delete`, `${kbd(",")}${kbd(".")} nudge a bar (Shift beat)`, `drag it (Shift beat, Alt free)`, `${kbd("⌘Z")} undo`]),
    group("Fx", [...FX_KEYS.filter((k) => k.kind !== "impact").map((k) => `${kbd(k.key)} ${k.kind}`), `(press twice: start, end)`, `${kbd("M")} impact`, `on a beat (Alt free)`, `${kbd("Shift X")} remove`]),
    group("Song", [`${kbd("Enter")} done, next`, `${kbd("Shift Enter")} later`]),
  ].join("");
}
$("labelmode").addEventListener("click", (e) => { setMode(!L.on); e.target.blur(); });
window.MelodyLab.label = {
  L, suggestSections,
  frame: labelFrame,
  onSong(id, doc) {
    L.loaded = { id, doc };
    flushSaves();
    if (L.on) openSong(id, doc); else L.id = null;
  },
  async onSongList(linked) {  // #label=1 without a song: go to where the queue stands
    if (!L.on || linked) return;
    await refreshQueue();
    if (L.queue?.next) goToSong(L.queue.next);
  },
};
if (new URLSearchParams(location.hash.slice(1)).get("label") === "1") setMode(true);
