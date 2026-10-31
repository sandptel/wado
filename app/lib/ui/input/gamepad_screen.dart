// The gamepad (port of js/gpad/send_pad.js + the Standard layout): a controller on the computer,
// one per phone, sessionless over the pad link — `game_on`, `btn {code, down}`, and `axes`
// frames (sticks ±32767, triggers 0..255) resent every 100 ms while a stick is off centre.
// Landscape, two sticks, a D-pad, A B X Y, shoulders and triggers, Back · Guide · Start.
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../motion/feel.dart';

import '../../core/core_host.dart';
import '../../wado/nav.dart';
import '../../wado/pad.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';

const _code = {
  'a': 0x130, 'b': 0x131, 'x': 0x133, 'y': 0x134, 'lb': 0x136, 'rb': 0x137, 'lt': 0x138, 'rt': 0x139, //
  'back': 0x13a, 'start': 0x13b, 'guide': 0x13c, 'l3': 0x13d, 'r3': 0x13e, 'du': 0x220, 'dd': 0x221, 'dl': 0x222, 'dr': 0x223,
};

class _Game {
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
    if (!_code.containsKey(b)) return;
    down ? held.add(b) : held.remove(b);
    Pad.I.send({'k': 'btn', 'code': _code[b], 'down': down});
    if (down) HapticFeedback.lightImpact();
  }

  void axis(String k, double v) {
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
    SystemChrome.setPreferredOrientations([DeviceOrientation.landscapeLeft, DeviceOrientation.landscapeRight]);
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
    SystemChrome.setPreferredOrientations([]);
    active(false);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final name = CoreHost.I.computer(Nav.I.selected)?.name ?? 'the computer';
    return Scaffold(
      backgroundColor: t.s0,
      body: DecoratedBox(
        // `.gpwash`: a faint light from below.
        decoration: BoxDecoration(gradient: RadialGradient(center: const Alignment(0, 1.4), radius: 1.2, colors: [fade(t.accent, 94), fade(t.accent, 100)])),
        child: SafeArea(
          child: LayoutBuilder(builder: (context, box) {
            final h = box.maxHeight, w = box.maxWidth;
            final u = math.min(h / 5.2, w / 9);
            return Stack(children: [
              // Shoulders (pills) and triggers (rounded).
              Positioned(left: u * .5, top: u * .25, child: Row(children: [_Btn(g, 'lt', 'LT', u * 1.25, u * .75), SizedBox(width: u * .3), _Btn(g, 'lb', 'LB', u * 1.25, u * .68, pill: true)])),
              Positioned(right: u * .5, top: u * .25, child: Row(children: [_Btn(g, 'rb', 'RB', u * 1.25, u * .68, pill: true), SizedBox(width: u * .3), _Btn(g, 'rt', 'RT', u * 1.25, u * .75)])),
              // Left stick, D-pad.
              Positioned(left: u * .75, top: h * .35, child: _Stick(g, 'lx', 'ly', 'l3', 'L', u * 1.9)),
              Positioned(left: u * 2.9, bottom: u * .45, child: _Dpad(g, u * 2.2)),
              // Right: face buttons, right stick.
              Positioned(right: u * .5, top: h * .33, child: _Face(g, u * 2.95)),
              Positioned(right: u * 3.75, bottom: u * .55, child: _Stick(g, 'rx', 'ry', 'r3', 'R', u * 1.9)),
              // Back · Guide · Start.
              Positioned(
                left: 0,
                right: 0,
                top: h * .32,
                child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                  _Btn(g, 'back', '', u * .95, u * .66, pill: true, small: true, icon: 'copy'),
                  SizedBox(width: u * .22),
                  _Btn(g, 'guide', '', u * .95, u * .66, pill: true, small: true, icon: 'house'),
                  SizedBox(width: u * .22),
                  _Btn(g, 'start', '≡', u * .95, u * .66, pill: true, small: true),
                ]),
              ),
              // The bar: close, which computer, which player.
              Positioned(
                top: 8,
                left: 0,
                right: 0,
                child: Center(
                  child: Container(
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

/// `.gpc`: a control's face — filled, tinted by its colour for the face buttons, and sinking
/// into that colour while held.
class _Btn extends StatefulWidget {
  const _Btn(this.g, this.bind, this.label, this.w, this.h, {this.round = false, this.pill = false, this.small = false, this.color, this.icon});
  final _Game g;
  final String bind, label;
  final double w, h;
  final bool round, pill, small;
  final Color? color;
  final String? icon;
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
          width: widget.w,
          height: widget.h,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: down ? Color.lerp(t.s2, widget.color ?? t.bright, .55) : rest,
            borderRadius: BorderRadius.circular(widget.round || widget.pill ? 999 : t.r2),
          ),
          child: widget.icon != null
              ? WIcon(widget.icon!, size: widget.h * .5, color: fg)
              : Text(widget.label, style: TextStyle(fontFamily: Tokens.body, fontWeight: FontWeight.w700, fontSize: widget.h * (widget.small ? .4 : .3), letterSpacing: widget.h * .012, color: fg)),
        ),
      ),
    );
  }
}

class _Face extends StatelessWidget {
  const _Face(this.g, this.s);
  final _Game g;
  final double s;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final b = s * .34;
    return SizedBox(
      width: s,
      height: s,
      child: Stack(children: [
        Positioned(top: 0, left: (s - b) / 2, child: _Btn(g, 'y', 'Y', b, b, round: true, color: t.base[0xA])),
        Positioned(bottom: 0, left: (s - b) / 2, child: _Btn(g, 'a', 'A', b, b, round: true, color: t.base[0xB])),
        Positioned(left: 0, top: (s - b) / 2, child: _Btn(g, 'x', 'X', b, b, round: true, color: t.base[0xD])),
        Positioned(right: 0, top: (s - b) / 2, child: _Btn(g, 'b', 'B', b, b, round: true, color: t.base[8])),
      ]),
    );
  }
}

/// `.gpdp`: one cross — four arms around a hub, each arm its own button with its triangle.
class _Dpad extends StatelessWidget {
  const _Dpad(this.g, this.s);
  final _Game g;
  final double s;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final a = s * .34, o = s * .33;
    final r = Radius.circular(t.r1), k = const Radius.circular(4);
    return SizedBox(
      width: s,
      height: s,
      child: Stack(children: [
        Positioned(left: o, top: o, width: a, height: a, child: ColoredBox(color: t.s2.withValues(alpha: .92))),
        Positioned(left: o, top: 0, child: _Arm(g, 'du', a, BorderRadius.only(topLeft: r, topRight: r, bottomLeft: k, bottomRight: k), 0)),
        Positioned(left: o, bottom: 0, child: _Arm(g, 'dd', a, BorderRadius.only(bottomLeft: r, bottomRight: r, topLeft: k, topRight: k), 2)),
        Positioned(left: 0, top: o, child: _Arm(g, 'dl', a, BorderRadius.only(topLeft: r, bottomLeft: r, topRight: k, bottomRight: k), 3)),
        Positioned(right: 0, top: o, child: _Arm(g, 'dr', a, BorderRadius.only(topRight: r, bottomRight: r, topLeft: k, bottomLeft: k), 1)),
      ]),
    );
  }
}

class _Arm extends StatefulWidget {
  const _Arm(this.g, this.bind, this.s, this.radius, this.quarter);
  final _Game g;
  final String bind;
  final double s;
  final BorderRadius radius;
  /// Which way its triangle points: 0 up, 1 right, 2 down, 3 left.
  final int quarter;
  @override
  State<_Arm> createState() => _ArmState();
}

class _ArmState extends State<_Arm> {
  bool down = false;
  void _set(bool d) {
    if (d == down) return;
    setState(() => down = d);
    widget.g.button(widget.bind, d);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Listener(
      onPointerDown: (_) => _set(true),
      onPointerUp: (_) => _set(false),
      onPointerCancel: (_) => _set(false),
      child: AnimatedScale(
        scale: down ? .94 : 1,
        duration: t.durPress,
        child: Container(
          width: widget.s,
          height: widget.s,
          decoration: BoxDecoration(color: down ? Color.lerp(t.s2, t.accent, .7) : t.s2, borderRadius: widget.radius),
          child: Center(
            child: RotatedBox(
              quarterTurns: widget.quarter,
              child: CustomPaint(size: Size.square(widget.s * .4), painter: _Tri(down ? t.onAccent : t.dim)),
            ),
          ),
        ),
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
        width: s,
        height: s,
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
              child: Text(widget.letter, style: body(context, size: .75, w: FontWeight.w700, color: on ? t.onAccent : t.dim)),
            ),
          ),
        ),
      ),
    );
  }
}
