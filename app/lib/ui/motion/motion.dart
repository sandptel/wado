// How the screens move (port of js/hero_flip.js and the shared keyframes of layout.css /
// computer.css): a hand-rolled FLIP — a computer's card grows into its page and back — and the
// small entrances every view uses (rise, pop, stagger). The live widget is under the finger from
// its first frame; nothing here blocks input.
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart' show timeDilation;

import '../theme/look.dart';
import '../theme/tokens.dart';

/// Shared-element sources by tag (`card:<key>`, `disc:<key>`, `name:<key>`), their rects as they
/// were when a flight began, and which card has stepped out for it.
class Motion {
  static final _keys = <String, GlobalKey>{};
  static final _rects = <String, Rect>{};
  static final flying = ValueNotifier<String?>(null);

  /// The computer page's scroll, so a close knows whether the hero is still near its card.
  static double pageScroll = 0;

  static GlobalKey key(String tag) => _keys.putIfAbsent(tag, () => GlobalKey(debugLabel: tag));

  static Rect? measure(String tag) {
    final ro = _keys[tag]?.currentContext?.findRenderObject();
    if (ro is! RenderBox || !ro.attached || !ro.hasSize) return null;
    return ro.localToGlobal(Offset.zero) & ro.size;
  }

  /// Snapshot a computer's card parts before anything moves. True when the card is on screen.
  static bool capture(String key, Size screen) {
    var ok = true;
    for (final p in ['card', 'disc', 'name']) {
      final r = measure('$p:$key');
      if (r == null) {
        ok = false;
        continue;
      }
      _rects['$p:$key'] = r;
    }
    final c = _rects['card:$key'];
    return ok && c != null && c.width > 0 && c.bottom > 0 && c.top < screen.height;
  }

  static Rect? rect(String tag) => _rects[tag];
}

/// The clock of the view a widget is in: the computer page's open/close (with `flight` when its
/// card is flying into the hero), or a pushed page's slide. Read by [Flip] and [Enter].
class ViewClock extends InheritedWidget {
  const ViewClock({super.key, required this.a, required this.flight, required super.child});
  final AnimationController a;
  final bool flight;
  static ViewClock? of(BuildContext c) => c.dependOnInheritedWidgetOfExactType<ViewClock>();
  @override
  bool updateShouldNotify(ViewClock o) => o.a != a || o.flight != flight;
}

/// A rect `s` as a transform on a box whose rest rect is `d` (hero_flip.js `tf`). `uni`: scale
/// evenly (text and discs keep their shape).
Matrix4 _tf(Rect s, Rect d, bool uni, double v) {
  final sx = s.width / d.width, sy = uni ? sx : s.height / d.height;
  final k = 1 - v;
  return Matrix4.identity()
    ..translateByDouble((s.left - d.left) * k, (s.top - d.top) * k, 0, 1)
    ..scaleByDouble(1 + (sx - 1) * k, 1 + (sy - 1) * k, 1, 1);
}

/// A part of the computer page's hero that flies from its card counterpart (`from` tag) while the
/// page opens, and back into it while it closes.
class Flip extends StatefulWidget {
  const Flip({super.key, required this.from, required this.child, this.uniform = true});
  final String from;
  final bool uniform;
  final Widget child;
  @override
  State<Flip> createState() => _FlipState();
}

class _FlipState extends State<Flip> {
  Rect? _d;

  void _measure() {
    final ro = context.findRenderObject();
    if (ro is RenderBox && ro.hasSize) {
      final r = ro.localToGlobal(Offset.zero) & ro.size;
      if (r != _d && mounted) setState(() => _d = r);
    }
  }

  @override
  Widget build(BuildContext context) {
    final clock = ViewClock.of(context);
    final s = Motion.rect(widget.from);
    if (clock == null || !clock.flight || s == null) return widget.child;
    return AnimatedBuilder(
      animation: clock.a,
      child: widget.child,
      builder: (context, child) {
        final a = clock.a;
        final v = a.status == AnimationStatus.reverse ? Tokens.spring.flipped.transform(a.value) : Tokens.spring.transform(a.value);
        if (v >= 1 || a.isCompleted) return child!;
        if (_d == null) {
          WidgetsBinding.instance.addPostFrameCallback((_) => _measure());
          return Opacity(opacity: 0, child: child);
        }
        return Transform(transform: _tf(s, _d!, widget.uniform, v), child: child);
      },
    );
  }
}

/// An entrance played once, when the widget first appears: from `opacity 0`, `dy` px lower and
/// `scale`, after `delay` (layout.css `rise`, computer.css `popin`, the section stagger). Inside a
/// closing [ViewClock] it fades out quickly first (hero_flip.js: exits are quicker).
class Enter extends StatefulWidget {
  const Enter({
    super.key,
    required this.child,
    this.delay = Duration.zero,
    this.ms = 520,
    this.dy = 18,
    this.dx = 0,
    this.scale = 1,
    this.pop = false,
    this.fadeOnClose = true,
  });
  final Widget child;
  final Duration delay;
  final int ms;
  final double dy, dx, scale;
  final bool pop, fadeOnClose;

  /// The n-th section after `base` ms, one stagger step (40 ms) apart.
  static Duration step(int n, {int base = 0}) => Duration(milliseconds: base + n * 40);

  @override
  State<Enter> createState() => _EnterState();
}

class _EnterState extends State<Enter> with SingleTickerProviderStateMixin {
  late final AnimationController _c;

  @override
  void initState() {
    super.initState();
    _c = AnimationController(vsync: this, duration: Duration(milliseconds: widget.ms));
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final t = Tok.of(context);
      if (t.reduced) {
        _c.duration = const Duration(milliseconds: 160);
        _c.forward();
        return;
      }
      Future.delayed(widget.delay * timeDilation, () {
        if (mounted) _c.forward();
      });
    });
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final clock = widget.fadeOnClose ? ViewClock.of(context) : null;
    final listen = clock == null ? _c : Listenable.merge([_c, clock.a]);
    return AnimatedBuilder(
      animation: listen,
      child: widget.child,
      builder: (context, child) {
        final e = _c.value;
        var o = Curves.easeOut.transform(e.clamp(0, 1));
        final m = (widget.pop && !t.reduced ? Tokens.popCurve : Tokens.spring).transform(e);
        if (clock != null && clock.a.status == AnimationStatus.reverse) {
          // 160 ms of the close, accelerating away.
          final gone = ((1 - clock.a.value) * (clock.a.reverseDuration ?? clock.a.duration!).inMilliseconds / 160).clamp(0.0, 1.0);
          o *= 1 - const Cubic(.3, 0, 1, 1).transform(gone);
        }
        if (e >= 1 && o >= 1) return child!;
        final travel = t.reduced ? 0.0 : 1 - m;
        return Opacity(
          opacity: o.clamp(0, 1),
          child: Transform(
            alignment: Alignment.center,
            transform: Matrix4.identity()
              ..translateByDouble(widget.dx * travel, widget.dy * travel, 0, 1)
              ..scaleByDouble(1 - (1 - widget.scale) * travel, 1 - (1 - widget.scale) * travel, 1, 1),
            child: child,
          ),
        );
      },
    );
  }
}
