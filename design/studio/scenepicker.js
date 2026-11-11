/* Scene picker: the scene and its masters live in the page bar. Runs after show.js. */
{
  /* The page dims behind the open picker. */
  const scrim = document.createElement("div"); scrim.className = "scrim"; scrim.hidden = true; document.body.append(scrim);
  const btn = $("scenebtn"), pop = $("scenepop"), input = $("scenefilter"), list = $("scenes");
  const applyFilter = () => { const q = input.value.trim().toLowerCase(); let any = false, folder = null, shown = false;
    const close = () => { if (folder) folder.hidden = q && !shown; };
    [...list.children].forEach(r => { if (r.matches("[data-folder]")) { close(); folder = r; shown = false; return; }
      if (!r.matches("[data-scene]")) return; const hit = !q || r.textContent.toLowerCase().includes(q); r.hidden = !hit; if (hit) { shown = true; any = true; } });
    close(); list.querySelector(".empty")?.remove(); if (!any) list.insertAdjacentHTML("beforeend", '<p class="empty">No scenes match.</p>'); };
  const open = on => { scrim.hidden = !on; btn.parentElement.classList.toggle("open", on); pop.hidden = !on; btn.setAttribute("aria-expanded", on); if (on) { input.value = ""; applyFilter(); input.focus(); list.querySelector(".on")?.scrollIntoView({ block: "nearest" }); } else if (pop.contains(document.activeElement)) btn.focus(); };
  btn.onclick = () => open(pop.hidden);
  input.oninput = applyFilter;
  input.onkeydown = e => { if (e.key === "Enter") list.querySelector("[data-scene]:not([hidden])")?.click(); };
  new MutationObserver(() => { if (!pop.hidden) applyFilter(); }).observe(list, { childList: true });
  list.addEventListener("click", e => { if (e.target.closest("[data-scene]")) open(false); });
  addEventListener("keydown", e => { if (e.key === "Escape" && !pop.hidden) { e.stopPropagation(); open(false); } }, true);
  addEventListener("pointerdown", e => { if (!pop.hidden && !e.target.closest(".pickwrap")) open(false); });

  /* show.js draws the masters; here they go to the page bar and the picker shows the scene. */
  drawMasters = function () { $("masters").innerHTML = '<div id="k1"></div><div id="k2"></div>';
    knob($("k1"), "Intensity", scene.master / 100, t => Math.round(t * 100), false, pctParse);
    knob($("k2"), "Speed", SPEEDS.indexOf(scene.speed) / 9, t => speedLabel(SPEEDS[Math.round(t * 9)]), false, speedParse, 1 / 9);
    $("scenename").textContent = scene.name; $("sceneid").textContent = "#" + scene.id; btn.title = `Scene being edited: ${scene.name}`; };
  drawMasters();
  if (new URLSearchParams(location.search).has("open")) { open(true); input.blur(); }
}
