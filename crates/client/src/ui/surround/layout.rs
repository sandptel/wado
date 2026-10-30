//! Where the picture sits in the console, and where every control around it goes.
//!
//! A pure function of the viewport, its safe insets, the session's aspect, the form factor and
//! the input face. No Dioxus, no DOM: `stage_fit.js` writes the answer as CSS variables, and
//! the cockpit needs none of it — its positions are today's CSS.
//!
//! **Scale, never resize** (spec §2.1): the video element always keeps the stage's own size. The
//! console moves and scales it with one transform; the session's output never changes size.
//!
//! **The gutter law** (§2.2): a band on one axis leaves proportional gutters on the other, so each
//! form has a band pair *and* a rail pair, and whatever height is left goes to the deck.

/// A box in CSS pixels, viewport coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    /// `other` lies entirely inside this box (half a pixel of rounding allowed).
    #[cfg(test)]
    pub fn holds(&self, o: &Rect) -> bool {
        o.x >= self.x - 0.5
            && o.y >= self.y - 0.5
            && o.right() <= self.right() + 0.5
            && o.bottom() <= self.bottom() + 0.5
    }
    #[cfg(test)]
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.right() - 0.5
            && o.x < self.right() - 0.5
            && self.y < o.bottom() - 0.5
            && o.y < self.bottom() - 0.5
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub t: f64,
    pub r: f64,
    pub b: f64,
    pub l: f64,
}

/// The form factor (spec §4). Decided by the page (`stage_fit.js`): the short side under 600 is
/// a phone; a coarse pointer above that is a tablet; anything else is a desktop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Form {
    #[default]
    Port,
    Land,
    TabPort,
    TabLand,
    Desk,
}

impl Form {
    pub fn parse(s: &str) -> Self {
        match s {
            "land" => Self::Land,
            "tport" => Self::TabPort,
            "tland" => Self::TabLand,
            "desk" => Self::Desk,
            _ => Self::Port,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Port => "port",
            Self::Land => "land",
            Self::TabPort => "tport",
            Self::TabLand => "tland",
            Self::Desk => "desk",
        }
    }
    pub fn touch(self) -> bool {
        self != Self::Desk
    }
}

/// What the deck holds (spec §3, "Input faces").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Face {
    #[default]
    Pad,
    Keys,
    Quick,
    Gamepad,
}

impl Face {
    pub const ALL: [Face; 4] = [Face::Pad, Face::Keys, Face::Quick, Face::Gamepad];

    /// `auto` (and anything unknown) is the form's own default: Touchpad on touch, Quick
    /// actions with a mouse — and a desktop never gets the Touchpad (spec §4).
    pub fn resolve(pref: &str, form: Form) -> Self {
        let f = match pref {
            "pad" => Self::Pad,
            "keys" => Self::Keys,
            "quick" => Self::Quick,
            "gamepad" => Self::Gamepad,
            _ if form.touch() => Self::Pad,
            _ => Self::Quick,
        };
        if f == Self::Pad && !form.touch() {
            Self::Quick
        } else {
            f
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Pad => "pad",
            Self::Keys => "keys",
            Self::Quick => "quick",
            Self::Gamepad => "gamepad",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Pad => "Touchpad",
            Self::Keys => "Keys",
            Self::Quick => "Quick actions",
            Self::Gamepad => "Gamepad",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Self::Pad => "trackpad",
            Self::Keys => "keys",
            Self::Quick => "bolt",
            Self::Gamepad => "pad",
        }
    }
}

/// How a group of controls is laid out inside its slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Row,
    Col,
    /// Two columns.
    Grid,
}

impl Flow {
    pub fn name(self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::Col => "col",
            Self::Grid => "grid",
        }
    }
}

/// One control's place. `right`: anchored by its right edge (`x` is then the gap from the
/// viewport's right edge) because its width is its label's. `w`/`h` of 0 are "its own size".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub r: Rect,
    pub right: bool,
    pub shown: bool,
}

/// Everything the page needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub form: Form,
    pub face: Face,
    /// The picture's content, where it lands.
    pub pic: Rect,
    /// The video element's transform: `translate(tx, ty) scale(k)` about its top-left corner.
    pub tx: f64,
    pub ty: f64,
    pub k: f64,
    pub slots: Vec<(&'static str, Slot)>,
    pub ws: Flow,
    pub nav: Flow,
    pub faces: Flow,
    /// The deck is a column beside the picture rather than a band under it.
    pub deck_col: bool,
}

impl Layout {
    #[cfg(test)]
    pub fn slot(&self, k: &str) -> Option<&Slot> {
        self.slots.iter().find(|(n, _)| *n == k).map(|(_, s)| s)
    }

    /// The CSS variables `stage_fit.js` writes on the root: `--c-<slot>-x|y|w|h` (an anchored-right
    /// slot writes `--c-<slot>-r` and `auto` for x), plus the picture's transform.
    pub fn vars(&self) -> Vec<(String, String)> {
        let px = |v: f64| format!("{:.1}px", v);
        let mut out = vec![
            ("--c-tx".to_string(), px(self.tx)),
            ("--c-ty".to_string(), px(self.ty)),
            ("--c-k".to_string(), format!("{:.5}", self.k)),
            // Corner radius in the video's own pixels, so it reads 16 px once scaled.
            ("--c-pr".to_string(), px(16.0 / self.k.max(0.01))),
        ];
        for (k, s) in &self.slots {
            let (l, r) = if s.right {
                ("auto".to_string(), px(s.r.x))
            } else {
                (px(s.r.x), "auto".to_string())
            };
            out.push((format!("--c-{k}-x"), l));
            out.push((format!("--c-{k}-r"), r));
            out.push((format!("--c-{k}-y"), px(s.r.y)));
            let dim = |v: f64| if v > 0.0 { px(v) } else { "auto".into() };
            out.push((format!("--c-{k}-w"), dim(s.r.w)));
            out.push((format!("--c-{k}-h"), dim(s.r.h)));
            out.push((
                format!("--c-{k}-o"),
                if s.shown { "1" } else { "0" }.to_string(),
            ));
        }
        out
    }
}

/// What the layout is computed from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub vw: f64,
    pub vh: f64,
    pub inset: Insets,
    /// The session's picture, any unit: only the aspect is used. 0 is "the viewport's".
    pub out_w: f64,
    pub out_h: f64,
    pub form: Form,
    pub face: Face,
}

/// "Full screen"'s width with its label, for the slots placed beside it on its left. The label is
/// fixed copy, so this is a constant rather than a measurement.
pub const FULL_W: f64 = 128.0;
const BTN: f64 = 44.0;

/// The picture's content fitted into `room` at the session's aspect, centred.
fn fit(room: Rect, aspect: f64) -> Rect {
    let (w, h) = if room.w / room.h.max(1.0) > aspect {
        (room.h * aspect, room.h)
    } else {
        (room.w, room.w / aspect)
    };
    Rect::new(
        room.x + (room.w - w) / 2.0,
        room.y + (room.h - h) / 2.0,
        w,
        h,
    )
}

struct B {
    slots: Vec<(&'static str, Slot)>,
}

impl B {
    fn put(&mut self, k: &'static str, r: Rect) {
        self.slots.push((
            k,
            Slot {
                r,
                right: false,
                shown: true,
            },
        ));
    }
    fn put_r(&mut self, k: &'static str, r: Rect) {
        self.slots.push((
            k,
            Slot {
                r,
                right: true,
                shown: true,
            },
        ));
    }
    fn hide(&mut self, k: &'static str, r: Rect) {
        self.slots.push((
            k,
            Slot {
                r,
                right: false,
                shown: false,
            },
        ));
    }
}

/// The console arrangement for `f`.
pub fn console(f: Frame) -> Layout {
    let (vw, vh, sa) = (f.vw.max(1.0), f.vh.max(1.0), f.inset);
    let aspect = if f.out_w > 0.0 && f.out_h > 0.0 {
        f.out_w / f.out_h
    } else {
        vw / vh
    };
    let mut b = B { slots: Vec::new() };
    let gp = f.face == Face::Gamepad;
    let (pic, ws, nav, faces, deck_col);
    match f.form {
        Form::Port | Form::TabPort => {
            let tab = f.form == Form::TabPort;
            let (e, g) = (6.0, 6.0);
            let rail = if tab { 56.0 } else { 48.0 };
            let top = if tab { 48.0 } else { 40.0 };
            let strip = 44.0;
            let dock = if tab { 56.0 } else { 52.0 };
            let deck_min = if tab { 150.0 } else { 88.0 };
            let y_top = sa.t + e;
            let avail_h = vh - y_top - top - g - (g + strip + g + deck_min + g) - dock - e - sa.b;
            let avail_w = vw - sa.l - sa.r - 2.0 * e - 2.0 * rail - 2.0 * g;
            // Width-bound on a phone: the leftover height goes to the deck, not a dead band.
            let p = fit(
                Rect::new(
                    sa.l + e + rail + g,
                    y_top + top + g,
                    avail_w,
                    avail_h.max(40.0),
                ),
                aspect,
            );
            let p = Rect::new(p.x, y_top + top + g, p.w, p.h);
            let left = sa.l + e;
            let inner_w = vw - sa.l - sa.r - 2.0 * e;
            b.put("band", Rect::new(left, y_top, inner_w, top));
            b.put(
                "link",
                Rect::new(left + 2.0, y_top + (top - 32.0) / 2.0, 0.0, 32.0),
            );
            b.put_r(
                "people",
                Rect::new(sa.r + e + 2.0, y_top + (top - 32.0) / 2.0, 0.0, 32.0),
            );
            // Left rail: where. Right rail: how.
            let xl = p.x - g - rail;
            let xr = p.right() + g;
            b.put("ws", Rect::new(xl, p.y + 6.0, rail, p.h - 12.0));
            let cxr = xr + (rail - BTN) / 2.0;
            b.put("rotate", Rect::new(cxr, p.y + 4.0, BTN, BTN));
            b.put("kbd", Rect::new(cxr, p.y + 52.0, BTN, BTN));
            let fh = 4.0 * BTN + 3.0 * 6.0;
            b.put("faces", Rect::new(cxr, p.bottom() - 4.0 - fh, BTN, fh));
            // The chin: the focused window's strip, the bezel's full width.
            let sy = p.bottom() + g;
            let strip_r = Rect::new(xl, sy, xr + rail - xl, strip);
            if gp {
                b.hide("strip", strip_r);
            } else {
                b.put("strip", strip_r);
            }
            b.put(
                "bezel",
                Rect::new(
                    xl - 4.0,
                    p.y - 6.0,
                    rail * 2.0 + g * 2.0 + p.w + 8.0,
                    // No chin while the Gamepad face takes its height.
                    p.h + 6.0 + if gp { 0.0 } else { g + strip } + 6.0,
                ),
            );
            // The dock row: Full screen · Apps Home Back · ⋯. It stays put through the morph.
            let dy = vh - sa.b - e - dock;
            let by = dy + (dock - BTN) / 2.0;
            b.put("full", Rect::new(left + 2.0, by, 0.0, BTN));
            // Apps · Home · Back between Full screen and ⋯, never touching either.
            let nav_x = left + 2.0 + FULL_W + 4.0;
            let nav_r = vw - sa.r - e - 2.0 - 54.0 - 4.0;
            b.put("nav", Rect::new(nav_x, by, (nav_r - nav_x).max(0.0), BTN));
            b.put_r("more", Rect::new(sa.r + e + 2.0, by, 0.0, BTN));
            // The deck takes whatever is left.
            let deck_y = if gp { sy } else { sy + strip + g };
            let deck = Rect::new(left, deck_y, inner_w, (dy - g - deck_y).max(deck_min));
            b.put("deck", deck);
            b.put("pad", deck);
            pic = p;
            ws = Flow::Col;
            nav = Flow::Row;
            faces = Flow::Col;
            deck_col = false;
        }
        Form::Land | Form::TabLand => {
            let tab = f.form == Form::TabLand;
            let (e, g) = (6.0, 6.0);
            let col = if tab { 140.0 } else { 104.0 };
            let top = 44.0;
            let strip = 40.0;
            let y_top = sa.t + e;
            let y0 = y_top + top + g;
            let avail_h = vh - y0 - g - strip - e - sa.b;
            let avail_w = vw - sa.l - sa.r - 2.0 * e - 2.0 * col - 2.0 * g;
            let p = fit(
                Rect::new(sa.l + e + col + g, y0, avail_w, avail_h.max(40.0)),
                aspect,
            );
            let xl = p.x - g - col;
            let xr = p.right() + g;
            let col_b = vh - sa.b - e;
            let left = sa.l + e;
            let inner_w = vw - sa.l - sa.r - 2.0 * e;
            b.put("band", Rect::new(left, y_top, inner_w, top));
            b.put("link", Rect::new(left + 2.0, y_top + 6.0, 0.0, 32.0));
            // Workspaces run along the band, centred.
            b.put("ws", Rect::new(left, y_top + 4.0, inner_w, 36.0));
            let sy = p.bottom() + g - 2.0;
            if gp {
                // Both columns become the controller: nav drops to the band under the picture.
                b.put_r("full", Rect::new(sa.r + e + 2.0, y_top, 0.0, BTN));
                b.put_r(
                    "people",
                    Rect::new(sa.r + e + 2.0 + FULL_W + 8.0, y_top + 6.0, 0.0, 32.0),
                );
                b.hide("kbd", Rect::new(vw - sa.r - e - BTN, y_top, BTN, BTN));
                b.hide(
                    "rotate",
                    Rect::new(vw - sa.r - e - 2.0 * BTN, y_top, BTN, BTN),
                );
                b.hide("strip", Rect::new(p.x, sy, p.w, BTN));
                b.put("nav", Rect::new(p.x, sy, 0.0, BTN));
                b.put_r("more", Rect::new(vw - p.right(), sy, 0.0, BTN));
                let fw = 4.0 * BTN + 3.0 * 2.0;
                b.put("faces", Rect::new(p.right() - BTN - 8.0 - fw, sy, fw, BTN));
                b.hide("deck", Rect::new(xr, y0, col, col_b - y0));
                b.put("pad", Rect::new(left, y0, inner_w, col_b - y0));
                faces = Flow::Row;
                nav = Flow::Row;
            } else {
                b.put("kbd", Rect::new(vw - sa.r - e - BTN, y_top, BTN, BTN));
                b.put(
                    "rotate",
                    Rect::new(vw - sa.r - e - 2.0 * BTN - 4.0, y_top, BTN, BTN),
                );
                b.put_r(
                    "people",
                    Rect::new(sa.r + e + 2.0 * BTN + 12.0, y_top + 6.0, 0.0, 32.0),
                );
                b.put("strip", Rect::new(p.x, sy, p.w, BTN));
                // Left column: nav 2×2 (⋯ in the fourth cell), faces 2×2, Full screen at the foot.
                b.put("nav", Rect::new(xl, y0, col, 2.0 * BTN + 6.0));
                b.put(
                    "more",
                    Rect::new(xl + col / 2.0 + 2.0, y0 + BTN + 6.0, 0.0, BTN),
                );
                b.put(
                    "faces",
                    Rect::new(xl, y0 + 2.0 * BTN + 18.0, col, 2.0 * BTN + 6.0),
                );
                b.put("full", Rect::new(xl, col_b - BTN, col, BTN));
                let deck = Rect::new(xr, y0, col, col_b - y0);
                b.put("deck", deck);
                b.put("pad", deck);
                faces = Flow::Grid;
                nav = Flow::Grid;
            }
            b.put(
                "bezel",
                Rect::new(p.x - 6.0, p.y - 6.0, p.w + 12.0, p.h + g + strip + 6.0),
            );
            pic = p;
            ws = Flow::Row;
            deck_col = true;
        }
        Form::Desk => {
            let (e, g) = (12.0, 10.0);
            let (top, bottom, rail, col) = (44.0, 48.0, 56.0, 104.0);
            let y0 = sa.t + e + top + g;
            let avail_h = vh - y0 - g - bottom - e - sa.b;
            let avail_w = vw - sa.l - sa.r - 2.0 * e - rail - col - 2.0 * g;
            let p0 = fit(Rect::new(0.0, y0, avail_w, avail_h.max(40.0)), aspect);
            let asm = rail + g + p0.w + g + col;
            let ax = sa.l + (vw - sa.l - sa.r - asm) / 2.0;
            let p = Rect::new(ax + rail + g, y0, p0.w, p0.h);
            let right_gap = vw - (ax + asm);
            b.put("band", Rect::new(ax, sa.t + e, asm, top));
            b.put("link", Rect::new(ax, sa.t + e + 6.0, 0.0, 32.0));
            b.put_r("full", Rect::new(right_gap, sa.t + e, 0.0, BTN));
            b.put_r(
                "people",
                Rect::new(right_gap + FULL_W + 10.0, sa.t + e + 6.0, 0.0, 32.0),
            );
            // A mouse and a keyboard need neither.
            b.hide(
                "rotate",
                Rect::new(vw - right_gap - BTN, sa.t + e, BTN, BTN),
            );
            b.hide("kbd", Rect::new(vw - right_gap - BTN, sa.t + e, BTN, BTN));
            b.put("ws", Rect::new(ax, p.y + 8.0, rail, p.h - 16.0));
            b.put(
                "bezel",
                Rect::new(ax - 6.0, p.y - 6.0, rail + g + p.w + 12.0, p.h + 12.0),
            );
            let sy = p.bottom() + g + 2.0;
            b.put(
                "strip",
                Rect::new(p.x, sy, 396.0_f64.min(p.w / 2.0 - 100.0), BTN),
            );
            b.put("nav", Rect::new(vw / 2.0 - 100.0, sy, 200.0, BTN));
            b.put_r("more", Rect::new(right_gap, sy, 0.0, BTN));
            let fw = 3.0 * BTN + 2.0 * 4.0;
            b.put("faces", Rect::new(ax + asm - 54.0 - 10.0 - fw, sy, fw, BTN));
            let deck = Rect::new(p.right() + g, p.y - 6.0, col, p.h + 12.0);
            if gp {
                b.hide("deck", deck);
            } else {
                b.put("deck", deck);
            }
            b.put("pad", deck);
            pic = p;
            ws = Flow::Col;
            nav = Flow::Row;
            faces = Flow::Row;
            deck_col = true;
        }
    }
    // The video element is the stage's own size with `object-fit: contain`; its content sits
    // at (ox, oy) inside it. One transform takes that content onto `pic`.
    let content = fit(Rect::new(0.0, 0.0, vw, vh), aspect);
    let k = pic.w / content.w;
    // The picture's own box, for what rides on it (the debug readout in its corner).
    b.slots.push((
        "pic",
        Slot {
            r: pic,
            right: false,
            shown: true,
        },
    ));
    Layout {
        form: f.form,
        face: f.face,
        pic,
        tx: pic.x - k * content.x,
        ty: pic.y - k * content.y,
        k,
        slots: b.slots,
        ws,
        nav,
        faces,
        deck_col,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(vw: f64, vh: f64, form: Form, face: Face) -> Frame {
        Frame {
            vw,
            vh,
            inset: Insets::default(),
            out_w: 0.0,
            out_h: 0.0,
            form,
            face,
        }
    }
    fn r(l: &Layout, k: &str) -> Rect {
        l.slot(k).unwrap_or_else(|| panic!("no slot {k}")).r
    }

    #[test]
    fn portrait_rails_hug_the_picture() {
        let l = console(frame(390.0, 844.0, Form::Port, Face::Pad));
        // 390×844 → 270×584, ≈ 69%.
        assert!((l.pic.w - 270.0).abs() < 0.6, "{:?}", l.pic);
        assert!((l.pic.h - 584.3).abs() < 1.0, "{:?}", l.pic);
        assert!((l.k - 0.692).abs() < 0.002);
        let ws = r(&l, "ws");
        let faces = r(&l, "faces");
        assert!(
            (l.pic.x - ws.right() - 6.0).abs() < 0.6,
            "left rail hugs: {ws:?}"
        );
        assert!(
            (faces.x - l.pic.right() - 6.0 - 2.0).abs() < 0.6,
            "right rail hugs"
        );
        // The chin sits under the picture, the deck under the chin, the dock row at the bottom.
        let strip = r(&l, "strip");
        let deck = r(&l, "deck");
        let nav = r(&l, "nav");
        assert!(strip.y > l.pic.bottom() && deck.y > strip.bottom() && nav.y > deck.bottom());
        assert!(deck.h >= 88.0, "the deck keeps its floor: {deck:?}");
        assert!(Rect::new(0.0, 0.0, 390.0, 844.0).holds(&deck));
        assert!(!l.deck_col);
    }

    #[test]
    fn a_taller_phone_gets_a_bigger_deck_not_a_dead_band() {
        // The same session (its output is fixed at Start) on two phones of one width.
        let at = |vh| {
            let mut f = frame(390.0, vh, Form::Port, Face::Pad);
            f.out_w = 1080.0;
            f.out_h = 2340.0;
            console(f)
        };
        let (short, tall) = (at(860.0), at(940.0));
        // Same width, so the picture is the same scale…
        assert!((short.pic.w - tall.pic.w).abs() < 0.6);
        // …and the leftover height went to the deck.
        assert!(r(&tall, "deck").h >= r(&short, "deck").h + 79.0);
    }

    #[test]
    fn landscape_has_two_columns_and_a_band_under_the_picture() {
        let l = console(frame(844.0, 390.0, Form::Land, Face::Pad));
        assert!(l.deck_col);
        let deck = r(&l, "deck");
        let nav = r(&l, "nav");
        assert!(nav.right() <= l.pic.x, "nav column left of the picture");
        assert!(deck.x >= l.pic.right(), "deck column right of it");
        assert!(r(&l, "strip").y >= l.pic.bottom());
        assert!(l.k > 0.6 && l.k < 0.8, "≈ 72%: {}", l.k);
        assert_eq!(l.faces, Flow::Grid);
    }

    #[test]
    fn landscape_gamepad_turns_both_columns_into_the_controller() {
        let l = console(frame(844.0, 390.0, Form::Land, Face::Gamepad));
        assert!(!l.slot("deck").unwrap().shown && !l.slot("strip").unwrap().shown);
        let pad = r(&l, "pad");
        assert!(pad.x < l.pic.x && pad.right() > l.pic.right(), "{pad:?}");
    }

    #[test]
    fn portrait_gamepad_hides_the_chin_and_takes_its_height() {
        let pad = console(frame(390.0, 844.0, Form::Port, Face::Pad));
        let gp = console(frame(390.0, 844.0, Form::Port, Face::Gamepad));
        assert!(!gp.slot("strip").unwrap().shown);
        assert!(r(&gp, "deck").h > r(&pad, "deck").h + 40.0);
    }

    #[test]
    fn desktop_frames_the_picture_evenly() {
        let l = console(frame(1280.0, 800.0, Form::Desk, Face::Quick));
        assert!(l.k > 0.78 && l.k < 0.86, "≈ 83%: {}", l.k);
        let ws = r(&l, "ws");
        let deck = r(&l, "deck");
        assert!(ws.right() < l.pic.x && deck.x > l.pic.right());
        assert!(!l.slot("rotate").unwrap().shown && !l.slot("kbd").unwrap().shown);
        // The assembly is centred.
        let left = ws.x;
        let right = 1280.0 - deck.right();
        assert!((left - right).abs() < 1.0, "{left} vs {right}");
    }

    #[test]
    fn a_joiner_letterboxes_inside_the_same_room() {
        // A landscape lead's picture on a portrait phone: width-bound, much shorter.
        let mut f = frame(390.0, 844.0, Form::Port, Face::Pad);
        f.out_w = 1920.0;
        f.out_h = 1080.0;
        let l = console(f);
        assert!((l.pic.w / l.pic.h - 16.0 / 9.0).abs() < 0.01);
        assert!((l.pic.w - 270.0).abs() < 0.6);
        // The video element's content lands exactly on the picture rect.
        let content_w = 390.0;
        let content_h = 390.0 * 9.0 / 16.0;
        let oy = (844.0 - content_h) / 2.0;
        assert!((l.k * content_w - l.pic.w).abs() < 0.01);
        assert!((l.ty + l.k * oy - l.pic.y).abs() < 0.01);
        // The deck absorbs the difference.
        assert!(r(&l, "deck").h > 300.0);
    }

    #[test]
    fn safe_insets_are_respected() {
        let mut f = frame(390.0, 844.0, Form::Port, Face::Pad);
        f.inset = Insets {
            t: 47.0,
            r: 0.0,
            b: 34.0,
            l: 0.0,
        };
        let l = console(f);
        assert!(r(&l, "band").y >= 47.0);
        assert!(r(&l, "nav").bottom() <= 844.0 - 34.0);
        assert!(l.pic.y > 47.0 + 40.0);
    }

    #[test]
    fn nothing_overlaps_the_picture() {
        for (vw, vh, form) in [
            (390.0, 844.0, Form::Port),
            (844.0, 390.0, Form::Land),
            (820.0, 1180.0, Form::TabPort),
            (1180.0, 820.0, Form::TabLand),
            (1280.0, 800.0, Form::Desk),
        ] {
            for face in Face::ALL {
                let l = console(frame(vw, vh, form, Face::resolve(face.id(), form)));
                for (k, s) in &l.slots {
                    if ["bezel", "pad", "band", "pic"].contains(k) || !s.shown || s.right {
                        continue;
                    }
                    if *k == "ws" && l.ws == Flow::Row {
                        continue;
                    }
                    assert!(
                        !s.r.overlaps(&l.pic),
                        "{form:?}/{face:?}: {k} {:?} overlaps the picture {:?}",
                        s.r,
                        l.pic
                    );
                }
            }
        }
    }

    #[test]
    fn faces_resolve_per_form() {
        assert_eq!(Face::resolve("auto", Form::Port), Face::Pad);
        assert_eq!(Face::resolve("auto", Form::Desk), Face::Quick);
        assert_eq!(Face::resolve("pad", Form::Desk), Face::Quick);
        assert_eq!(Face::resolve("keys", Form::Desk), Face::Keys);
        assert_eq!(Face::resolve("nonsense", Form::Land), Face::Pad);
    }

    #[test]
    fn vars_name_every_slot() {
        let l = console(frame(390.0, 844.0, Form::Port, Face::Pad));
        let v = l.vars();
        let has = |k: &str| v.iter().any(|(n, _)| n == k);
        assert!(has("--c-k") && has("--c-deck-y") && has("--c-people-r") && has("--c-strip-o"));
        let people_x = v.iter().find(|(n, _)| n == "--c-people-x").unwrap();
        assert_eq!(people_x.1, "auto", "anchored right");
    }
}
