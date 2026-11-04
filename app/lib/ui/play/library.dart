// Home's play section for the selected computer. Played lately → "Jump back in", a carousel of
// recent games; otherwise one Continue card for the last thing done. Then Games: every app with
// Game mode on, and a dashed poster to turn it on for another. Listens to its own sources only —
// never to all settings — so nothing else on home makes it rebuild.
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/apps.dart';
import '../../wado/game_mode.dart';
import '../../wado/nav.dart';
import '../../wado/play.dart';
import '../../wado/session.dart';
import '../home/cards.dart' show go, Dashed;
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/sheet.dart';
import 'launch.dart';
import 'poster.dart';
import 'setup.dart';

Computer? selectedComputer() {
  final sel = Nav.I.selected;
  for (final c in CoreHost.I.computers.value) {
    if (c.key == sel) return c;
  }
  return null;
}

Map<String, dynamic>? appOf(String exec) {
  for (final a in Session.I.apps.value) {
    if (a['exec'] == exec) return a;
  }
  return null;
}

class PlaySection extends StatelessWidget {
  const PlaySection({super.key});
  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: Listenable.merge([GameMode.I, Play.I, Session.I.apps, CoreHost.I.computers, Nav.I.view]),
        builder: (context, _) {
          final c = selectedComputer();
          if (c == null) return const SizedBox.shrink();
          final games = GameMode.I.on(c.key);
          final mine = Play.I.recent.where((r) => r.computer == c.key).toList();
          final lately = Play.I.gamedLately(c.key);
          final recentGames = [
            for (final r in mine)
              if (r.kind == 'game') ?GameMode.I.of(c.key, r.exec),
          ];
          return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            if (lately && recentGames.isNotEmpty) ...[
              const Sect('Jump back in'),
              const SizedBox(height: 8),
              _Carousel(c: c, games: recentGames),
            ] else if (mine.isNotEmpty) ...[
              const Sect('Continue'),
              const SizedBox(height: 8),
              _Continue(c: c, r: mine.first),
            ],
            Sect('Games', action: games.isEmpty ? null : 'Add', onAction: games.isEmpty ? null : () => addGame(context, c)),
            const SizedBox(height: 8),
            _Shelf(c: c, games: games),
          ]);
        },
      );
}

String _setupWords(GameProfile p) => switch (p.play) { 'controller' => 'controller', 'touch' => 'touch', _ => 'on-screen pad' };

void _manage(BuildContext context, Computer c, GameProfile p) {
  final a = appOf(p.exec);
  openSheet(context, (ctx) => Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        SheetHead(p.name, sub: 'Game mode · ${_setupWords(p)}', lead: PosterGlyph(name: p.name, icon: a?['icon'] as String?, size: 44)),
        Btn('Game mode settings', icon: 'sliders', onTap: () async {
          Navigator.of(ctx).pop();
          final q = await setUpGame(context, name: p.name, exec: p.exec, icon: a?['icon'] as String?, from: p);
          if (q != null) GameMode.I.save(c.key, q);
        }),
        const SizedBox(height: 6),
        Btn('Turn off Game mode', danger: true, onTap: () {
          Navigator.of(ctx).pop();
          GameMode.I.off(c.key, p.exec);
        }),
        const SizedBox(height: 8),
        Text('Off, it opens as an ordinary app and leaves Games.', style: body(ctx, size: .75, color: Tok.of(ctx).dim)),
      ]));
}

class _Shelf extends StatelessWidget {
  const _Shelf({required this.c, required this.games});
  final Computer c;
  final List<GameProfile> games;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return SizedBox(
      height: 172,
      child: ListView(
        scrollDirection: Axis.horizontal,
        clipBehavior: Clip.none,
        children: [
          for (final p in games) ...[
            _GamePoster(c: c, p: p),
            const SizedBox(width: 10),
          ],
          Press(
            onTap: () => addGame(context, c),
            label: 'Turn on Game mode for an app',
            child: SizedBox(
              width: 96,
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                AspectRatio(
                  aspectRatio: 3 / 4,
                  child: Dashed(color: t.line, radius: t.r3, child: Center(child: WIcon('plus', color: t.dim))),
                ),
                const SizedBox(height: 6),
                Text(games.isEmpty ? 'Add a game' : 'Add', style: body(context, size: .75, w: FontWeight.w600, color: t.dim)),
                if (games.isEmpty) Text('set up once', style: body(context, size: .6875, color: t.dim)),
              ]),
            ),
          ),
        ],
      ),
    );
  }
}

class _GamePoster extends StatefulWidget {
  const _GamePoster({required this.c, required this.p});
  final Computer c;
  final GameProfile p;
  @override
  State<_GamePoster> createState() => _GamePosterState();
}

class _GamePosterState extends State<_GamePoster> {
  final _key = GlobalKey();
  @override
  Widget build(BuildContext context) {
    final p = widget.p;
    final a = appOf(p.exec);
    return Poster(
      posterKey: _key,
      name: p.name,
      icon: a?['icon'] as String?,
      running: a?['running'] == true,
      sub: _setupWords(p),
      onTap: () => openGame(context, widget.c, name: p.name, exec: p.exec, icon: a?['icon'] as String?, from: _key),
      onLongPress: () => _manage(context, widget.c, p),
    );
  }
}

/// Recent games as wide cards, the next one peeking; ▶ under the thumb. Stateful: a controller
/// made in build would snap back to the first card on every rebuild.
class _Carousel extends StatefulWidget {
  const _Carousel({required this.c, required this.games});
  final Computer c;
  final List<GameProfile> games;
  @override
  State<_Carousel> createState() => _CarouselState();
}

class _CarouselState extends State<_Carousel> {
  final _pc = PageController(viewportFraction: .82);
  @override
  void dispose() {
    _pc.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final w = MediaQuery.of(context).size.width.clamp(0.0, 460.0);
    return SizedBox(
      height: (w * .78) * 11 / 16,
      child: PageView.builder(
        controller: _pc,
        padEnds: false,
        itemCount: widget.games.length,
        itemBuilder: (context, i) => Padding(padding: const EdgeInsets.only(right: 10), child: _Wide(c: widget.c, p: widget.games[i])),
      ),
    );
  }
}

class _Wide extends StatefulWidget {
  const _Wide({required this.c, required this.p});
  final Computer c;
  final GameProfile p;
  @override
  State<_Wide> createState() => _WideState();
}

class _WideState extends State<_Wide> {
  final _key = GlobalKey();
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final p = widget.p;
    final a = appOf(p.exec);
    final running = a?['running'] == true;
    void open() => openGame(context, widget.c, name: p.name, exec: p.exec, icon: a?['icon'] as String?, from: _key);
    return Press(
      onTap: open,
      onLongPress: () => _manage(context, widget.c, p),
      scale: .98,
      label: running ? '${p.name}, still running' : p.name,
      child: Container(
        key: _key,
        padding: const EdgeInsets.all(14),
        decoration: wash(t, posterHue(p.name), t.r3),
        child: Stack(children: [
          Positioned(top: 0, right: 0, child: PosterGlyph(name: p.name, icon: a?['icon'] as String?, size: 52)),
          Positioned(
            left: 0,
            right: 60,
            bottom: 0,
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
              _Tag(running ? 'Still running' : 'Game mode', color: running ? t.good : t.bright),
              const SizedBox(height: 6),
              Text(p.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, 20, ls: -.4, color: t.bright)),
              Text(_setupWords(p), style: body(context, size: .75, color: fade(t.bright, 25))),
            ]),
          ),
          Positioned(
            right: 0,
            bottom: 0,
            child: Container(
              width: 46,
              height: 46,
              decoration: BoxDecoration(color: t.accent, shape: BoxShape.circle),
              child: Center(child: WIcon('play', color: t.onAccent, fill: true)),
            ),
          ),
        ]),
      ),
    );
  }
}

class _Tag extends StatelessWidget {
  const _Tag(this.words, {required this.color});
  final String words;
  final Color color;
  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(color: fade(color, 84), borderRadius: BorderRadius.circular(999)),
        child: Text(words.toUpperCase(), style: body(context, size: .625, w: FontWeight.w700, color: color).copyWith(letterSpacing: .6)),
      );
}

/// The last thing done on this computer, whatever kind.
class _Continue extends StatelessWidget {
  const _Continue({required this.c, required this.r});
  final Computer c;
  final Recent r;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final p = r.kind == 'game' ? GameMode.I.of(c.key, r.exec) : null;
    final words = switch (r.kind) { 'game' => 'Game mode', 'shell' => 'Shell', 'file' => 'File', _ => 'Desktop' };
    return Press(
      onTap: () => p != null ? openGame(context, c, name: p.name, exec: p.exec, icon: appOf(p.exec)?['icon'] as String?) : go(c),
      scale: .98,
      label: 'Continue ${r.name}',
      child: Container(
        padding: const EdgeInsets.all(14),
        decoration: wash(t, posterHue(r.name), t.r3),
        child: Row(children: [
          PosterGlyph(name: r.name, icon: appOf(r.exec)?['icon'] as String?, size: 52),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(r.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, 18, ls: -.2, color: t.bright)),
              Text('$words · ${_ago(r.at)}', style: body(context, size: .75, color: fade(t.bright, 25))),
            ]),
          ),
          Container(
            width: 48,
            height: 48,
            decoration: BoxDecoration(color: t.accent, shape: BoxShape.circle),
            child: Center(child: WIcon('play', color: t.onAccent, fill: true)),
          ),
        ]),
      ),
    );
  }
}

String _ago(int at) {
  final s = DateTime.now().millisecondsSinceEpoch ~/ 1000 - at;
  if (s < 90) return 'just now';
  if (s < 3600) return '${s ~/ 60} min ago';
  if (s < 86400) return '${s ~/ 3600} h ago';
  return '${s ~/ 86400} d ago';
}

/// Turn Game mode on for an app: pick it from the computer's apps, then the setup.
Future<void> addGame(BuildContext context, Computer c) async {
  if (Session.I.apps.value.isEmpty) Apps.I.request();
  final picked = await openSheet<Map<String, dynamic>>(context, (ctx) => _Pick(c: c));
  if (picked == null || !context.mounted) return;
  final p = await setUpGame(context, name: '${picked['name']}', exec: '${picked['exec']}', icon: picked['icon'] as String?);
  if (p != null) GameMode.I.save(c.key, p);
}

class _Pick extends StatefulWidget {
  const _Pick({required this.c});
  final Computer c;
  @override
  State<_Pick> createState() => _PickState();
}

class _PickState extends State<_Pick> {
  String _q = '';
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ValueListenableBuilder(
      valueListenable: Session.I.apps,
      builder: (context, all, _) {
        final on = {for (final p in GameMode.I.on(widget.c.key)) p.exec};
        final list = [
          for (final a in all)
            if (a['hidden'] != true && !on.contains(a['exec']) && '${a['name']}'.toLowerCase().contains(_q.toLowerCase())) a,
        ]..sort((x, y) => (needsX('${y['exec']}') ? 1 : 0) - (needsX('${x['exec']}') ? 1 : 0));
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          SheetHead('Turn on Game mode', sub: 'For any app you play on ${widget.c.name}. Set it up once; it opens that way every time.'),
          TextField(
            onChanged: (v) => setState(() => _q = v),
            style: body(context, size: 1, color: t.bright),
            decoration: InputDecoration(
              hintText: 'Search apps',
              filled: true,
              fillColor: t.s0,
              border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1), borderSide: BorderSide(color: t.line)),
            ),
          ),
          const SizedBox(height: 10),
          if (all.isEmpty)
            Padding(
              padding: const EdgeInsets.all(16),
              child: Text('Asking ${widget.c.name} for its apps…', textAlign: TextAlign.center, style: body(context, color: t.dim)),
            ),
          for (final a in list.take(60))
            Press(
              onTap: () => Navigator.of(context).pop(a),
              label: 'Game mode for ${a['name']}',
              child: SizedBox(
                height: 56,
                child: Row(children: [
                  PosterGlyph(name: '${a['name']}', icon: a['icon'] as String?, size: 36),
                  const SizedBox(width: 12),
                  Expanded(child: Text('${a['name']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright))),
                  WIcon('plus', color: t.accent),
                ]),
              ),
            ),
        ]);
      },
    );
  }
}
