/* Wires the mockup to SPARTACUS_SOLI (data.js, generated, read-only). Edits stay in the page. */
const S = window.SHOW, FADERS = ["Alpha", "Strobe", "Focus", "Tilt", "Pan", "Hue", "Saturation", "Value"];
const SPEEDS = ["_1_16", "_1_8", "_1_4", "_1_2", "_1", "_2", "_4", "_8", "_16", "_32"];
const speedLabel = s => s.slice(1).replace("_", "/") + "x";
/* Typed knob values: intensity in percent; speed as 1/4, 0.25 or 2x, snapped to the nearest step. */
const pctParse = s => parseFloat(s) / 100;
const speedParse = s => { const m = s.replace(/x$/i, "").trim(), r = m.includes("/") ? m.split("/").reduce((a, b) => +a / +b) : +m;
  return r > 0 ? Math.max(0, Math.min(9, Math.round(Math.log2(r) + 4))) / 9 : NaN; };
const fresh = id => { const o = $(id), n = o.cloneNode(false); o.replaceWith(n); return n; };
["scenes", "groups", "fixtures", "changes", "pals", "anims", "chipsOld"].forEach(i => $(i) && fresh(i));
const groupOf = id => S.groups.find(g => g.id === id);
let scene = S.scenes.find(s => s.id === S.focus), selGroups = new Set([3]), limited = new Set(), folded = new Set(), openC = new Set(), removed = new Set();
/* What each scene looked like when the page loaded, so a fader change can be undone. */
const base = new Map(S.scenes.map(s => [s.id, { states: JSON.parse(JSON.stringify(s.states)), changes: new Set(s.changes.map(c => c.join(":"))) }]));

const folderRows = (kind, items, row) => { let h = "";
  S.folders.filter(f => f.kind === kind).forEach(f => { const k = items.filter(i => i.folder === f.id), o = !folded.has(f.id);
    h += `<button class="row folder" data-folder="${f.id}"><i class="ph-bold ph-caret-${o ? "down" : "right"}"></i><i class="ph-fill ph-folder${o ? "-open" : ""} fic"></i>${f.name}<span class="v num">${o ? "" : k.length}</span></button>`;
    if (o) h += k.map(row).join(""); });
  h += items.filter(i => i.folder == null).map(row).join(""); return h; };

function drawLists() {
  $("scenes").innerHTML = folderRows("Scenes", S.scenes, s => `<button class="row leaf ${s === scene ? "on" : ""}" data-scene="${s.id}">${s.name}<span class="v sid">#${s.id}</span></button>`);
  $("groups").innerHTML = folderRows("Groups", S.groups, g => `<button class="row leaf ${selGroups.has(g.id) ? "on" : ""}" data-group="${g.id}">${g.name}<span class="v">${g.fixtures.length}</span></button>`);
  const many = selGroups.size > 1; let total = 0;
  $("fixtures").innerHTML = [...selGroups].map(id => { const g = groupOf(id); total += g.fixtures.length;
    return (many ? `<div class="fxhead">${g.name}</div>` : "") + g.fixtures.map(f => { const on = limited.has(`${id}:${f.id}`); return `<button class="fx" title="${f.name}" aria-pressed="${on}" data-k="${id}:${f.id}"><span class="nm">${f.name}</span></button>`; }).join(""); }).join("")
    || '<p class="empty">Select a group to list its fixtures.</p>';
  $("count").textContent = limited.size ? `${limited.size}/${total}` : (total || "");
  const names = [...selGroups].map(i => groupOf(i).name).join(", ");
  $("selname").textContent = !selGroups.size ? "Nothing selected" : limited.size ? `${names} (${limited.size} of ${total} fixtures)` : `${names} (all ${total} fixtures)`;
}
const selected = () => { if (limited.size) return [...limited]; const r = []; selGroups.forEach(id => groupOf(id).fixtures.forEach(f => r.push(`${id}:${f.id}`))); return r; };
const changedKeys = () => new Set(scene.changes.filter(c => !removed.has(c.join(":"))).map(c => c.join(":")));

function drawFaders() { const sel = selected(), ch = changedKeys(), none = !sel.length;
  document.querySelector(".work").classList.toggle("noselect", none);
  FADERS.forEach(n => { const el = document.querySelector(`[data-name=${n}]`), vs = sel.map(k => scene.states[k] && scene.states[k][n]).filter(Boolean);
    const nums = vs.map(v => v.v), lo = Math.min(...nums), hi = Math.max(...nums), mixed = nums.length && lo !== hi, frozen = vs.find(v => v.frozen);
    const v = nums.length ? nums[0] : 0; el.querySelector(".track").style.setProperty("--v", v * 100 / maxOf(n)); el.querySelector(".val").textContent = nums.length ? v : "--";
    el.classList.toggle("mixed", !!mixed); el.classList.toggle("frozen", !!frozen);
    el.classList.toggle("changed", sel.some(k => { const [g, f] = k.split(":"); return ch.has(`${g}:${f}:${n}`); }));
    el.title = frozen ? `${n} is set by palette ${frozen.frozen}` : mixed ? `${n}: selected fixtures range from ${lo} to ${hi}` : ""; });
  hsv(); }

function drawPals() { const sel = selected(), order = [];
  sel.forEach(k => (scene.pals[k] || []).forEach(id => order.includes(id) || order.push(id)));
  const col = c => `hsl(${c.h} ${c.s * 100}% ${c.v * 50 * (2 - c.s)}%)`;
  /* Only the kinds this show has. */
  $("pals").innerHTML = '<div class="palrows"><h4>Colour</h4><div class="chips">' + S.palettes.map(p => { const i = order.indexOf(p.id);
    return `<button class="chip" data-p="${p.id}" aria-pressed="${i >= 0}" title="${p.name}${p.target ? ": follows " + p.target : ""}"><b style="background:${col(p.color)}"></b><span class="nm">${p.name}</span>${p.target ? '<i class="ln ph ph-link"></i>' : ""}${i >= 0 ? `<span class="order">${i + 1}</span>` : ""}</button>`; }).join("") + "</div></div>";
  $("palcount").textContent = order.length + " active"; }

function drawAnims() { $("acount").textContent = scene.anims.length;
  $("anims").innerHTML = scene.anims.length ? scene.anims.map((a, i) => { const gs = [...new Set(a.fixtures.map(f => groupOf(f[0]).name))].join(", ");
    return `<div class="anim ${a.on ? "" : "paused"}" data-i="${i}"><button class="ib play" aria-pressed="${a.on}" title="${a.on ? "Pause" : "Play"}"><i class="ph-fill ph-${a.on ? "pause" : "play"}"></i></button><div class="info" title="${a.name}">${a.name}<small>${gs}</small></div><div class="ak"></div><button class="ib quiet" title="Edit"><i class="ph ph-pencil-simple"></i></button><button class="ib quiet rm" title="Remove"><i class="ph ph-x"></i></button></div>`; }).join("") : '<p class="empty">No animations in this scene.</p>';
  document.querySelectorAll(".ak").forEach((k, i) => knob(k, "", SPEEDS.indexOf(scene.anims[i].speed) / 9, t => speedLabel(SPEEDS[Math.round(t * 9)]), true, speedParse, 1 / 9)); }

function drawChanges2() { const by = new Map(); let total = 0;
  scene.changes.forEach(c => { if (removed.has(c.join(":")) || !groupOf(c[0])) return; total++;
    if (!by.has(c[0])) by.set(c[0], new Map()); const m = by.get(c[0]); if (!m.has(c[1])) m.set(c[1], []); m.get(c[1]).push(c[2]); });
  $("ctotal").textContent = total; const X = '<button class="x" aria-label="Remove"><i class="ph ph-x"></i></button>', car = o => `<i class="caret ph-bold ph-caret-${o ? "down" : "right"}"></i>`;
  let h = ""; [...by.keys()].sort((a, b) => a - b).forEach(g => { const m = by.get(g), G = groupOf(g), o = openC.has(`${g}`), n = [...m.values()].reduce((a, p) => a + p.length, 0);
    h += `<div class="row g ${selGroups.has(g) ? "sel" : ""}" data-t="${g}" data-g="${g}">${car(o)}${G.name}<span class="v num">${n}</span>${X}</div>`;
    if (o) [...m.keys()].sort((a, b) => a - b).forEach(f => { const k = `${g}:${f}`, fo = openC.has(k), ps = m.get(f), nm = (G.fixtures.find(x => x.id === f) || { name: "#" + f }).name;
      h += `<div class="row f" data-t="${k}" data-g="${g}" data-f="${f}">${car(fo)}${nm}<span class="v num">${ps.length}</span>${X}</div>`;
      if (fo) ps.forEach(p => { const st = scene.states[k] && scene.states[k][p]; h += `<div class="row p" data-g="${g}" data-f="${f}" data-p="${p}">${p}<span class="v num">${st ? st.v : ""}</span>${X}</div>`; }); }); });
  $("changes").innerHTML = '<div class="ctree">' + (h || '<p class="empty">No changes in this scene.</p>') + "</div>"; }

function drawMasters() { $("masters").innerHTML = '<div id="k1"></div><div id="k2"></div>';
  knob($("k1"), "Master intensity", scene.master / 100, t => Math.round(t * 100), false, pctParse);
  knob($("k2"), "Master speed", SPEEDS.indexOf(scene.speed) / 9, t => speedLabel(SPEEDS[Math.round(t * 9)]), false, speedParse, 1 / 9);
  const head = document.querySelector(".side > header"); if (head) head.textContent = "Scene: " + scene.name; }
const drawAll = () => { drawLists(); drawFaders(); drawPals(); drawAnims(); drawChanges2(); drawMasters(); };

$("scenes").onclick = e => { const d = e.target.closest("[data-folder]"), r = e.target.closest("[data-scene]");
  if (d) { const i = +d.dataset.folder; folded.has(i) ? folded.delete(i) : folded.add(i); drawLists(); }
  if (r) { scene = S.scenes.find(s => s.id === +r.dataset.scene); drawAll(); } };
$("groups").onclick = e => { const d = e.target.closest("[data-folder]"), r = e.target.closest("[data-group]");
  if (d) { const i = +d.dataset.folder; folded.has(i) ? folded.delete(i) : folded.add(i); drawLists(); }
  if (r) { const g = +r.dataset.group; if (selGroups.has(g)) { selGroups.delete(g); [...limited].forEach(k => k.startsWith(g + ":") && limited.delete(k)); } else selGroups.add(g); drawLists(); drawFaders(); drawPals(); drawChanges2(); } };
$("fixtures").onclick = e => { const f = e.target.closest(".fx"); if (!f) return; const k = f.dataset.k; limited.has(k) ? limited.delete(k) : limited.add(k); drawLists(); drawFaders(); drawPals(); };
$("clear").onclick = () => { selGroups.clear(); limited.clear(); drawLists(); drawFaders(); drawPals(); drawChanges2(); };
$("changes").onclick = e => { const r = e.target.closest(".row"); if (!r) return; const { t, g, f, p } = r.dataset;
  if (e.target.closest(".x")) { const hit = scene.changes.filter(c => !removed.has(c.join(":")) && c[0] === +g && (f === undefined || c[1] === +f) && (p === undefined || c[2] === p)), go = () => edit(() => hit.forEach(c => removed.add(c.join(":")))); if (hit.length > 1) BL.dialog(`Remove ${hit.length} changes?`, "<p>These values go back to the scene's saved state. This can't be undone.</p>", [["Cancel"], ["Remove changes", "red-text", go]]); else go(); return; }
  if (t) { openC.has(t) ? openC.delete(t) : openC.add(t); drawChanges2(); } };
$("undoall").onclick = () => { const n = changedKeys().size; if (!n) return; BL.dialog(`Clear all ${n} changes?`, "<p>Every change in this scene goes back to its saved state. This can't be undone.</p>", [["Cancel"], ["Clear all", "red-text", () => { edit(() => scene.changes.forEach(c => removed.add(c.join(":")))); BL.toast(`Cleared ${n} changes`); }]]); };
$("anims").onclick = e => { const a = e.target.closest(".anim"); if (!a) return; const x = scene.anims[+a.dataset.i];
  if (e.target.closest(".rm")) BL.dialog(`Remove ${x.name}?`, "<p>It stops and its settings are lost. This can't be undone.</p>", [["Cancel"], ["Remove animation", "red-text", () => { edit(() => scene.anims.splice(+a.dataset.i, 1)); BL.toast("Removed animation"); }]]);
  else if (e.target.closest(".play")) { x.on = !x.on; drawAnims(); } };
$("xymarks").innerHTML = "";
/* Test hooks: #none, #multi, #frozen open those states. */
const st = location.hash.slice(1);
if (st === "none") selGroups.clear();
if (st === "multi") selGroups = new Set([3, 4, 1]);
if (st === "frozen") { scene = S.scenes.find(s => s.name === "STATIC"); selGroups = new Set([1]); }
drawAll();
/* Sidebar: System entries open the System draft. */
const SYSNAV = { System: "general", General: "general", DMX: "dmx", "DMX & Art-Net": "dmx", Plugins: "plugins", "MIDI & Serial": "devices", Screens: "screens", "Remote UI": "remote" };
document.querySelector(".nav").addEventListener("click", e => { const b = e.target.closest("button"); const k = b && SYSNAV[b.textContent.trim()]; if (k) location.href = "system.html#" + k; });
document.querySelectorAll(".nav button.sub").forEach(b => { const t = b.textContent.trim(); if (t === "Art-Net") b.remove(); else if (t === "Serial") b.remove(); else if (t === "MIDI") b.textContent = "MIDI & Serial"; else if (t === "DMX") b.textContent = "DMX & Art-Net"; });
document.querySelector(".nav").addEventListener("click", e => { const b = e.target.closest("button"), n = b && b.textContent.trim(); if (["Fixtures", "Groups", "Scenes", "Palettes"].includes(n)) location.href = "setup.html#" + n.toLowerCase(); });
const PAGENAV = {Logs:"logs",Audio:"audio","Views":"views","Perform":"performance","Scene Graph":"graph",Visualizer:"visualizer"};
document.querySelector(".nav").addEventListener("click", e => { const b = e.target.closest("button"), n = b && b.textContent.trim(); if (PAGENAV[n]) location.href = "pages.html#" + PAGENAV[n]; if (n === "Animations") location.href = "animation-editor.html"; });
$("pals").onclick = e => { const c = e.target.closest(".chip"); if (!c) return; const id = isNaN(+c.dataset.p) ? c.dataset.p : +c.dataset.p, sel = selected(); if (!sel.length) return;
  const on = sel.every(k => (scene.pals[k] || []).includes(id)); edit(() => sel.forEach(k => { const l = scene.pals[k] = (scene.pals[k] || []).filter(x => x !== id); if (!on) l.push(id); })); };

function edit(fn) { fn(); drawAll(); BL.dirty(); }

/* Fader edits become scene changes, so the list, count and save state stay true. Esc cancels a drag. */
const faderText = () => Object.fromEntries(FADERS.map(n => [n, document.querySelector(`[data-name=${n}] .val`).textContent]));
let drag = null;
addEventListener("pointerdown", e => { if (e.target.closest("#bank .track, #wheel, #xy")) drag = { before: faderText(), target: e.target }; }, true);
addEventListener("pointermove", e => { if (drag && drag.cancelled) e.stopImmediatePropagation(); }, true);
const commit = before => { const now = faderText(), names = FADERS.filter(n => now[n] !== before[n] && now[n] !== "--"), sel = selected(); if (!names.length || !sel.length) return; edit(() => sel.forEach(k => { const [g, f] = k.split(":").map(Number); scene.states[k] = scene.states[k] || {}; names.forEach(n => { scene.states[k][n] = { ...(scene.states[k][n] || {}), v: +now[n] }; const key = `${g}:${f}:${n}`; removed.delete(key); if (!scene.changes.some(c => c.join(":") === key)) scene.changes.push([g, f, n]); }); })); };
let lastText = faderText();
addEventListener("pointerup", () => { if (!drag) return; const d = drag; drag = null; if (!d.cancelled) commit(d.before); lastText = faderText(); }, true);
addEventListener("fadersync", () => { commit(lastText); lastText = faderText(); });
addEventListener("keydown", e => { if (e.key === "Escape" && drag && !drag.cancelled) { drag.cancelled = true; drawFaders(); hsv(); BL.toast("Change cancelled"); } }, true);

/* Labels, blackout and preview */
const iname = document.querySelector("[data-name=Alpha] .name"); if (iname) iname.textContent = "Intensity";
const blk = () => { $("pgb").textContent = BL.gm() === 0 ? "Restore" : "Blackout"; }; addEventListener("bl:gm", blk); blk(); $("pgb").onclick = () => BL.toggleBlack();
$("plv").setAttribute("aria-pressed", BL.live()); $("plv").onclick = () => { const on = !BL.live(); BL.setLive(on); $("plv").setAttribute("aria-pressed", on); BL.toast(on ? "Exclusive preview on: overlays are suppressed" : "Exclusive preview off"); };
const syncSoon = () => setTimeout(() => dispatchEvent(new Event("fadersync")));
$("gcentre").addEventListener("click", syncSoon);
$("xy").addEventListener("keydown", e => { if (e.key.startsWith("Arrow")) syncSoon(); });
["cr", "cg", "cb"].forEach(i => $(i).addEventListener("keydown", e => { if (e.key === "ArrowUp" || e.key === "ArrowDown") syncSoon(); }));

/* Below 1200px the Programmer needs the sidebar's width, so it collapses without changing the saved choice. */
{ const narrow = matchMedia("(max-width: 1199px)"), shell = document.querySelector(".shell"), saved = () => { try { return JSON.parse(sessionStorage.getItem("bl-state") || "{}").nav; } catch { return false; } };
  const fit = () => shell.classList.toggle("navmin", narrow.matches || !!saved()); narrow.addEventListener("change", fit); if (narrow.matches) fit(); }

/* A fader is selected by clicking, dragging or tabbing to it. Backspace or Delete undoes its
   changes from this session for the selected fixtures, removing changes this session added.
   With nothing left to undo, it removes the scene's earlier changes for it from the list. */
let picked = null;
const pick = el => { if (picked) picked.classList.remove("picked"); picked = el; if (el) el.classList.add("picked"); };
document.querySelectorAll("#bank .strip").forEach(st => { st.tabIndex = 0; st.title = st.title || "Backspace undoes this fader's change"; st.addEventListener("focus", () => pick(st)); });
addEventListener("pointerdown", e => { const st = e.target.closest("#bank .strip"); pick(st); if (st && !e.target.closest("input")) st.focus({ preventScroll: true }); }, true);
/* Arrow keys move a focused fader by its smallest unit: 1, or one degree on Hue. */
document.querySelectorAll("#bank .strip").forEach(st => st.addEventListener("keydown", e => { const d = { ArrowUp: 1, ArrowRight: 1, ArrowDown: -1, ArrowLeft: -1 }[e.key];
  if (!d || e.target !== st) return; e.preventDefault(); const n = st.dataset.name, v = st.querySelector(".val").textContent;
  if (st.classList.contains("frozen") || v === "--") return; setFader(n, Math.max(0, Math.min(maxOf(n), +v + d * BL.keySteps(e, maxOf(n))))); hsv(); syncSoon(); }));
function undoFader(n) { const sel = selected(), b = base.get(scene.id), label = document.querySelector(`[data-name=${n}] .name`).textContent; if (!sel.length) return;
  const isOn = key => scene.changes.some(c => c.join(":") === key);
  const edited = sel.filter(k => { const key = `${k}:${n}`, was = b.states[k] && b.states[k][n], now = scene.states[k] && scene.states[k][n];
    return (now && now.v) !== (was && was.v) || (!b.changes.has(key) && isOn(key) && !removed.has(key)); });
  if (edited.length) { edit(() => edited.forEach(k => { const key = `${k}:${n}`, was = b.states[k] && b.states[k][n];
      if (was) scene.states[k][n] = { ...was }; else if (scene.states[k]) delete scene.states[k][n];
      if (b.changes.has(key)) removed.delete(key); else { const i = scene.changes.findIndex(c => c.join(":") === key); if (i >= 0) scene.changes.splice(i, 1); } }));
    BL.toast(`${label}: undid the change on ${edited.length} fixture${edited.length > 1 ? "s" : ""}`); return; }
  const earlier = sel.map(k => `${k}:${n}`).filter(key => b.changes.has(key) && !removed.has(key));
  if (earlier.length) { edit(() => earlier.forEach(key => removed.add(key))); BL.toast(`${label}: removed ${earlier.length} change${earlier.length > 1 ? "s" : ""} from Scene changes`); return; }
  BL.toast(`${label} has no change to undo`); }
addEventListener("keydown", e => { if (!picked || (e.key !== "Backspace" && e.key !== "Delete")) return;
  if (e.target.closest && e.target.closest("input, textarea, select, [contenteditable]")) return;
  if (!picked.isConnected) { picked = null; return; } e.preventDefault(); undoFader(picked.dataset.name); });

/* Add animation: the starter presets from AnimationPreset (crates/shared/src/state/engine.rs), as the egui
   Programmer offers them. One tap applies a preset to the selection, a second tap removes it. Names come
   from the preset; details are edited in the Animation editor. */
{ const P = (label, prop, desc) => ({ label, prop, desc });
  const CATS = [
    ["Beat", "React to the beat pulse", [P("Kick Flash", "Alpha", "Beat pulse to additive intensity, instant attack"), P("Strobe Stab", "Strobe", "Beat pulse to a shutter burst, very short release"), P("Beam Punch", "Focus", "Beat pulse to focus, slower decay"), P("Color Flicker", "ColorHue", "Beat pulse to a hue nudge that settles back")]],
    ["Frequency", "React to the energy in a frequency band", [P("Bass Pump", "Alpha", "40-180 Hz energy scales intensity"), P("Sub Rumble", "ColorValue", "20-60 Hz energy scales colour value"), P("Hi-Hat Shimmer", "Alpha", "6-16 kHz energy adds intensity sparkle"), P("Mid Bloom", "ColorValue", "300 Hz-2.5 kHz energy adds colour value"), P("Energy Lift", "ColorValue", "Broad-band energy scales colour value, slowly")]],
    ["Section", "Only in a section of the song: drop or breakdown", [P("Drop Blinder", "Alpha", "Energy adds intensity, drop only"), P("Breakdown Glow", "Alpha", "Inverted energy adds intensity, breakdown only"), P("Drop Strobe", "Strobe", "Beat pulse to the shutter, drop only")]],
    ["Movement", "Tempo-locked, not sound-reactive", [P("Tempo Sweep", "Pan", "Sine on pan, 4-beat cycle"), P("Ping-Pong Tilt", "Tilt", "Triangle on tilt, 2 beats, reverses"), P("Eighth Chase", "Alpha", "1/8 square on intensity, 1-beat chase"), P("Hue Drift", "ColorHue", "Sine on hue, free-running 30 s")]]];
  const FADER = { Alpha: "Intensity", Strobe: "Strobe", Focus: "Focus", ColorHue: "Hue", ColorValue: "Value", Pan: "Pan", Tilt: "Tilt" };
  const esc = s => String(s).replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
  const key = fx => fx.map(f => f.join(":")).sort().join(",");
  const onSel = (name, fx) => scene.anims.findIndex(a => a.name === name && key(a.fixtures) === key(fx));
  const ORDER = ["Alpha", "Strobe", "Focus", "ColorHue", "ColorValue", "Pan", "Tilt"], shut = new Set();
  const ALL = CATS.flatMap(([c, , ps]) => ps.map(p => ({ ...p, cat: c })));
  function body(d) { const fx = selected().map(k => k.split(":").map(Number)), none = !fx.length;
    const frozen = n => document.querySelector(`[data-name=${FADER[n]}]`)?.classList.contains("frozen");
    let h = `<p class="ap-who">${none ? "Select groups or fixtures first." : `${esc($("selname").textContent)} in ${esc(scene.name)}. Click to add, click again to remove.`}</p><div class="body ap-list">`;
    ORDER.forEach(prop => { const ps = ALL.filter(p => p.prop === prop), o = !shut.has(prop), fr = frozen(prop), n = none ? 0 : ps.filter(p => onSel(p.label, fx) >= 0).length;
      h += `<button class="row folder" data-pf="${prop}" aria-expanded="${o}"><i class="ph-bold ph-caret-${o ? "down" : "right"}"></i><i class="ph-fill ph-folder${o ? "-open" : ""} fic"></i>${FADER[prop]}${fr ? '<i class="ph ph-link ap-lk" title="Set by a palette"></i>' : ""}<span class="v num">${n ? n + " on" : o ? "" : ps.length}</span></button>`;
      if (o) h += ps.map(p => { const on = !none && onSel(p.label, fx) >= 0, off = none || (!on && fr);
        return `<button class="row leaf ap ${on ? "on" : ""}" data-p="${esc(p.label)}" aria-pressed="${on}" ${off ? "disabled" : ""} title="${esc(fr && !on ? `${FADER[prop]} is set by a palette` : p.desc)}"><span class="cut">${esc(p.label)}</span><span class="v">${p.cat}</span></button>`; }).join(""); });
    d.querySelector(".dlgb").innerHTML = h + "</div>"; }
  $("addanim").onclick = () => { if (!BL.online()) return BL.notSent("Add animation");
    const d = BL.dialog("Add animation", "", [["Done", "primary"]]); if (!d) return; d.querySelector(".dlg").classList.add("ap-dlg"); body(d);
    d.querySelector(".dlgb").addEventListener("click", e => { const f = e.target.closest("[data-pf]"); if (f) { const k = f.dataset.pf; shut.has(k) ? shut.delete(k) : shut.add(k); body(d); d.querySelector(`[data-pf="${k}"]`).focus(); return; }
      const b = e.target.closest(".ap"); if (!b || b.disabled) return;
      const fx = selected().map(k => k.split(":").map(Number)), p = ALL.find(x => x.label === b.dataset.p), i = onSel(p.label, fx);
      if (i >= 0) edit(() => scene.anims.splice(i, 1)); else edit(() => scene.anims.push({ name: p.label, prop: p.prop, on: true, speed: "_1", fixtures: fx }));
      body(d); d.querySelector(`.ap[data-p="${CSS.escape(p.label)}"]`)?.focus(); });
    d.querySelector(".ap:not([disabled])")?.focus(); };
}
