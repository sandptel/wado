// The step-up sheet (port of ui/passkey/mod.rs + approve.rs + glyph.rs + passkey.css): an action
// that needs the passkey raises it; it says what the key unlocks here, tries the key, offers
// Approve on the computer instead, and on success says "Unlocked" for a moment as the lock's
// shackle swings open — then goes, and the action runs.
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/passkey.dart';
import '../motion/motion.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'passkey_card.dart' show stayWords;

String _lead(String need, String name) => switch (need) {
      'shell' => 'Shells on $name open with your passkey.',
      'pad' => 'The trackpad and keyboard for $name need your passkey.',
      'files' => 'Changing files on $name needs your passkey. Looking around stays free.',
      'allow' => 'Letting Claude go ahead needs your passkey.',
      'claude' => "Claude's shell opens with your passkey.",
      'new' => 'Starting Claude needs your passkey.',
      'again' => '$name locked itself while you were away. Unlock to carry on.',
      _ => 'Your screen lock — fingerprint, face or PIN — becomes the key for shells, file changes and the trackpad on $name.',
    };

class PasskeySheet extends StatefulWidget {
  const PasskeySheet({super.key});
  @override
  State<PasskeySheet> createState() => _PasskeySheetState();
}

class _PasskeySheetState extends State<PasskeySheet> with SingleTickerProviderStateMixin {
  late final _up = AnimationController(vsync: this, duration: const Duration(milliseconds: 480));
  bool _cheer = false;
  String _need = '';
  int _shown = 0;

  @override
  void initState() {
    super.initState();
    Passkey.I.need.addListener(_sync);
    Passkey.I.opened.addListener(_open);
  }

  @override
  void dispose() {
    Passkey.I.need.removeListener(_sync);
    Passkey.I.opened.removeListener(_open);
    _up.dispose();
    super.dispose();
  }

  void _sync() {
    final n = Passkey.I.need.value;
    if (n.isNotEmpty) {
      setState(() {
        _need = n;
        _shown++;
      });
      _up.animateTo(1, curve: Tokens.spring);
    } else if (!_cheer) {
      _up.animateBack(0, duration: const Duration(milliseconds: 384), curve: Tokens.spring);
    }
  }

  void _open() {
    setState(() => _cheer = true);
    Timer(const Duration(milliseconds: 760), () {
      if (!mounted) return;
      setState(() => _cheer = false);
      if (Passkey.I.need.value.isEmpty) _up.animateBack(0, duration: const Duration(milliseconds: 384), curve: Tokens.spring);
    });
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width >= 720;
    return AnimatedBuilder(
      animation: _up,
      builder: (context, _) {
        final v = _up.value;
        if (v == 0) return const SizedBox.shrink();
        final sheet = ValueListenableBuilder(
          valueListenable: Passkey.I.view,
          builder: (context, st, _) => _body(context, st),
        );
        return Stack(children: [
          Positioned.fill(
            child: GestureDetector(
              onTap: Passkey.I.cancel,
              child: ColoredBox(color: Color.fromRGBO(0, 0, 0, .45 * v.clamp(0, 1))),
            ),
          ),
          Align(
            alignment: wide ? Alignment.center : Alignment.bottomCenter,
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 460),
              child: wide
                  ? Opacity(opacity: v.clamp(0, 1), child: Transform.translate(offset: Offset(0, 60 * (1 - v)), child: sheet))
                  : FractionalTranslation(translation: Offset(0, 1.05 * (1 - v)), child: sheet),
            ),
          ),
        ]);
      },
    );
  }

  Widget _body(BuildContext context, PasskeyView st) {
    final t = Tok.of(context);
    final mq = MediaQuery.of(context);
    final wide = mq.size.width >= 720;
    final name = CoreHost.I.computer(Nav.I.selected)?.name ?? 'the computer';
    final need = _need;
    final registered = st.registered;
    final fresh = need == 'paired' || need == 'add';
    final title = _cheer ? 'Unlocked' : (!registered ? (need == 'paired' ? 'One last step: a passkey' : 'Add a passkey') : 'Unlock with your passkey');
    final words = !registered && !fresh
        ? 'This phone has no passkey on $name yet. Add one now: your fingerprint, face or PIN becomes the key, and nothing about it leaves this phone.'
        : (fresh ? 'Nothing about it leaves this phone; the computer keeps only a public key.' : '');
    final failed = !st.busy && st.note.isNotEmpty;
    final button = st.busy ? 'Waiting for your passkey…' : (failed ? 'Try again' : (registered && need != 'add' ? 'Unlock' : 'Add a passkey'));
    final hatches = !fresh && !_cheer;
    final ap = st.approve;
    final device = '${(Host.I.config.value?['grants'] as Map?)?['name'] ?? '<this device>'}';
    var i = 0;
    // Content rises in after the sheet, a stagger step apart (`--i * --stagger + 60ms`).
    Widget rise(Widget w) => Enter(key: ValueKey('pk$_shown.${i++}'), delay: Duration(milliseconds: 60 + (i - 1).clamp(0, 4) * 40), ms: 480, dy: 10, fadeOnClose: false, child: w);
    final kids = <Widget>[
      if (!wide) Center(child: Container(width: 38, height: 5, margin: const EdgeInsets.symmetric(vertical: 8), decoration: BoxDecoration(color: t.s3, borderRadius: BorderRadius.circular(3)))) else const SizedBox(height: 10),
      rise(Row(children: [
        LockGlyph(busy: st.busy, open: _cheer),
        const SizedBox(width: 12),
        Expanded(child: Text(title, style: display(context, rem(context, 1.25), ls: -.2).copyWith(height: 1.15))),
      ])),
      rise(Text(_lead(need, name), style: body(context, size: .9375, color: t.bright))),
      if (words.isNotEmpty) rise(Text(words, style: body(context, color: t.text))),
      if (st.note.isNotEmpty) _Note(key: ValueKey(st.note), text: st.note),
      rise(Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Btn(button, primary: true, busy: st.busy, onTap: st.busy || _cheer ? null : () => !registered || need == 'add' ? Passkey.I.add() : Passkey.I.go()),
        if (hatches && ap?['state'] != 'waiting') ...[const SizedBox(height: 8), Btn('Approve on $name instead', onTap: Passkey.I.approveOnComputer)],
        const SizedBox(height: 8),
        Btn(fresh ? 'Later' : 'Not now', onTap: Passkey.I.cancel),
      ])),
      if (hatches && ap != null)
        if (ap['state'] == 'waiting')
          Container(
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: Color.alphaBlend(fade(t.accent, 88), t.s1),
              borderRadius: BorderRadius.circular(t.r2),
              border: Border.all(color: fade(t.accent, 70)),
            ),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('Waiting for someone at $name', style: body(context, w: FontWeight.w700, color: t.bright)),
              const SizedBox(height: 6),
              Text.rich(TextSpan(style: body(context, size: .8125), children: [
                const TextSpan(text: 'There, run '),
                TextSpan(text: 'wado passkey approve', style: mono(context, size: .8125)),
                const TextSpan(text: ' or answer in '),
                TextSpan(text: 'wado tui', style: mono(context, size: .8125)),
                const TextSpan(text: '. Only someone at the computer can say yes — nobody answering in 2 minutes counts as no.'),
              ])),
              const SizedBox(height: 6),
              Btn('Stop asking', expand: false, onTap: Passkey.I.cancel),
            ]),
          )
        else if (ap['state'] != 'approved')
          _Note(key: ValueKey('ap${ap['state']}'), text: '${ap['message'] ?? ''}'.isEmpty ? 'Not approved on the computer.' : '${ap['message']}'),
      if (hatches && (failed || ap != null))
        Text.rich(TextSpan(style: body(context, size: .75, color: t.dim), children: [
          TextSpan(text: 'Passkey never works on this phone? On $name, its owner can stop asking for it: '),
          TextSpan(text: 'wado passkey require $device off', style: mono(context, size: .75, color: t.dim)),
        ])),
      rise(Text(stayWords('${st.state['stay'] ?? '15m'}', st.state['lock_on_leave'] != false), style: body(context, size: .75, color: t.dim))),
    ];
    return Container(
      padding: EdgeInsets.fromLTRB(16, 0, 16, 18 + mq.padding.bottom),
      decoration: BoxDecoration(
        color: t.s0,
        borderRadius: wide ? BorderRadius.circular(t.r4) : BorderRadius.vertical(top: Radius.circular(t.r4)),
        boxShadow: const [BoxShadow(color: Color(0x59000000), blurRadius: 60, offset: Offset(0, -20))],
      ),
      child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        for (var j = 0; j < kids.length; j++) ...[if (j > 1) const SizedBox(height: 12), kids[j]],
      ]),
    );
  }
}

/// `.pknote`: what went wrong, arriving with a shake.
class _Note extends StatelessWidget {
  const _Note({super.key, required this.text});
  final String text;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return TweenAnimationBuilder<double>(
      tween: Tween(begin: 0, end: 1),
      duration: const Duration(milliseconds: 360),
      builder: (context, v, child) {
        // 0 → −8 → 6 → −3 → 0 at 0, 20, 45, 70, 100%.
        final x = v < .2 ? -8 * v / .2 : v < .45 ? -8 + 14 * (v - .2) / .25 : v < .7 ? 6 - 9 * (v - .45) / .25 : -3 + 3 * (v - .7) / .3;
        return Transform.translate(offset: Offset(x, 0), child: child);
      },
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        decoration: BoxDecoration(color: fade(t.bad, 86), borderRadius: BorderRadius.circular(t.r2)),
        child: Text(text, style: body(context, size: .8125, color: t.bad)),
      ),
    );
  }
}

/// The lock (glyph.rs): a 52 px well, the shackle its own stroke so it can tug while busy and
/// lift and swing open on an unlock while the body pops.
class LockGlyph extends StatefulWidget {
  const LockGlyph({super.key, required this.busy, required this.open});
  final bool busy, open;
  @override
  State<LockGlyph> createState() => _LockGlyphState();
}

class _LockGlyphState extends State<LockGlyph> with TickerProviderStateMixin {
  late final _tug = AnimationController(vsync: this, duration: const Duration(milliseconds: 900));
  late final _open = AnimationController(vsync: this, duration: const Duration(milliseconds: 480));

  @override
  void didUpdateWidget(LockGlyph old) {
    super.didUpdateWidget(old);
    widget.busy && !widget.open ? (_tug.isAnimating ? null : _tug.repeat()) : _tug.stop();
    if (widget.open != old.open) widget.open ? _open.forward(from: 0) : _open.reverse();
  }

  @override
  void initState() {
    super.initState();
    if (widget.busy) _tug.repeat();
  }

  @override
  void dispose() {
    _tug.dispose();
    _open.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = widget.open ? t.good : t.accent;
    return AnimatedContainer(
      duration: t.dur,
      curve: Tokens.spring,
      width: 52,
      height: 52,
      padding: const EdgeInsets.all(9),
      decoration: BoxDecoration(color: fade(c, widget.open ? 82 : 84), borderRadius: BorderRadius.circular(t.r2)),
      child: AnimatedBuilder(
        animation: Listenable.merge([_tug, _open]),
        builder: (context, _) {
          final tv = _tug.value;
          final tug = tv < .3 ? -2.5 * tv / .3 : tv < .6 ? -2.5 * (1 - (tv - .3) / .3) : 0.0;
          final o = Tokens.popCurve.transform(_open.value);
          final pop = _open.value < .4 ? 1 + .12 * _open.value / .4 : 1.12 - .12 * (_open.value - .4) / .6;
          return CustomPaint(painter: _Lock(c, tug - 5 * o, 28 * o * math.pi / 180, pop));
        },
      ),
    );
  }
}

class _Lock extends CustomPainter {
  _Lock(this.c, this.lift, this.swing, this.pop);
  final Color c;
  final double lift, swing, pop;
  @override
  void paint(Canvas canvas, Size size) {
    final k = size.width / 48;
    canvas.scale(k);
    final stroke = Paint()
      ..color = c
      ..style = PaintingStyle.stroke
      ..strokeWidth = 3.2
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    // The shackle, about its bottom-right corner (33, 21).
    canvas.save();
    canvas.translate(0, lift);
    canvas.translate(33, 21);
    canvas.rotate(swing);
    canvas.translate(-33, -21);
    canvas.drawPath(
      Path()
        ..moveTo(15, 21)
        ..lineTo(15, 15)
        ..arcToPoint(const Offset(33, 15), radius: const Radius.circular(9))
        ..lineTo(33, 21),
      stroke,
    );
    canvas.restore();
    // The body and keyhole pop about (24, 32).
    canvas.translate(24, 32);
    canvas.scale(pop);
    canvas.translate(-24, -32);
    canvas.drawRRect(RRect.fromRectAndRadius(const Rect.fromLTWH(9, 21, 30, 21), const Radius.circular(6)), stroke);
    canvas.drawCircle(const Offset(24, 31), 2.6, Paint()..color = c);
  }

  @override
  bool shouldRepaint(_Lock o) => o.c != c || o.lift != lift || o.swing != swing || o.pop != pop;
}
