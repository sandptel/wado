// Home's widgets (port of ui/home/widgets/*.rs + home.css widgets + widget_flip/drag/touch.js):
// the few controls wanted without opening a computer, each bound to one computer, in the order
// this phone keeps them. A widget is never hidden for a computer that is away or a grant that is
// missing: it says so, in its body, with what would fix it. In edit mode the bodies fold away and
// each header grows remove, ↑ ↓ and a drag grip; the Add tray and the computer picker sheet.
import 'dart:ui' as ui show Clip;

import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/claude.dart';
import '../../wado/clip.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/reach.dart';
import '../../wado/relay_link.dart';
import '../../wado/settings.dart';
import '../../wado/side_links.dart';
import '../../wado/widgets.dart';
import '../computer/actions.dart' show openDoor, Door;
import '../computer/claude.dart' show ClaudeCard;
import '../motion/feel.dart';
import '../motion/motion.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/slider.dart';
import 'cards.dart' show hues, Dashed;
import 'home_screen.dart' show LinkWords;

/// Keep a light link to every computer a widget follows, other than the selected one — and none
/// while a session streams (widgets/links.rs).
void watchLinks({bool session = false}) {
  final sel = Nav.I.selected;
  final want = <String, Set<String>>{};
  for (final w in Widgets.list) {
    if (w.host == sel) continue;
    (want[w.host] ??= {}).addAll([if (w.kind == WidgetKind.clipboard) 'clips', if (w.kind == WidgetKind.trackpad) 'actions']);
  }
  SideLinks.I.watch(session
      ? const []
      : [
          for (final e in want.entries)
            if (CoreHost.I.computer(e.key) case final c?) (c.key, c.relayUrl, c.remoteId, e.value.toList()),
        ]);
}

class WidgetsSection extends StatefulWidget {
  const WidgetsSection({super.key});
  @override
  State<WidgetsSection> createState() => _WidgetsSectionState();
}

class _WidgetsSectionState extends State<WidgetsSection> {
  bool _edit = false;

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([Settings.I, CoreHost.I.computers, SideLinks.I.state, Host.I.state, Host.I.config, RelayLink.I.up, Claude.I.cards, Clip.I.entries]),
      builder: (context, _) {
        if (CoreHost.I.computers.value.isEmpty) return const SizedBox.shrink();
        final list = Widgets.list;
        final n = list.length;
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Sect(
            'Widgets',
            action: n > 0 || _edit ? (_edit ? 'Done' : 'Edit') : null,
            onAction: () {
              buzz(6);
              setState(() => _edit = !_edit);
            },
          ),
          const SizedBox(height: 8),
          if (n == 0 && !_edit)
            Press(
              onTap: () => setState(() => _edit = true),
              label: 'Add a widget',
              child: Dashed(
                color: t.line,
                radius: t.r3,
                child: SizedBox(
                  height: 52,
                  child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                    WIcon('plus', color: t.dim),
                    const SizedBox(width: 8),
                    Text('Add a widget', style: body(context, w: FontWeight.w600, color: t.dim)),
                  ]),
                ),
              ),
            ),
          // Drag the grip to reorder: the card lifts (1.03, the popover shadow), the others shift
          // on the spring, and it settles with a pop where it lands.
          ReorderableListView(
            shrinkWrap: true,
            physics: const NeverScrollableScrollPhysics(),
            buildDefaultDragHandles: false,
            onReorderStart: (_) => buzz(8),
            onReorder: (a, b) {
              Widgets.place(a, b > a ? b - 1 : b);
              buzz(4);
            },
            proxyDecorator: (child, i, a) => AnimatedBuilder(
              animation: a,
              child: child,
              builder: (context, child) {
                final v = Tokens.popCurve.transform(a.value.clamp(0, 1));
                return Transform.scale(
                  scale: 1 + .03 * v,
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(t.r3),
                      boxShadow: [BoxShadow(color: Color.fromRGBO(0, 0, 0, .67 * v), blurRadius: 50, offset: const Offset(0, 18), spreadRadius: -20)],
                    ),
                    child: child,
                  ),
                );
              },
            ),
            children: [
              for (var k = 0; k < n; k++)
                Padding(
                  key: ValueKey(list[k].id),
                  padding: const EdgeInsets.only(bottom: 8),
                  child: _Card(w: list[k], k: k, n: n, editing: _edit, onPick: () => _pick(context, list[k])),
                ),
            ],
          ),
          if (_edit) ...[
            Enter(
              delay: const Duration(milliseconds: 60),
              ms: 420,
              dy: 12,
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4),
                child: Text('Drag the dots to reorder, or use the arrows. Each widget follows one computer — tap its name to change it.', style: body(context, size: .75, color: t.dim)),
              ),
            ),
            Enter(
              delay: const Duration(milliseconds: 80),
              ms: 420,
              dy: 12,
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                const Sect('Add a widget'),
                const SizedBox(height: 8),
                LayoutBuilder(builder: (context, box) {
                  final cols = (box.maxWidth / 112).floor().clamp(1, 6);
                  final w = (box.maxWidth - 8 * (cols - 1)) / cols;
                  return Wrap(spacing: 8, runSpacing: 8, children: [
                    for (final kind in WidgetKind.values)
                      SizedBox(
                        width: w,
                        child: Press(
                          onTap: () => Widgets.add(kind, Nav.I.selected.isEmpty ? CoreHost.I.computers.value.first.key : Nav.I.selected),
                          label: 'Add ${kind.short}',
                          child: Dashed(color: t.line, radius: t.r2, child: Container(
                            constraints: const BoxConstraints(minHeight: 48),
                            padding: const EdgeInsets.symmetric(horizontal: 12),
                            child: Row(children: [
                              WIcon('plus', size: 18, color: t.accent),
                              const SizedBox(width: 8),
                              Flexible(child: Text(kind.short, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .8125, w: FontWeight.w600))),
                            ]),
                          )),
                        ),
                      ),
                  ]);
                }),
              ]),
            ),
          ],
        ]);
      },
    );
  }

  /// The host picker (widgets/picker.rs): which computer a widget follows. The chosen row's check
  /// pops; the sheet closes a beat later.
  void _pick(BuildContext context, HomeWidget w) {
    final t = Tok.of(context);
    showModalBottomSheet(
      context: context,
      backgroundColor: t.s0,
      showDragHandle: true,
      sheetAnimationStyle: AnimationStyle(duration: t.durSheet, curve: Tokens.spring, reverseCurve: Tokens.spring),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(t.r4))),
      builder: (ctx) => StatefulBuilder(builder: (ctx, set) {
        final cur = Widgets.list.where((x) => x.id == w.id).firstOrNull?.host ?? w.host;
        final comps = CoreHost.I.computers.value;
        return SafeArea(
          top: false,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 20),
            child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Text('${w.kind.label} shows', style: display(ctx, rem(ctx, 1.25))),
              const SizedBox(height: 6),
              Text('One computer per widget. Add another ${w.kind.short.toLowerCase()} widget to see a second one.', style: body(ctx, size: .8125, color: t.dim)),
              const SizedBox(height: 10),
              ListBox(children: [
                for (var i = 0; i < comps.length; i++)
                  Press(
                    scale: 1,
                    onTap: () {
                      Widgets.rebind(w.id, comps[i].key);
                      set(() {});
                      Future.delayed(const Duration(milliseconds: 260), () {
                        if (ctx.mounted) Navigator.of(ctx).pop();
                      });
                    },
                    child: Container(
                      constraints: const BoxConstraints(minHeight: 60),
                      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                      child: Row(children: [
                        Disc(hue: slot(ctx, hues[i % 6]), child: WIcon('monitor', color: slot(ctx, hues[i % 6]))),
                        const SizedBox(width: 14),
                        Expanded(
                          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                            Text(comps[i].name, style: body(ctx, w: FontWeight.w600, color: t.bright)),
                            LinkWords(tone: Reach.of(comps[i]).tone, words: Reach.of(comps[i]).words),
                          ]),
                        ),
                        AnimatedScale(
                          scale: comps[i].key == cur ? 1 : 0,
                          duration: const Duration(milliseconds: 300),
                          curve: Tokens.popCurve,
                          child: Container(
                            width: 28,
                            height: 28,
                            decoration: BoxDecoration(shape: BoxShape.circle, color: t.accent),
                            alignment: Alignment.center,
                            child: WIcon('check', size: 16, color: t.onAccent),
                          ),
                        ),
                      ]),
                    ),
                  ),
              ]),
            ]),
          ),
        );
      }),
    );
  }
}

/// One widget's card (widgets/shell.rs): what it is, a chip naming the computer it follows (its
/// status dot, its name; a tap opens that computer's page, or in edit mode picks another), and
/// its body. In edit mode the body folds and the header grows remove, ↑ ↓ and the grip.
class _Card extends StatelessWidget {
  const _Card({required this.w, required this.k, required this.n, required this.editing, required this.onPick});
  final HomeWidget w;
  final int k, n;
  final bool editing;
  final VoidCallback onPick;

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final comps = CoreHost.I.computers.value;
    final i = comps.indexWhere((c) => c.key == w.host);
    final c = i < 0 ? null : comps[i];
    final name = c?.name ?? 'A forgotten computer';
    final r = c == null ? const Reach(tone: 'bad', words: 'Forgotten', online: false) : Reach.of(c);
    final away = !r.online && r.tone == 'bad';
    final title = Text(
      w.kind.label.toUpperCase(),
      style: TextStyle(fontFamily: Tokens.body, fontSize: rem(context, .6875), fontWeight: FontWeight.w600, letterSpacing: rem(context, .6875) * .1, color: t.dim),
    );
    final chip = Press(
      onTap: editing ? onPick : () => c != null ? Nav.I.openComputer(c.key) : null,
      label: editing ? '${w.kind.short} shows $name — change computer' : '$name — open its page',
      child: Container(
        constraints: const BoxConstraints(minHeight: 28),
        padding: editing ? EdgeInsets.zero : const EdgeInsets.fromLTRB(8, 0, 10, 0),
        decoration: BoxDecoration(color: editing ? null : t.s2, borderRadius: BorderRadius.circular(999)),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          if (!editing) ...[LinkWords(tone: r.tone, words: ''), const SizedBox(width: 2)],
          Flexible(child: Text(name, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, w: FontWeight.w600, color: editing ? t.accent : t.text))),
          if (editing) ...[const SizedBox(width: 4), WIcon('down', size: 14, color: t.accent)],
        ]),
      ),
    );
    return AnimatedContainer(
      duration: const Duration(milliseconds: 360),
      curve: Tokens.spring,
      padding: editing ? const EdgeInsets.fromLTRB(8, 8, 6, 8) : const EdgeInsets.fromLTRB(14, 12, 14, 14),
      decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        ConstrainedBox(
          constraints: BoxConstraints(minHeight: editing ? 48 : 28),
          child: Row(children: [
            if (editing) ...[
              Press(
                onTap: () => Widgets.remove(w.id),
                label: 'Remove the ${w.kind.short} widget',
                child: SizedBox(
                  width: 44,
                  height: 44,
                  child: Center(
                    child: Enter(
                      delay: Duration(milliseconds: k * 30),
                      ms: 360,
                      dy: 0,
                      scale: 0,
                      pop: true,
                      child: Container(
                        width: 26,
                        height: 26,
                        decoration: BoxDecoration(shape: BoxShape.circle, color: fade(t.bad, 80)),
                        alignment: Alignment.center,
                        child: WIcon('minus', size: 15, color: t.bad),
                      ),
                    ),
                  ),
                ),
              ),
              Enter(
                delay: Duration(milliseconds: k * 30 + 40),
                ms: 360,
                dy: 0,
                scale: 0,
                pop: true,
                child: Container(
                  width: 38,
                  height: 38,
                  decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r2)),
                  alignment: Alignment.center,
                  child: WIcon(w.kind.icon, size: 18, color: t.bright),
                ),
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text(w.kind.label, style: body(context, w: FontWeight.w600, color: t.bright)),
                  chip,
                ]),
              ),
              Enter(
                delay: Duration(milliseconds: k * 30),
                ms: 420,
                dy: 0,
                dx: 24,
                child: Row(mainAxisSize: MainAxisSize.min, children: [
                  SizedBox(width: 38, child: IconBtn('up', label: 'Move up', onTap: k == 0 ? null : () => Widgets.step(w.id, -1))),
                  SizedBox(width: 38, child: IconBtn('down', label: 'Move down', onTap: k + 1 == n ? null : () => Widgets.step(w.id, 1))),
                  ReorderableDragStartListener(
                    index: k,
                    child: SizedBox(width: 44, height: 48, child: Center(child: WIcon('grip', color: t.dim))),
                  ),
                ]),
              ),
            ] else ...[
              WIcon(w.kind.icon, size: 16, color: t.dim),
              const SizedBox(width: 8),
              w.kind == WidgetKind.clipboard
                  ? Press(onTap: c == null ? null : () => Nav.I.openComputer(c.key), scale: 1, child: Row(children: [title, const SizedBox(width: 6), WIcon('right', size: 12, color: t.dim)]))
                  : title,
              const Spacer(),
              Flexible(child: Align(alignment: Alignment.centerRight, child: chip)),
            ],
          ]),
        ),
        AnimatedSize(
          duration: const Duration(milliseconds: 360),
          curve: Tokens.spring,
          alignment: Alignment.topCenter,
          child: editing
              ? const SizedBox(width: double.infinity)
              : Enter(
                  key: ValueKey('${w.host}${w.kind}'),
                  ms: 420,
                  dy: -6,
                  fadeOnClose: false,
                  child: Padding(
                    padding: const EdgeInsets.only(top: 10),
                    child: Opacity(opacity: away ? .55 : 1, child: _body(context, c, name, r, away)),
                  ),
                ),
        ),
      ]),
    );
  }

  Widget _body(BuildContext context, Computer? c, String name, Reach r, bool away) {
    final t = Tok.of(context);
    Widget note(String icon, Widget text, {bool quiet = false}) => ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 32),
          child: Row(children: [
            WIcon(icon, size: 18, color: t.dim),
            const SizedBox(width: 8),
            Expanded(child: DefaultTextStyle(style: body(context, size: .7812, color: quiet ? t.dim : t.text), child: text)),
          ]),
        );
    if (c == null) return note('alert', const Text('Its computer was forgotten. Remove this widget, or pick another computer.'));
    if (away) return note('alert', Text.rich(TextSpan(children: [TextSpan(text: '$name is offline.', style: TextStyle(color: t.bad, fontWeight: FontWeight.w600)), const TextSpan(text: ' This fills in when it is back.')])));
    if (r.nolive.isNotEmpty) {
      return note('info', Text('${r.nolive == 'old_relay' ? 'Live values for $name need a newer relay.' : 'Update wado on $name to see live values here.'} Open its page to control it.'), quiet: true);
    }
    final sel = c.key == Nav.I.selected;
    final cfg = sel ? Host.I.config.value : SideLinks.I.of(c.key).config;
    final host = sel ? Host.I.state.value : SideLinks.I.of(c.key).host;
    final g = cfg?['grants'] as Map?;
    if (g != null && (w.kind.needs == 'clipboard' ? g['clipboard'] == false : g[w.kind.needs] == false)) {
      final what = switch (w.kind) {
        WidgetKind.clipboard => "see $name's clipboard",
        WidgetKind.trackpad => "use $name's touchpad, keyboard or gamepad",
        WidgetKind.claude => 'see Claude on $name',
        WidgetKind.media => "control $name's media",
        WidgetKind.brightness => "set $name's brightness",
        _ => "control $name's volume",
      };
      return note('lock', Text('This phone may not $what. Open its page to see how to allow it.'));
    }
    Widget asking() => Row(children: [
          SizedBox(width: 14, height: 14, child: CircularProgressIndicator(strokeWidth: 2, color: t.accent, backgroundColor: t.s3)),
          const SizedBox(width: 8),
          Text('Asking $name…', style: body(context, size: .7812, color: t.dim)),
        ]);
    switch (w.kind) {
      case WidgetKind.media:
        if (host == null) return asking();
        final media = (host['media'] as List? ?? []);
        if (media.isEmpty) return note('music', Text('Nothing is playing on $name.'), quiet: true);
        return _Media(k: c.key, p: Map<String, dynamic>.from(media.first as Map), host: host);
      case WidgetKind.volume:
        if (host == null) return asking();
        final a = host['audio'] as Map? ?? {};
        final sinks = [for (final s in (a['sinks'] as List? ?? [])) Map<String, dynamic>.from(s as Map)];
        final s = sinks.where((x) => x['name'] == a['default_sink']).firstOrNull ?? sinks.firstOrNull;
        if (s == null) return note('sound', Text('$name has no speakers wado can see.'), quiet: true);
        final muted = s['muted'] == true;
        return Row(children: [
          Press(
            onTap: () => actOn(c.key, {'do': 'sink_mute', 'id': s['id'], 'muted': !muted}),
            label: muted ? 'Unmute $name' : 'Mute $name',
            child: AnimatedContainer(
              duration: t.dur,
              width: 44,
              height: 44,
              decoration: BoxDecoration(shape: BoxShape.circle, color: muted ? fade(t.bad, 86) : null),
              child: Center(child: Enter(key: ValueKey(muted), ms: 260, dy: 0, scale: .4, pop: true, fadeOnClose: false, child: WIcon(muted ? 'mute' : 'sound', color: muted ? t.bad : t.bright))),
            ),
          ),
          const SizedBox(width: 6),
          Expanded(
            child: ThickSlider(
              label: 'Volume',
              value: ((s['volume'] ?? 0) as num).toDouble().clamp(0, 1),
              shown: muted ? 'muted' : '',
              onChange: (v) => actOn(c.key, {'do': 'sink_volume', 'id': s['id'], 'volume': v}),
            ),
          ),
        ]);
      case WidgetKind.brightness:
        if (host == null) return asking();
        final b = host['brightness'];
        if (b == null) return note('sun', Text('$name has no screen brightness wado can set.'), quiet: true);
        return ThickSlider(label: 'Brightness', icon: 'sun', value: (b as num).toDouble(), step: .1, ticks: 10, onChange: (v) => actOn(c.key, {'do': 'brightness', 'level': v}));
      case WidgetKind.clipboard:
        final entries = sel ? Clip.I.entries.value : SideLinks.I.of(c.key).clips;
        if (entries == null || (sel && !Clip.I.loaded.value)) return asking();
        return _Clips(k: c.key, name: name, entries: entries);
      case WidgetKind.trackpad:
        return _Pad(c: c, name: name, sel: sel, cfg: cfg);
      case WidgetKind.claude:
        if (!sel) {
          return Press(
            onTap: () => Nav.I.openComputer(c.key),
            scale: 1,
            child: note('term', Text("Claude on $name shows while $name is the computer you're on — tap to open it."), quiet: true),
          );
        }
        final st = Claude.I.cards.value;
        if (st == null) return note('term', Text('Asking $name…'), quiet: true);
        final live = [for (final s in (st['sessions'] as List? ?? [])) Map<String, dynamic>.from(s as Map)].where((s) => s['state'] != 'ended').toList()
          ..sort((a, b) => (a['ask'] == null ? 1 : 0) - (b['ask'] == null ? 1 : 0));
        if (st['hooks'] != true && live.isEmpty) return note('info', Text('Run `wado claude hooks install` on $name to see Claude here.'), quiet: true);
        if (live.isEmpty) return note('term', Text('No Claude sessions on $name.'), quiet: true);
        return Column(children: [for (final x in live.take(3)) Padding(padding: const EdgeInsets.only(bottom: 8), child: ClaudeCard(c: x, compact: true))]);
    }
  }
}

/// The Media widget: cover, title, artist and where it plays, play/pause and next, and a thin
/// progress line that runs as a clock between polls.
class _Media extends StatelessWidget {
  const _Media({required this.k, required this.p, required this.host});
  final String k;
  final Map<String, dynamic> p, host;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final a = host['audio'] as Map? ?? {};
    final out = [for (final s in (a['sinks'] as List? ?? [])) s as Map].where((s) => s['name'] == a['default_sink']).firstOrNull?['label'] ?? '';
    final title = '${p['title'] ?? ''}'.isEmpty ? '${p['app']}' : '${p['title']}';
    final sub = ['${p['artist'] ?? ''}', '$out'].where((x) => x.isNotEmpty).join(' · ');
    final playing = p['playing'] == true;
    final len = ((p['length_ms'] ?? 0) as num).toInt(), pos = ((p['position_ms'] ?? 0) as num).toInt();
    final art = (p['art'] ?? p['icon']) as String?;
    return Column(children: [
      Row(children: [
        Container(
          width: 52,
          height: 52,
          clipBehavior: ui.Clip.antiAlias,
          decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r1), boxShadow: const [BoxShadow(color: Color(0x40000000), blurRadius: 14, offset: Offset(0, 4))]),
          child: art == null || !art.contains(',') ? Center(child: WIcon('music', color: t.dim)) : ArtSwap(child: Image.memory(UriData.parse(art).contentAsBytes(), key: ValueKey(art.length), fit: BoxFit.cover, gaplessPlayback: true)),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Enter(key: ValueKey(title), ms: 320, dy: 10, fadeOnClose: false, child: Text(title, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1)).copyWith(height: 1.2))),
            Text(sub, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .7812, color: t.dim)),
          ]),
        ),
        Press(
          onTap: () => actOn(k, {'do': 'media', 'bus': p['bus'], 'op': {'op': 'play_pause'}}),
          label: playing ? 'Pause' : 'Play',
          child: Container(
            width: 44,
            height: 44,
            decoration: BoxDecoration(shape: BoxShape.circle, color: t.accent),
            child: Center(child: Enter(key: ValueKey(playing), ms: 260, dy: 0, scale: .4, pop: true, fadeOnClose: false, child: WIcon(playing ? 'pause' : 'play', fill: true, color: t.onAccent))),
          ),
        ),
        IconBtn('next-f', label: 'Next', color: t.bright, onTap: p['can_next'] == true ? () => actOn(k, {'do': 'media', 'bus': p['bus'], 'op': {'op': 'next'}}) : null),
      ]),
      if (len > 0) ...[const SizedBox(height: 10), _ThinBar(key: ValueKey('$pos$playing'), from: pos / len, leftMs: playing ? len - pos : 0)],
    ]);
  }
}

/// `.thinbar i.run`: from where the poll said to the end, linear, over the time left.
class _ThinBar extends StatefulWidget {
  const _ThinBar({super.key, required this.from, required this.leftMs});
  final double from;
  final int leftMs;
  @override
  State<_ThinBar> createState() => _ThinBarState();
}

class _ThinBarState extends State<_ThinBar> with SingleTickerProviderStateMixin {
  late final _c = AnimationController(vsync: this, duration: Duration(milliseconds: widget.leftMs.clamp(1, 1 << 30)));
  @override
  void initState() {
    super.initState();
    if (widget.leftMs > 0) _c.forward();
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ClipRRect(
      borderRadius: BorderRadius.circular(2),
      child: Container(
        height: 4,
        color: t.s3,
        child: AnimatedBuilder(
          animation: _c,
          builder: (context, _) => FractionallySizedBox(
            alignment: Alignment.centerLeft,
            widthFactor: (widget.from + (1 - widget.from) * _c.value).clamp(0, 1),
            child: ColoredBox(color: t.bright),
          ),
        ),
      ),
    );
  }
}

/// The Clipboard widget: the newest three, pinned first, each one tap onto this phone or back
/// onto the computer; and this phone's clipboard sent the other way.
class _Clips extends StatelessWidget {
  const _Clips({required this.k, required this.name, required this.entries});
  final String k, name;
  final List<Map<String, dynamic>> entries;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final shown = [...entries]..sort((a, b) => (b['pinned'] == true ? 1 : 0) - (a['pinned'] == true ? 1 : 0));
    final more = (entries.length - 3).clamp(0, 1 << 30);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (shown.isEmpty)
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          child: Text('Nothing copied on $name yet.', style: body(context, size: .8125, color: t.dim)),
        ),
      for (var i = 0; i < shown.length && i < 3; i++)
        Glide(
          key: ValueKey('g${shown[i]['id']}'),
          child: Enter(
            delay: Duration(milliseconds: i * 40),
            ms: 480,
            dy: 10,
            scale: .97,
            fadeOnClose: false,
            child: Container(
              margin: const EdgeInsets.only(bottom: 6),
              padding: const EdgeInsets.fromLTRB(10, 6, 6, 6),
              decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2)),
              child: Row(children: [
                Container(
                  width: 28,
                  height: 28,
                  decoration: BoxDecoration(shape: BoxShape.circle, color: fade(t.base[0xC], 80)),
                  alignment: Alignment.center,
                  child: WIcon('${shown[i]['kind']}' == 'image' ? 'image' : ('${shown[i]['kind']}' == 'link' ? 'link' : 'text'), size: 15, color: mix(t.base[0xC], t.bright, 30)),
                ),
                const SizedBox(width: 8),
                Expanded(child: Text('${shown[i]['preview']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(context, size: .7812, color: t.text))),
                IconBtn('copy', label: 'Copy to this phone', color: mix(t.accent, t.bright, 35), onTap: () => Clip.I.copyFrom(k, '${shown[i]['id']}', '${shown[i]['kind']}')),
                IconBtn('laptop', label: "Put on $name's clipboard", onTap: () => Clip.I.sendBackOn(k, '${shown[i]['id']}')),
              ]),
            ),
          ),
        ),
      if (more > 0) Padding(padding: const EdgeInsets.fromLTRB(4, 0, 4, 6), child: Text('$more more on its page', style: body(context, size: .75, color: t.dim))),
      Btn("Send this phone's clipboard", icon: 'paste', onTap: k == Nav.I.selected ? Clip.I.sendPhone : null),
    ]);
  }
}

/// The Virtual input widget: a touch surface that opens the touchpad, the three doors, and the
/// computer's quick actions as chips. The surface blooms a ring from the finger.
class _Pad extends StatefulWidget {
  const _Pad({required this.c, required this.name, required this.sel, required this.cfg});
  final Computer c;
  final String name;
  final bool sel;
  final Map<String, dynamic>? cfg;
  @override
  State<_Pad> createState() => _PadState();
}

class _PadState extends State<_Pad> with SingleTickerProviderStateMixin {
  late final _ring = AnimationController(vsync: this, duration: const Duration(milliseconds: 520));
  Offset _at = Offset.zero;

  @override
  void dispose() {
    _ring.dispose();
    super.dispose();
  }

  void _go(Door d) {
    if (!widget.sel) Nav.I.select(widget.c.key);
    openDoor(context, d);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final actions = widget.sel ? const <Map<String, dynamic>>[] : SideLinks.I.of(widget.c.key).actions;
    final quick = ((widget.cfg?['trackpad'] as Map?)?['quick'] as List?)?.cast<String>() ?? const ['workspace-prev', 'workspace-next', 'fullscreen', 'play-pause'];
    final chips = [for (final id in quick) ?actions.where((a) => a['id'] == id).firstOrNull];
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      GestureDetector(
        onTapDown: (d) {
          _at = d.localPosition;
          _ring.forward(from: 0);
        },
        onTap: () => _go(Door.touchpad),
        child: Container(
          height: 104,
          clipBehavior: ui.Clip.antiAlias,
          decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2), border: Border.all(color: t.line)),
          child: Stack(children: [
            Center(
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                WIcon('trackpad', size: 22, color: t.dim),
                const SizedBox(height: 4),
                Text("Touch to use ${widget.name}'s pointer", style: body(context, size: .8125, color: t.dim)),
                Text('Opens the trackpad', style: body(context, size: .7188, color: t.dim)),
              ]),
            ),
            AnimatedBuilder(
              animation: _ring,
              builder: (context, _) {
                if (!_ring.isAnimating) return const SizedBox.shrink();
                final v = const Cubic(.2, 0, 0, 1).transform(_ring.value);
                return Positioned(
                  left: _at.dx - 22,
                  top: _at.dy - 22,
                  child: Opacity(
                    opacity: 1 - v,
                    child: Transform.scale(scale: .4 + 1.8 * v, child: Container(width: 44, height: 44, decoration: BoxDecoration(shape: BoxShape.circle, color: fade(t.accent, 70)))),
                  ),
                );
              },
            ),
          ]),
        ),
      ),
      const SizedBox(height: 10),
      Row(children: [
        for (final (d, label, icon) in const [(Door.touchpad, 'Touchpad', 'trackpad'), (Door.keyboard, 'Keyboard', 'kbd'), (Door.gamepad, 'Gamepad', 'pad')]) ...[
          if (d != Door.touchpad) const SizedBox(width: 6),
          Expanded(
            child: Press(
              onTap: () => _go(d),
              label: label,
              child: Container(
                constraints: const BoxConstraints(minHeight: 56),
                decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r2)),
                child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
                  WIcon(icon, color: t.bright),
                  Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, w: FontWeight.w600, color: t.bright)),
                ]),
              ),
            ),
          ),
        ],
      ]),
      if (chips.isNotEmpty) ...[
        const SizedBox(height: 10),
        SizedBox(
          height: 40,
          child: ListView(scrollDirection: Axis.horizontal, children: [
            for (final a in chips)
              Padding(
                padding: const EdgeInsets.only(right: 6),
                child: Press(
                  onTap: () => SideLinks.I.send(widget.c.key, {'type': 'pad_in', 'msg': {'k': 'run', 'id': a['id']}}),
                  label: '${a['name']}',
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(999)),
                    child: Row(children: [
                      WIcon('${a['icon'] ?? ''}'.isEmpty ? 'dots' : '${a['icon']}', size: 16, color: t.bright),
                      const SizedBox(width: 6),
                      Text('${a['name']}', style: body(context, size: .7812, w: FontWeight.w600, color: t.bright)),
                    ]),
                  ),
                ),
              ),
          ]),
        ),
      ],
    ]);
  }
}
