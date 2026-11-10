// The Mac's own apps (desk_apps.dart): a shelf like its Dock on the computer page, every app on
// a page of its own ("All apps"), and the same launcher as a sheet over the touchpad and keyboard
// for opening something while you drive the Mac.
//
// The shelf holds what the Mac's Dock keeps, in its order, then — past a divider — what runs that
// the Dock does not. Each icon says its state the way the Dock does: a dot under what runs, the
// app in front raised and lit, a hidden one dimmed and set back, one just opened bouncing until
// it runs. Tap opens it or brings it forward; a long press is the Dock's menu.
//
// Motion (DESIGN.md): --pop for what a finger flipped (the dot), --spring for travel (the lift, the
// set-back), --dur everywhere; the launch bounce is the Dock's own: up decelerating, down
// accelerating, until the app runs. Reduced motion keeps every state and drops the travel.
import 'package:flutter/material.dart';

import '../../wado/desk_apps.dart';
import '../../wado/host_caps.dart';
import '../../wado/multi.dart';
import '../../wado/nav.dart';
import '../../wado/relay_link.dart';
import '../../wado/session.dart';
import '../motion/feel.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/data_image.dart';

/// The selected computer is a Mac with its own apps.
bool get macApps => HostCaps.now.mac && HostCaps.now.has('desk_apps');

/// Linked to the Mac now. Not: its kept apps still show, dimmed, and a tap says why nothing opens.
bool get _linked => RelayLink.I.up.value;

/// What to show: offline, the kept list without its last states — nothing is known to run.
List<App> _shown() => _linked ? Session.I.apps.value : [for (final a in Session.I.apps.value) {...a, 'running': false, 'front': false, 'away': false}];

final _apps = Listenable.merge([Session.I.apps, RelayLink.I.up]);

/// Said once over the apps while the Mac is not connected.
class _NotLinked extends StatelessWidget {
  const _NotLinked();
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Row(children: [
        WIcon('alert', color: t.dim),
        const SizedBox(width: 8),
        Expanded(child: Text('Not connected — these open once the Mac is back.', style: body(context, size: .8125, color: t.dim))),
      ]),
    );
  }
}

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

/// The shelf on the computer page.
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
      ListenableBuilder(
        listenable: _apps,
        builder: (context, _) {
          final all = _shown();
          final (kept, extra) = dockGroups(all);
          if (all.isEmpty) return _Waiting(t: t);
          final shelf = Container(
            decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r4)),
            padding: const EdgeInsets.symmetric(vertical: 12),
            child: LayoutBuilder(builder: (context, box) {
              const w = 80.0;
              final icons = [
                for (final a in kept) AppIcon(a: a, key: ValueKey('k${a['exec']}')),
                if (kept.isNotEmpty && extra.isNotEmpty) Container(key: const ValueKey('div'), width: 1, height: 56, margin: const EdgeInsets.only(top: 4), color: t.line),
                for (final a in extra) AppIcon(a: a, key: ValueKey('x${a['exec']}')),
              ];
              // Room for every icon: they all show, in rows. Otherwise a strip the thumb scrolls.
              final fits = (kept.length + extra.length) * w + 24 <= box.maxWidth * 2;
              if (fits && box.maxWidth >= 600) {
                return Padding(padding: const EdgeInsets.symmetric(horizontal: 8), child: Wrap(alignment: WrapAlignment.center, runSpacing: 12, children: icons));
              }
              return SizedBox(
                height: 108,
                child: ListView(scrollDirection: Axis.horizontal, padding: const EdgeInsets.symmetric(horizontal: 8), children: icons),
              );
            }),
          );
          return _linked ? shelf : Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [const _NotLinked(), shelf]);
        },
      ),
    ]);
  }
}

class _Waiting extends StatelessWidget {
  const _Waiting({required this.t});
  final Tokens t;
  @override
  Widget build(BuildContext context) => Container(
        height: 132,
        alignment: Alignment.center,
        decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r4)),
        child: Text(_linked ? 'Asking the Mac what it has…' : 'Its apps show here once it has been connected.', style: body(context, size: .8125, color: t.dim)),
      );
}

/// One app: its icon (a letter tile until the icon arrives), its name, its state.
class AppIcon extends StatefulWidget {
  const AppIcon({super.key, required this.a, this.size = 64, this.width = 80});
  final App a;
  final double size, width;
  @override
  State<AppIcon> createState() => _AppIconState();
}

class _AppIconState extends State<AppIcon> with SingleTickerProviderStateMixin {
  late final _bounce = AnimationController(vsync: this, duration: const Duration(milliseconds: 360));

  String get _exec => '${widget.a['exec']}';

  @override
  void initState() {
    super.initState();
    DeskApps.I.launching.addListener(_launching);
    _launching();
  }

  @override
  void didUpdateWidget(AppIcon old) {
    super.didUpdateWidget(old);
    _launching();
  }

  /// The Dock's launch bounce: up and down until the app runs, then it settles.
  void _launching() {
    final on = DeskApps.I.launching.value.contains(_exec) && widget.a['running'] != true;
    if (on && !_bounce.isAnimating) {
      _bounce.repeat(reverse: true);
    } else if (!on && _bounce.isAnimating) {
      _bounce.animateBack(0, duration: const Duration(milliseconds: 160), curve: Curves.easeOut);
    }
  }

  @override
  void dispose() {
    DeskApps.I.launching.removeListener(_launching);
    _bounce.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final a = widget.a;
    final run = a['running'] == true, front = a['front'] == true, away = a['away'] == true;
    final name = '${a['name']}';
    final s = widget.size;
    final r = BorderRadius.circular(s * .225);
    final tile = DecoratedBox(
      decoration: BoxDecoration(color: t.s3, borderRadius: r),
      child: Center(child: Text(name.isEmpty ? '?' : name.characters.first.toUpperCase(), style: display(context, s * .42).copyWith(color: t.bright))),
    );
    final on = _linked;
    void why() => Multi.I.note('Not connected — $name opens once the Mac is back.');
    return Semantics(
      button: true,
      label: on ? '$name, ${stateOf(a)}' : '$name, not available — not connected',
      child: Press(
        onTap: () {
          if (!on) return why();
          feel('tap');
          DeskApps.I.act(a, tapAction(a));
        },
        onLongPress: () {
          if (!on) return why();
          feel('claim');
          showAppMenu(context, a);
        },
        label: name,
        child: Opacity(opacity: on ? 1 : .45, child: SizedBox(
          width: widget.width,
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            SizedBox(
              width: s,
              height: s + 8,
              child: AnimatedBuilder(
                animation: _bounce,
                builder: (context, child) {
                  // Up decelerating, down accelerating: a ball, not a spring. Reduced motion: still.
                  final hop = t.reduced ? 0.0 : Curves.easeOut.transform(_bounce.value) * 12;
                  return Transform.translate(offset: Offset(0, 8 - hop), child: child);
                },
                child: AnimatedSlide(
                  // The app in front stands a little proud of the rest.
                  offset: Offset(0, front && !t.reduced ? -.06 : 0),
                  duration: t.dur,
                  curve: Tokens.spring,
                  child: AnimatedScale(
                    // A hidden app is set back as well as dimmed: its state is not colour alone.
                    scale: away ? .9 : 1,
                    duration: t.dur,
                    curve: Tokens.spring,
                    child: AnimatedOpacity(
                      opacity: away ? .45 : 1,
                      duration: t.dur,
                      child: SizedBox(width: s, height: s, child: DataPicture(a['icon'] as String?, size: s, fallback: tile)),
                    ),
                  ),
                ),
              ),
            ),
            const SizedBox(height: 6),
            // The running dot pops in (--pop); lit for the app in front.
            SizedBox(
              height: 6,
              child: AnimatedScale(
                scale: run ? 1 : 0,
                duration: t.dur,
                curve: t.pop,
                child: AnimatedContainer(
                  duration: t.dur,
                  width: front ? 6 : 5,
                  height: front ? 6 : 5,
                  decoration: BoxDecoration(color: front ? t.accent : t.text, shape: BoxShape.circle),
                ),
              ),
            ),
            const SizedBox(height: 4),
            Text(name, maxLines: 1, overflow: TextOverflow.ellipsis, textAlign: TextAlign.center, style: body(context, size: .75, color: away ? t.dim : t.text)),
          ]),
        )),
      ),
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
    showDragHandle: true,
    builder: (s) => SafeArea(
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
          child: Row(children: [
            SizedBox(width: 48, height: 48, child: DataPicture(a['icon'] as String?, size: 48, fallback: WIcon('apps', color: t.bright))),
            const SizedBox(width: 14),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text(name, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1.125))),
                Text(stateOf(a), style: body(context, size: .8125, color: a['front'] == true ? t.accent : t.dim)),
              ]),
            ),
          ]),
        ),
        for (final (action, label, danger) in menuFor(a))
          ListTile(
            minTileHeight: 52,
            leading: WIcon(switch (action) { 'hide' => 'eye', 'quit' => 'x', 'force_quit' => 'alert', _ => 'play' }, color: danger ? t.bad : t.bright),
            title: Text(label, style: body(context, w: FontWeight.w600, color: danger ? t.bad : t.bright)),
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

/// Every app on the Mac: those open now, then all of them A–Z; a search narrows both. A page of
/// its own, or (`sheet`) the launcher over the touchpad and keyboard.
class MacAppsPage extends StatefulWidget {
  const MacAppsPage({super.key, this.scroll});
  /// The sheet's own scroll, so dragging the list drags the sheet.
  final ScrollController? scroll;
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
    return ListenableBuilder(
      listenable: _apps,
      builder: (context, _) {
        final all = _shown();
        final q = _q.text.trim().toLowerCase();
        bool hit(App a) => q.isEmpty || '${a['name']}'.toLowerCase().contains(q);
        final now = openNow(all).where(hit).toList();
        final every = [for (final a in all) if (hit(a)) a]..sort((a, b) => '${a['name']}'.toLowerCase().compareTo('${b['name']}'.toLowerCase()));
        SliverGrid grid(List<App> l, String tag) => SliverGrid(
              gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(maxCrossAxisExtent: 96, mainAxisExtent: 116, crossAxisSpacing: 4, mainAxisSpacing: 12),
              delegate: SliverChildBuilderDelegate((context, i) => Center(child: AppIcon(a: l[i], key: ValueKey('$tag${l[i]['exec']}'))), childCount: l.length),
            );
        Widget head(String s) => SliverToBoxAdapter(child: Padding(padding: const EdgeInsets.fromLTRB(4, 20, 4, 10), child: Text(s, style: body(context, w: FontWeight.w600, color: t.dim))));
        return CustomScrollView(controller: widget.scroll, slivers: [
          SliverPadding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
            sliver: SliverToBoxAdapter(
              child: TextField(
                controller: _q,
                onChanged: (_) => setState(() {}),
                style: body(context, size: 1),
                decoration: InputDecoration(
                  hintText: 'Search the Mac’s apps',
                  prefixIcon: Padding(padding: const EdgeInsets.all(12), child: WIcon('search', color: t.dim)),
                  filled: true,
                  fillColor: t.s2,
                  border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r3), borderSide: BorderSide.none),
                ),
              ),
            ),
          ),
          if (!_linked) const SliverPadding(padding: EdgeInsets.fromLTRB(16, 12, 16, 0), sliver: SliverToBoxAdapter(child: _NotLinked())),
          SliverPadding(
            padding: EdgeInsets.fromLTRB(12, 0, 12, 32 + MediaQuery.of(context).padding.bottom),
            sliver: SliverMainAxisGroup(slivers: [
              if (now.isNotEmpty) ...[head('Open now'), grid(now, 'n')],
              head(q.isEmpty ? 'All apps' : 'Matches'),
              if (every.isEmpty)
                SliverToBoxAdapter(
                  child: Padding(
                    padding: const EdgeInsets.all(24),
                    child: Center(child: Text(all.isEmpty ? (_linked ? 'Asking the Mac what it has…' : 'Its apps show here once it has been connected.') : 'No app is called “${_q.text.trim()}”.', style: body(context, color: t.dim))),
                  ),
                )
              else
                grid(every, 'a'),
            ]),
          ),
        ]);
      },
    );
  }
}

/// The launcher over whatever is on screen (the touchpad, the keyboard): drag it up for more,
/// down to go back to driving.
void showAppsSheet(BuildContext context) {
  final t = Tok.of(context);
  showModalBottomSheet(
    context: context,
    backgroundColor: t.s1,
    isScrollControlled: true,
    showDragHandle: true,
    shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(t.r4))),
    builder: (s) => DraggableScrollableSheet(
      expand: false,
      initialChildSize: .62,
      minChildSize: .35,
      maxChildSize: .94,
      builder: (context, scroll) => MacAppsPage(scroll: scroll),
    ),
  );
}
