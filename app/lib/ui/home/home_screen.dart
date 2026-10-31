// Home (port of ui/landing/mod.rs + landing.css #landing): the wordmark, Computers, Running,
// On this phone. A soft accent wash at the top, as the web's.
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/online.dart';
import '../../wado/relay_link.dart';
import '../../wado/session.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'cards.dart';
import 'running.dart';
import 'widgets.dart';

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key});
  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  @override
  void initState() {
    super.initState();
    CoreHost.I.computers.addListener(_probe);
    _probe();
    // Widgets' light links follow the list, the selected computer and the session.
    for (final l in _links) {
      l.addListener(_watch);
    }
    _watch();
    Host.I.sessionsWatch(true);
  }

  @override
  void dispose() {
    CoreHost.I.computers.removeListener(_probe);
    for (final l in _links) {
      l.removeListener(_watch);
    }
    Host.I.sessionsWatch(false);
    super.dispose();
  }

  late final List<Listenable> _links = [Settings.I, Nav.I.view, Session.I.shown, CoreHost.I.computers];
  void _watch() => watchLinks(session: Session.I.shown.value);

  void _probe() {
    final sel = Nav.I.selected;
    Online.I.watch([
      for (final c in CoreHost.I.computers.value)
        if (c.key != sel) (c.relayUrl, c.remoteId),
    ]);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final mq = MediaQuery.of(context);
    final w = mq.size.width;
    final side = w > 492 ? (w - 460) / 2 : 16.0;
    return Container(
      decoration: BoxDecoration(
        color: t.s0,
        gradient: RadialGradient(
          center: const Alignment(0, -1),
          radius: 1.1,
          colors: [fade(t.accent, 80), fade(t.accent, 100)],
          stops: const [0, .7],
        ),
      ),
      child: Stack(children: [
        Positioned.fill(
          child: DecoratedBox(
            decoration: BoxDecoration(
              gradient: RadialGradient(
                center: const Alignment(1, 1),
                radius: .9,
                colors: [fade(t.base[0xE], 88), fade(t.base[0xE], 100)],
                stops: const [0, .7],
              ),
            ),
          ),
        ),
        ListenableBuilder(
          listenable: Listenable.merge([
            CoreHost.I.computers,
            Online.I.map,
            RelayLink.I.up,
            RelayLink.I.words,
            RelayLink.I.seatBusy,
            Host.I.sessions,
            Session.I.on,
            Settings.I,
          ]),
          builder: (context, _) => ListView(
            padding: EdgeInsets.fromLTRB(side, 64 + mq.padding.top, side, 32 + mq.padding.bottom),
            children: [
              Text.rich(
                TextSpan(text: 'wado', children: [TextSpan(text: '.', style: TextStyle(color: t.accent))]),
                style: display(context, (w * .12).clamp(42, 64), ls: -(w * .12).clamp(42, 64) * .045),
              ),
              const SizedBox(height: 6),
              Text("Your computers, at this screen's own resolution.", style: body(context, color: t.dim)),
              if (CoreHost.I.keyReset) ...[
                const SizedBox(height: 12),
                const WCard(alert: true, children: [Text("This phone's key was reset — pair your computers again.")]),
              ],
              const Sect('Computers'),
              const SizedBox(height: 8),
              const Cards(),
              const Running(),
              const Sect('On this phone'),
              const SizedBox(height: 8),
              const PhoneList(),
              const SizedBox(height: 4),
              const WidgetsSection(),
            ],
          ),
        ),
      ]),
    );
  }
}

/// On this phone (home/settings.rs).
class PhoneList extends StatelessWidget {
  const PhoneList({super.key});
  @override
  Widget build(BuildContext context) {
    final s = Settings.I;
    final layout = switch (s.s('placement')) {
      'top_left' => 'top left',
      'cascade' => 'cascade',
      'maximized' => 'maximized',
      'tile' => 'tiled',
      'strip' => 'strip',
      'center' => 'centred',
      _ => 'automatic',
    };
    final look = '${s.s('theme')} · ${s.s('radius')} corners · $layout';
    final input =
        '${s.s('touch_mode') == 'touch' ? 'Raw touch' : 'Gestures'} · scroll ${s.d('scroll_speed').toStringAsFixed(2)}×${s.b('natural_scroll') ? ' · natural' : ''}';
    final tiles = '${s.get<List<String>>('tiles').length} tiles in the control centre';
    return ListBox(children: [
      NavRow(icon: 'palette', hue: '0A', title: 'Appearance', sub: look, onOpen: () => Nav.I.openPage('appearance', aboutComputer: false)),
      NavRow(icon: 'hand', hue: '0C', title: 'Input feel', sub: input, onOpen: () => Nav.I.openPage('input_feel', aboutComputer: false)),
      NavRow(icon: 'apps', hue: '0E', title: 'Quick tiles', sub: tiles, onOpen: () => Nav.I.openPage('tiles', aboutComputer: false)),
      NavRow(icon: 'bell', hue: '0B', title: 'Notifications & background', sub: Settings.I.b('stay_connected') ? 'Staying connected' : 'Volume keys · notifications', onOpen: () => Nav.I.openPage('background', aboutComputer: false)),
      NavRow(icon: 'pulse', hue: '08', title: 'Diagnostics', sub: 'Readouts · health · server log', onOpen: () => Nav.I.openPage('diagnostics', aboutComputer: false)),
    ]);
  }
}

/// `.lstate`: the tone dot and words.
class LinkWords extends StatelessWidget {
  const LinkWords({super.key, required this.tone, required this.words});
  final String tone, words;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = switch (tone) { 'ok' => t.good, 'warn' => t.warn, 'bad' => t.bad, _ => t.dim };
    return Row(mainAxisSize: MainAxisSize.min, children: [
      _Dot(color: c, breathe: tone == 'warn'),
      const SizedBox(width: 6),
      Flexible(child: Text(words, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, color: tone == 'off' ? t.dim : t.text))),
    ]);
  }
}

class _Dot extends StatefulWidget {
  const _Dot({required this.color, required this.breathe});
  final Color color;
  final bool breathe;
  @override
  State<_Dot> createState() => _DotState();
}

class _DotState extends State<_Dot> with SingleTickerProviderStateMixin {
  late final _c = AnimationController(vsync: this, duration: const Duration(milliseconds: 1200));
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(_Dot old) {
    super.didUpdateWidget(old);
    _sync();
  }

  void _sync() {
    if (widget.breathe && !Tok.of(context).still) {
      if (!_c.isAnimating) _c.repeat(reverse: true);
    } else {
      _c.stop();
      _c.value = 0;
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => RepaintBoundary(
        child: FadeTransition(
          opacity: Tween(begin: 1.0, end: .35).animate(CurvedAnimation(parent: _c, curve: Tokens.spring)),
          child: Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: widget.color)),
        ),
      );
}
