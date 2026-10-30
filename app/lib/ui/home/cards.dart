// Computer cards (port of ui/home/cards.rs + home.css .ccard): OS logo in the computer's hue,
// name, OS · desktop, link words, session count, and ▶ (start, or rejoin when one runs).
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/reach.dart';
import '../../wado/session.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'home_screen.dart';
import 'running.dart';

const hues = ['0D', '0E', '0C', '0B', '0A', '09'];

/// The OS logo in a disc of the computer's hue.
class OsDisc extends StatelessWidget {
  const OsDisc({super.key, required this.logo, required this.hue, this.size = 46});
  final String logo;
  final Color hue;
  final double size;
  @override
  Widget build(BuildContext context) => Disc(
        hue: hue,
        size: size,
        child: SvgPicture.asset('assets/os/${logo.isEmpty ? 'linux' : logo}.svg',
            width: size * .46, height: size * .46, colorFilter: ColorFilter.mode(hue, BlendMode.srcIn)),
      );
}

void go(Computer c) {
  if (Nav.I.selected != c.key) {
    Nav.I.select(c.key);
    Session.I.start();
    return;
  }
  if (Host.I.sessions.value.isEmpty) {
    Session.I.start();
  } else {
    joinBest();
  }
}

class Cards extends StatelessWidget {
  const Cards({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final list = CoreHost.I.computers.value;
    final sel = Nav.I.selected;
    final n = Host.I.sessions.value.length;
    return Column(children: [
      for (var i = 0; i < list.length; i++) ...[
        _Card(c: list[i], hue: slot(context, hues[i % 6]), sessions: list[i].key == sel ? n : 0),
        const SizedBox(height: 8),
      ],
      Press(
        onTap: () => Nav.I.openPage('add_computer', aboutComputer: false),
        label: 'Add a computer',
        child: _Dashed(
          color: t.line,
          radius: t.r3,
          child: SizedBox(
            height: 52,
            child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
              WIcon('plus', color: t.dim),
              const SizedBox(width: 8),
              Text('Add a computer', style: body(context, w: FontWeight.w600, color: t.dim)),
            ]),
          ),
        ),
      ),
    ]);
  }
}

class _Card extends StatelessWidget {
  const _Card({required this.c, required this.hue, required this.sessions});
  final Computer c;
  final Color hue;
  final int sessions;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final r = Reach.of(c);
    final os = [c.os, c.desktop].where((x) => x.isNotEmpty).join(' · ');
    final live = sessions > 0;
    return Container(
      constraints: const BoxConstraints(minHeight: 80),
      padding: const EdgeInsets.fromLTRB(6, 6, 8, 6),
      decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3), border: Border.all(color: t.line)),
      child: Row(children: [
        Expanded(
          child: Press(
            onTap: () => Nav.I.openComputer(c.key),
            label: '${c.name} — open its page',
            child: Padding(
              padding: const EdgeInsets.only(left: 8),
              child: ConstrainedBox(
                constraints: const BoxConstraints(minHeight: 66),
                child: Row(children: [
                  OsDisc(logo: c.logo, hue: hue),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                      Text(c.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1.0625), ls: -.17)),
                      if (os.isNotEmpty) Text(os, style: mono(context, size: .7188, color: t.dim)),
                      const SizedBox(height: 3),
                      Row(children: [
                        Flexible(child: LinkWords(tone: r.tone, words: r.words)),
                        if (live) Text('  · ${sessions == 1 ? '1 session' : '$sessions sessions'}', style: body(context, size: .75, color: t.dim)),
                      ]),
                    ]),
                  ),
                ]),
              ),
            ),
          ),
        ),
        const SizedBox(width: 4),
        _Go(live: live, enabled: r.online || r.busy, label: live ? 'Rejoin the desktop on ${c.name}' : 'Start a desktop on ${c.name}', onTap: () => go(c)),
      ]),
    );
  }
}

/// `.cgo`: the round ▶, solid with a pulse ring while a session runs.
class _Go extends StatefulWidget {
  const _Go({required this.live, required this.enabled, required this.label, required this.onTap});
  final bool live, enabled;
  final String label;
  final VoidCallback onTap;
  @override
  State<_Go> createState() => _GoState();
}

class _GoState extends State<_Go> with SingleTickerProviderStateMixin {
  late final _ring = AnimationController(vsync: this, duration: const Duration(milliseconds: 2400));
  @override
  void dispose() {
    _ring.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    if (widget.live && widget.enabled && !t.still) {
      if (!_ring.isAnimating) _ring.repeat();
    } else {
      _ring.stop();
    }
    final bg = !widget.enabled ? t.s2 : (widget.live ? t.accent : fade(t.accent, 82));
    final fg = !widget.enabled ? t.dim : (widget.live ? t.onAccent : t.accent);
    return Press(
      onTap: widget.enabled ? widget.onTap : null,
      label: widget.label,
      child: SizedBox(
        width: 48,
        height: 48,
        child: Stack(alignment: Alignment.center, clipBehavior: Clip.none, children: [
          if (widget.live && widget.enabled)
            AnimatedBuilder(
              animation: _ring,
              builder: (context, _) {
                final p = (_ring.value / .6).clamp(0.0, 1.0);
                final c = const Cubic(.2, 0, 0, 1).transform(p);
                return Transform.scale(
                  scale: 1 + .35 * c,
                  child: Container(
                    width: 48,
                    height: 48,
                    decoration: BoxDecoration(shape: BoxShape.circle, border: Border.all(color: t.accent.withValues(alpha: .5 * (1 - c)), width: 2)),
                  ),
                );
              },
            ),
          AnimatedContainer(
            duration: t.dur,
            width: 48,
            height: 48,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: bg,
              boxShadow: widget.live && widget.enabled ? [BoxShadow(color: fade(t.accent, 30), blurRadius: 22, offset: const Offset(0, 8), spreadRadius: -10)] : null,
            ),
            child: Padding(padding: const EdgeInsets.only(left: 2), child: Center(child: WIcon('play', size: 18, fill: true, color: fg))),
          ),
        ]),
      ),
    );
  }
}

/// A 1.5 px dashed border (`.ccard.add`).
class _Dashed extends StatelessWidget {
  const _Dashed({required this.child, required this.color, required this.radius});
  final Widget child;
  final Color color;
  final double radius;
  @override
  Widget build(BuildContext context) => CustomPaint(painter: _DashPainter(color, radius), child: child);
}

class _DashPainter extends CustomPainter {
  _DashPainter(this.color, this.radius);
  final Color color;
  final double radius;
  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.5;
    final path = Path()..addRRect(RRect.fromRectAndRadius(Offset.zero & size, Radius.circular(radius)));
    for (final m in path.computeMetrics()) {
      var d = 0.0;
      while (d < m.length) {
        canvas.drawPath(m.extractPath(d, d + 6), p);
        d += 10;
      }
    }
  }

  @override
  bool shouldRepaint(_DashPainter o) => o.color != color || o.radius != radius;
}

