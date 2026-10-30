// wado bridge — the viewer's pictures: png, jpg, webp, gif, avif, bmp, ico; SVG only ever as an
// <img> (scripts never run there); HEIC where the browser decodes it, else the computer's own
// thumbnail with an honest note.
//
// Pinch, wheel, double-tap and drag zoom and pan (Panzoom, SRI-pinned in index.html); + − 0 on a
// keyboard. The footer holds − / the zoom / +; a tap on the zoom fits it back. While zoomed a
// swipe pans instead of changing picture. A single tap shows the picture alone.
//
// What is zoomed is a layer the size of the stage with the picture fitted inside it
// (`object-fit: contain`), not the picture itself: Panzoom's "keep it covering" rule then holds
// for any picture, where one smaller than the screen fought it (2026-10-05). One double-tap
// path: touch double-taps are detected here and the browser's dblclick after them is ignored.

(() => {
  const F = W.files;
  const V = F.viewer;
  const h = F.h;

  function zoomable(ctx, img, onScale) {
    const layer = h("div", { class: "fvzoom" }, img);
    ctx.stage.replaceChildren(layer);
    if (!window.Panzoom) {
      img.addEventListener("click", ctx.bare);
      return null;
    }
    const pz = window.Panzoom(layer, { maxScale: 12, minScale: 1, step: 0.35, contain: "outside", panOnlyWhenZoomed: true, cursor: "default", touchAction: "none" });
    const wheel = (ev) => pz.zoomWithWheel(ev);
    ctx.stage.addEventListener("wheel", wheel, { passive: false });
    layer.addEventListener("panzoomchange", (ev) => { ctx.zoomed(ev.detail.scale > 1.02); onScale(ev.detail.scale); });
    const flip = (x, y) => {
      if (pz.getScale() > 1.02) pz.reset({ animate: true });
      else pz.zoomToPoint(3, { clientX: x, clientY: y }, { animate: true });
    };
    let lastTouch = 0, tapAt = 0, tapTimer = null, x0 = 0, y0 = 0, moved = false;
    layer.addEventListener("pointerdown", (ev) => { x0 = ev.clientX; y0 = ev.clientY; moved = false; });
    layer.addEventListener("pointermove", (ev) => { if (Math.abs(ev.clientX - x0) + Math.abs(ev.clientY - y0) > 10) moved = true; });
    layer.addEventListener("pointerup", (ev) => {
      if (ev.pointerType === "mouse" || moved || !ev.isPrimary) return;
      lastTouch = Date.now();
      if (lastTouch - tapAt < 300) { clearTimeout(tapTimer); tapAt = 0; flip(ev.clientX, ev.clientY); return; }
      tapAt = lastTouch;
      tapTimer = setTimeout(ctx.bare, 300);
    });
    layer.addEventListener("dblclick", (ev) => { if (Date.now() - lastTouch > 700) flip(ev.clientX, ev.clientY); });
    const keys = (ev) => {
      if (F.inField(ev) || document.querySelector(".fvdetails.up:not(.side)")) return;
      if (ev.key === "+" || ev.key === "=") { ev.preventDefault(); pz.zoomIn(); }
      else if (ev.key === "-") { ev.preventDefault(); pz.zoomOut(); }
      else if (ev.key === "0") { ev.preventDefault(); pz.reset(); }
    };
    addEventListener("keydown", keys);
    return { pz, destroy() { clearTimeout(tapTimer); removeEventListener("keydown", keys); ctx.stage.removeEventListener("wheel", wheel); try { pz.destroy(); } catch (_) {} ctx.zoomed(false); } };
  }

  // A picture this browser cannot decode (HEIC on most): the computer's thumbnail, said plainly.
  function undecodable(ctx) {
    const { e, stage } = ctx;
    const x = V.ext(e.name);
    stage.replaceChildren(h("div", { class: "fspin", role: "status", "aria-label": "Loading" }));
    F.req("thumb", { path: e.path }).then((r) => {
      if (!ctx.alive()) return;
      const card = V.cantPreview(e, `This browser can't open .${x} pictures at full size. Below is a small preview made by the computer.`);
      card.querySelector(".fvcardicon").replaceChildren(h("img", { class: "fvthumbimg", src: "data:image/png;base64," + r.png, alt: "Small preview of " + e.name }));
      stage.replaceChildren(card);
    }, () => { if (ctx.alive()) ctx.fail(`This browser can't open .${x} pictures.`); });
  }

  V.add("image", {
    fetch: "whole",
    render(ctx) {
      const { e, bytes: v, stage } = ctx;
      const img = h("img", { class: "fvimg", src: v.url, alt: e.name, draggable: "false" });
      const pct = h("span", { class: "fvpct", "aria-live": "polite" }, "Fit");
      const z = zoomable(ctx, img, (sc) => { pct.textContent = sc > 1.02 ? Math.round(sc * 100) + "%" : "Fit"; });
      const pz = z && z.pz;
      const tb = (icon, label, run) => h("button", { class: "fvbtn", "aria-label": label, title: label, onclick: run }, V.ico(icon));
      if (pz) {
        ctx.tools(h("div", { class: "fvtools", role: "group", "aria-label": "Zoom" },
          tb("minus", "Zoom out", () => pz.zoomOut({ animate: true })),
          h("button", { class: "fvpctbtn", title: "Fit to screen", "aria-label": "Fit to screen", onclick: () => pz.reset({ animate: true }) }, pct),
          tb("plus", "Zoom in", () => pz.zoomIn({ animate: true }))));
      }
      img.onload = async () => {
        const w = img.naturalWidth, hh = img.naturalHeight;
        const mp = w * hh >= 1e5 ? ` · ${(w * hh / 1e6).toFixed(1)} MP` : "";
        ctx.add("Picture", [["Dimensions", `${w} × ${hh}${mp}`]]);
        const c = F.exif ? await F.exif(v.blob) : null;
        if (!c || !ctx.alive()) return;
        const shot = [c.fnumber, c.exposure, c.iso, c.focal].filter(Boolean).join(" · ");
        const place = c.lat != null && c.lon != null ? `${c.lat.toFixed(5)}, ${c.lon.toFixed(5)}` : "";
        ctx.add("From the camera", [
          ["Taken", c.taken ? c.taken.toLocaleString() : ""],
          ["Camera", c.camera],
          ["Lens", c.lens],
          ["Exposure", shot],
          ["Place", place, place ? h("a", { class: "fvlink", target: "_blank", rel: "noopener noreferrer", href: `https://www.openstreetmap.org/?mlat=${c.lat}&mlon=${c.lon}#map=16/${c.lat}/${c.lon}` }, "Map") : null],
        ]);
      };
      img.onerror = () => { if (z) z.destroy(); ctx.tools(); undecodable(ctx); };
      stage.classList.add("dark");
      return z;
    },
  });
})();
