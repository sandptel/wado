// wado bridge — the Computer page's bar: clear over the hero, solid once the hero has scrolled
// away, when the computer's name fades and rises into it. A class toggle on scroll; the motion is
// CSS. Listened for at the document (capturing — scroll does not bubble), because the page is
// rendered afresh each time it opens.

document.addEventListener("scroll", (e) => {
  const c = e.target;
  if (!c || c.id !== "computer") return;
  const bar = c.querySelector(".cbar");
  if (bar) bar.classList.toggle("solid", c.scrollTop > 120);
}, { capture: true, passive: true });
