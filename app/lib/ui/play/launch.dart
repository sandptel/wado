// "The poster becomes the game": opening a game grows its poster's rect to the whole screen on the
// spring; the wash becomes the loading screen, the session's hops told as plain steps; when the
// first frame lands the wash fades and the picture is underneath. A failed hop names itself.
import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/game_mode.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/play.dart';
import '../../wado/session.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/sheet.dart';
import '../widgets/soon.dart';
import 'poster.dart';
import 'setup.dart';

/// Open an app of [c] as a game: set up once, then straight in.
Future<void> openGame(BuildContext context, Computer c, {required String name, required String exec, String? icon, GlobalKey? from}) async {
  // ponytail: only the selected computer's features are known; another's answers when it dials.
  if (Nav.I.selected == c.key && !may('session')) return;
  var p = GameMode.I.of(c.key, exec);
  if (p == null) {
    p = await setUpGame(context, name: name, exec: exec, icon: icon);
    if (p == null) return;
    GameMode.I.save(c.key, p);
  }
  if (!context.mounted) return;
  // The X server is chosen when a session starts: one already running may not have it.
  if (p.x && Nav.I.selected == c.key && Host.I.sessions.value.isNotEmpty && Session.I.applied?['x_server'] != true) {
    final go = await openSheet<String>(context, (ctx) => _Restart(name: name, icon: icon));
    if (go == null || !context.mounted) return;
    if (go == 'restart') {
      Session.I.stop();
      // ponytail: a pause for the stop to land before the fresh start; the daemon's own
      // "stopped" event is the better trigger if this ever races.
      await Future<void>.delayed(const Duration(milliseconds: 1500));
      if (!context.mounted) return;
    } else {
      p.x = false;
    }
  }
  if (!context.mounted) return;
  final box = from?.currentContext?.findRenderObject() as RenderBox?;
  final rect = box != null && box.hasSize ? box.localToGlobal(Offset.zero) & box.size : null;
  late final OverlayEntry real;
  real = OverlayEntry(builder: (_) => _Launch(from: rect, name: name, icon: icon, computer: c.name, onDone: () => real.remove()));
  Overlay.of(context, rootOverlay: true).insert(real);
  Play.I.open(c.key, p);
}

class _Restart extends StatelessWidget {
  const _Restart({required this.name, this.icon});
  final String name;
  final String? icon;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      SheetHead('$name needs a quick restart', lead: PosterGlyph(name: name, icon: icon, size: 44)),
      Text(
        '$name runs in an older mode the computer switches on when a session starts. wado will end the session and start it again with that mode on. Apps open in it close.',
        style: body(context, color: t.text),
      ),
      const SizedBox(height: 14),
      Btn('Restart and open $name', primary: true, onTap: () => Navigator.of(context).pop('restart')),
      const SizedBox(height: 6),
      Btn('Try without it', onTap: () => Navigator.of(context).pop('without')),
      const SizedBox(height: 8),
      Text('Details · X server (Xwayland) · restart-only · Display & session', style: mono(context, color: t.dim)),
    ]);
  }
}

class _Launch extends StatefulWidget {
  const _Launch({required this.from, required this.name, required this.computer, required this.onDone, this.icon});
  final Rect? from;
  final String name, computer;
  final String? icon;
  final VoidCallback onDone;
  @override
  State<_Launch> createState() => _LaunchState();
}

class _LaunchState extends State<_Launch> with TickerProviderStateMixin {
  late final _grow = AnimationController(vsync: this);
  late final _out = AnimationController(vsync: this, duration: const Duration(milliseconds: 300));
  late final _links = Listenable.merge([Session.I.stage, Session.I.stageError, Session.I.hasVideo, Session.I.shown]);
  Timer? _giveUp;

  @override
  void initState() {
    super.initState();
    _links.addListener(_follow);
    // Never a screen left over the session: whatever happens, it goes after a while.
    _giveUp = Timer(const Duration(seconds: 45), _leave);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_grow.duration == null) {
      _grow.duration = Tok.of(context).durHero;
      _grow.animateTo(1, curve: Tokens.spring);
    }
  }

  void _follow() {
    if (!mounted) return;
    if (Session.I.hasVideo.value || !Session.I.shown.value) {
      _leave();
    } else {
      setState(() {});
    }
  }

  void _leave() {
    if (_out.isAnimating || _out.value == 1) return;
    _out.forward().whenComplete(widget.onDone);
  }

  @override
  void dispose() {
    _giveUp?.cancel();
    _links.removeListener(_follow);
    _grow.dispose();
    _out.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final size = MediaQuery.of(context).size;
    final full = Offset.zero & size;
    final from = widget.from ?? Rect.fromCenter(center: full.center, width: size.width * .4, height: size.width * .53);
    final stage = Session.I.stage.value;
    final err = Session.I.stageError.value;
    final steps = [
      ('Reaching ${widget.computer}', stage >= 1),
      ('Making room for this phone', stage >= 3),
      ('Opening ${widget.name}', stage >= 4),
      ('Here it comes', Session.I.hasVideo.value),
    ];
    final now = steps.indexWhere((s) => !s.$2);
    return AnimatedBuilder(
      animation: Listenable.merge([_grow, _out]),
      builder: (context, child) {
        final a = _grow.value;
        final r = Rect.lerp(from, full, a)!;
        return Positioned.fromRect(
          rect: r,
          child: IgnorePointer(
            ignoring: _out.value > 0,
            child: Opacity(
              opacity: 1 - _out.value,
              child: DecoratedBox(
                decoration: wash(t, posterHue(widget.name), t.r3 * (1 - a)),
                child: Opacity(opacity: ((a - .55) / .45).clamp(0.0, 1.0), child: child),
              ),
            ),
          ),
        );
      },
      // The content is laid out at full size once and only faded in: no text reflows mid-grow.
      child: OverflowBox(
        maxWidth: size.width,
        maxHeight: size.height,
        child: SizedBox.fromSize(
          size: size,
          child: Material(
            type: MaterialType.transparency,
            child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
              PosterGlyph(name: widget.name, icon: widget.icon, size: 96),
              const SizedBox(height: 18),
              Text(widget.name, style: display(context, 28, ls: -.8, color: t.bright)),
              const SizedBox(height: 22),
              SizedBox(
                width: 240,
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  for (var i = 0; i < steps.length; i++) _Hop(words: steps[i].$1, done: steps[i].$2, now: i == now && err.isEmpty, failed: i == now && err.isNotEmpty),
                ]),
              ),
              if (err.isNotEmpty) ...[
                const SizedBox(height: 16),
                SizedBox(width: 260, child: Text(err, textAlign: TextAlign.center, style: body(context, size: .8125, color: t.text))),
                const SizedBox(height: 14),
                SizedBox(
                  width: 260,
                  child: Row(children: [
                    Expanded(child: Btn('Close', onTap: _leave)),
                    const SizedBox(width: 8),
                    Expanded(child: Btn('Try again', primary: true, onTap: Session.I.start)),
                  ]),
                ),
              ],
            ]),
          ),
        ),
      ),
    );
  }
}

class _Hop extends StatelessWidget {
  const _Hop({required this.words, required this.done, required this.now, required this.failed});
  final String words;
  final bool done, now, failed;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = done || now ? t.bright : fade(t.bright, 50);
    return SizedBox(
      height: 30,
      child: Row(children: [
        AnimatedContainer(
          duration: t.dur,
          curve: Tokens.spring,
          width: 18,
          height: 18,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: done ? t.good : (failed ? t.bad : Colors.transparent),
            border: done || failed ? null : Border.all(color: c, width: now ? 2 : 1.5),
          ),
          child: done ? WIcon('check', size: 12, color: t.base[0]) : (failed ? WIcon('x', size: 12, color: t.base[0]) : null),
        ),
        const SizedBox(width: 10),
        Text(words, style: body(context, size: .8125, color: c)),
      ]),
    );
  }
}
