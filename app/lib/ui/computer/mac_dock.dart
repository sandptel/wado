// The Mac's own apps on its computer page (desk_apps.dart): a dock like the Mac's — the apps
// its Dock keeps, in its order, then (past a divider) those running that it does not. A dot
// under what runs, the one in front lit and raised, a hidden one dimmed, one just opened bouncing
// until it runs. Tap opens or brings it forward; a long press is the Dock's menu — Show, Hide,
// Quit, Force Quit. "All apps" opens every app on the Mac, searchable, those open now first.
import 'package:flutter/material.dart';

import '../../wado/desk_apps.dart';
import '../../wado/nav.dart';
import '../../wado/session.dart';
import '../motion/feel.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/data_image.dart';
import 'sessions.dart' show LinkMeter;

/// Keeps the list fresh while it is on screen.
mixin _Watching<T extends StatefulWidget> on State<T> {
  @override
  void initState() {
    super.initState();
    DeskApps.I.watch(true);
  }

  @override
  void dispose() {
    DeskApps.I.watch(false);
    super.dispose();
  }
}

class MacDock extends StatefulWidget {
  const MacDock({super.key});
  @override
  State<MacDock> createState() => _MacDockState();
}

class _MacDockState extends State<MacDock> with _Watching {
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Sect('Apps', action: 'All apps', onAction: () => Nav.I.openPage('mac_apps')),
      const SizedBox(height: 8),
      ValueListenableBuilder(
        valueListenable: Session.I.apps,
        builder: (context, all, _) {
          final (kept, extra) = dockGroups(all);
          return Container(
            height: 92,
            decoration: BoxDecoration(
              color: fade(t.s2, 20),
              borderRadius: BorderRadius.circular(t.r4),
              border: Border.all(color: t.line),
            ),
            child: all.isEmpty
                ? Center(child: Text('Asking the Mac what it has…', style: body(context, size: .8125, color: t.dim)))
                : ListView(
                    scrollDirection: Axis.horizontal,
                    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                    children: [
                      for (final a in kept) AppIcon(a: a, key: ValueKey(a['exec'])),
                      if (kept.isNotEmpty && extra.isNotEmpty)
                        Container(width: 1, margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 10), color: t.line),
                      for (final a in extra) AppIcon(a: a, key: ValueKey('x${a['exec']}')),
                      if (kept.isEmpty && extra.isEmpty)
                        Center(child: Text('Nothing open — All apps has everything', style: body(context, size: .8125, color: t.dim))),
                    ],
                  ),
          );
        },
      ),
      const SizedBox(height: 8),
      const LinkMeter(intent: 'start'),
    ]);
  }
}

/// One app: its icon (a letter tile until it arrives), name, and state.
class AppIcon extends StatefulWidget {
  const AppIcon({super.key, required this.a, this.size = 48, this.width = 64});
  final App a;
  final double size, width;
  @override
  State<AppIcon> createState() => _AppIconState();
}

class _AppIconState extends State<AppIcon> with SingleTickerProviderStateMixin {
  late final _bounce = AnimationController(vsync: this, duration: const Duration(milliseconds: 520));

  @override
  void dispose() {
    _bounce.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final a = widget.a;
    final run = a['running'] == true, front = a['front'] == true, away = a['away'] == true;
    final name = '${a['name']}';
    return ValueListenableBuilder(
      valueListenable: DeskApps.I.launching,
      builder: (context, launching, _) {
        final bouncing = launching.contains('${a['exec']}') && !run;
        if (bouncing && !_bounce.isAnimating) {
          _bounce.repeat(reverse: true);
        } else if (!bouncing && _bounce.isAnimating) {
          _bounce.animateTo(0, duration: const Duration(milliseconds: 160));
        }
        final tile = Container(
          width: widget.size,
          height: widget.size,
          alignment: Alignment.center,
          decoration: BoxDecoration(color: t.s3, borderRadius: BorderRadius.circular(widget.size * .24)),
          child: Text(name.isEmpty ? '?' : name.characters.first.toUpperCase(), style: body(context, w: FontWeight.w700, color: t.bright)),
        );
        return Semantics(
          button: true,
          label: '$name, ${stateOf(a)}',
          child: Press(
            onTap: () {
              feel('tap');
              DeskApps.I.act(a, tapAction(a));
            },
            onLongPress: () {
              feel('claim');
              showAppMenu(context, a);
            },
            label: name,
            child: SizedBox(
              width: widget.width,
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                AnimatedBuilder(
                  animation: _bounce,
                  builder: (context, child) => Transform.translate(
                    offset: Offset(0, -10 * Curves.easeOut.transform(_bounce.value) - (front ? 3 : 0)),
                    child: child,
                  ),
                  child: AnimatedOpacity(
                    duration: t.dur,
                    opacity: away ? .5 : 1,
                    child: AnimatedContainer(
                      duration: t.dur,
                      curve: Tokens.spring,
                      decoration: BoxDecoration(
                        borderRadius: BorderRadius.circular(widget.size * .24),
                        boxShadow: front ? [BoxShadow(color: fade(t.accent, 60), blurRadius: 14, spreadRadius: 1)] : const [],
                      ),
                      child: DataPicture(a['icon'] as String?, size: widget.size, fallback: tile),
                    ),
                  ),
                ),
                const SizedBox(height: 3),
                // The running dot: lit for the app in front.
                AnimatedContainer(
                  duration: t.dur,
                  curve: Tokens.spring,
                  width: run ? (front ? 6 : 4) : 0,
                  height: run ? (front ? 6 : 4) : 0,
                  decoration: BoxDecoration(color: front ? t.accent : t.dim, shape: BoxShape.circle),
                ),
                const SizedBox(height: 2),
                Text(name, maxLines: 1, overflow: TextOverflow.ellipsis, textAlign: TextAlign.center, style: body(context, size: .625, color: away ? t.dim : t.text)),
              ]),
            ),
          ),
        );
      },
    );
  }
}

/// The Dock's menu for `a`: what it is now, and what can be done.
void showAppMenu(BuildContext context, App a) {
  final t = Tok.of(context);
  final name = '${a['name']}';
  showModalBottomSheet(
    context: context,
    backgroundColor: t.s1,
    builder: (s) => SafeArea(
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        ListTile(
          leading: DataPicture(a['icon'] as String?, size: 36, fallback: WIcon('apps', color: t.bright)),
          title: Text(name, style: body(context, w: FontWeight.w600, color: t.bright)),
          subtitle: Text(stateOf(a), style: body(context, size: .75, color: a['front'] == true ? t.accent : t.dim)),
        ),
        for (final (action, label, danger) in menuFor(a))
          ListTile(
            leading: WIcon(switch (action) { 'hide' => 'eye', 'quit' => 'x', 'force_quit' => 'alert', _ => 'play' }, color: danger ? t.bad : t.bright),
            title: Text(label, style: TextStyle(color: danger ? t.bad : null)),
            onTap: () async {
              Navigator.pop(s);
              if (action == 'force_quit') {
                final sure = await showDialog<bool>(
                  context: context,
                  builder: (d) => AlertDialog(
                    title: Text('Force Quit $name?'),
                    content: Text('Unsaved changes in $name will be lost.'),
                    actions: [
                      TextButton(onPressed: () => Navigator.pop(d, false), child: const Text('Cancel')),
                      TextButton(onPressed: () => Navigator.pop(d, true), child: Text('Force Quit', style: TextStyle(color: t.bad))),
                    ],
                  ),
                );
                if (sure != true) return;
              }
              feel(danger ? 'heavy' : 'tick');
              DeskApps.I.act(a, action);
            },
          ),
      ]),
    ),
  );
}

/// Every app on the Mac: those open now, then all of them A–Z; a search narrows both.
class MacAppsPage extends StatefulWidget {
  const MacAppsPage({super.key});
  @override
  State<MacAppsPage> createState() => _MacAppsPageState();
}

class _MacAppsPageState extends State<MacAppsPage> with _Watching {
  final _q = TextEditingController();

  @override
  void dispose() {
    _q.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ValueListenableBuilder(
      valueListenable: Session.I.apps,
      builder: (context, all, _) {
        final q = _q.text.trim().toLowerCase();
        bool hit(App a) => q.isEmpty || '${a['name']}'.toLowerCase().contains(q);
        final now = openNow(all).where(hit).toList();
        final every = [for (final a in all) if (hit(a)) a]..sort((a, b) => '${a['name']}'.toLowerCase().compareTo('${b['name']}'.toLowerCase()));
        SliverGrid grid(List<App> l) => SliverGrid(
              gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(maxCrossAxisExtent: 88, mainAxisExtent: 96, crossAxisSpacing: 4, mainAxisSpacing: 8),
              delegate: SliverChildBuilderDelegate((context, i) => Center(child: AppIcon(a: l[i], size: 52, width: 80, key: ValueKey(l[i]['exec']))), childCount: l.length),
            );
        Widget head(String s) => SliverToBoxAdapter(child: Padding(padding: const EdgeInsets.fromLTRB(4, 16, 4, 8), child: Text(s, style: body(context, size: .75, w: FontWeight.w600, color: t.dim))));
        return CustomScrollView(slivers: [
          SliverPadding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
            sliver: SliverToBoxAdapter(
              child: TextField(
                controller: _q,
                onChanged: (_) => setState(() {}),
                decoration: InputDecoration(
                  isDense: true,
                  hintText: 'Search the Mac’s apps',
                  prefixIcon: Padding(padding: const EdgeInsets.all(10), child: WIcon('search', color: t.dim)),
                  filled: true,
                  fillColor: t.s1,
                  border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r3), borderSide: BorderSide.none),
                ),
              ),
            ),
          ),
          SliverPadding(
            padding: EdgeInsets.fromLTRB(16, 0, 16, 32 + MediaQuery.of(context).padding.bottom),
            sliver: SliverMainAxisGroup(slivers: [
              if (now.isNotEmpty) ...[head('OPEN NOW'), grid(now)],
              head(q.isEmpty ? 'ALL APPS' : 'MATCHES'),
              if (every.isEmpty)
                SliverToBoxAdapter(child: Padding(padding: const EdgeInsets.all(24), child: Center(child: Text(all.isEmpty ? 'Asking the Mac what it has…' : 'No app by that name.', style: body(context, color: t.dim)))))
              else
                grid(every),
            ]),
          ),
        ]);
      },
    );
  }
}
