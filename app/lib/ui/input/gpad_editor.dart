// The gamepad is its own editor (port of gpad/edit.js + inspector.js, the parts a phone needs):
// tap selects a control and, by default, its group; drag moves it 1:1 on an 8 px grid; the size
// slider grows or shrinks it (a group spreads around its centre; never under 44 px); Hide, Mirror,
// Undo / Redo (60 steps), Reset. Every change is saved as it is made (gpad_edits.dart).
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../wado/gpad_edits.dart';
import '../../wado/gpad_layouts.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/slider.dart';
import 'gamepad_screen.dart' show GpadView;

class GpadEditor extends StatefulWidget {
  const GpadEditor({super.key, required this.layout, required this.o, required this.onDone});
  final GLayout layout;
  final String o;
  final VoidCallback onDone;
  @override
  State<GpadEditor> createState() => _GpadEditorState();
}

class _GpadEditorState extends State<GpadEditor> {
  late Tweaks _t = tweaksFor(widget.layout.id, widget.o);
  final _undo = <Tweaks>[], _redo = <Tweaks>[];
  String? _sel;
  bool _together = true;
  /// A drag or a slide in progress: where it started, and the drag's unsnapped travel.
  Tweaks? _from;
  Offset _travel = Offset.zero;

  List<String> get _ids {
    if (_sel == null) return const [];
    if (!_together) return [_sel!];
    final g = widget.layout.controls.firstWhere((c) => c.id == _sel).g;
    return [for (final c in widget.layout.controls) if (c.g == g) c.id];
  }

  void _commit(Tweaks before, Tweaks next) {
    _undo.add(before);
    if (_undo.length > 60) _undo.removeAt(0);
    _redo.clear();
    setState(() => _t = next);
    saveTweaks(widget.layout.id, widget.o, next);
  }

  void _step(List<Tweaks> from, List<Tweaks> to) {
    if (from.isEmpty) return;
    to.add(_t);
    setState(() => _t = from.removeLast());
    saveTweaks(widget.layout.id, widget.o, _t);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return LayoutBuilder(builder: (context, box) {
      final size = box.biggest;
      final at = placeTweaked(widget.layout, widget.o, 1, size, _t);
      final ids = _ids;
      final sel = _sel == null ? null : (_t[_sel!] ?? const Tweak());
      final low = _sel != null && at[_sel!]!.center.dy > size.height * .55;
      return Stack(children: [
        // Tap on nothing: nothing selected.
        Positioned.fill(child: GestureDetector(behavior: HitTestBehavior.opaque, onTap: () => setState(() => _sel = null))),
        // A faint 8 px grid under the controls while editing.
        Positioned.fill(child: IgnorePointer(child: CustomPaint(painter: _Grid(fade(t.line, 40))))),
        Positioned.fill(child: GpadView(layout: widget.layout, o: widget.o, k: 1, tweaks: _t, editing: true)),
        for (final c in widget.layout.controls)
          Positioned.fromRect(
            rect: at[c.id]!,
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: () {
                HapticFeedback.selectionClick();
                setState(() => _sel = c.id);
              },
              onPanStart: (_) {
                if (!ids.contains(c.id)) setState(() => _sel = c.id);
                _from = _t;
                _travel = Offset.zero;
              },
              onPanUpdate: (d) {
                _travel += d.delta;
                final snap = Offset((_travel.dx / 8).roundToDouble() * 8, (_travel.dy / 8).roundToDouble() * 8);
                setState(() => _t = moved(_from!, _ids, snap, size));
              },
              onPanEnd: (_) {
                final from = _from!;
                _from = null;
                if (_travel.distance > 0) _commit(from, _t);
              },
              child: Semantics(
                label: '${c.label.isEmpty ? c.bind : c.label}${ids.contains(c.id) ? ', selected' : ''}',
                child: AnimatedContainer(
                  duration: t.dur,
                  decoration: BoxDecoration(
                    borderRadius: BorderRadius.circular(14),
                    border: Border.all(color: ids.contains(c.id) ? (c.id == _sel ? t.accent : fade(t.accent, 45)) : Colors.transparent, width: 2),
                  ),
                ),
              ),
            ),
          ),
        // The strip: what can be done to the selection, then to the whole layout — at the top
        // when the selection is low, so it never covers what is being changed. It glides there
        // rather than jumping, and the selection's row unfolds into it.
        Positioned.fill(
          child: AnimatedAlign(
            duration: t.durSheet,
            curve: Tokens.spring,
            alignment: low ? Alignment.topCenter : Alignment.bottomCenter,
            child: Padding(
            padding: const EdgeInsets.all(8),
            child: Container(
              constraints: const BoxConstraints(maxWidth: 720),
              padding: const EdgeInsets.fromLTRB(8, 6, 8, 6),
              decoration: BoxDecoration(color: fade(t.s1, 6), borderRadius: BorderRadius.circular(t.r4), border: Border.all(color: t.line)),
              child: AnimatedSize(
                duration: t.dur,
                curve: Tokens.spring,
                alignment: low ? Alignment.topCenter : Alignment.bottomCenter,
                child: Column(mainAxisSize: MainAxisSize.min, children: [
                if (sel != null)
                  Row(children: [
                    Expanded(
                      child: ThickSlider(
                        label: 'Size',
                        thin: true,
                        value: sel.s,
                        min: .6,
                        max: 1.8,
                        step: .05,
                        shown: '${(sel.s * 100).round()}%',
                        onInput: (v) {
                          _from ??= _t;
                          setState(() => _t = sized(_t, _ids, v, at, size));
                        },
                        onChange: (v) {
                          final from = _from ?? _t;
                          _from = null;
                          _commit(from, sized(_t, _ids, v, at, size));
                        },
                      ),
                    ),
                    const SizedBox(width: 6),
                    _Toggle(icon: 'layers', label: 'Move together', on: _together, onTap: () => setState(() => _together = !_together)),
                    _Toggle(icon: 'eye', label: sel.hide ? 'Show' : 'Hide', on: sel.hide, onTap: () => _commit(_t, {..._t, for (final id in _ids) id: (_t[id] ?? const Tweak()).copy(hide: !sel.hide)})),
                  ]),
                // Done never scrolls away; the rest scrolls beside it on a narrow phone.
                Row(children: [
                  Expanded(
                    child: SingleChildScrollView(
                  scrollDirection: Axis.horizontal,
                  child: Row(mainAxisSize: MainAxisSize.min, children: [
                    // Moving without dragging (WCAG 2.5.7): 8 px a tap, the grid's own step.
                    if (sel != null)
                      for (final (icon, name, d) in const [('left', 'Nudge left', Offset(-8, 0)), ('up', 'Nudge up', Offset(0, -8)), ('down', 'Nudge down', Offset(0, 8)), ('right', 'Nudge right', Offset(8, 0))])
                        IconBtn(icon, label: name, color: t.bright, onTap: () => _commit(_t, moved(_t, _ids, d, size))),
                    TextButton(onPressed: _undo.isEmpty ? null : () => _step(_undo, _redo), child: const Text('Undo')),
                    TextButton(onPressed: _redo.isEmpty ? null : () => _step(_redo, _undo), child: const Text('Redo')),
                    TextButton(onPressed: () => _commit(_t, mirrored(widget.layout, widget.o, 1, size, _t)), child: const Text('Mirror')),
                    TextButton(onPressed: _t.isEmpty ? null : () => _commit(_t, {}), child: const Text('Reset')),
                  ]),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Btn('Done', primary: true, expand: false, onTap: widget.onDone),
                ]),
              ]),
              ),
            ),
            ),
          ),
        ),
        if (_sel == null)
          Positioned(
            top: 12,
            left: 0,
            right: 0,
            child: IgnorePointer(child: Text('Tap a control to change it · drag to move it', textAlign: TextAlign.center, style: body(context, size: .8125, color: t.dim))),
          ),
      ]);
    });
  }
}

class _Toggle extends StatelessWidget {
  const _Toggle({required this.icon, required this.label, required this.on, required this.onTap});
  final String icon, label;
  final bool on;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Press(
      onTap: onTap,
      label: label,
      child: Container(
        height: 44,
        margin: const EdgeInsets.only(left: 4),
        padding: const EdgeInsets.symmetric(horizontal: 10),
        decoration: BoxDecoration(color: on ? fade(t.accent, 80) : t.s2, borderRadius: BorderRadius.circular(999)),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          WIcon(icon, size: 16, color: on ? t.accent : t.bright),
          const SizedBox(width: 6),
          Text(label, style: body(context, size: .8125, w: FontWeight.w600, color: on ? t.accent : t.bright)),
        ]),
      ),
    );
  }
}

class _Grid extends CustomPainter {
  _Grid(this.c);
  final Color c;
  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()..color = c;
    for (var x = 0.0; x < size.width; x += 32) {
      for (var y = 0.0; y < size.height; y += 32) {
        canvas.drawCircle(Offset(x, y), 1, p);
      }
    }
  }

  @override
  bool shouldRepaint(_Grid o) => o.c != c;
}
