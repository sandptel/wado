// The gamepad (port of js/gpad/send_pad.js + the Standard layout): a controller on the computer,
// one per phone, sessionless over the pad link — `game_on`, `btn {code, down}`, and `axes`
// frames (sticks ±32767, triggers 0..255) resent every 100 ms while a stick is off centre.
// Landscape, two sticks, a D-pad, A B X Y, shoulders and triggers, Back · Guide · Start.
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../motion/feel.dart';

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

  @override
  void initState() {
    super.initState();
    SystemChrome.setPreferredOrientations([DeviceOrientation.landscapeLeft, DeviceOrientation.landscapeRight]);
    active(true);
    Pad.I.start();
    Pad.I.onResend = g.resend;
    g.start();
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
    return Scaffold(
      backgroundColor: t.s0,
      body: SafeArea(
        child: LayoutBuilder(builder: (context, box) {
          final h = box.maxHeight, w = box.maxWidth;
          final u = math.min(h / 5.2, w / 9);
          return Stack(children: [
            Positioned(left: 8, top: 4, child: IconBtn('left', label: 'Back', color: t.bright, onTap: () => Navigator.of(context).maybePop())),
            // Shoulders and triggers.
            Positioned(left: u * .6, top: u * .2, child: Row(children: [_Btn(g, 'lt', 'LT', u * 1.5, u * .7), SizedBox(width: u * .2), _Btn(g, 'lb', 'LB', u * 1.5, u * .7)])),
            Positioned(right: u * .6, top: u * .2, child: Row(children: [_Btn(g, 'rb', 'RB', u * 1.5, u * .7), SizedBox(width: u * .2), _Btn(g, 'rt', 'RT', u * 1.5, u * .7)])),
            // Left stick, D-pad.
            Positioned(left: u * .5, top: h * .32, child: _Stick(g, 'lx', 'ly', 'l3', u * 2)),
            Positioned(left: u * 2.9, bottom: u * .25, child: _Dpad(g, u * 1.9)),
            // Right: face buttons, right stick.
            Positioned(right: u * .5, top: h * .3, child: _Face(g, u * 2.2)),
            Positioned(right: u * 2.9, bottom: u * .25, child: _Stick(g, 'rx', 'ry', 'r3', u * 1.9)),
            // Middle.
            Positioned(
              left: 0,
              right: 0,
              top: h * .42,
              child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                _Btn(g, 'back', '⧉', u * .9, u * .6),
                SizedBox(width: u * .3),
                _Btn(g, 'guide', '◎', u * .8, u * .8, round: true),
                SizedBox(width: u * .3),
                _Btn(g, 'start', '≡', u * .9, u * .6),
              ]),
            ),
            Positioned(
              left: 0,
              right: 0,
              bottom: 6,
              child: ListenableBuilder(
                listenable: Listenable.merge([Pad.I.state, Pad.I.error]),
                builder: (context, _) => Text(
                  Pad.I.error.value ?? (Pad.I.state.value == 'direct' ? 'Gamepad · direct' : 'Gamepad · through the relay'),
                  textAlign: TextAlign.center,
                  style: body(context, size: .75, color: Pad.I.error.value != null ? t.bad : t.dim),
                ),
              ),
            ),
          ]);
        }),
      ),
    );
  }
}

class _Btn extends StatefulWidget {
  const _Btn(this.g, this.bind, this.label, this.w, this.h, {this.round = false, this.color});
  final _Game g;
  final String bind, label;
  final double w, h;
  final bool round;
  final Color? color;
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
    final c = widget.color ?? t.bright;
    return Listener(
      onPointerDown: (_) => _set(true),
      onPointerUp: (_) => _set(false),
      onPointerCancel: (_) => _set(false),
      child: AnimatedScale(
        scale: down ? .92 : 1,
        duration: t.durPress,
        child: Container(
          width: widget.w,
          height: widget.h,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: down ? fade(c, 60) : t.s2,
            borderRadius: BorderRadius.circular(widget.round ? widget.h : t.r2),
            border: Border.all(color: fade(c, 50), width: 1.5),
          ),
          child: Text(widget.label, style: TextStyle(fontFamily: Tokens.display, fontWeight: FontWeight.w700, fontSize: widget.h * .38, color: c)),
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
    final b = s * .38;
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

class _Dpad extends StatelessWidget {
  const _Dpad(this.g, this.s);
  final _Game g;
  final double s;
  @override
  Widget build(BuildContext context) {
    final b = s * .36;
    return SizedBox(
      width: s,
      height: s,
      child: Stack(children: [
        Positioned(top: 0, left: (s - b) / 2, child: _Btn(g, 'du', '▲', b, b)),
        Positioned(bottom: 0, left: (s - b) / 2, child: _Btn(g, 'dd', '▼', b, b)),
        Positioned(left: 0, top: (s - b) / 2, child: _Btn(g, 'dl', '◀', b, b)),
        Positioned(right: 0, top: (s - b) / 2, child: _Btn(g, 'dr', '▶', b, b)),
      ]),
    );
  }
}

class _Stick extends StatefulWidget {
  const _Stick(this.g, this.ax, this.ay, this.click, this.s);
  final _Game g;
  final String ax, ay, click;
  final double s;
  @override
  State<_Stick> createState() => _StickState();
}

class _StickState extends State<_Stick> {
  Offset knob = Offset.zero;
  void _at(Offset p) {
    final r = widget.s / 2;
    var d = p - Offset(r, r);
    if (d.distance > r) d = d / d.distance * r;
    setState(() => knob = d);
    widget.g.axis(widget.ax, d.dx / r);
    widget.g.axis(widget.ay, d.dy / r);
  }

  void _end() {
    setState(() => knob = Offset.zero);
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
        decoration: BoxDecoration(shape: BoxShape.circle, color: t.s1, border: Border.all(color: t.line, width: 1.5)),
        child: Center(
          child: Transform.translate(
            offset: knob,
            child: Container(width: s * .42, height: s * .42, decoration: BoxDecoration(shape: BoxShape.circle, color: t.s3, border: Border.all(color: fade(t.accent, 40), width: 2))),
          ),
        ),
      ),
    );
  }
}
