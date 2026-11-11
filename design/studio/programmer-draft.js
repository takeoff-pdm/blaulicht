/* Draft Programmer, wired to SPARTACUS_SOLI (data.js). Edits stay in the page. */
const $ = id => document.getElementById(id), S = window.SHOW, work = $("work");
const SPEEDS = ["_1_16", "_1_8", "_1_4", "_1_2", "_1", "_2", "_4", "_8", "_16", "_32"], speedLabel = s => s.slice(1).replace("_", "/") + "x";
const ATTRS = ["Alpha", "Strobe", "Focus", "Tilt", "Pan", "Hue", "Saturation", "Value"], maxOf = n => n === "Hue" ? 360 : 255;
const groupOf = id => S.groups.find(g => g.id === id);
let scene = S.scenes.find(s => s.id === S.focus), selGroups = new Set([3]), limited = new Set(), openG = new Set([3]), folded = new Set(), openC = new Set(), removed = new Set(), filter = "";

/* Selection */
const selected = () => { if (limited.size) return [...limited]; const r = []; selGroups.forEach(id => groupOf(id).fixtures.forEach(f => r.push(`${id}:${f.id}`))); return r; };
function drawTree() { const q = filter.toLowerCase(), hit = t => t.toLowerCase().includes(q);
  const groupHtml = g => { const fx = g.fixtures.filter(f => !q || hit(g.name) || hit(f.name)); if (q && !fx.length) return "";
    const o = openG.has(g.id) || !!q;
    return `<div class="row leaf ${selGroups.has(g.id) ? "on" : ""}"><button class="tw" data-tw="${g.id}" aria-label="${o ? "Hide" : "Show"} fixtures of ${g.name}"><i class="ph-bold ph-caret-${o ? "down" : "right"}"></i></button><button class="pick" data-group="${g.id}"><span class="cut">${g.name}</span></button><span class="v">${g.fixtures.length}</span></div>` +
      (o ? fx.map(f => `<button class="fx" title="${f.name}" aria-pressed="${limited.has(`${g.id}:${f.id}`)}" data-k="${g.id}:${f.id}"><span class="nm">${f.name}</span></button>`).join("") : ""); };
  let h = ""; S.folders.filter(f => f.kind === "Groups").forEach(f => { const inner = S.groups.filter(g => g.folder === f.id).map(groupHtml).join(""); if (q && !inner) return; const o = !folded.has(f.id) || !!q;
    h += `<button class="row folder" data-folder="${f.id}"><i class="ph-bold ph-caret-${o ? "down" : "right"}"></i><i class="ph-fill ph-folder${o ? "-open" : ""} fic"></i>${f.name}</button>` + (o ? inner : ""); });
  h += S.groups.filter(g => g.folder == null).map(groupHtml).join("");
  $("tree").innerHTML = h || '<p class="empty">No groups or fixtures match.</p>';
  const names = [...selGroups].map(i => groupOf(i).name).join(", "), total = [...selGroups].reduce((a, i) => a + groupOf(i).fixtures.length, 0);
  $("count").textContent = limited.size ? `${limited.size}/${total}` : total || "";
  $("selname").textContent = !selGroups.size ? "Nothing selected" : limited.size ? `${names} (${limited.size} of ${total} fixtures)` : `${names} (all ${total} fixtures)`;
  work.classList.toggle("none", !selGroups.size); }
$("tree").onclick = e => { const d = e.target.closest("[data-folder]"), t = e.target.closest("[data-tw]"), p = e.target.closest("[data-group]"), x = e.target.closest(".fx");
  if (d) { const i = +d.dataset.folder; folded.has(i) ? folded.delete(i) : folded.add(i); }
  if (t) { const i = +t.dataset.tw; openG.has(i) ? openG.delete(i) : openG.add(i); }
  if (p) { const g = +p.dataset.group; if (selGroups.has(g)) { selGroups.delete(g); [...limited].forEach(k => k.startsWith(g + ":") && limited.delete(k)); } else { selGroups.add(g); openG.add(g); } }
  if (x) { const k = x.dataset.k; limited.has(k) ? limited.delete(k) : limited.add(k); }
  if (d || t || p || x) drawAll(); };
$("filter").oninput = e => { filter = e.target.value; drawTree(); };
$("clear").onclick = () => { selGroups.clear(); limited.clear(); drawAll(); };

/* Faders */
const strip = (n, label) => `<div class="strip" data-name="${n}"><div class="track"><div class="scale"></div><div class="mask"></div><div class="slot"></div><div class="cap"><i class="bevel"></i><i class="ridges"></i><i class="ridges low"></i><i class="ind"></i></div></div><span class="val num"></span><span class="name">${label}</span></div>`;
$("f-int").innerHTML = strip("Alpha", "Intensity"); $("f-val").innerHTML = strip("Value", "Value"); $("f-beam").innerHTML = strip("Strobe", "Strobe") + strip("Focus", "Focus");

/* Values: one read and one write, used by every control. */
const read = n => { const vs = selected().map(k => scene.states[k] && scene.states[k][n]).filter(Boolean), nums = vs.map(v => v.v);
  return { v: nums.length ? nums[0] : null, mixed: nums.length && Math.min(...nums) !== Math.max(...nums), lo: Math.min(...nums), hi: Math.max(...nums), frozen: vs.find(v => v.frozen)?.frozen }; };
const isChanged = n => selected().some(k => { const [g, f] = k.split(":"); return scene.changes.some(c => c[0] === +g && c[1] === +f && c[2] === n && !removed.has(c.join(":"))); });
const write = (n, v) => { v = Math.round(Math.max(0, Math.min(maxOf(n), v))); selected().forEach(k => { const [g, f] = k.split(":").map(Number); scene.states[k] = scene.states[k] || {}; scene.states[k][n] = { ...scene.states[k][n], v };
    const key = `${g}:${f}:${n}`; removed.delete(key); if (!scene.changes.some(c => c.join(":") === key)) scene.changes.push([g, f, n]); }); dirty = true; };
let dirty = false;
const flush = () => { if (dirty) { dirty = false; BL.dirty(); drawChanges(); } };
function drawControls() { ATTRS.forEach(n => { const r = read(n), el = document.querySelector(`[data-name=${n}]`), ch = isChanged(n);
    if (el) { el.querySelector(".track").style.setProperty("--v", (r.v || 0) * 100 / maxOf(n)); el.querySelector(".val").textContent = r.v == null ? "--" : r.v; el.classList.toggle("changed", ch); el.classList.toggle("mixed", !!r.mixed); el.classList.toggle("frozen", !!r.frozen);
      el.title = r.frozen ? `${n} is set by palette ${r.frozen}` : r.mixed ? `${n}: selected fixtures range from ${r.lo} to ${r.hi}` : ""; }
    const l = $("l-" + n); if (l) { l.classList.toggle("changed", ch); const i = l.querySelector("input"); if (document.activeElement !== i) i.value = r.v == null ? "--" : r.v; } });
  const val = n => (read(n).v || 0) / maxOf(n), h = val("Hue") * 360, s = val("Saturation"), v = val("Value"), l = v * (1 - s / 2), sl = l === 0 || l === 1 ? 0 : (v - l) / Math.min(l, 1 - l);
  $("preview").style.background = `hsl(${h} ${sl * 100}% ${l * 100}%)`; const a = h * Math.PI / 180;
  $("mark").style.left = 50 + 50 * s * Math.sin(a) + "%"; $("mark").style.top = 50 - 50 * s * Math.cos(a) + "%"; $("wheel").style.setProperty("--dim", 1 - v);
  const x = val("Pan") * 100, y = val("Tilt") * 100; $("xdot").style.left = $("xv").style.left = x + "%"; $("xdot").style.top = $("xh").style.top = y + "%"; }

/* Dragging. Esc restores the values from before the drag. */
let snap = null;
const snapshot = () => { snap = selected().map(k => [k, JSON.parse(JSON.stringify(scene.states[k] || {}))]); snap.changes = scene.changes.length; };
const dragOn = (el, set) => el.addEventListener("pointerdown", e => { if (e.target.closest(".strip.frozen")) return; el.setPointerCapture(e.pointerId); snapshot(); const on = ev => { if (!snap) return; set(ev, el); drawControls(); }; on(e); el.onpointermove = on; el.onpointerup = el.onpointercancel = () => { el.onpointermove = null; snap = null; flush(); }; });
addEventListener("keydown", e => { if (e.key === "Escape" && snap) { snap.forEach(([k, st]) => scene.states[k] = st); scene.changes.length = snap.changes; snap = null; dirty = false; drawControls(); BL.toast("Change cancelled"); } }, true);
document.querySelectorAll(".track").forEach(t => dragOn(t, (ev, el) => { const r = el.getBoundingClientRect(), n = el.parentElement.dataset.name; write(n, (r.bottom - 18 - ev.clientY) / (r.height - 36) * maxOf(n)); }));
dragOn($("wheel"), (ev, w) => { const r = w.getBoundingClientRect(), R = r.width / 2, dx = ev.clientX - r.left - R, dy = ev.clientY - r.top - R; let a = Math.atan2(dx, -dy) / (Math.PI * 2); if (a < 0) a += 1; write("Hue", a * 360); write("Saturation", Math.min(1, Math.hypot(dx, dy) / R) * 255); });
dragOn($("xy"), (ev, xy) => { const r = xy.getBoundingClientRect(); write("Pan", (ev.clientX - r.left) / r.width * 255); write("Tilt", (ev.clientY - r.top) / r.height * 255); });
$("xy").addEventListener("keydown", e => { const s = e.shiftKey ? 10 : 1, d = { ArrowLeft: [-s, 0], ArrowRight: [s, 0], ArrowUp: [0, -s], ArrowDown: [0, s] }[e.key]; if (!d) return; e.preventDefault(); write("Pan", (read("Pan").v || 0) + d[0]); write("Tilt", (read("Tilt").v || 0) + d[1]); drawControls(); flush(); });
$("gcentre").onclick = () => { write("Pan", 128); write("Tilt", 128); drawControls(); flush(); };
document.querySelectorAll(".fields input").forEach(i => { i.addEventListener("focus", () => { i.dataset.was = i.value; i.select(); });
  i.addEventListener("keydown", e => { if (e.key === "Enter") i.blur(); if (e.key === "Escape") { i.value = i.dataset.was; i.blur(); } });
  i.addEventListener("blur", () => { const n = parseInt(i.value, 10); if (i.value !== i.dataset.was && !isNaN(n)) { write(i.dataset.a, n); flush(); } drawControls(); }); });

/* Palettes */
function drawPals() { const sel = selected(), order = []; sel.forEach(k => (scene.pals[k] || []).forEach(id => order.includes(id) || order.push(id)));
  const col = c => `hsl(${c.h} ${c.s * 100}% ${c.v * 50 * (2 - c.s)}%)`;
  $("pals").innerHTML = S.palettes.map(p => { const i = order.indexOf(p.id); return `<button class="chip" data-p="${p.id}" aria-pressed="${i >= 0}" title="${p.name}${p.target ? ": follows " + p.target : ""}"><b style="background:${col(p.color)}"></b><span class="nm">${p.name}</span>${p.target ? '<i class="ln ph ph-link"></i>' : ""}${i >= 0 ? `<span class="order">${i + 1}</span>` : ""}</button>`; }).join("");
  $("palcount").textContent = order.length ? order.length + " active" : ""; }
$("pals").onclick = e => { const c = e.target.closest(".chip"), sel = selected(); if (!c || !sel.length) return; const id = +c.dataset.p, on = sel.every(k => (scene.pals[k] || []).includes(id)); sel.forEach(k => { const l = scene.pals[k] = (scene.pals[k] || []).filter(x => x !== id); if (!on) l.push(id); }); BL.dirty(); drawPals(); };

/* What changed */
function drawChanges() { const by = new Map(); let total = 0;
  scene.changes.forEach(c => { if (removed.has(c.join(":")) || !groupOf(c[0])) return; total++; if (!by.has(c[0])) by.set(c[0], new Map()); const m = by.get(c[0]); if (!m.has(c[1])) m.set(c[1], []); m.get(c[1]).push(c[2]); });
  $("ctotal").textContent = total; $("chg").textContent = `Changes ${total}`;
  const X = '<button class="x" aria-label="Remove"><i class="ph ph-x"></i></button>', car = o => `<i class="caret ph-bold ph-caret-${o ? "down" : "right"}"></i>`; let h = "";
  [...by.keys()].sort((a, b) => a - b).forEach(g => { const m = by.get(g), G = groupOf(g), o = openC.has(`${g}`), n = [...m.values()].reduce((a, p) => a + p.length, 0);
    h += `<div class="row g ${selGroups.has(g) ? "sel" : ""}" data-t="${g}" data-g="${g}">${car(o)}${G.name}<span class="v num">${n}</span>${X}</div>`;
    if (o) [...m.keys()].sort((a, b) => a - b).forEach(f => { const k = `${g}:${f}`, fo = openC.has(k), ps = m.get(f), nm = (G.fixtures.find(x => x.id === f) || { name: "#" + f }).name;
      h += `<div class="row f" data-t="${k}" data-g="${g}" data-f="${f}">${car(fo)}${nm}<span class="v num">${ps.length}</span>${X}</div>`;
      if (fo) ps.forEach(p => { const st = scene.states[k] && scene.states[k][p]; h += `<div class="row p" data-g="${g}" data-f="${f}" data-p="${p}">${p}<span class="v num">${st ? st.v : ""}</span>${X}</div>`; }); }); });
  $("changes").innerHTML = '<div class="ctree">' + (h || '<p class="empty">No changes in this scene.</p>') + "</div>"; }
$("changes").onclick = e => { const r = e.target.closest(".row"); if (!r) return; const { t, g, f, p } = r.dataset;
  if (e.target.closest(".x")) { const hit = scene.changes.filter(c => !removed.has(c.join(":")) && c[0] === +g && (f === undefined || c[1] === +f) && (p === undefined || c[2] === p)), go = () => { hit.forEach(c => removed.add(c.join(":"))); BL.dirty(); drawChanges(); drawControls(); };
    if (hit.length > 1) BL.dialog(`Remove ${hit.length} changes?`, "<p>These values go back to the scene's saved state. This can't be undone.</p>", [["Cancel"], ["Remove changes", "red-text", go]]); else go(); return; }
  if (t) { openC.has(t) ? openC.delete(t) : openC.add(t); drawChanges(); } };
$("undoall").onclick = () => { const n = scene.changes.filter(c => !removed.has(c.join(":"))).length; if (!n) return; BL.dialog(`Clear all ${n} changes?`, "<p>Every change in this scene goes back to its saved state. This can't be undone.</p>", [["Cancel"], ["Clear all", "red-text", () => { scene.changes.forEach(c => removed.add(c.join(":"))); BL.dirty(); drawChanges(); drawControls(); BL.toast(`Cleared ${n} changes`); }]]); };
$("chg").onclick = () => { const on = work.classList.toggle("showright"); $("chg").setAttribute("aria-expanded", on); };

/* Knobs, as on the current page. */
function knobSvg(t) { const pt = (a, r) => [50 + r * Math.cos(a), 50 + r * Math.sin(a)].map(x => x.toFixed(2)), start = Math.PI / 2 + Math.PI * .1, sweep = Math.PI * 1.8; let ticks = "";
  for (let i = 0; i <= 12; i++) { const f = i / 12, a = start + sweep * f, [x1, y1] = pt(a, 41), [x2, y2] = pt(a, i % 3 ? 45 : 48); ticks += `<line class="tick ${f <= t ? "lit" : ""}" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/>`; }
  const a = start + sweep * t, [px, py] = pt(a, 29), [qx, qy] = pt(a, 12);
  return `<svg viewBox="0 0 100 100" aria-hidden="true">${ticks}<circle class="face" cx="50" cy="50" r="36"/><circle class="inner" cx="50" cy="50" r="26"/><line class="ptr" x1="${qx}" y1="${qy}" x2="${px}" y2="${py}"/></svg>`; }
function knob(el, name, t, fmt, small) { const draw = () => el.innerHTML = knobSvg(t) + `<span class="val num">${fmt(t)}</span>` + (name ? `<span class="name">${name}</span>` : ""); el.className = "knob" + (small ? " small" : ""); draw();
  el.onpointerdown = e => { el.setPointerCapture(e.pointerId); let y = e.clientY; el.onpointermove = m => { t = Math.max(0, Math.min(1, t + (y - m.clientY) * .005)); y = m.clientY; draw(); }; el.onpointerup = () => el.onpointermove = null; }; }
function drawAnims() { $("acount").textContent = scene.anims.length;
  $("anims").innerHTML = scene.anims.length ? scene.anims.map((a, i) => { const gs = [...new Set(a.fixtures.map(f => groupOf(f[0]).name))].join(", ");
    return `<div class="anim ${a.on ? "" : "paused"}" data-i="${i}"><button class="ib play" aria-pressed="${a.on}" title="${a.on ? "Pause" : "Play"}"><i class="ph-fill ph-${a.on ? "pause" : "play"}"></i></button><div class="info" title="${a.name}">${a.name}<small>${gs}</small></div><div class="ak"></div><button class="ib quiet" title="Edit"><i class="ph ph-pencil-simple"></i></button><button class="ib quiet rm" title="Remove"><i class="ph ph-x"></i></button></div>`; }).join("") : '<p class="empty">No animations in this scene.</p>';
  document.querySelectorAll(".ak").forEach((k, i) => knob(k, "", SPEEDS.indexOf(scene.anims[i].speed) / 9, t => speedLabel(SPEEDS[Math.round(t * 9)]), true)); }
$("anims").onclick = e => { const a = e.target.closest(".anim"); if (!a) return; const x = scene.anims[+a.dataset.i];
  if (e.target.closest(".rm")) BL.dialog(`Remove ${x.name}?`, "<p>It stops and its settings are lost. This can't be undone.</p>", [["Cancel"], ["Remove animation", "red-text", () => { scene.anims.splice(+a.dataset.i, 1); BL.dirty(); drawAnims(); }]]);
  else if (e.target.closest(".play")) { x.on = !x.on; drawAnims(); } };

/* Scene bar */
const sceneOpts = s => `<option value="${s.id}">${s.name}</option>`;
$("scene").innerHTML = S.folders.filter(f => f.kind === "Scenes").map(f => `<optgroup label="${f.name}">${S.scenes.filter(s => s.folder === f.id).map(sceneOpts).join("")}</optgroup>`).join("") + S.scenes.filter(s => s.folder == null).map(sceneOpts).join("");
$("scene").onchange = e => { scene = S.scenes.find(s => s.id === +e.target.value); drawAll(); };
const blk = () => { $("pgb").textContent = BL.gm() === 0 ? "Restore" : "Blackout"; }; addEventListener("bl:gm", blk); blk(); $("pgb").onclick = () => BL.toggleBlack();
$("plv").setAttribute("aria-pressed", BL.live()); $("plv").onclick = () => { const on = !BL.live(); BL.setLive(on); $("plv").setAttribute("aria-pressed", on); };
function drawMasters() { $("scene").value = scene.id; knob($("k1"), "Intensity", scene.master / 100, t => Math.round(t * 100)); knob($("k2"), "Speed", SPEEDS.indexOf(scene.speed) / 9, t => speedLabel(SPEEDS[Math.round(t * 9)])); }

const drawAll = () => { drawTree(); drawControls(); drawPals(); drawChanges(); drawAnims(); drawMasters(); };
drawAll();

/* Below 1200px the sidebar collapses too, without changing the saved choice. */
{ const narrow = matchMedia("(max-width: 1199px)"), shell = document.querySelector(".shell"), saved = () => { try { return JSON.parse(sessionStorage.getItem("bl-state") || "{}").nav; } catch { return false; } };
  const fit = () => shell.classList.toggle("navmin", narrow.matches || !!saved()); narrow.addEventListener("change", fit); if (narrow.matches) fit(); }
