// The app drawer (port of ui/drawer/*): a sheet of application icons over the picture. One box
// is both the filter and the command line — typing narrows the grid, and whatever is in it runs
// as-is with ▶ or Enter. Tap a tile to launch; a long-press puts its command in the box. 👁
// shows what is hidden (NoDisplay entries, and entries no icon theme draws).
import 'package:flutter/material.dart';

import '../../wado/relay_link.dart';
import '../../wado/session.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';

final drawerOpen = ValueNotifier<bool>(false);

/// Open the drawer, asking again for the list: it says which apps are running *now*.
void openDrawer() {
  drawerOpen.value = true;
  RelayLink.I.send({'type': 'apps_request'});
}

bool _listable(Map<String, dynamic> a) => a['hidden'] != true && a['icon'] != null;

/// What ▶ and Enter launch: a name or command of an installed app runs its `exec`; anything else
/// runs verbatim, flags and case kept (drawer/run.rs).
String? resolve(String typed, List<Map<String, dynamic>> apps) {
  final t = typed.trim();
  if (t.isEmpty) return null;
  final l = t.toLowerCase();
  for (final a in apps) {
    if ('${a['name']}'.toLowerCase() == l || '${a['exec']}'.toLowerCase() == l) return '${a['exec']}';
  }
  return t;
}

class AppDrawer extends StatefulWidget {
  const AppDrawer({super.key});
  @override
  State<AppDrawer> createState() => _AppDrawerState();
}

class _AppDrawerState extends State<AppDrawer> with SingleTickerProviderStateMixin {
  late final _a = AnimationController(vsync: this, duration: const Duration(milliseconds: 220));
  late final _box = TextEditingController(text: Settings.I.s('command'));

  @override
  void initState() {
    super.initState();
    drawerOpen.addListener(_sync);
  }

  @override
  void dispose() {
    drawerOpen.removeListener(_sync);
    _a.dispose();
    _box.dispose();
    super.dispose();
  }

  void _sync() => drawerOpen.value ? _a.forward(from: 0) : _a.reverse();

  void _launch(String cmd) {
    Session.I.launch(cmd);
    drawerOpen.value = false;
  }

  void _go() {
    final c = resolve(_box.text, Session.I.apps.value);
    if (c != null) _launch(c);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final mq = MediaQuery.of(context);
    final land = mq.size.width > mq.size.height && mq.size.height <= 520;
    return AnimatedBuilder(
      animation: _a,
      builder: (context, child) {
        if (_a.value == 0) return const SizedBox.shrink();
        final v = Curves.ease.transform(_a.value);
        return Align(
          alignment: Alignment.bottomCenter,
          child: Opacity(opacity: v, child: Transform.translate(offset: Offset(0, 40 * (1 - v)), child: child)),
        );
      },
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: land ? mq.size.height * .84 : (mq.size.height * .62).clamp(0, 560)),
        child: Container(
          padding: EdgeInsets.fromLTRB(12, 0, 12, 12 + mq.padding.bottom),
          decoration: BoxDecoration(
            color: t.base[1],
            border: Border(top: BorderSide(color: t.base[2])),
            borderRadius: const BorderRadius.vertical(top: Radius.circular(14)),
            boxShadow: const [BoxShadow(color: Color(0x59000000), blurRadius: 24, offset: Offset(0, -8))],
          ),
          child: ListenableBuilder(
            listenable: Listenable.merge([Session.I.apps, Settings.I, _box]),
            builder: (context, _) {
              final apps = Session.I.apps.value;
              final q = _box.text.trim().toLowerCase();
              final hidden = Settings.I.b('show_hidden');
              final found = apps.where((a) => q.isEmpty || '${a['name']}'.toLowerCase().contains(q) || '${a['exec']}'.toLowerCase().contains(q)).toList();
              final buried = found.where((a) => !_listable(a)).length;
              final matches = found.where((a) => hidden || _listable(a)).take(400).toList();
              final recent = [
                for (final c in Settings.I.get<List<String>>('recent').take(4))
                  apps.firstWhere((a) => a['exec'] == c, orElse: () => {'name': c, 'exec': c}),
              ];
              final eye = hidden ? 'Show only applications' : 'Show everything installed ($buried more)';
              Widget bar(String glyph, String label, bool on, VoidCallback? tap) => Press(
                    onTap: tap,
                    label: label,
                    child: Opacity(
                      opacity: tap == null ? .4 : 1,
                      child: Container(
                        width: 44,
                        decoration: BoxDecoration(color: on ? t.accent : null, borderRadius: BorderRadius.circular(999)),
                        child: Center(child: Text(glyph, style: TextStyle(fontSize: 18, color: on ? t.onAccent : t.text))),
                      ),
                    ),
                  );
              Widget grid(List<Map<String, dynamic>> list) => LayoutBuilder(builder: (context, box) {
                    final min = (mq.size.width * .2).clamp(74.0, 96.0);
                    final cols = ((box.maxWidth + 6) / (min + 6)).floor().clamp(1, 20);
                    final w = (box.maxWidth - 6 * (cols - 1)) / cols;
                    return Wrap(spacing: 6, runSpacing: 6, children: [for (final a in list) SizedBox(width: w, child: _Tile(a: a, onTap: _launch, onHold: (c) => _box.text = c))]);
                  });
              return Column(mainAxisSize: MainAxisSize.min, children: [
                Press(
                  onTap: () => drawerOpen.value = false,
                  label: 'Close app drawer',
                  scale: 1,
                  child: SizedBox(
                    height: land ? 20 : 28,
                    width: 120,
                    child: Align(
                      alignment: Alignment.topCenter,
                      child: Container(margin: EdgeInsets.only(top: land ? 8 : 12), width: 44, height: 4, decoration: BoxDecoration(color: t.base[3], borderRadius: BorderRadius.circular(2))),
                    ),
                  ),
                ),
                SizedBox(
                  height: 44,
                  child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                    Expanded(
                      child: TextField(
                        controller: _box,
                        textInputAction: TextInputAction.go,
                        autocorrect: false,
                        onChanged: (v) => Settings.I.set('command', v),
                        onSubmitted: (_) => _go(),
                        style: body(context, size: .9375, color: t.bright),
                        decoration: InputDecoration(
                          hintText: 'search apps, or type a command',
                          hintStyle: body(context, size: .9375, color: t.dim),
                          filled: true,
                          fillColor: t.s0,
                          contentPadding: const EdgeInsets.symmetric(horizontal: 12),
                          border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r2), borderSide: BorderSide(color: t.line)),
                          enabledBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r2), borderSide: BorderSide(color: t.line)),
                        ),
                      ),
                    ),
                    const SizedBox(width: 6),
                    bar('👁', eye, hidden, buried == 0 && !hidden ? null : () => Settings.I.set('show_hidden', !hidden)),
                    const SizedBox(width: 6),
                    bar('▶\uFE0E', 'Run', false, _box.text.trim().isEmpty ? null : _go),
                  ]),
                ),
                Flexible(
                  child: SingleChildScrollView(
                    padding: const EdgeInsets.only(top: 4),
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      if (recent.isNotEmpty && q.isEmpty) ...[
                        _label(context, 'Recent'),
                        grid(recent),
                        _label(context, 'All apps'),
                      ],
                      if (matches.isEmpty)
                        Padding(
                          padding: const EdgeInsets.symmetric(vertical: 12, horizontal: 2),
                          child: Text(
                            apps.isEmpty
                                ? 'No application list from the server — ▶ still runs whatever you type.'
                                : (buried > 0 ? 'Nothing here matches — 👁 searches everything installed too. ▶ runs what you typed.' : 'Nothing matches. ▶ still runs what you typed.'),
                            style: body(context, size: .8125, color: t.dim),
                          ),
                        )
                      else
                        grid(matches),
                    ]),
                  ),
                ),
              ]);
            },
          ),
        ),
      ),
    );
  }

  Widget _label(BuildContext context, String s) => Padding(padding: const EdgeInsets.fromLTRB(2, 10, 2, 4), child: Text(s, style: body(context, size: .8125, color: Tok.of(context).dim)));
}

class _Tile extends StatelessWidget {
  const _Tile({required this.a, required this.onTap, required this.onHold});
  final Map<String, dynamic> a;
  final ValueChanged<String> onTap, onHold;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final mq = MediaQuery.of(context);
    final s = (mq.size.width * .11).clamp(38.0, 48.0);
    final name = '${a['name']}';
    final exec = '${a['exec']}';
    final running = a['running'] == true;
    final icon = a['icon'] as String?;
    Widget img = Container(
      decoration: BoxDecoration(color: t.base[2], borderRadius: BorderRadius.circular(12)),
      child: Center(child: Text(name.isEmpty ? '' : name[0].toUpperCase(), style: TextStyle(fontSize: (mq.size.width * .05).clamp(16, 22), color: t.base[4]))),
    );
    if (icon != null && icon.startsWith('data:')) {
      try {
        img = Image.memory(UriData.parse(icon).contentAsBytes(), fit: BoxFit.contain, gaplessPlayback: true);
      } catch (_) {}
    }
    return Press(
      onTap: () => onTap(exec),
      onLongPress: () => onHold(exec),
      label: running ? '$name — running' : name,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 10, horizontal: 4),
        child: Column(children: [
          SizedBox(
            width: s,
            height: s,
            child: Stack(clipBehavior: Clip.none, children: [
              Positioned.fill(child: img),
              if (running)
                Positioned(
                  right: -2,
                  bottom: -2,
                  child: Container(width: 10, height: 10, decoration: BoxDecoration(shape: BoxShape.circle, color: t.base[0xB], boxShadow: [BoxShadow(color: t.base[1], spreadRadius: 2)])),
                ),
            ]),
          ),
          const SizedBox(height: 6),
          Text(name, maxLines: 2, overflow: TextOverflow.ellipsis, textAlign: TextAlign.center, style: body(context, size: .6875, color: running ? t.base[6] : t.base[5]).copyWith(height: 1.25)),
        ]),
      ),
    );
  }
}
