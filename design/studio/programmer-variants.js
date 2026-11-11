/* Options for programmer-variants.html. B3 moves page bar actions into the panes they affect. */
if (document.documentElement.classList.contains("vB3")) {
  const tools = document.querySelector(".tools"), b = tools.children, [sa, an, cl, li, pa, la] = [...b];
  const row = document.createElement("div"); row.className = "fxtools"; cl.textContent = "Clear"; row.append(cl, li, la);
  document.querySelector(".fxfilter").after(row);
  const h = document.querySelector("#anims").previousElementSibling; sa.textContent = "Add"; sa.style.marginLeft = "auto"; h.append(sa);
  an.hidden = true; pa.hidden = true;
}
