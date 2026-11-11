/* Folder trees: drag an item onto a folder (or onto any item in it) to move it there.
   Setup lists only. Live lists (Programmer) don't move things by drag, so a slip mid-show can't reorganise the show.
   Pointer events with a 5px threshold, so a click still selects. Esc cancels. Dropping below the folders
   moves the item out of any folder where that kind allows it (Groups). */
(() => {
  const LISTS = { group: ["groups", "Groups", true], gsel: ["groups", "Groups", true], scene: ["scenes", "Scenes", false], pal: ["palettes", "Palettes", false] };
  let drag = null, swallow = false;
  const item = el => { const [k, id] = el.dataset.pick.split(":"); const L = LISTS[k]; return L && { k, L, it: S[L[0]].find(x => String(x.id) === id) }; };
  function target(x, y) { const el = document.elementFromPoint(x, y), body = el?.closest(".body");
    if (drag.fx) { const g = el?.closest('.row.leaf[data-pick^="group:"]'); const id = g && +g.dataset.pick.split(":")[1]; return g && id !== st.group ? { el: g, group: id } : null; }
    if (!body || body !== drag.body) return null;
    const f = el.closest(".row.folder"); if (f) return { el: f, folder: +f.dataset.fold };
    const r = el.closest(".row.leaf[data-pick]"); if (r) { const t = item(r); if (t?.k === drag.k) return { el: t.it.folder == null ? body : prevFolder(r), folder: t.it.folder ?? null }; }
    return drag.L[2] ? { el: body, folder: null } : null; }
  const prevFolder = r => { let p = r.previousElementSibling; while (p && !p.matches(".row.folder")) p = p.previousElementSibling; return p; };
  function mark(t) { document.querySelectorAll(".dropin").forEach(e => e.classList.remove("dropin")); if (t?.el) t.el.classList.add("dropin"); }
  function end(apply) { if (!drag) return; const d = drag; drag = null; d.ghost?.remove(); mark(null); document.body.classList.remove("treedragging"); d.row.classList.remove("dragsrc");
    if (!d.on) return; swallow = true; setTimeout(() => swallow = false, 0);
    if (d.fx) { if (apply && d.t) moveFixtures(d.fx, d.t.group); return; }
    if (!apply || !d.t || d.t.folder === (d.it.folder ?? null)) return;
    const name = id => id == null ? "no folder" : S.folders.find(f => f.id === id).name;
    d.it.folder = d.t.folder; if (d.t.folder != null) st.folded.delete(d.t.folder); draw(); BL.dirty?.();
    BL.toast(`Moved ${d.it.name} to ${name(d.t.folder)}`); }
  /* Fixture rows on Setup → Fixtures drag onto a group in the tree; a selected row carries the whole selection. */
  document.addEventListener("pointerdown", e => { if (e.button || e.shiftKey || e.metaKey || e.ctrlKey) return; const tr = e.target.closest("tr[data-fx]"); if (!tr) return; const id = +tr.dataset.fx;
    const g = S.groups.find(x => x.id === st.group), ids = st.fxs.has(id) ? g.fixtures.filter(x => st.fxs.has(x.id)).map(x => x.id) : [id];
    drag = { fx: ids, it: { name: ids.length === 1 ? g.fixtures.find(x => x.id === id).name : `${ids.length} fixtures` }, x: e.clientX, y: e.clientY, row: tr, on: false }; });
  document.addEventListener("pointerdown", e => { if (e.button || drag) return; const r = e.target.closest(".row.leaf[data-pick]"); if (!r) return; const t = item(r); if (!t?.it) return;
    drag = { ...t, x: e.clientX, y: e.clientY, body: r.closest(".body"), row: r, on: false }; });
  document.addEventListener("pointermove", e => { if (!drag) return;
    if (!drag.on) { if (Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < 5) return; drag.on = true; document.body.classList.add("treedragging"); drag.row.classList.add("dragsrc");
      drag.ghost = document.createElement("div"); drag.ghost.className = "treeghost"; drag.ghost.textContent = drag.it.name; document.body.append(drag.ghost); }
    drag.ghost.style.transform = `translate(${e.clientX + 12}px, ${e.clientY + 8}px)`; drag.t = target(e.clientX, e.clientY); mark(drag.t);
    drag.ghost.classList.toggle("no", !drag.t); });
  document.addEventListener("pointerup", () => end(true));
  document.addEventListener("pointercancel", () => end(false));
  addEventListener("keydown", e => { if (drag?.on && e.key === "Escape") { e.stopPropagation(); end(false); } }, true);
  document.addEventListener("click", e => { if (swallow) { e.stopPropagation(); e.preventDefault(); } }, true);
})();
