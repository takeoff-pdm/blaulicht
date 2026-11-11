/* Shared by every page: one status bar and one source of truth for blackout, save state,
   and connection. Runs right after <footer class="status"> so page scripts find BL. */
(() => {
  const K = "bl-state", url = new URLSearchParams(location.search), $$ = (s, r = document) => [...r.querySelectorAll(s)], byId = i => document.getElementById(i);
  let G = { gm: 100, back: 100, dirty: false, fail: false, pending: 0, warn: 3, err: 1, live: false, state: "ok", remote: false, theme: "dark", setlistOk: false, nav: false };
  try { Object.assign(G, JSON.parse(sessionStorage.getItem(K)) || {}); } catch {}
  const save = () => { try { sessionStorage.setItem(K, JSON.stringify(G)); } catch {} };
  if (url.has("state")) G.state = url.get("state");
  if (url.has("remote")) G.remote = url.get("remote") !== "0";
  if (url.get("state") === "ok") G.fail = false;
  save();
  const f = document.querySelector("footer.status"), shell = document.querySelector(".shell"), root = document.documentElement;
  if (location.hash === "#light" || location.hash === "#dark") G.theme = location.hash.slice(1);
  root.dataset.theme = G.theme; new MutationObserver(() => { G.theme = root.dataset.theme; save(); }).observe(root, { attributes: true, attributeFilter: ["data-theme"] });
  const online = () => G.state !== "disconnected" && G.state !== "restarting", black = () => G.gm === 0;
  const themeBtn = document.createElement("button"); themeBtn.id = "theme"; themeBtn.hidden = true; document.body.append(themeBtn);

  /* Status bar */
  const spark = (id, n) => `<span class="cell gauge" id="${id}"><svg class="spark" viewBox="0 0 60 14" preserveAspectRatio="none" aria-hidden="true"><polyline/></svg><span>${n}</span></span>`;
  function status() { const st = online() ? "" : " stale", savetxt = G.fail ? `<span class="fail" id="saved"><i class="ph ph-warning-octagon"></i> Save failed</span>` : G.dirty ? `<span class="dirty" id="saved">Unsaved changes</span>` : `<span class="dot"></span><span id="saved">Saved</span>`;
    f.innerHTML = (black() ? `<button class="cell black" id="stb" title="Restore Grand Master (Cmd/Ctrl+B)"><i class="ph-fill ph-prohibit"></i>${online() ? "BLACKOUT" : "BLACKOUT, last known"}${online() ? `, press to restore ${G.back}%` : ""}</button>` : "") +
      `<span class="cell show"><i class="ph ph-file"></i><b id="sf">SPARTACUS_SOLI</b>${savetxt}</span>` +
      (online() ? "" : `<span class="cell bad"><span class="dot bad"></span>Engine ${G.state}</span>`) +
      `<span class="cell gauge audio${st}"><span class="meter" aria-label="Audio input level"><i id="ml" style="--v:${online() ? 60 : 0}%"></i></span><span>Audio</span></span><span class="cell${st}"><span class="tempo" id="bpm">${online() ? "128.0" : "---"}</span>BPM</span>` +
      `<span class="${st}">${spark("gl", "Engine")}</span><span class="${st}">${spark("gp", "Plugins")}</span>` +
      `<span class="r">${G.pending ? `<a class="cell dirty" href="system.html#dmx">${G.pending} routing pending</a>` : ""}${G.warn || G.err ? `<a class="cell probs" href="system.html#general" title="${[G.err ? `${G.err} error${G.err === 1 ? "" : "s"}` : "", G.warn ? `${G.warn} warning${G.warn === 1 ? "" : "s"}` : ""].filter(Boolean).join(", ")}. Open the problem list.">${G.err ? '<i class="ph-fill ph-x-circle e"></i>' : ""}${G.warn ? '<i class="ph ph-warning w"></i>' : ""}${G.err + G.warn} issue${G.err + G.warn === 1 ? "" : "s"}</a>` : ""}<span class="cell${st}">Remote UI ${online() ? 1 : "--"}</span></span>`;
    drawSparks(); }
  const series = [["gl", "Engine", 1, 8], ["gp", "Plugins", .6, 3]].map(([id, name, base, max]) => ({ id, name, base, max, d: Array.from({ length: 30 }, () => base * (.7 + Math.random() * .6)) }));
  const drawSparks = () => series.forEach(s => { const el = byId(s.id); if (!el) return; const v = s.d[s.d.length - 1], t = v / s.max, l = el.querySelector("polyline");
    l.setAttribute("points", s.d.map((x, i) => `${(i * 60 / (s.d.length - 1)).toFixed(1)},${(13 - Math.min(1, x / s.max) * 12).toFixed(1)}`).join(" "));
    l.style.stroke = !online() ? "var(--muted)" : t > .85 ? "var(--red)" : t > .6 ? "var(--warn)" : "var(--ok)"; el.title = online() ? `${s.name}: ${v.toFixed(1)} ms of ${s.max} ms` : "Not updating, engine offline"; });
  if (!matchMedia("(prefers-reduced-motion: reduce)").matches) {
    setInterval(() => { if (!online()) return; series.forEach(s => { s.d.shift(); s.d.push(s.base * (.7 + Math.random() * .6)); }); drawSparks(); }, 200);
    setInterval(() => { if (online() && byId("ml")) byId("ml").style.setProperty("--v", 45 + Math.random() * 40 + "%"); }, 60000 / 128);
  }

  /* Messages and dialogs. The native modal gives a focus trap, a locked page and Esc. */
  /* Notifications, JetBrains style: cards stacked in the bottom right, newest at the bottom.
     Information closes after 5 s (8 s with an action, paused while hovered); errors stay until closed.
     A repeated message counts up instead of stacking, and at most 4 show at once. */
  const toast = (t, bad, action) => { const host = document.querySelector("dialog[open]") || document.body;
    let stack = host.querySelector(":scope > .notes"); if (!stack) { stack = document.createElement("div"); stack.className = "notes"; stack.setAttribute("role", "status"); stack.setAttribute("aria-live", "polite"); host.append(stack); }
    const same = [...stack.children].find(n => n.dataset.text === t && !n.classList.contains("leaving"));
    if (same) { const c = same.querySelector(".nt-count"), k = (+same.dataset.count || 1) + 1; same.dataset.count = k; c.textContent = "×" + k; c.hidden = false; same.restart?.(); return same; }
    const o = document.createElement("div"); o.className = "note" + (bad ? " bad" : ""); o.dataset.text = t;
    o.innerHTML = `<i class="nt-ic ph-fill ${bad ? "ph-x-circle" : "ph-info"}" aria-hidden="true"></i><div class="nt-msg"><span class="nt-txt"></span><span class="nt-count" hidden></span><div class="nt-acts"></div></div><button class="nt-x" aria-label="Close notification" title="Close"><i class="ph ph-x"></i></button>`;
    o.querySelector(".nt-txt").textContent = t;
    const close = () => { if (o.classList.contains("leaving")) return; o.classList.add("leaving"); clearTimeout(o.timer); setTimeout(() => o.remove(), 160); };
    if (action) { const b = document.createElement("button"); b.className = "nt-act"; b.textContent = action[0]; b.onclick = () => { close(); action[1](); }; o.querySelector(".nt-acts").append(b); }
    o.querySelector(".nt-x").onclick = close;
    const life = bad ? 0 : action ? 8000 : 5000;
    o.restart = () => { clearTimeout(o.timer); if (life) o.timer = setTimeout(close, life); };
    o.addEventListener("pointerenter", () => clearTimeout(o.timer)); o.addEventListener("pointerleave", o.restart);
    stack.append(o); o.restart();
    const live = [...stack.children].filter(n => !n.classList.contains("leaving")); if (live.length > 4) live.slice(0, live.length - 4).forEach(n => n.querySelector(".nt-x").click());
    return o; };
  const notSent = what => toast(`${what} not sent: engine ${G.state}`, true);
  function dialog(title, body, actions) { if (document.querySelector("dialog[open]")) return null; const d = document.createElement("dialog"); d.innerHTML = `<div class="dlg" aria-label="${title}"><header>${title}</header><div class="dlgb">${body}</div><footer>${actions.map(([t, c], i) => `<button class="mini ${c || ""}" data-a="${i}">${t}</button>`).join("")}</footer></div>`;
    d.addEventListener("close", () => { const ns = d.querySelector(":scope > .notes"), top = document.querySelector("body > .notes"); if (ns?.children.length) top ? top.append(...ns.children) : document.body.append(ns); d.remove(); }); d.addEventListener("click", e => { const b = e.target.closest("[data-a]"); if (b && actions[+b.dataset.a][2]?.(d) !== false) d.close(); }); document.body.append(d); d.showModal(); (d.querySelector("input,select") || d.querySelector("footer button")).focus(); return d; }

  /* Type a value in place of its readout. Enter or leaving the field applies, Esc cancels.
     parse turns the text into a number; values are clamped to min..max before apply. */
  function editValue(el, { value, min = -Infinity, max = Infinity, label = "Value", parse = Number, apply }) {
    if (el.querySelector("input.inplace")) return;
    if (!online()) return notSent(label);
    /* The field floats over the readout, which stays in place (hidden), so nothing around it moves. */
    const i = document.createElement("input"), staticPos = getComputedStyle(el).position === "static";
    i.className = "inplace"; i.value = value; i.setAttribute("aria-label", `${label}, type a value`);
    i.style.width = Math.max(44, el.offsetWidth + 8) + "px"; i.style.height = Math.max(18, el.offsetHeight + 2) + "px";
    if (staticPos) el.style.position = "relative"; el.classList.add("editing"); el.append(i); i.focus(); i.select();
    let done = false;
    const finish = ok => { if (done) return; done = true; const text = i.value.trim(), v = parse(text); i.remove(); el.classList.remove("editing"); if (staticPos) el.style.position = "";
      if (ok && text !== "" && text !== String(value) && Number.isFinite(v)) apply(Math.min(max, Math.max(min, v))); };
    i.addEventListener("keydown", e => { if (e.key === "b" && (e.metaKey || e.ctrlKey)) return; e.stopPropagation();
      if (e.key === "Enter") { e.preventDefault(); finish(true); } if (e.key === "Escape") { e.preventDefault(); finish(false); } });
    i.addEventListener("blur", () => finish(true));
    ["pointerdown", "dblclick"].forEach(t => i.addEventListener(t, e => e.stopPropagation()));
  }

  /* Arrow-key acceleration: one step per press, then held keys speed up after 300 ms and
     reach the cap after about 2 s. The cap is a 25th of the control's range, so short ranges barely accelerate. */
  let held = 0;
  const keySteps = (e, range) => { const now = performance.now(); if (!e.repeat) { held = now; return 1; }
    const cap = Math.max(1, Math.round(range / 25)); return Math.round(Math.min(cap, 1 + Math.max(0, now - held - 300) / 1700 * (cap - 1))); };

  /* Public */
  const BL = window.BL = { toast, dialog, notSent, online, editValue, keySteps, gm: () => G.gm, live: () => G.live,
    setGm(v) { if (!online()) { notSent("Grand Master"); return; } G.gm = v; if (v) G.back = v; save(); status(); dispatchEvent(new Event("bl:gm")); },
    toggleBlack() { if (!online()) return notSent(black() ? "Restore" : "Blackout"); if (black()) { G.gm = G.back; toast(`Grand Master restored to ${G.gm}%`); } else { if (G.gm) G.back = G.gm; G.gm = 0; toast("Blackout on", true); } save(); status(); dispatchEvent(new Event("bl:gm")); },
    dirty() { G.dirty = true; save(); status(); dispatchEvent(new Event("bl:save")); }, saveState: () => G.fail ? ["bad", "Save failed"] : G.dirty ? ["wrn", "Unsaved changes"] : ["ok", "Saved"], setPending(n) { if (G.pending !== n) { G.pending = n; save(); setTimeout(status); } },
    setLive(v) { G.live = v; save(); }, pluginReloaded() { if (!G.setlistOk) { G.setlistOk = true; G.warn = Math.max(0, G.warn - 1); save(); status(); toast("setlist reloaded"); } }, setlistOk: () => G.setlistOk,
    remote: () => G.remote };
  status();

  /* Sidebar collapse */
  if (G.nav) shell.classList.add("navmin");
  document.addEventListener("click", e => { if (e.target.closest(".fold")) { G.nav = shell.classList.toggle("navmin"); save(); e.stopPropagation(); } }, true);

  /* Long names: one line, ellipsis, full text as a tooltip. */
  const wrap = () => $$(".row, .li").forEach(r => [...r.childNodes].forEach(n => { if (n.nodeType === 3 && n.textContent.trim()) { const s = document.createElement("span"); s.className = "cut"; s.textContent = n.textContent; s.title = n.textContent.trim(); n.replaceWith(s); } }));

  /* Filters: <input data-filter="key"> hides rows in its pane, survives redraws, and says when it hides selected rows. */
  const vals = {};
  const filter = () => $$("[data-filter]").forEach(i => { const k = i.dataset.filter, pane = i.closest(".pane"); if (i.value !== (vals[k] || "") && document.activeElement !== i) i.value = vals[k] || ""; const t = i.value.toLowerCase(); let hid = 0; $$(".fx, tbody tr", pane).forEach(x => { x.hidden = !!t && !x.textContent.toLowerCase().includes(t); if (x.hidden && (x.getAttribute("aria-pressed") === "true" || x.classList.contains("on"))) hid++; });
    let n = pane.querySelector(".notice"); if (hid) { if (!n) { n = document.createElement("div"); n.className = "notice"; i.parentElement.after(n); } const txt = `<i class="ph ph-warning"></i>${hid} selected hidden by this filter<button class="mini" data-showall="${k}">Show all</button>`; if (n.dataset.h !== String(hid)) { n.innerHTML = txt; n.dataset.h = hid; } } else if (n) n.remove(); });
  document.addEventListener("input", e => { const i = e.target.closest("[data-filter]"); if (i) { vals[i.dataset.filter] = i.value; filter(); } });
  document.addEventListener("click", e => { const b = e.target.closest("[data-showall]"); if (b) { vals[b.dataset.showall] = ""; const i = document.querySelector(`[data-filter="${b.dataset.showall}"]`); i.value = ""; filter(); } if (e.target.closest("#clear")) { for (const k in vals) vals[k] = ""; $$("[data-filter]").forEach(i => i.value = ""); filter(); } });

  /* Web UI: anything that only works next to the engine, or would strand other users, disappears. */
  const hideRemote = () => { if (G.remote) $$("button").forEach(b => { if (["Shut down", "Restart", "Quit"].includes(b.textContent.trim()) || (["Close", "Load"].includes(b.textContent.trim()) && b.closest(".pane")?.querySelector("header")?.textContent.startsWith("Show"))) b.hidden = true; }); };
  const sweep = () => { obs.disconnect(); wrap(); filter(); hideRemote(); obs.observe(document.body, { childList: true, subtree: true }); };
  const obs = new MutationObserver(sweep); addEventListener("DOMContentLoaded", sweep); obs.observe(document.body, { childList: true, subtree: true });

  /* Actions with dialogs */
  const unsaved = () => G.dirty || G.fail, warnUnsaved = `<p class="warnt"><i class="ph ph-warning"></i> SPARTACUS_SOLI has unsaved changes.</p>`;
  const saveNow = () => { if (url.get("state") === "savefail" || G.state === "savefail") { G.fail = true; save(); status(); dispatchEvent(new Event("bl:save")); setTimeout(() => dialog("Couldn't save the show", `<p>SPARTACUS_SOLI.json couldn't be written because the disk is full. Your changes are still in memory.</p>`, [["Cancel"], ["Save as...", "primary"]])); return; } G.dirty = G.fail = false; save(); status(); dispatchEvent(new Event("bl:save")); toast("Saved SPARTACUS_SOLI"); };
  const needEngine = (what, f) => () => online() ? f() : notSent(what);
  const groupNames = () => (window.BL.groups ? BL.groups() : ["MAC250E", "DIMMER"]);
  function addFixture() { const d = dialog("Add fixture", `<label class="dlgf">Type<select id="af-t"><option value="18">Martin MAC 250E (18 ch)</option><option value="8">LiteCraft AT10 (8 ch)</option><option value="7">ADJ Mega Hex Par (7 ch)</option><option value="4">Varytec VP1 (4 ch)</option><option value="1">Dimmer (1 ch)</option></select></label><label class="dlgf">Name<input id="af-n" value="New fixture"></label><label class="dlgf">Group<select id="af-g">${groupNames().map(g => `<option>${g}</option>`).join("")}<option value="">No group</option></select></label><label class="dlgf">Universe<select id="af-u"><option value="2">2 (online, empty)</option><option value="0">0 (online, 37 free)</option><option value="1">1 (offline)</option></select></label><label class="dlgf">First channel<input id="af-c" value="1" inputmode="numeric"></label><p class="note" id="af-m"></p>`, [["Cancel"], ["Add fixture", "primary", dd => { const add = dd.querySelector('footer [data-a="1"]'); if (add.disabled) return false; const v = q => dd.querySelector(q).value; BL.onAddFixture?.({ name: v("#af-n").trim(), group: v("#af-g"), u: +v("#af-u"), a: +v("#af-c"), n: +v("#af-t") }); BL.dirty(); toast(`Added ${v("#af-n").trim()}`); }]]);
    if (!d) return; const q = s => d.querySelector(s), add = q('footer [data-a="1"]'), chk = () => { const raw = q("#af-c").value, n = +q("#af-t").value, c = /^\d+$/.test(raw) ? +raw : NaN, top = 513 - n, okc = c >= 1 && c <= top, nm = q("#af-n").value.trim(), m = q("#af-m"), off = q("#af-u").value === "1"; q("#af-c").classList.toggle("bad", !okc); q("#af-n").classList.toggle("bad", !nm); m.textContent = !nm ? "Enter a name." : !okc ? `First channel must be 1 to ${top}, so all ${n} channels fit.` : off ? `Fits on channels ${c}-${c + n - 1}, but universe 1 is offline.` : `Channels ${c}-${c + n - 1}.`; m.style.color = okc && nm && !off ? "" : "var(--warn)"; add.disabled = !(okc && nm); }; d.addEventListener("input", chk); chk(); }
  const ACT = {
    "Shut down": () => dialog("Shut down this device?", `<p>Lighting output stops and this device powers off. You'll need physical access to turn it back on.</p>`, [["Cancel"], ["Shut down", "fill-red", () => toast("Shutting down")]]),
    "Restart": () => dialog("Restart Blaulicht?", `<p>Output pauses for a few seconds while the engine restarts.</p>${unsaved() ? warnUnsaved : ""}`, [["Cancel"], ["Restart", "primary"]]),
    "Quit": () => dialog("Quit Blaulicht?", `<p>Lighting output stops.</p>${unsaved() ? warnUnsaved : ""}`, [["Cancel"], ["Quit", "primary"]]),
    "Save": needEngine("Save", saveNow),
    "Save as": needEngine("Save", () => dialog("Save show as", `<label class="dlgf">File name<input value="SPARTACUS_SOLI copy.json"></label>`, [["Cancel"], ["Save", "primary", saveNow]])),
    "Load": () => dialog("Load a show", `<p>Loading replaces the open show. Lights change as soon as it loads.</p>${unsaved() ? warnUnsaved : ""}<label class="dlgf">Show file<select><option>SPARTACUS_SOLI.json</option><option>club_default.json</option></select></label>`, unsaved() ? [["Cancel"], ["Save and load"], ["Load without saving", "red-text"]] : [["Cancel"], ["Load", "primary"]]),
    "Close": () => dialog("Close SPARTACUS_SOLI?", `<p>Lights go dark until another show is loaded.</p>${unsaved() ? warnUnsaved : ""}`, unsaved() ? [["Cancel"], ["Save and close"], ["Close without saving", "red-text"]] : [["Cancel"], ["Close show", "red-text"]]),
    "Add fixture": needEngine("Add fixture", addFixture),
  };
  document.addEventListener("click", e => { const b = e.target.closest(".syswork button, .work button"); const fn = b && !b.onclick && !b.closest("dialog") && ACT[b.textContent.trim()]; if (fn) { e.stopPropagation(); fn(); } }, true);

  /* Removals ask first. */
  const CONFIRM = [["#vdel", "Delete this view?", "Its overlay list is lost.", "Delete view"], ["[data-rm]", "Remove this screen?", "Its window closes.", "Remove screen"], ["#pcr", "Remove the passcode?", "Anyone on the network can then control the lights.", "Remove passcode"], ["#lc", "Clear the logs?", "Lines already shown can't be brought back.", "Clear logs"], ['[aria-label="Allow remote connections"][aria-checked="true"]', "Turn off remote access?", "1 connected client is dropped. If you're using the web UI, this includes you, and it can only be turned back on from the desk.", "Turn off"]];
  document.addEventListener("click", e => { for (const [sel, title, body, go] of CONFIRM) { const b = e.target.closest(sel); if (!b) continue; if (b.dataset.ok) { delete b.dataset.ok; return; } e.stopPropagation(); e.preventDefault(); dialog(title, `<p>${body}</p>`, [["Cancel"], [go, "red-text", () => { b.dataset.ok = "1"; b.click(); }]]); return; } }, true);

  /* Keys: blackout from anywhere. Key repeat is ignored so holding can't strobe. */
  addEventListener("keydown", e => { const mod = e.metaKey || e.ctrlKey, k = e.key.toLowerCase(); if (mod && k === "b") { e.preventDefault(); if (!e.repeat) BL.toggleBlack(); return; } }, true);
  document.addEventListener("click", e => { const nb = e.target.closest("nav button"); if (nb && nb.textContent.trim() === "Setup" && !location.pathname.endsWith("setup.html")) location.href = "setup.html#fixtures"; if (e.target.closest("#stb")) BL.toggleBlack(); if (e.target.closest("[data-recon]")) toast(online() ? "Reconnecting /dev/ttyACM0" : "Not sent"); const u = e.target.closest("[data-uni]"); if (u) { try { sessionStorage.setItem("bl-uni", u.dataset.uni); } catch {} if (BL.onUni) { e.preventDefault(); BL.onUni(+u.dataset.uni); } } });

  /* Connection: the strip takes its own row, so nothing is covered. */
  if (!online()) { const t = G.state === "disconnected" ? ["Engine disconnected", "Reconnecting. Values are the last known and may not match the lights."] : ["Engine restarting", "Output is paused. This usually takes a few seconds."], bar = document.createElement("div"); bar.className = "engstate"; bar.setAttribute("role", "alert"); bar.innerHTML = `<b><i class="ph ph-warning-octagon"></i> ${t[0]}</b><span>${t[1]}</span>`; shell.prepend(bar); shell.classList.add("hasstrip"); const m = shell.querySelector("main"); m.inert = true; m.classList.add("paused"); }
})();

/* Dropdown: one popover for every choice list, the scene picker's style.
   BL.dropdown(anchor, {items, value, multi, filter, dim, width, onChange})
   items: [{value, label, icon?, meta?, group?, cls?}]. Groups draw as folder bands.
   multi: value is a Set; rows get checks, an "All" row, and Alt-click picks only that row.
   Every native <select> opens it too, so forms need no extra code. */
(() => {
  let cur = null;
  const scrim = document.createElement("div"); scrim.className = "scrim"; scrim.hidden = true;
  const esc = s => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;");
  function close(refocus = true) { if (!cur) return; const c = cur; cur = null; c.pop.remove(); scrim.hidden = true;
    c.anchor.removeAttribute?.("aria-expanded"); c.anchor.classList?.remove("ddopen");
    if (refocus && c.anchor.isConnected) c.anchor.focus({ preventScroll: true }); }
  function dropdown(anchor, o) {
    if (cur?.anchor === anchor) return close();
    close(false); if (!scrim.isConnected) document.body.append(scrim);
    const multi = !!o.multi, items = o.items, filt = o.filter ?? items.length >= 8;
    let sel = multi ? new Set(o.value) : o.value, q = "", act = -1;
    const pop = document.createElement("div"); pop.className = "ddpop"; pop.setAttribute("role", "listbox"); if (multi) pop.setAttribute("aria-multiselectable", "true");
    pop.innerHTML = (filt ? `<label class="ddq"><i class="ph ph-magnifying-glass"></i><input placeholder="${esc(o.placeholder || "Filter")}" aria-label="Filter"></label>` : "") + `<div class="ddlist"></div>`;
    const list = pop.querySelector(".ddlist"), input = pop.querySelector("input");
    const shown = () => items.filter(i => !q || (i.label + " " + (i.meta ?? "")).toLowerCase().includes(q));
    function draw() { const v = shown(); let g, h = "";
      if (multi && !q && items.length > 2) { const all = items.every(i => sel.has(i.value)); h += `<button class="ddrow ddall" data-all role="option" aria-selected="${all}"><i class="ddck ph${all ? "-bold ph-check" : ""}"></i>All</button>`; }
      v.forEach(i => { if (i.group !== undefined && i.group !== g) { g = i.group; h += `<div class="ddgroup"><i class="ph-fill ph-folder"></i>${esc(g)}</div>`; }
        const on = multi ? sel.has(i.value) : i.value === sel;
        h += `<button class="ddrow ${i.group !== undefined ? "leaf" : ""} ${!multi && on ? "on" : ""} ${i.cls || ""}" data-v="${esc(i.value)}" role="option" aria-selected="${on}">${multi ? `<i class="ddck ph${on ? "-bold ph-check" : ""}"></i>` : ""}${i.icon || ""}<span class="ddl">${esc(i.label)}</span>${i.meta != null ? `<span class="ddm">${esc(i.meta)}</span>` : ""}</button>`; });
      list.innerHTML = h || `<p class="ddempty">${esc(o.empty || "Nothing matches.")}</p>`; mark(); }
    const rows = () => [...list.querySelectorAll(".ddrow")];
    function mark() { rows().forEach((r, k) => r.classList.toggle("act", k === act)); rows()[act]?.scrollIntoView({ block: "nearest" }); }
    function pick(r, only) { if (!r) return;
      if (!multi) { const it = items.find(i => String(i.value) === r.dataset.v); close(); o.onChange?.(it.value, it); return; }
      if (r.hasAttribute("data-all")) { const all = items.every(i => sel.has(i.value)); sel = new Set(all ? [] : items.map(i => i.value)); }
      else { const it = items.find(i => String(i.value) === r.dataset.v);
        if (only) sel = new Set([it.value]); else sel.has(it.value) ? sel.delete(it.value) : sel.add(it.value); }
      const k = act; draw(); act = k; mark(); o.onChange?.(new Set(sel)); }
    list.onclick = e => { const r = e.target.closest(".ddrow"); if (r) { act = rows().indexOf(r); pick(r, e.altKey); } };
    list.onpointermove = e => { const r = e.target.closest(".ddrow"); if (r && rows().indexOf(r) !== act) { act = rows().indexOf(r); mark(); } };
    if (input) input.oninput = () => { q = input.value.trim().toLowerCase(); act = q ? 0 : -1; draw(); };
    pop.onkeydown = e => { const n = rows().length;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); act = n ? (act + (e.key === "ArrowDown" ? 1 : n - 1 + (act < 0 ? 1 : 0))) % n : -1; mark(); }
      else if (e.key === "Home" || e.key === "End") { if (e.target === input) return; e.preventDefault(); act = e.key === "Home" ? 0 : n - 1; mark(); }
      else if (e.key === "Enter" || (e.key === " " && e.target !== input)) { e.preventDefault(); pick(rows()[act < 0 ? 0 : act], e.altKey); }
      else if (e.key === "Tab") close(false); };
    (anchor.closest("dialog[open]") || document.body).append(pop);
    const r = anchor.getBoundingClientRect(), w = o.width || Math.max(r.width, 180);
    pop.style.minWidth = w + "px"; pop.style.left = Math.min(r.left, innerWidth - pop.offsetWidth - 8) + "px";
    const below = innerHeight - r.bottom - 12, above = r.top - 12;
    if (below >= Math.min(pop.offsetHeight, 240) || below >= above) { pop.style.top = r.bottom + 4 + "px"; pop.style.maxHeight = Math.min(480, below) + "px"; }
    else { pop.style.bottom = innerHeight - r.top + 4 + "px"; pop.style.maxHeight = Math.min(480, above) + "px"; }
    scrim.hidden = !o.dim; anchor.setAttribute("aria-expanded", "true"); anchor.classList.add("ddopen");
    cur = { anchor, pop }; draw();
    if (!multi) { act = rows().findIndex(x => x.classList.contains("on")); mark(); }
    (input || list.querySelector(".ddrow.act") || list.querySelector(".ddrow"))?.focus({ preventScroll: true });
    return { close };
  }
  dropdown.close = close;
  addEventListener("keydown", e => { if (cur && e.key === "Escape") { e.stopPropagation(); e.preventDefault(); close(); } }, true);
  addEventListener("pointerdown", e => { if (cur && !cur.pop.contains(e.target) && !cur.anchor.contains(e.target)) close(false); }, true);
  addEventListener("resize", () => close(false));
  addEventListener("scroll", e => { if (cur && !cur.pop.contains(e.target)) close(false); }, true);

  /* Native selects open the same popover; the select keeps the value and fires change. */
  const fromSelect = s => { const items = [];
    [...s.querySelectorAll("option")].forEach(op => { const g = op.parentElement.tagName === "OPTGROUP" ? op.parentElement.label : undefined;
      items.push({ value: op.value, label: op.textContent, group: g, cls: op.value === "" ? "ddnone" : "" }); });
    dropdown(s, { items, value: s.value, width: s.offsetWidth, onChange: v => { if (s.value !== v) { s.value = v; s.dispatchEvent(new Event("input", { bubbles: true })); s.dispatchEvent(new Event("change", { bubbles: true })); } } }); };
  addEventListener("mousedown", e => { const s = e.target.closest?.("select"); if (!s || s.multiple || s.disabled || e.button) return; e.preventDefault(); s.focus(); fromSelect(s); }, true);
  addEventListener("keydown", e => { const s = e.target; if (s.tagName !== "SELECT" || s.multiple || cur) return;
    if (e.key === "Enter" || e.key === " " || (e.altKey && e.key === "ArrowDown")) { e.preventDefault(); fromSelect(s); } });
  window.BL.dropdown = dropdown;
})();

/* Number fields scrub: drag sideways on any input.num to change it, like a knob. A plain click still
   places the caret. Step from data-step (default 1) per 4px; Shift is ten times finer. Esc restores.
   Fires input on every step and change on release, so the field's own validation and apply run. */
(() => { let s = null;
  const dec = st => (String(st).split(".")[1] || "").length;
  document.addEventListener("pointerdown", e => { const i = e.target.closest?.("input.num"); if (!i || e.button || i.disabled || i.readOnly || document.activeElement === i) return;
    s = { i, x: e.clientX, v0: parseFloat(i.value) || 0, step: parseFloat(i.dataset.step) || 1, on: false, id: e.pointerId }; e.preventDefault(); }, true);
  document.addEventListener("pointermove", e => { if (!s) return; const dx = e.clientX - s.x;
    if (!s.on) { if (Math.abs(dx) < 3) return; s.on = true; document.body.classList.add("scrubbing"); s.i.classList.add("scrub"); }
    if (!BL.online()) { BL.notSent("Change value"); end(false); return; }
    const st = s.step * (e.shiftKey ? .1 : 1), v = Math.round((s.v0 + Math.round(dx / 4) * st) / st) * st, min = s.i.dataset.min, max = s.i.dataset.max;
    let c = v; if (min != null) c = Math.max(+min, c); if (max != null) c = Math.min(+max, c);
    const t = c.toFixed(Math.max(dec(s.step), e.shiftKey ? dec(s.step) + 1 : 0)).replace(/\.?0+$/, m => m.includes(".") ? "" : m);
    if (s.i.value !== t) { s.i.value = String(+t); s.i.dispatchEvent(new Event("input", { bubbles: true })); } });
  function end(apply) { if (!s) return; const t = s; s = null; document.body.classList.remove("scrubbing"); t.i.classList.remove("scrub");
    if (!t.on) { t.i.focus(); return; }
    if (!apply) { t.i.value = String(t.v0); t.i.dispatchEvent(new Event("input", { bubbles: true })); }
    t.i.dispatchEvent(new Event("change", { bubbles: true })); }
  document.addEventListener("pointerup", () => end(true), true);
  addEventListener("keydown", e => { if (s?.on && e.key === "Escape") { e.stopPropagation(); end(false); } }, true);
})();
