// The clipboard's voice, over everything (port of ui/clipboard/dock.rs + clips.css #clipdock): one
// pill at the bottom that says what sharing did ("Sent to Laptop", "Copied to this phone from
// Laptop") for 2.8 s. Higher while a session runs, clear of the dock.
import 'package:flutter/material.dart';

import '../../wado/clip.dart';
import '../../wado/session.dart';
import '../motion/motion.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import 'basics.dart';

class ClipDock extends StatelessWidget {
  const ClipDock({super.key});
  @override
  Widget build(BuildContext context) {
    final mq = MediaQuery.of(context);
    return IgnorePointer(
      child: ListenableBuilder(
        listenable: Listenable.merge([Clip.I.said, Session.I.shown]),
        builder: (context, _) {
          final s = Clip.I.said.value;
          return Align(
            alignment: Alignment.bottomCenter,
            child: Padding(
              padding: EdgeInsets.only(bottom: (Session.I.shown.value ? 84 : 16) + mq.padding.bottom),
              child: s.n == 0 || s.text.isEmpty ? const SizedBox.shrink() : _Said(key: ValueKey(s.n), s: s),
            ),
          );
        },
      ),
    );
  }
}

/// `saidin` (480 ms, the spring, from 14 px down at .94) then `saidout` at 2620 ms (180 ms).
class _Said extends StatefulWidget {
  const _Said({super.key, required this.s});
  final Said s;
  @override
  State<_Said> createState() => _SaidState();
}

class _SaidState extends State<_Said> with SingleTickerProviderStateMixin {
  late final _c = AnimationController(vsync: this, duration: const Duration(milliseconds: 2800))..forward();
  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = widget.s;
    final (Color c, String icon) = switch (s.tone) { 'bad' => (t.bad, 'alert'), 'dim' => (t.dim, 'info'), _ => (t.good, 'check') };
    return AnimatedBuilder(
      animation: _c,
      builder: (context, child) {
        final ms = _c.value * 2800;
        final inP = Tokens.spring.transform((ms / 480).clamp(0, 1));
        final outP = const Cubic(.3, 0, 1, 1).transform(((ms - 2620) / 180).clamp(0, 1));
        return Opacity(
          opacity: (inP * (1 - outP)).clamp(0, 1),
          child: Transform.translate(
            offset: Offset(0, 14 * (1 - inP) + 6 * outP),
            child: Transform.scale(scale: .94 + .06 * inP, child: child),
          ),
        );
      },
      child: Container(
        constraints: const BoxConstraints(minHeight: 40, maxWidth: 440),
        padding: const EdgeInsets.fromLTRB(8, 6, 16, 6),
        decoration: BoxDecoration(
          color: t.s2,
          borderRadius: BorderRadius.circular(999),
          boxShadow: const [BoxShadow(color: Color(0xAA000000), blurRadius: 50, offset: Offset(0, 18), spreadRadius: -20)],
        ),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          Enter(
            delay: const Duration(milliseconds: 80),
            ms: 420,
            dy: 0,
            scale: .6,
            pop: true,
            fadeOnClose: false,
            child: Container(
              width: 28,
              height: 28,
              decoration: BoxDecoration(shape: BoxShape.circle, color: s.tone == 'dim' ? t.s3 : fade(c, 75)),
              alignment: Alignment.center,
              child: WIcon(icon, size: 16, color: c),
            ),
          ),
          const SizedBox(width: 8),
          Flexible(child: Text(s.text, style: body(context, size: .8438, w: FontWeight.w600, color: t.bright))),
        ]),
      ),
    );
  }
}
