// The gamepad editor's changes (port of gpad/edit.js's model, kept on this phone like the layout
// choice): per layout and orientation, each control's move (a fraction of the play area, so a
// bigger screen keeps the shape), its size, and whether it is hidden. Built-ins never change;
// "Reset" drops the changes.
import 'dart:convert';
import 'dart:math' as math;
import 'dart:ui' show Offset, Rect, Size;

import 'gpad_layouts.dart';
import 'settings.dart';

class Tweak {
  const Tweak({this.dx = 0, this.dy = 0, this.s = 1, this.hide = false});
  final double dx, dy, s;
  final bool hide;
  Tweak copy({double? dx, double? dy, double? s, bool? hide}) => Tweak(dx: dx ?? this.dx, dy: dy ?? this.dy, s: s ?? this.s, hide: hide ?? this.hide);
  List<Object> toJson() => [dx, dy, s, hide];
  static Tweak from(Object? j) => j is List && j.length == 4 ? Tweak(dx: (j[0] as num).toDouble(), dy: (j[1] as num).toDouble(), s: (j[2] as num).toDouble(), hide: j[3] == true) : const Tweak();
}

/// Never under 44 px on its short side, the way the web's slider stops.
const minSide = 44.0;

typedef Tweaks = Map<String, Tweak>;

Tweaks tweaksFor(String layout, String o) {
  try {
    final all = jsonDecode(Settings.I.s('gpad_edits')) as Map;
    final m = (all['$layout:$o'] as Map?) ?? const {};
    return {for (final e in m.entries) '${e.key}': Tweak.from(e.value)};
  } catch (_) {
    return {};
  }
}

void saveTweaks(String layout, String o, Tweaks t) {
  Map all;
  try {
    all = jsonDecode(Settings.I.s('gpad_edits')) as Map;
  } catch (_) {
    all = {};
  }
  if (t.isEmpty) {
    all.remove('$layout:$o');
  } else {
    all['$layout:$o'] = {for (final e in t.entries) e.key: e.value.toJson()};
  }
  Settings.I.set('gpad_edits', jsonEncode(all));
}

/// The layout's places with the changes on top.
Map<String, Rect> placeTweaked(GLayout l, String o, double k, Size box, Tweaks t) {
  final at = place(l, o, k, box);
  return {
    for (final e in at.entries)
      e.key: () {
        final w = t[e.key];
        if (w == null) return e.value;
        final r = e.value;
        final grow = math.max(w.s, minSide / math.max(1, r.shortestSide));
        return Rect.fromCenter(center: r.center + Offset(w.dx * box.width, w.dy * box.height), width: r.width * grow, height: r.height * grow);
      }(),
  };
}

/// Move `ids` by `d` px in a `box`.
Tweaks moved(Tweaks t, Iterable<String> ids, Offset d, Size box) => {
      ...t,
      for (final id in ids) id: (t[id] ?? const Tweak()).copy(dx: (t[id]?.dx ?? 0) + d.dx / box.width, dy: (t[id]?.dy ?? 0) + d.dy / box.height),
    };

/// Size `ids` to `s` (of the layout's own): a group spreads around its centre, so its buttons
/// never overlap.
Tweaks sized(Tweaks t, Iterable<String> ids, double s, Map<String, Rect> now, Size box) {
  final ids_ = ids.toList();
  if (ids_.isEmpty) return t;
  final c = ids_.map((id) => now[id]!.center).reduce((a, b) => a + b) / ids_.length.toDouble();
  final out = {...t};
  for (final id in ids_) {
    final w = t[id] ?? const Tweak();
    final f = s / w.s;
    final shift = (now[id]!.center - c) * (f - 1);
    out[id] = w.copy(s: s, dx: w.dx + shift.dx / box.width, dy: w.dy + shift.dy / box.height);
  }
  return out;
}

/// Left for right: every control's centre reflected across the middle.
Tweaks mirrored(GLayout l, String o, double k, Size box, Tweaks t) {
  final now = placeTweaked(l, o, k, box, t), base = place(l, o, k, box);
  return {
    for (final c in l.controls)
      c.id: (t[c.id] ?? const Tweak()).copy(dx: (box.width - now[c.id]!.center.dx - base[c.id]!.center.dx) / box.width),
  };
}
