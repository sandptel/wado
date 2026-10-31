// The gamepad (port of js/gpad/send_pad.js + the Standard layout): a controller on the computer,
// one per phone, sessionless over the pad link — `game_on`, `btn {code, down}`, and `axes`
// frames (sticks ±32767, triggers 0..255) resent every 100 ms while a stick is off centre.
// The controls come from the chosen layout (gpad_layouts.dart), placed for the way the phone is held.
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../motion/feel.dart';

import '../../core/core_host.dart';
import '../../wado/nav.dart';
import '../../wado/gpad_layouts.dart';
import '../../wado/pad.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';

const _code = {
  'a': 0x130, 'b': 0x131, 'x': 0x133, 'y': 0x134, 'lb': 0x136, 'rb': 0x137, 'lt': 0x138, 'rt': 0x139, //
  'back': 0x13a, 'start': 0x13b, 'guide': 0x13c, 'l3': 0x13d, 'r3': 0x13e, 'du': 0x220, 'dd': 0x221, 'dl': 0x222, 'dr': 0x223,
};

class _Game {
  /// A gamepad that sends nothing: thumbnails.
  static final idle = _Game()..quiet = true;
  bool quiet = false;
  int _seq = 0;
  final axes = <String, double>{'lx': 0, 'ly': 0, 'rx': 0, 'ry': 0, 'lt': 0, 'rt': 0};
  final held = <String>{};
  Timer? _keep;

  Map<String, dynamic> _frame() => {
        'k': 'axes',
        'seq': _seq = (_seq + 1) & 0xffffffff,
        'a': [for (final k in ['lx', 'ly', 'rx', 'ry']) (axes[k]! * 32767).round(), for (final k in ['lt', 'rt']) (axes[k]! * 255).round()],
      };

  void start() => Pad.I.send({'k': 'game_on'});
  void stop() {
    _keep?.cancel();
    Pad.I.send({'k': 'game_off'});
  }

  void resend() {
    start();
    for (final b in held) {
      Pad.I.send({'k': 'btn', 'code': _code[b], 'down': true});
    }
    Pad.I.send(_frame(), lossy: true);
  }

  void button(String b, bool down) {
    if (quiet) return;
    if (!_code.containsKey(b)) return;
    down ? held.add(b) : held.remove(b);
    Pad.I.send({'k': 'btn', 'code': _code[b], 'down': down});
    if (down) HapticFeedback.lightImpact();
  }

  void axis(String k, double v) {
    if (quiet) return;
    axes[k] = v;
    Pad.I.send(_frame(), lossy: true);
    _keep?.cancel();
    if (axes.values.any((x) => x != 0)) {
      _keep = Timer.periodic(const Duration(milliseconds: 100), (_) => Pad.I.send(_frame(), lossy: true));
    }
  }
}

class GamepadScreen extends StatefulWidget {
  const GamepadScreen({super.key});
  @override
  State<GamepadScreen> createState() => _GamepadScreenState();
}

class _GamepadScreenState extends State<GamepadScreen> {
  final g = _Game();
  int _player = 1;

  @override
  void initState() {
    super.initState();
    active(true);
    Pad.I.start();
    Pad.I.onResend = g.resend;
    Pad.I.onMsg(_msg);
    g.start();
  }

  void _msg(Map<String, dynamic> m) {
    if (m['k'] == 'game' && mounted) setState(() => _player = ((m['player'] ?? 1) as num).toInt());
  }

  @override
  void dispose() {
    g.stop();
    Pad.I.stop();
    active(false);
    super.dispose();
  }

  void _pick() => showModalBottomSheet(
        context: context,
        isScrollControlled: true,
        backgroundColor: Tok.of(context).s0,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(Tok.of(context).r4))),
        builder: (c) => _Picker(onPick: (id) {
          Settings.I.set('gpad_layout', id);
          Navigator.pop(c);
          setState(() {});
        }),
      );

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final name = CoreHost.I.computer(Nav.I.selected)?.name ?? 'the computer';
    final layout = layoutById(Settings.I.s('gpad_layout'));
    return Scaffold(
      backgroundColor: t.s0,
      body: DecoratedBox(
        // `.gpwash`: a faint light from below.
        decoration: BoxDecoration(gradient: RadialGradient(center: const Alignment(0, 1.4), radius: 1.2, colors: [fade(t.accent, 94), fade(t.accent, 100)])),
        child: SafeArea(
          child: LayoutBuilder(builder: (context, box) {
            final size = box.biggest;
            final o = orient(size);
            return Stack(children: [
              Positioned.fill(child: GpadView(layout: layout, g: g, o: o, k: 1)),
              // The bar: close, which computer, which player; the layouts.
              Positioned(
                top: 8,
                left: 0,
                right: 0,
                child: Center(
                  child: Row(mainAxisSize: MainAxisSize.min, children: [
                    Container(
                      height: 52,
                      padding: const EdgeInsets.fromLTRB(4, 4, 12, 4),
                      decoration: BoxDecoration(color: fade(t.s1, 10), borderRadius: BorderRadius.circular(999)),
                      child: Row(mainAxisSize: MainAxisSize.min, children: [
                        IconBtn('x', label: 'Close gamepad', color: t.bright, onTap: () => Navigator.of(context).maybePop()),
                        const SizedBox(width: 6),
                        ListenableBuilder(
                          listenable: Pad.I.state,
                          builder: (context, _) {
                            final s = Pad.I.state.value;
                            return Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: s == 'direct' ? t.good : (s == 'relayed' ? t.warn : t.dim)));
                          },
                        ),
                        const SizedBox(width: 8),
                        Text(name, style: body(context, size: .9375, w: FontWeight.w600, color: t.bright)),
                        const SizedBox(width: 8),
                        Container(
                          padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
                          decoration: BoxDecoration(color: fade(t.accent, 80), borderRadius: BorderRadius.circular(8)),
                          child: Text('P$_player', style: mono(context, size: .8125, w: FontWeight.w600, color: t.accent)),
                        ),
                      ]),
                    ),
                    const SizedBox(width: 10),
                    Container(
                      height: 52,
                      padding: const EdgeInsets.all(4),
                      decoration: BoxDecoration(color: fade(t.s1, 10), borderRadius: BorderRadius.circular(999)),
                      child: IconBtn('apps', label: 'Layouts', color: t.bright, onTap: _pick),
                    ),
                  ]),
                ),
              ),
              // What went wrong, under the bar (`.gperr`).
              Positioned(
                left: 0,
                right: 0,
                top: 70,
                child: ValueListenableBuilder(
                  valueListenable: Pad.I.error,
                  builder: (context, err, _) => err == null || err.isEmpty
                      ? const SizedBox.shrink()
                      : Center(
                          child: Container(
                            constraints: const BoxConstraints(maxWidth: 520),
                            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                            decoration: BoxDecoration(color: Color.alphaBlend(fade(t.bad, 86), t.s1), borderRadius: BorderRadius.circular(t.r2), border: Border.all(color: fade(t.bad, 60))),
                            child: Text(padWhy(err).title.replaceAll('trackpad', 'gamepad'), textAlign: TextAlign.center, style: body(context, size: .8125, color: t.bright)),
                          ),
                        ),
                ),
              ),
            ]);
          }),
        ),
      ),
    );
  }
}

/// A layout drawn into its box: each control at its place. `g` null: a still thumbnail.
class GpadView extends StatelessWidget {
  const GpadView({super.key, required this.layout, required this.o, required this.k, this.g});
  final GLayout layout;
  final String o;
  final double k;
  final _Game? g;
  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(builder: (context, box) {
      final at = place(layout, o, k, box.biggest);
      return IgnorePointer(
        ignoring: g == null,
        child: Stack(children: [
          for (final c in layout.controls) Positioned.fromRect(rect: at[c.id]!, child: _Control(c: c, layout: layout, g: g ?? _Game.idle)),
        ]),
      );
    });
  }
}

/// The face buttons' colours (`labels: colors`) and the shapes' (`labels: shapes`).
Color? _tint(Tokens t, GLayout l, GControl c) {
  if (c.kind != 'btn' || l.labels == 'letters') return null;
  return switch (c.bind) { 'a' => t.base[l.labels == 'shapes' ? 0xD : 0xB], 'b' => t.base[8], 'x' => t.base[l.labels == 'shapes' ? 0xE : 0xD], 'y' => t.base[l.labels == 'shapes' ? 0xC : 0xA], _ => null };
}

class _Control extends StatelessWidget {
  const _Control({required this.c, required this.layout, required this.g});
  final GControl c;
  final GLayout layout;
  final _Game g;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return LayoutBuilder(builder: (context, box) {
      final w = box.maxWidth, h = box.maxHeight;
      switch (c.kind) {
        case 'stick':
          return _Stick(g, c.bind == 'rs' ? 'rx' : 'lx', c.bind == 'rs' ? 'ry' : 'ly', c.bind == 'rs' ? 'r3' : 'l3', c.label, w);
        case 'dpad':
          return _Dpad(g, w, eight: c.dir == 8);
        case 'pedal':
          return _Pedal(g: g, bind: c.bind, label: c.label, w: w, h: h);
        case 'slider':
          return _Steer(g: g, w: w, h: h);
      }
      final tint = _tint(t, layout, c);
      final radius = switch (c.kind) { 'small' || 'bumper' => 999.0, 'trig' => t.r2, _ => layout.shape == 'round' ? t.r2 : 999.0 };
      Widget? glyph;
      if (layout.labels == 'shapes' && c.kind == 'btn' && tint != null) glyph = CustomPaint(size: Size.square(h * .38), painter: _Shape(c.bind, tint));
      return _Btn(g, c.bind, c.label, w, h, radius: radius, small: c.kind == 'small', color: tint, icon: c.word ? null : c.icon, glyph: glyph);
    });
  }
}

/// △ ○ ✕ □, drawn.
class _Shape extends CustomPainter {
  _Shape(this.bind, this.c);
  final String bind;
  final Color c;
  @override
  void paint(Canvas canvas, Size s) {
    final p = Paint()
      ..color = c
      ..style = PaintingStyle.stroke
      ..strokeWidth = s.width * .1
      ..strokeJoin = StrokeJoin.round
      ..strokeCap = StrokeCap.round;
    final r = Rect.fromLTWH(s.width * .1, s.height * .1, s.width * .8, s.height * .8);
    switch (bind) {
      case 'y':
        canvas.drawPath(Path()..addPolygon([r.topCenter + Offset(0, s.height * .05), r.bottomRight, r.bottomLeft], true), p);
      case 'b':
        canvas.drawCircle(r.center, r.width / 2, p);
      case 'a':
        canvas.drawLine(r.topLeft, r.bottomRight, p);
        canvas.drawLine(r.topRight, r.bottomLeft, p);
      default:
        canvas.drawRect(r.deflate(s.width * .04), p);
    }
  }

  @override
  bool shouldRepaint(_Shape o) => o.c != c || o.bind != bind;
}

/// `.gpc`: a control's face — filled, tinted by its colour for the face buttons, and sinking
/// into that colour while held.
class _Btn extends StatefulWidget {
  const _Btn(this.g, this.bind, this.label, this.w, this.h, {required this.radius, this.small = false, this.color, this.icon, this.glyph});
  final _Game g;
  final String bind, label;
  final double w, h, radius;
  final bool small;
  final Color? color;
  final String? icon;
  final Widget? glyph;
  @override
  State<_Btn> createState() => _BtnState();
}

class _BtnState extends State<_Btn> {
  bool down = false;
  void _set(bool d) {
    if (d == down) return;
    setState(() => down = d);
    widget.g.button(widget.bind, d);
    // Triggers are analog: full pull while held.
    if (widget.bind == 'lt' || widget.bind == 'rt') widget.g.axis(widget.bind, d ? 1 : 0);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = widget.color ?? (widget.small ? t.dim : t.bright);
    final rest = widget.color != null ? Color.lerp(t.s2, c, .28)! : t.s2;
    final fg = down ? t.s0 : c;
    return Listener(
      onPointerDown: (_) => _set(true),
      onPointerUp: (_) => _set(false),
      onPointerCancel: (_) => _set(false),
      child: AnimatedScale(
        scale: down ? .92 : 1,
        duration: t.durPress,
        child: AnimatedContainer(
          duration: t.durPress,
          alignment: Alignment.center,
          decoration: BoxDecoration(color: down ? Color.lerp(t.s2, widget.color ?? t.bright, .55) : rest, borderRadius: BorderRadius.circular(widget.radius)),
          child: widget.glyph ??
              (widget.icon != null
                  ? WIcon(widget.icon!, size: widget.h * .5, color: fg)
                  : Text(widget.label, maxLines: 1, style: TextStyle(fontFamily: Tokens.body, fontWeight: FontWeight.w700, fontSize: widget.h * (widget.small ? .27 : .3), letterSpacing: widget.h * .012, color: fg))),
        ),
      ),
    );
  }
}

/// `.gpdp`: one cross — the finger's direction from the hub presses the arms it points at
/// (four ways, or eight when diagonals are allowed).
class _Dpad extends StatefulWidget {
  const _Dpad(this.g, this.s, {this.eight = false});
  final _Game g;
  final double s;
  final bool eight;
  @override
  State<_Dpad> createState() => _DpadState();
}

class _DpadState extends State<_Dpad> {
  final _on = <String>{};

  void _at(Offset p) {
    final c = Offset(widget.s / 2, widget.s / 2), d = p - c;
    final next = <String>{};
    if (d.distance > widget.s * .1) {
      final a = math.atan2(d.dy, d.dx);
      final sector = ((a / (math.pi / 4)).round() + 8) % 8; // 0 right, 2 down, 4 left, 6 up
      if (widget.eight) {
        next.addAll(const [['dr'], ['dr', 'dd'], ['dd'], ['dd', 'dl'], ['dl'], ['dl', 'du'], ['du'], ['du', 'dr']][sector]);
      } else {
        next.add(d.dx.abs() > d.dy.abs() ? (d.dx > 0 ? 'dr' : 'dl') : (d.dy > 0 ? 'dd' : 'du'));
      }
    }
    for (final b in _on.difference(next)) {
      widget.g.button(b, false);
    }
    for (final b in next.difference(_on)) {
      widget.g.button(b, true);
    }
    setState(() => _on
      ..clear()
      ..addAll(next));
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = widget.s, a = s * .34, o = s * .33;
    final r = Radius.circular(t.r1), k = const Radius.circular(4);
    Widget arm(String id, BorderRadius radius, int q) {
      final on = _on.contains(id);
      return AnimatedScale(
        scale: on ? .94 : 1,
        duration: t.durPress,
        child: Container(
          width: a,
          height: a,
          decoration: BoxDecoration(color: on ? Color.lerp(t.s2, t.accent, .7) : t.s2, borderRadius: radius),
          child: Center(child: RotatedBox(quarterTurns: q, child: CustomPaint(size: Size.square(a * .4), painter: _Tri(on ? t.onAccent : t.dim)))),
        ),
      );
    }

    return Listener(
      behavior: HitTestBehavior.opaque,
      onPointerDown: (e) => _at(e.localPosition),
      onPointerMove: (e) => _at(e.localPosition),
      onPointerUp: (_) => _at(Offset(s / 2, s / 2)),
      onPointerCancel: (_) => _at(Offset(s / 2, s / 2)),
      child: SizedBox(
        width: s,
        height: s,
        child: Stack(children: [
          Positioned(left: o, top: o, width: a, height: a, child: ColoredBox(color: t.s2.withValues(alpha: .92))),
          Positioned(left: o, top: 0, child: arm('du', BorderRadius.only(topLeft: r, topRight: r, bottomLeft: k, bottomRight: k), 0)),
          Positioned(left: o, bottom: 0, child: arm('dd', BorderRadius.only(bottomLeft: r, bottomRight: r, topLeft: k, topRight: k), 2)),
          Positioned(left: 0, top: o, child: arm('dl', BorderRadius.only(topLeft: r, bottomLeft: r, topRight: k, bottomRight: k), 3)),
          Positioned(right: 0, top: o, child: arm('dr', BorderRadius.only(topRight: r, bottomRight: r, topLeft: k, bottomLeft: k), 1)),
        ]),
      ),
    );
  }
}

class _Tri extends CustomPainter {
  _Tri(this.c);
  final Color c;
  @override
  void paint(Canvas canvas, Size s) {
    canvas.drawPath(
      Path()
        ..moveTo(s.width / 2, s.height * .22)
        ..lineTo(s.width * .82, s.height * .74)
        ..lineTo(s.width * .18, s.height * .74)
        ..close(),
      Paint()..color = c,
    );
  }

  @override
  bool shouldRepaint(_Tri o) => o.c != c;
}

/// `.gpc[data-kind=stick]`: a base, and a thumb with its letter that follows the finger.
class _Stick extends StatefulWidget {
  const _Stick(this.g, this.ax, this.ay, this.click, this.letter, this.s);
  final _Game g;
  final String ax, ay, click, letter;
  final double s;
  @override
  State<_Stick> createState() => _StickState();
}

class _StickState extends State<_Stick> {
  Offset knob = Offset.zero;
  bool on = false;
  void _at(Offset p) {
    final r = widget.s / 2;
    var d = p - Offset(r, r);
    if (d.distance > r) d = d / d.distance * r;
    setState(() {
      knob = d;
      on = true;
    });
    widget.g.axis(widget.ax, d.dx / r);
    widget.g.axis(widget.ay, d.dy / r);
  }

  void _end() {
    setState(() {
      knob = Offset.zero;
      on = false;
    });
    widget.g.axis(widget.ax, 0);
    widget.g.axis(widget.ay, 0);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = widget.s;
    return GestureDetector(
      onPanStart: (d) => _at(d.localPosition),
      onPanUpdate: (d) => _at(d.localPosition),
      onPanEnd: (_) => _end(),
      onPanCancel: _end,
      onDoubleTap: () {
        widget.g.button(widget.click, true);
        widget.g.button(widget.click, false);
      },
      child: Container(
        decoration: BoxDecoration(shape: BoxShape.circle, color: fade(t.s2, 30), border: Border.all(color: t.line, width: 1.5)),
        child: Center(
          child: Transform.translate(
            offset: knob,
            child: AnimatedContainer(
              duration: t.durPress,
              width: s * .46,
              height: s * .46,
              alignment: Alignment.center,
              decoration: BoxDecoration(shape: BoxShape.circle, color: on ? Color.lerp(t.s3, t.accent, .6) : t.s3, border: Border.all(color: fade(t.bright, 70), width: 1.5)),
              child: Text(widget.letter, style: TextStyle(fontFamily: Tokens.body, fontSize: s * .1, fontWeight: FontWeight.w700, color: on ? t.onAccent : t.dim)),
            ),
          ),
        ),
      ),
    );
  }
}

/// `.gpc[data-kind=pedal]`: an analog trigger — how far up the finger is, the fill and the axis.
class _Pedal extends StatefulWidget {
  const _Pedal({required this.g, required this.bind, required this.label, required this.w, required this.h});
  final _Game g;
  final String bind, label;
  final double w, h;
  @override
  State<_Pedal> createState() => _PedalState();
}

class _PedalState extends State<_Pedal> {
  double v = 0;
  void _at(double y) {
    final nv = (1 - y / widget.h).clamp(0.0, 1.0);
    if ((nv > 0) != (v > 0)) widget.g.button(widget.bind, nv > 0);
    setState(() => v = nv);
    widget.g.axis(widget.bind, nv);
  }

  void _end() {
    if (v > 0) widget.g.button(widget.bind, false);
    setState(() => v = 0);
    widget.g.axis(widget.bind, 0);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Listener(
      onPointerDown: (e) => _at(e.localPosition.dy),
      onPointerMove: (e) => _at(e.localPosition.dy),
      onPointerUp: (_) => _end(),
      onPointerCancel: (_) => _end(),
      child: Container(
        clipBehavior: Clip.antiAlias,
        decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r3)),
        child: Stack(children: [
          Positioned(left: 0, right: 0, bottom: 0, height: widget.h * v, child: ColoredBox(color: fade(t.accent, 70))),
          Positioned(top: 14, left: 0, right: 0, child: Center(child: Opacity(opacity: .6, child: WIcon('up', size: 24, color: t.bright)))),
          Positioned(left: 0, right: 0, bottom: 12, child: Text(widget.label.toUpperCase(), textAlign: TextAlign.center, style: TextStyle(fontFamily: Tokens.body, fontWeight: FontWeight.w700, fontSize: widget.h * .1, letterSpacing: 1, color: t.bright))),
        ]),
      ),
    );
  }
}

/// `.gpc[data-kind=slider]`: steering — the finger's place along it is the left stick's x.
class _Steer extends StatefulWidget {
  const _Steer({required this.g, required this.w, required this.h});
  final _Game g;
  final double w, h;
  @override
  State<_Steer> createState() => _SteerState();
}

class _SteerState extends State<_Steer> {
  double x = 0;
  void _at(double px) {
    setState(() => x = ((px / widget.w) * 2 - 1).clamp(-1.0, 1.0));
    widget.g.axis('lx', x);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final tw = widget.w * .3;
    return Listener(
      onPointerDown: (e) => _at(e.localPosition.dx),
      onPointerMove: (e) => _at(e.localPosition.dx),
      onPointerUp: (_) => _at(widget.w / 2),
      onPointerCancel: (_) => _at(widget.w / 2),
      child: Container(
        decoration: BoxDecoration(color: fade(t.s2, 30), borderRadius: BorderRadius.circular(999), border: Border.all(color: t.line, width: 1.5)),
        child: Stack(children: [
          Positioned(
            left: (widget.w - tw) / 2 + x * (widget.w - tw) / 2,
            top: widget.h * .1,
            width: tw,
            height: widget.h * .8,
            child: Container(decoration: BoxDecoration(color: x != 0 ? Color.lerp(t.s3, t.accent, .6) : t.s3, borderRadius: BorderRadius.circular(999))),
          ),
        ]),
      ),
    );
  }
}

/// The layouts (picker.js): each drawn small, the one in use ringed.
class _Picker extends StatelessWidget {
  const _Picker({required this.onPick});
  final ValueChanged<String> onPick;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final now = layoutById(Settings.I.s('gpad_layout')).id;
    final o = orient(MediaQuery.sizeOf(context));
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          Center(child: Container(width: 38, height: 5, margin: const EdgeInsets.only(bottom: 12), decoration: BoxDecoration(color: t.s3, borderRadius: BorderRadius.circular(3)))),
          Text('Layouts', style: display(context, rem(context, 1.25))),
          const SizedBox(height: 12),
          Flexible(
            child: SingleChildScrollView(
              child: LayoutBuilder(builder: (context, box) {
                final cols = box.maxWidth > 700 ? 4 : (box.maxWidth > 420 ? 3 : 2);
                final w = (box.maxWidth - 10 * (cols - 1)) / cols;
                final th = Size(w - 16, (w - 16) * (o == 'l' ? 370 / 824 : 824 / 370) * (o == 'l' ? 1 : .5));
                return Wrap(spacing: 10, runSpacing: 10, children: [
                  for (final l in layouts)
                    Press(
                      onTap: () => onPick(l.id),
                      label: '${l.name}: ${l.sub}',
                      child: Container(
                        width: w,
                        padding: const EdgeInsets.all(8),
                        decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3), border: Border.all(color: l.id == now ? t.accent : Colors.transparent, width: 2)),
                        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                          Container(
                            width: th.width,
                            height: th.height,
                            decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2)),
                            child: GpadView(layout: l, o: o, k: thumbK(o, th)),
                          ),
                          const SizedBox(height: 8),
                          Text(l.name, style: body(context, w: FontWeight.w600, color: t.bright)),
                          Text(l.sub, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, color: t.dim)),
                        ]),
                      ),
                    ),
                ]);
              }),
            ),
          ),
        ]),
      ),
    );
  }
}
