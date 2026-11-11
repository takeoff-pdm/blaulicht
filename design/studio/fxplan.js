/* Fixture position: a 2D view of the rig next to the position fields, and a larger one in a dialog.
   Top shows across (X) and depth (Z), Front shows across (X) and height (Y). Every fixture is a dot; the
   selected group is brighter, the selected fixture is blue. Click a dot to select it, drag the blue one to
   move it. Wheel or +/- zooms around the pointer, dragging empty space pans, Fit shows every fixture.
   Zoom and pan are shared by both sizes and kept per view (Top, Front). */
st.planView = st.planView || "top";
st.planZoom = st.planZoom || { top: { z: 1, px: 0, py: 0 }, front: { z: 1, px: 0, py: 0 } };
const SIZES = { small: { W: 268, H: 190 }, large: { W: 860, H: 540 } };
function planAxes() { return st.planView === "top" ? ["x", "z", "Across (X)", "Depth (Z)"] : ["x", "y", "Across (X)", "Height (Y)"]; }
/* World to screen: fit every fixture, then apply zoom about the centre and pan (in world units). */
function planFit(W, H) { const [a, b] = planAxes(), all = S.groups.flatMap(g => g.fixtures), xs = all.map(f => f.pos[a]), ys = all.map(f => f.pos[b]);
  const x0 = Math.min(...xs, 0) - 1, x1 = Math.max(...xs, 0) + 1, y0 = Math.min(...ys, 0) - 1, y1 = Math.max(...ys, 0) + 1, Z = st.planZoom[st.planView];
  const k = Math.min((W - 28) / (x1 - x0), (H - 28) / (y1 - y0)) * Z.z, mx = (x0 + x1) / 2 + Z.px, my = (y0 + y1) / 2 + Z.py;
  return { k, sx: v => W / 2 + (v - mx) * k, sy: v => H / 2 - (v - my) * k, ix: px => mx + (px - W / 2) / k, iy: py => my - (py - H / 2) / k }; }
/* A grid line every 1 m, or 5 m when zoomed out, so distances can be read. */
function grid(F, W, H) { const step = F.k < 12 ? 5 : 1; let h = "";
  for (let v = Math.ceil(F.ix(0) / step) * step; v <= F.ix(W); v += step) h += `<line class="gr ${v === 0 ? "ax" : ""}" x1="${F.sx(v).toFixed(1)}" y1="0" x2="${F.sx(v).toFixed(1)}" y2="${H}"/>`;
  for (let v = Math.ceil(F.iy(H) / step) * step; v <= F.iy(0); v += step) h += `<line class="gr ${v === 0 ? "ax" : ""}" x1="0" y1="${F.sy(v).toFixed(1)}" x2="${W}" y2="${F.sy(v).toFixed(1)}"/>`;
  return h + `<text class="axl" x="6" y="${H - 6}">Grid ${step} m</text>`; }
function plan(f, size = "small") { const { W, H } = SIZES[size], [a, b, la, lb] = planAxes(), F = planFit(W, H), g = S.groups.find(x => x.fixtures.includes(f)), big = size === "large";
  let dots = "";
  S.groups.forEach(gr => gr.fixtures.forEach(x => { if (x === f) return; const same = gr === g;
    dots += `<circle class="${same ? "same" : ""}" cx="${F.sx(x.pos[a]).toFixed(1)}" cy="${F.sy(x.pos[b]).toFixed(1)}" r="${(same ? 3.5 : 2.5) * (big ? 1.4 : 1)}" data-plan="${gr.id}:${x.id}"><title>${x.name} (${gr.name})</title></circle>`;
    if (big && same) dots += `<text class="nm" x="${F.sx(x.pos[a]) + 7}" y="${F.sy(x.pos[b]) + 4}">${x.name}</text>`; }));
  const cx = F.sx(f.pos[a]), cy = F.sy(f.pos[b]), right = cx > W - 110;
  const seg = `<span class="fxp-seg"><button class="mini ${st.planView === "top" ? "on" : ""}" data-planview="top" aria-pressed="${st.planView === "top"}">Top</button><button class="mini ${st.planView === "front" ? "on" : ""}" data-planview="front" aria-pressed="${st.planView === "front"}">Front</button></span>`;
  const zoom = `<span class="fxp-seg"><button class="mini ic" data-planzoom="out" title="Zoom out" aria-label="Zoom out"><i class="ph ph-minus"></i></button><button class="mini" data-planzoom="fit" title="Show every fixture">Fit</button><button class="mini ic" data-planzoom="in" title="Zoom in" aria-label="Zoom in"><i class="ph ph-plus"></i></button></span>`;
  return `<div class="fxp-bar">${seg}${zoom}${big ? `<small>Drag the blue dot to move ${f.name}. Wheel zooms, drag empty space to pan.</small>` : `<button class="mini ic fxp-open" data-planopen title="Open a larger view" aria-label="Open a larger view"><i class="ph ph-arrows-out-simple"></i></button>`}</div>
    <svg class="fxp ${size}" data-size="${size}" viewBox="0 0 ${W} ${H}" role="img" aria-label="${st.planView === "top" ? "Top" : "Front"} view of all fixtures, ${f.name} highlighted">${grid(F, W, H)}${dots}
    <circle class="sel" cx="${cx.toFixed(1)}" cy="${cy.toFixed(1)}" r="${big ? 8 : 6}" data-plansel><title>${f.name}</title></circle>
    <text class="lbl" x="${right ? cx - 11 : cx + 11}" y="${cy + 4}" text-anchor="${right ? "end" : "start"}">${f.name}</text>
    <text class="axl" x="${W - 6}" y="${H - 6}" text-anchor="end">${la} →</text><text class="axl" x="6" y="14">↑ ${lb}</text>${st.planView === "top" ? `<text class="axl" x="${W / 2}" y="${H - 6}" text-anchor="middle">Audience</text>` : ""}</svg>`; }
(() => { let drag = null;
  const cur = () => { const g = S.groups.find(x => x.id === st.group) || S.groups[0]; return g.fixtures.find(x => x.id === st.fx); };
  const redraw = () => { const f = cur(); if (!f) return;
    document.querySelectorAll(".fxplan").forEach(el => el.innerHTML = plan(f, el.dataset.size || "small"));
    document.querySelectorAll(".fxplan-big").forEach(el => el.innerHTML = plan(f, "large"));
    ["x", "y", "z"].forEach(a => document.querySelectorAll(`[data-v="pos.${a}"]`).forEach(i => { if (document.activeElement !== i) i.value = f.pos[a]; })); };
  window.planRedraw = redraw;
  const zoomAt = (fac, svg, px, py) => { const Z = st.planZoom[st.planView], s = SIZES[svg.dataset.size], F = planFit(s.W, s.H), wx = F.ix(px), wy = F.iy(py);
    Z.z = Math.max(.5, Math.min(12, Z.z * fac)); const G = planFit(s.W, s.H); Z.px += wx - G.ix(px); Z.py += wy - G.iy(py); redraw(); };
  const local = (svg, e) => { const r = svg.getBoundingClientRect(), s = SIZES[svg.dataset.size]; return [(e.clientX - r.left) * s.W / r.width, (e.clientY - r.top) * s.H / r.height, r]; };
  function openBig() { const f = cur(); if (!f) return;
    const d = BL.dialog(`Position: ${f.name}`, `<div class="fxplan-big"></div><div class="frm fxp-fields"><label>Position</label><div class="xyz">${["x", "y", "z"].map((a, i) => `<label class="xyzc"><input class="num" data-v="pos.${a}" data-step="0.05" value="${f.pos[a]}" aria-label="Position ${["Across (X)", "Height (Y)", "Depth (Z)"][i]} in metres"><small>${["Across (X)", "Height (Y)", "Depth (Z)"][i]}, m</small></label>`).join("")}</div></div>`, [["Done", "primary"]]);
    if (!d) return; d.querySelector(".dlg").classList.add("fxp-dlg"); d.addEventListener("close", () => draw()); redraw(); }
  document.addEventListener("click", e => { const v = e.target.closest("[data-planview]"); if (v) { st.planView = v.dataset.planview; redraw(); return; }
    const z = e.target.closest("[data-planzoom]"); if (z) { const Z = st.planZoom[st.planView]; if (z.dataset.planzoom === "fit") Object.assign(Z, { z: 1, px: 0, py: 0 }); else { const svg = z.closest(".fxplan, .fxplan-big").querySelector("svg"), s = SIZES[svg.dataset.size]; zoomAt(z.dataset.planzoom === "in" ? 1.5 : 1 / 1.5, svg, s.W / 2, s.H / 2); return; } redraw(); return; }
    if (e.target.closest("[data-planopen]")) { openBig(); return; }
    const d = e.target.closest("[data-plan]"); if (d && !drag?.moved) { const [g, id] = d.dataset.plan.split(":").map(Number); st.group = g; st.fx = id; st.fxs = new Set([id]); if (document.querySelector("dialog[open] .fxplan-big")) { redraw(); document.querySelector("dialog[open] header").textContent = `Position: ${cur().name}`; } else draw(); } });
  document.addEventListener("wheel", e => { const svg = e.target.closest?.("svg.fxp"); if (!svg) return; e.preventDefault(); const [x, y] = local(svg, e); zoomAt(Math.exp(-e.deltaY * .002), svg, x, y); }, { passive: false });
  document.addEventListener("pointerdown", e => { const svg = e.target.closest?.("svg.fxp"); if (!svg || e.button) return; if (e.target.closest("[data-plan]")) return; e.preventDefault();
    const s = SIZES[svg.dataset.size], [x, y, r] = local(svg, e);
    drag = { mode: e.target.closest("[data-plansel]") ? "move" : "pan", r, s, F: planFit(s.W, s.H), f: cur(), x, y, Z: { ...st.planZoom[st.planView] } }; });
  document.addEventListener("pointermove", e => { if (!drag) return; const px = (e.clientX - drag.r.left) * drag.s.W / drag.r.width, py = (e.clientY - drag.r.top) * drag.s.H / drag.r.height;
    if (drag.mode === "pan") { const Z = st.planZoom[st.planView]; Z.px = drag.Z.px - (px - drag.x) / drag.F.k; Z.py = drag.Z.py + (py - drag.y) / drag.F.k; drag.moved = true; redraw(); return; }
    const [a, b] = planAxes(), snap = v => Math.round(v * 20) / 20;
    drag.f.pos[a] = snap(drag.F.ix(px)); drag.f.pos[b] = snap(drag.F.iy(py)); if (b === "y") drag.f.pos.y = Math.max(0, drag.f.pos.y); drag.moved = true; redraw(); });
  document.addEventListener("pointerup", () => { if (drag?.moved && drag.mode === "move") BL.dirty?.(); setTimeout(() => drag = null, 0); });
  /* Live while typing or scrubbing: valid numbers apply at once; change still reports bad input. */
  document.addEventListener("input", e => { const t = e.target, k = t.dataset?.v; if (!k) return; const f = cur(); if (!f) return; const [o, a] = k.split("."), v = parseFloat(t.value.replace(",", "."));
    if (!isFinite(v) || f[o][a] === v) return; f[o][a] = v; BL.dirty?.(); redraw(); });
  document.addEventListener("change", e => { const t = e.target, k = t.dataset?.v; if (!k) return; const f = cur(); if (!f) return; const [o, a] = k.split("."), v = parseFloat(t.value.replace(",", "."));
    if (!isFinite(v)) { BL.toast(`${t.value || "Empty"} isn't a number`, true); t.value = f[o][a]; return; } f[o][a] = v; BL.dirty?.(); redraw(); });

  /* Identify: a small panel drops down from the button and steps through fixtures one at a time; the
     one flashing gets a red row. Several selected: steps through them. One selected: starts there and steps
     through its group. "Select this fixture" stops and makes the identified fixture the only selection. */
  st.identFx = null; let idp = null;
  const mark = () => { document.querySelectorAll("tr[data-fx]").forEach(r => r.classList.toggle("ident", +r.dataset.fx === st.identFx)); document.getElementById("fxident")?.setAttribute("aria-pressed", st.identFx != null); };
  const stop = () => { if (!idp) return; idp.el.remove(); idp = null; st.identFx = null; mark(); };
  function step(k) { const { list } = idp; idp.i = (idp.i + k + list.length) % list.length; const f = list[idp.i]; st.identFx = f.id;
    idp.el.querySelector(".idn b").textContent = f.name; idp.el.querySelector(".idn .mono").textContent = `${idp.i + 1} of ${list.length}`; mark();
    document.querySelector(`tr[data-fx="${f.id}"]`)?.scrollIntoView({ block: "nearest" }); }
  document.addEventListener("click", e => { const btn = e.target.closest("#fxident");
    if (!btn) { if (idp && !idp.el.contains(e.target) && !e.target.closest("tr[data-fx]")) stop(); return; }
    if (idp) return stop(); if (!BL.online()) return BL.notSent("Identify");
    const g = S.groups.find(x => x.id === st.group); if (!g) return; const sel = g.fixtures.filter(x => st.fxs.has(x.id)), list = sel.length > 1 ? sel : g.fixtures;
    const el = document.createElement("div"); el.className = "ddpop idpop"; el.setAttribute("role", "dialog"); el.setAttribute("aria-label", "Identify");
    el.innerHTML = `<div class="idn"><span class="dot"></span><b></b><span class="mono"></span></div><div class="idbtns"><button class="mini" data-id="-1"><i class="ph ph-caret-left"></i>Previous</button><button class="mini" data-id="1">Next<i class="ph ph-caret-right"></i></button></div><div class="idfoot"><button class="mini" data-id="take" title="Stop identifying and select only this fixture">Transfer to selection</button><button class="mini" data-id="stop">Stop</button></div><small>${sel.length > 1 ? "Through the selected fixtures" : `Through ${g.name}`}. ← and → step, Esc stops.</small>`;
    document.body.append(el); const r = btn.getBoundingClientRect(); el.style.left = Math.min(r.left, innerWidth - 280) + "px"; el.style.top = r.bottom + 4 + "px";
    idp = { el, list, i: Math.max(0, list.findIndex(x => x.id === (sel.length > 1 ? sel[0].id : st.fx))) }; step(0);
    el.addEventListener("click", ev => { const b = ev.target.closest("[data-id]"); if (!b) return; const k = b.dataset.id;
      if (k === "stop") return stop(); if (k === "take") { const id = st.identFx; stop(); st.fxs = new Set([id]); st.fx = id; draw(); return; } step(+k); });
    el.querySelector('[data-id="1"]').focus(); });
  addEventListener("keydown", e => { if (!idp || e.target.matches?.("input, textarea")) return;
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") { e.preventDefault(); step(e.key === "ArrowRight" ? 1 : -1); } else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); stop(); } }, true);
  /* Initialize: sends the start-up values to the selected fixtures. Lit while on, text and width fixed. */
  document.addEventListener("click", e => { const b = e.target.closest("#fxinit"); if (!b) return; if (!BL.online()) return BL.notSent("Initialize");
    const k2 = S.groups.find(x => x.id === st.group)?.fixtures.filter(x => st.fxs.has(x.id)).map(x => x.id).join(",");
    st.finitKey = st.finitKey === k2 ? null : k2; b.setAttribute("aria-pressed", st.finitKey === k2); });
})();
