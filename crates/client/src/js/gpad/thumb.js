// wado bridge — a picture of a layout: the real renderer at thumbnail scale, for the picker, the
// settings carousel, the hub card and the console deck's face switcher. Never pressed.
//
//   W.gpadThumb(box, L, o?)    draws L into box (its size decides the scale); o defaults to "l"
//   W.gpadVariantCard(item, L, checked) → a radio card element (thumb, name, sub, check)

(() => {
  W.gpadThumb = (box, L, o) => {
    o = o || "l";
    const w = box.clientWidth || 130, h = box.clientHeight || 64;
    box.classList.add("gpthumbs");
    const ref = W.gpadVariants.REF[o];
    // Portrait layouts live in the lower part of a tall screen: crop to it.
    const k = o === "p" ? Math.min(w / ref[0], h / (ref[1] * 0.66)) : W.gpadPlace.thumbK(o, w, h);
    const place = W.gpadPlace.all(L, o, k, o === "p" ? ref[0] * k : w, ref[1] * k);
    if (o === "p") for (const r of place.values()) r.y -= ref[1] * k * 0.34;
    else for (const r of place.values()) r.y += (h - ref[1] * k) / 2;
    W.gpadRender(box, L, { o, k, place });
  };

  W.gpadVariantCard = (item, L, checked) => {
    const b = document.createElement("button");
    b.className = "gpvcard";
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(!!checked));
    b.dataset.variant = item.id;
    b.innerHTML = `<span class="gpvthumb"></span><span class="gpvck">${W.padIcon("check")}</span><b></b><small></small>` +
      (item.badge ? `<span class="whenbadge apply gpvbadge">${item.badge}</span>` : "");
    b.querySelector("b").textContent = item.name;
    b.querySelector("small").textContent = item.sub || "";
    b._L = L;
    return b;
  };
})();
