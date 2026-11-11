/* Programmer panes: dividers, folding and reset. Sizes are saved per machine, per layout. */
{
  const work = document.querySelector(".work"), pane = n => work.querySelector("." + n);
  /* Pages without a Scenes list (the scene picker draft) keep their own saved layout. */
  const KEY = work.dataset.panels || "bl-panels", LISTS = ["scenes", "groups", "fxcol"].filter(pane);
  const stackMq = matchMedia("(max-width: 1399px)"), mode = () => stackMq.matches ? "stack" : "rack";
  const fresh = () => ({ w: { scenes: 1, groups: 1, fxcol: 1 }, palW: 440, sideW: 260, folded: {} });
  const load = () => { try { return JSON.parse(localStorage.getItem(KEY)) || {}; } catch { return {}; } };
  const saved = load();
  const state = { rack: { ...fresh(), ...saved.rack }, stack: { ...fresh(), ...saved.stack } };
  const save = () => { try { localStorage.setItem(KEY, JSON.stringify(state)); } catch {} };
  const now = () => state[mode()];
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
  const STRIP = "26px", GAP = 2;

  /* In the rack every pane folds sideways. In the stack the controls and Palettes share rows, so they fold to a title bar. */
  const FOLD = { scenes: "Scenes", groups: "Groups", fxcol: "Fixtures", faders: "Faders", colourpane: "Colour", gizmopane: "Pan / Tilt", pal: "Palettes", side: "Scene" };
  for (const [n, title] of Object.entries(FOLD)) {
    const p = pane(n); if (!p) { delete FOLD[n]; continue; } p.classList.add("foldable");
    const v = document.createElement("div"); v.className = "vtitle"; v.textContent = title; p.append(v);
    const b = document.createElement("button"); b.className = "pfold"; p.append(b);
    const toggle = () => { if (noFold().includes(n)) return; const s = now(); s.folded[n] = !s.folded[n]; layout(); save(); };
    b.onclick = toggle; v.onclick = toggle;
  }

  /* On the rack the controls hold the row Palettes and the Scene pane sit under, so they never fold there. */
  const noFold = () => mode() === "stack" ? [] : ["faders", "colourpane", "gizmopane"];
  const listTracks = (s, min) => LISTS.map(n => s.folded[n] ? STRIP : `minmax(${min}px, ${s.w[n]}fr)`);
  const anyListOpen = s => LISTS.some(n => !s.folded[n]);

  function layout() {
    const s = now(), f = { ...s.folded }, stack = mode() === "stack";
    noFold().forEach(n => delete f[n]);
    if (stack) {
      const ctlOpen = !f.colourpane || !f.gizmopane;
      const cols = listTracks(s, 100);
      /* Nothing else can take the spare width when every list and the Scene pane are folded. */
      cols.push("196px", (anyListOpen(s) || !f.side) ? "244px" : "minmax(244px, 1fr)", f.side ? STRIP : `${s.sideW}px`);
      work.style.gridTemplateColumns = cols.join(" ");
      work.style.gridTemplateRows = `var(--bar, 30px) ${f.faders ? STRIP : "320px"} ${ctlOpen ? "282px" : STRIP} minmax(0, 1fr)`;
    } else {
      const cols = listTracks(s, 120);
      const open = anyListOpen(s);
      cols.push(f.faders ? STRIP : "440px", f.colourpane ? STRIP : "196px", f.gizmopane ? STRIP : open ? "170px" : "minmax(170px, 1fr)");
      work.style.gridTemplateColumns = cols.join(" ");
      work.style.gridTemplateRows = "var(--bar, 30px) 320px minmax(0, 1fr)";
      const lower = work.querySelector(".lower");
      lower.style.gridTemplateColumns = f.pal ? `${STRIP} minmax(0, 1fr)` : f.side ? `minmax(0, 1fr) ${STRIP}` : `var(--pal-w) minmax(0, 1fr)`;
      const lw = lower.getBoundingClientRect().width;
      work.style.setProperty("--pal-w", clamp(s.palW, 160, Math.max(160, lw - 220 - GAP)) + "px");
    }
    if (stack) work.querySelector(".lower").style.gridTemplateColumns = "";
    for (const n of Object.keys(FOLD)) {
      const p = pane(n), on = !!f[n], bar = stack && ["faders", "colourpane", "gizmopane", "pal"].includes(n);
      p.classList.toggle("collapsed", on); p.classList.toggle("bar", on && bar); p.classList.toggle("strip", on && !bar);
      const b = p.querySelector(":scope > .pfold"); b.style.display = noFold().includes(n) ? "none" : ""; const label = `${on ? "Open" : "Fold"} ${FOLD[n]}`;
      b.setAttribute("aria-label", label); b.title = label; b.setAttribute("aria-expanded", !on);
      b.innerHTML = `<i class="ph ph-caret-${on ? (bar ? "down" : "right") : (bar ? "up" : "left")}"></i>`;
    }
    placeGrips();
  }

  /* Each grip sits over the 2px gap, so the panes keep their spacing. */
  const grips = [
    { a: "scenes", b: "groups", kind: "list" }, { a: "groups", b: "fxcol", kind: "list" },
    { a: "pal", b: "side", kind: "pal" }, { a: "fxcol", b: "side", kind: "side" },
  ].filter(g => pane(g.a) && pane(g.b));
  for (const g of grips) {
    const el = g.el = document.createElement("div"); el.className = "panelgrip"; el.title = "Drag to resize, double-click to restore"; work.append(el);
    el.addEventListener("dblclick", () => { const s = now(), d = fresh(); if (g.kind === "list") { s.w[g.a] = s.w[g.b] = (s.w[g.a] + s.w[g.b]) / 2; } else if (g.kind === "pal") s.palW = d.palW; else s.sideW = d.sideW; layout(); save(); });
    el.addEventListener("pointerdown", e => startDrag(e, g));
  }

  function placeGrips() {
    const s = now(), f = s.folded, stack = mode() === "stack", box = work.getBoundingClientRect();
    for (const g of grips) {
      const relevant = g.kind === "list" || (g.kind === "pal" ? !stack : stack);
      const show = relevant && !f[g.b] && (g.kind === "side" || !f[g.a]);
      g.el.hidden = !show; g.el.style.display = show ? "" : "none"; if (!show) continue;
      const bp = pane(g.b).getBoundingClientRect();
      g.el.style.left = (bp.left - box.left - 5) + "px"; g.el.style.top = (bp.top - box.top) + "px"; g.el.style.height = bp.height + "px";
    }
  }

  let active = null;
  function startDrag(e, g) {
    e.preventDefault();
    const s = now(), snap = JSON.parse(JSON.stringify(s));
    const min = mode() === "stack" ? 100 : 120, start = e.clientX;
    const wa = pane(g.a).getBoundingClientRect().width, wb = pane(g.b).getBoundingClientRect().width;
    const widths = Object.fromEntries(LISTS.map(n => [n, pane(n).getBoundingClientRect().width]));
    const lw = g.kind === "pal" ? work.querySelector(".lower").getBoundingClientRect().width : 0;
    const listSpace = LISTS.reduce((t, n) => t + (s.folded[n] ? 26 : min), 0) + 440 + 5 * GAP + 2;
    const sideMax = Math.max(220, Math.min(520, work.clientWidth - listSpace));
    active = { g, snap };
    g.el.classList.add("on"); g.el.setPointerCapture(e.pointerId);
    const move = ev => {
      const d = ev.clientX - start;
      if (g.kind === "list") {
        const a = clamp(wa + d, min, wa + wb - min);
        LISTS.forEach(n => { s.w[n] = s.folded[n] ? s.w[n] : widths[n]; });
        s.w[g.a] = a; s.w[g.b] = wa + wb - a;
      } else if (g.kind === "pal") s.palW = clamp(wa + d, 160, lw - 220 - GAP);
      else s.sideW = clamp(wb - d, 220, sideMax);
      layout();
    };
    const end = () => {
      g.el.classList.remove("on"); g.el.removeEventListener("pointermove", move); g.el.removeEventListener("pointerup", end); g.el.removeEventListener("pointercancel", end);
      if (active) { active = null; save(); }
    };
    active.end = end;
    g.el.addEventListener("pointermove", move); g.el.addEventListener("pointerup", end); g.el.addEventListener("pointercancel", end);
  }
  /* Esc cancels a drag, like every other drag on the page. */
  addEventListener("keydown", e => { if (e.key !== "Escape" || !active) return; const { snap, end } = active; active = null; Object.assign(now(), snap); end(); layout(); }, true);

  document.getElementById("lreset").onclick = () => { state.rack = fresh(); state.stack = fresh(); layout(); save(); BL.toast("Layout reset"); };
  stackMq.addEventListener("change", layout);
  new ResizeObserver(layout).observe(work);
  layout();
}
