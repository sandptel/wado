// The least DOM the trackpad's JS-built views need (js/pad_rail.js, js/pad_settings.js), for
// scripts/pad-check.mjs. Elements, attributes, dataset, classList, text, events (with bubbling),
// and querySelector(All) for simple selectors: `tag`, `.class`, `#id`, `[attr]`, `[attr="v"]`,
// chained (no combinators).
// innerHTML is stored, not parsed: views put only icon markup there.

class El {
  constructor(tag) {
    this.tagName = tag.toUpperCase();
    this.attrs = {};
    this.children = [];
    this.parentNode = null;
    this.listeners = {};
    this._text = "";
    this.innerHTML = "";
    this.style = { props: {}, setProperty(k, v) { this.props[k] = v; } };
    this.value = "";
    this.checked = false;
    this.disabled = false;
    const self = this;
    this.dataset = new Proxy({}, {
      get: (_, k) => self.attrs["data-" + k],
      set: (_, k, v) => { self.attrs["data-" + k] = String(v); return true; },
      deleteProperty: (_, k) => { delete self.attrs["data-" + k]; return true; },
      has: (_, k) => ("data-" + k) in self.attrs,
    });
    this.classList = {
      list: () => (self.attrs.class || "").split(/\s+/).filter(Boolean),
      contains: (c) => self.classList.list().includes(c),
      add: (...cs) => { self.attrs.class = [...new Set([...self.classList.list(), ...cs])].join(" "); },
      remove: (...cs) => { self.attrs.class = self.classList.list().filter((x) => !cs.includes(x)).join(" "); },
      toggle: (c, on) => {
        const want = on === undefined ? !self.classList.contains(c) : !!on;
        if (want) self.classList.add(c); else self.classList.remove(c);
        return want;
      },
    };
  }
  get className() { return this.attrs.class || ""; }
  set className(v) { this.attrs.class = v; }
  get id() { return this.attrs.id || ""; }
  set id(v) { this.attrs.id = v; }
  get hidden() { return "hidden" in this.attrs; }
  set hidden(v) { if (v) this.attrs.hidden = ""; else delete this.attrs.hidden; }
  get textContent() { return this._text + this.children.map((c) => c.textContent).join(""); }
  set textContent(v) { this._text = String(v); this.children = []; }
  setAttribute(k, v) { this.attrs[k] = String(v); }
  getAttribute(k) { return k in this.attrs ? this.attrs[k] : null; }
  removeAttribute(k) { delete this.attrs[k]; }
  hasAttribute(k) { return k in this.attrs; }
  appendChild(c) { if (c.parentNode) c.remove(); c.parentNode = this; this.children.push(c); return c; }
  append(...cs) { for (const c of cs) this.appendChild(c); }
  replaceChildren(...cs) { for (const c of this.children) c.parentNode = null; this.children = []; this._text = ""; this.append(...cs); }
  remove() { if (!this.parentNode) return; const p = this.parentNode; p.children = p.children.filter((x) => x !== this); this.parentNode = null; }
  addEventListener(t, f) { (this.listeners[t] ||= []).push(f); }
  removeEventListener(t, f) { this.listeners[t] = (this.listeners[t] || []).filter((x) => x !== f); }
  dispatch(type, extra = {}) {
    const ev = { type, target: this, preventDefault() {}, stopPropagation() { this._stop = true; }, ...extra };
    for (let n = this; n && !ev._stop; n = n.parentNode) for (const f of n.listeners[type] || []) f(ev);
    return ev;
  }
  click() { if (!this.disabled) this.dispatch("click"); }
  focus() {}
  blur() {}
  matches(sel) { return sel.split(",").some((s) => one(this, s.trim())); }
  closest(sel) { for (let n = this; n; n = n.parentNode) if (n.matches && n.matches(sel)) return n; return null; }
  querySelectorAll(sel) {
    const out = [];
    const walk = (n) => { for (const c of n.children) { if (c.matches(sel)) out.push(c); walk(c); } };
    walk(this);
    return out;
  }
  querySelector(sel) { return this.querySelectorAll(sel)[0] || null; }
}

// One compound selector: tag, .class and [attr] / [attr="v"] parts, all of which must match.
function one(el, sel) {
  const parts = sel.match(/(^[a-z]+)|(\.[\w-]+)|(#[\w-]+)|(\[[^\]]+\])/gi) || [];
  return parts.length > 0 && parts.every((p) => {
    if (p[0] === ".") return el.classList.contains(p.slice(1));
    if (p[0] === "#") return el.attrs.id === p.slice(1);
    if (p[0] === "[") {
      const m = p.slice(1, -1).match(/^([\w-]+)(?:="?([^"]*)"?)?$/);
      return m && (m[2] === undefined ? m[1] in el.attrs : el.attrs[m[1]] === m[2]);
    }
    return el.tagName === p.toUpperCase();
  });
}

export function makeDocument() {
  const body = new El("body");
  return {
    body,
    documentElement: new El("html"),
    visibilityState: "visible",
    createElement: (t) => new El(t),
    addEventListener() {},
    removeEventListener() {},
    getAnimations: () => [],
  };
}
