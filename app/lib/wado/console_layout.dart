// Where the picture sits in the console, and where every control around it goes (port of
// ui/surround/layout.rs). A pure function of the viewport, its safe insets, the session's aspect,
// the form factor and the input face.
//
// **Scale, never resize** (Pocket Console spec §2.1): the video keeps the stage's own size; the
// console moves and scales it with one transform; the session's output never changes size.
// **The gutter law** (§2.2): a band on one axis leaves proportional gutters on the other, so each
// form has a band pair *and* a rail pair, and whatever height is left goes to the deck.
import 'dart:math' as math;
import 'package:flutter/painting.dart' show EdgeInsets, Rect, Size;

enum Form {
  port,
  land,
  tport,
  tland,
  desk;

  bool get touch => this != desk;

  /// The short side under 600 is a phone; a coarse pointer above that is a tablet.
  static Form of(Size s, {bool touch = true}) {
    final portrait = s.height >= s.width;
    if (s.shortestSide < 600) return portrait ? port : land;
    if (!touch) return desk;
    return portrait ? tport : tland;
  }
}

enum Face {
  pad('Touchpad', 'trackpad'),
  keys('Keys', 'keys'),
  quick('Quick actions', 'bolt'),
  gamepad('Gamepad', 'pad');

  const Face(this.label, this.icon);
  final String label, icon;

  /// `auto` (and anything unknown) is the form's own default: Touchpad on touch, Quick actions
  /// with a mouse — and a desktop never gets the Touchpad.
  static Face resolve(String pref, Form form) {
    final f = switch (pref) {
      'pad' => pad,
      'keys' => keys,
      'quick' => quick,
      'gamepad' => gamepad,
      _ => form.touch ? pad : quick,
    };
    return f == pad && !form.touch ? quick : f;
  }
}

enum Flow { row, col, grid }

/// One control's place. `right`: anchored by its right edge (`r.left` is then the gap from the
/// viewport's right edge) because its width is its label's. A width or height of 0 is its own.
class Slot {
  const Slot(this.r, {this.right = false, this.shown = true});
  final Rect r;
  final bool right, shown;
}

class ConsoleLayout {
  ConsoleLayout({required this.form, required this.face, required this.pic, required this.tx, required this.ty, required this.k, required this.slots, required this.ws, required this.nav, required this.faces, required this.deckCol});
  final Form form;
  final Face face;
  /// The picture's content, where it lands.
  final Rect pic;
  /// The video's transform: `translate(tx, ty) scale(k)` about its top-left corner.
  final double tx, ty, k;
  final Map<String, Slot> slots;
  final Flow ws, nav, faces;
  /// The deck is a column beside the picture rather than a band under it.
  final bool deckCol;

  Slot? operator [](String k) => slots[k];
}

/// "Full screen"'s width with its label, for the slots placed beside it.
const fullW = 128.0;
const _btn = 44.0;

/// The picture's content fitted into `room` at the session's aspect, centred.
Rect fit(Rect room, double aspect) {
  final (w, h) = room.width / math.max(room.height, 1) > aspect ? (room.height * aspect, room.height) : (room.width, room.width / aspect);
  return Rect.fromLTWH(room.left + (room.width - w) / 2, room.top + (room.height - h) / 2, w, h);
}

/// The console arrangement.
ConsoleLayout console({required double vw, required double vh, EdgeInsets inset = EdgeInsets.zero, double outW = 0, double outH = 0, required Form form, required Face face, bool deck = true}) {
  vw = math.max(vw, 1);
  vh = math.max(vh, 1);
  final sa = inset;
  final aspect = outW > 0 && outH > 0 ? outW / outH : vw / vh;
  final s = <String, Slot>{};
  void put(String k, Rect r) => s[k] = Slot(r);
  void putR(String k, Rect r) => s[k] = Slot(r, right: true);
  void hide(String k, Rect r) => s[k] = Slot(r, shown: false);
  final gp = face == Face.gamepad;
  Rect pic;
  Flow ws, nav, faces;
  bool deckCol;
  switch (form) {
    case Form.port || Form.tport:
      final tab = form == Form.tport;
      const e = 6.0, g = 6.0;
      final rail = tab ? 56.0 : 48.0;
      final top = tab ? 48.0 : 40.0;
      const strip = 44.0;
      final dock = tab ? 56.0 : 52.0;
      // The deck put away: its height goes to the picture (an upright session grows into it).
      final deckMin = !deck ? 0.0 : (tab ? 150.0 : 88.0);
      final yTop = sa.top + e;
      final availW = vw - sa.left - sa.right - 2 * e - 2 * rail - 2 * g;
      Rect pick(double reserve) {
        final availH = vh - yTop - top - g - (g + strip + g + (deck ? deckMin + g : 0) + reserve) - dock - e - sa.bottom;
        // Width-bound on a phone: the leftover height goes to the deck, not a dead band.
        final r = fit(Rect.fromLTWH(sa.left + e + rail + g, yTop + top + g, availW, math.max(availH, 40)), aspect);
        return Rect.fromLTWH(r.left, yTop + top + g, r.width, r.height);
      }

      var p = pick(0);
      // With no deck, a picture too short for the rail's faces still needs room for their row.
      if (!deck && p.height < 52 + _btn + 8 + 4 * _btn + 3 * 6.0) p = pick(_btn + g);
      final left = sa.left + e;
      final innerW = vw - sa.left - sa.right - 2 * e;
      put('band', Rect.fromLTWH(left, yTop, innerW, top));
      put('link', Rect.fromLTWH(left + 2, yTop + (top - 32) / 2, 0, 32));
      putR('people', Rect.fromLTWH(sa.right + e + 2, yTop + (top - 32) / 2, 0, 32));
      // Left rail: where. Right rail: how.
      final xl = p.left - g - rail;
      final xr = p.right + g;
      put('ws', Rect.fromLTWH(xl, p.top + 6, rail, p.height - 12));
      final cxr = xr + (rail - _btn) / 2;
      put('rotate', Rect.fromLTWH(cxr, p.top + 4, _btn, _btn));
      put('kbd', Rect.fromLTWH(cxr, p.top + 52, _btn, _btn));
      const fh = 4 * _btn + 3 * 6.0;
      // A landscape session in a portrait console leaves the picture shorter than the rail's
      // three controls: the faces then lead the deck as a row instead of overlapping Rotate.
      final facesRow = p.height < 52 + _btn + 8 + fh;
      if (!facesRow) put('faces', Rect.fromLTWH(cxr, p.bottom - 4 - fh, _btn, fh));
      // The chin: the focused window's strip, the bezel's full width.
      final sy = p.bottom + g;
      final stripR = Rect.fromLTWH(xl, sy, xr + rail - xl, strip);
      gp ? hide('strip', stripR) : put('strip', stripR);
      put('bezel', Rect.fromLTWH(xl - 4, p.top - 6, rail * 2 + g * 2 + p.width + 8, p.height + 6 + (gp ? 0 : g + strip) + 6));
      // The dock row: Full screen · Apps Home Back · ⋯. It stays put through the morph.
      final dy = vh - sa.bottom - e - dock;
      final by = dy + (dock - _btn) / 2;
      put('full', Rect.fromLTWH(left + 2, by, 0, _btn));
      final navX = left + 2 + fullW + 4;
      final navR = vw - sa.right - e - 2 - 54 - 4;
      put('nav', Rect.fromLTWH(navX, by, math.max(navR - navX, 0), _btn));
      putR('more', Rect.fromLTWH(sa.right + e + 2, by, 0, _btn));
      // The deck takes whatever is left.
      var deckY = gp ? sy : sy + strip + g;
      if (facesRow) {
        const fw = 4 * _btn + 3 * 2.0;
        put('faces', Rect.fromLTWH(left + (innerW - fw) / 2, deckY, fw, _btn));
        deckY += _btn + g;
      }
      final d = Rect.fromLTWH(left, deckY, innerW, math.max(dy - g - deckY, deckMin));
      deck ? put('deck', d) : hide('deck', d);
      deck ? put('pad', d) : hide('pad', d);
      pic = p;
      ws = Flow.col;
      nav = Flow.row;
      faces = facesRow ? Flow.row : Flow.col;
      deckCol = false;
    case Form.land || Form.tland:
      final tab = form == Form.tland;
      const e = 6.0, g = 6.0;
      final col = tab ? 140.0 : 104.0;
      const top = 44.0, strip = 40.0;
      final yTop = sa.top + e;
      final y0 = yTop + top + g;
      final availH = vh - y0 - g - strip - e - sa.bottom;
      final availW = vw - sa.left - sa.right - 2 * e - 2 * col - 2 * g;
      final p = fit(Rect.fromLTWH(sa.left + e + col + g, y0, availW, math.max(availH, 40)), aspect);
      final xl = p.left - g - col;
      final xr = p.right + g;
      final colB = vh - sa.bottom - e;
      final left = sa.left + e;
      final innerW = vw - sa.left - sa.right - 2 * e;
      put('band', Rect.fromLTWH(left, yTop, innerW, top));
      put('link', Rect.fromLTWH(left + 2, yTop + 6, 0, 32));
      // Workspaces run along the band, centred.
      put('ws', Rect.fromLTWH(left, yTop + 4, innerW, 36));
      final sy = p.bottom + g - 2;
      if (gp) {
        // Both columns become the controller: nav drops to the band under the picture.
        putR('full', Rect.fromLTWH(sa.right + e + 2, yTop, 0, _btn));
        putR('people', Rect.fromLTWH(sa.right + e + 2 + fullW + 8, yTop + 6, 0, 32));
        hide('kbd', Rect.fromLTWH(vw - sa.right - e - _btn, yTop, _btn, _btn));
        hide('rotate', Rect.fromLTWH(vw - sa.right - e - 2 * _btn, yTop, _btn, _btn));
        hide('strip', Rect.fromLTWH(p.left, sy, p.width, _btn));
        put('nav', Rect.fromLTWH(p.left, sy, 0, _btn));
        putR('more', Rect.fromLTWH(vw - p.right, sy, 0, _btn));
        const fw = 4 * _btn + 3 * 2.0;
        put('faces', Rect.fromLTWH(p.right - _btn - 8 - fw, sy, fw, _btn));
        hide('deck', Rect.fromLTWH(xr, y0, col, colB - y0));
        final pad = Rect.fromLTWH(left, y0, innerW, colB - y0);
        deck ? put('pad', pad) : hide('pad', pad);
        faces = Flow.row;
        nav = Flow.row;
      } else {
        put('kbd', Rect.fromLTWH(vw - sa.right - e - _btn, yTop, _btn, _btn));
        put('rotate', Rect.fromLTWH(vw - sa.right - e - 2 * _btn - 4, yTop, _btn, _btn));
        putR('people', Rect.fromLTWH(sa.right + e + 2 * _btn + 12, yTop + 6, 0, 32));
        put('strip', Rect.fromLTWH(p.left, sy, p.width, _btn));
        // Left column: nav 2×2 (⋯ in the fourth cell), faces 2×2, Full screen at the foot.
        put('nav', Rect.fromLTWH(xl, y0, col, 2 * _btn + 6));
        put('more', Rect.fromLTWH(xl + col / 2 + 2, y0 + _btn + 6, 0, _btn));
        put('faces', Rect.fromLTWH(xl, y0 + 2 * _btn + 18, col, 2 * _btn + 6));
        put('full', Rect.fromLTWH(xl, colB - _btn, col, _btn));
        final d = Rect.fromLTWH(xr, y0, col, colB - y0);
        deck ? put('deck', d) : hide('deck', d);
        deck ? put('pad', d) : hide('pad', d);
        faces = Flow.grid;
        nav = Flow.grid;
      }
      put('bezel', Rect.fromLTWH(p.left - 6, p.top - 6, p.width + 12, p.height + g + strip + 6));
      pic = p;
      ws = Flow.row;
      deckCol = true;
    case Form.desk:
      const e = 12.0, g = 10.0;
      const top = 44.0, bottom = 48.0, rail = 56.0, col = 104.0;
      final y0 = sa.top + e + top + g;
      final availH = vh - y0 - g - bottom - e - sa.bottom;
      final availW = vw - sa.left - sa.right - 2 * e - rail - col - 2 * g;
      final p0 = fit(Rect.fromLTWH(0, y0, availW, math.max(availH, 40)), aspect);
      final asm = rail + g + p0.width + g + col;
      final ax = sa.left + (vw - sa.left - sa.right - asm) / 2;
      final p = Rect.fromLTWH(ax + rail + g, y0, p0.width, p0.height);
      final rightGap = vw - (ax + asm);
      put('band', Rect.fromLTWH(ax, sa.top + e, asm, top));
      put('link', Rect.fromLTWH(ax, sa.top + e + 6, 0, 32));
      putR('full', Rect.fromLTWH(rightGap, sa.top + e, 0, _btn));
      putR('people', Rect.fromLTWH(rightGap + fullW + 10, sa.top + e + 6, 0, 32));
      // A mouse and a keyboard need neither.
      hide('rotate', Rect.fromLTWH(vw - rightGap - _btn, sa.top + e, _btn, _btn));
      hide('kbd', Rect.fromLTWH(vw - rightGap - _btn, sa.top + e, _btn, _btn));
      put('ws', Rect.fromLTWH(ax, p.top + 8, rail, p.height - 16));
      put('bezel', Rect.fromLTWH(ax - 6, p.top - 6, rail + g + p.width + 12, p.height + 12));
      final sy = p.bottom + g + 2;
      put('strip', Rect.fromLTWH(p.left, sy, math.min(396, p.width / 2 - 100), _btn));
      put('nav', Rect.fromLTWH(vw / 2 - 100, sy, 200, _btn));
      putR('more', Rect.fromLTWH(rightGap, sy, 0, _btn));
      const fw = 3 * _btn + 2 * 4.0;
      put('faces', Rect.fromLTWH(ax + asm - 54 - 10 - fw, sy, fw, _btn));
      final d = Rect.fromLTWH(p.right + g, p.top - 6, col, p.height + 12);
      gp || !deck ? hide('deck', d) : put('deck', d);
      deck ? put('pad', d) : hide('pad', d);
      pic = p;
      ws = Flow.col;
      nav = Flow.row;
      faces = Flow.row;
      deckCol = true;
  }
  // The video is the stage's own size with `contain`; its content sits inside it. One transform
  // takes that content onto `pic`.
  final content = fit(Rect.fromLTWH(0, 0, vw, vh), aspect);
  final k = pic.width / content.width;
  put('pic', pic);
  return ConsoleLayout(form: form, face: face, pic: pic, tx: pic.left - k * content.left, ty: pic.top - k * content.top, k: k, slots: s, ws: ws, nav: nav, faces: faces, deckCol: deckCol);
}

/// Between two arrangements of the same console (the deck going away or coming back).
ConsoleLayout lerpLayout(ConsoleLayout a, ConsoleLayout b, double t) => ConsoleLayout(
      form: b.form,
      face: b.face,
      pic: Rect.lerp(a.pic, b.pic, t)!,
      tx: a.tx + (b.tx - a.tx) * t,
      ty: a.ty + (b.ty - a.ty) * t,
      k: a.k + (b.k - a.k) * t,
      slots: {
        for (final e in b.slots.entries)
          e.key: Slot(a.slots[e.key] == null ? e.value.r : Rect.lerp(a.slots[e.key]!.r, e.value.r, t)!, right: e.value.right, shown: e.value.shown),
      },
      ws: b.ws,
      nav: b.nav,
      faces: b.faces,
      deckCol: b.deckCol,
    );
