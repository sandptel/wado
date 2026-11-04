// Home's top bar: the computer dropdown on the left (All computers first), then scan and ⋯ — the
// picked computer's settings and connection, named so it is never ambiguous which one it acts on.
import 'dart:io';

import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/nav.dart';
import '../../wado/settings.dart';
import '../hints/cheat_sheet.dart';
import '../pages/add_computer.dart' show scanFromHome;
import '../theme/look.dart';
import '../widgets/basics.dart';
import 'cards.dart' show hues;

// One bar on screen; a key made in build would rebuild the chip from scratch each time.
final _chipKey = GlobalKey();

/// Home shows every computer: its own signal, so only what reads it rebuilds.
final homeView = ValueNotifier<bool>(Settings.I.b('home_all'));
bool get homeAll => homeView.value && CoreHost.I.computers.value.length > 1;

void _setAll(bool v) {
  Settings.I.set('home_all', v);
  homeView.value = v;
}

Color hueOf(BuildContext context, String key) {
  final l = CoreHost.I.computers.value;
  final i = l.indexWhere((c) => c.key == key);
  return slot(context, hues[(i < 0 ? 0 : i) % 6]);
}

class TopBar extends StatelessWidget {
  const TopBar({super.key});
  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: Listenable.merge([CoreHost.I.computers, Nav.I.view, homeView]),
        builder: (context, _) {
          final t = Tok.of(context);
          final list = CoreHost.I.computers.value;
          final sel = list.where((c) => c.key == Nav.I.selected).firstOrNull ?? list.firstOrNull;
          final all = homeAll;
          final label = all ? 'All computers' : (sel?.name ?? 'wado');
          return Row(children: [
            Flexible(
              child: Press(
                onTap: list.isEmpty ? null : () => _pick(context, _chipKey),
                label: 'Computer: $label. Change',
                child: Container(
                  key: _chipKey,
                  height: 40,
                  padding: const EdgeInsets.fromLTRB(12, 0, 8, 0),
                  decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(999), border: Border.all(color: t.line)),
                  child: Row(mainAxisSize: MainAxisSize.min, children: [
                    Container(width: 8, height: 8, decoration: BoxDecoration(color: all ? t.accent : (sel == null ? t.dim : hueOf(context, sel.key)), shape: BoxShape.circle)),
                    const SizedBox(width: 8),
                    Flexible(child: Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright))),
                    const SizedBox(width: 2),
                    WIcon('down', size: 16, color: t.dim),
                  ]),
                ),
              ),
            ),
            const SizedBox(width: 8),
            const Spacer(),
            // One plate for the bar's actions, the chip's own height, surface and border: the
            // controls read as a pair with the chip, not as loose glyphs on the wash.
            Container(
              height: 40,
              decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(999), border: Border.all(color: t.line)),
              child: Row(mainAxisSize: MainAxisSize.min, children: [
                if (Platform.isAndroid || Platform.isIOS) ...[
                  _BarBtn(icon: 'camera', label: 'Scan a code', onTap: () => scanFromHome(context)),
                  Container(width: 1, height: 18, color: t.line),
                ],
                _BarBtn(icon: 'dots', label: sel == null ? 'More' : '${sel.name}: settings and connection', onTap: () => _more(context, sel)),
              ]),
            ),
          ]);
        },
      );

  Future<void> _pick(BuildContext context, GlobalKey at) async {
    final t = Tok.of(context);
    final box = at.currentContext!.findRenderObject() as RenderBox;
    final o = box.localToGlobal(Offset.zero);
    final list = CoreHost.I.computers.value;
    final v = await showMenu<String>(
      context: context,
      color: t.s2,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(t.r3), side: BorderSide(color: t.line)),
      position: RelativeRect.fromLTRB(o.dx, o.dy + box.size.height + 6, o.dx + box.size.width, 0),
      items: [
        if (list.length > 1) _item(context, 'all', 'All computers', 'One home made from every computer', t.accent, homeAll),
        for (final c in list) _item(context, c.key, c.name, c.os.isEmpty ? 'Linux' : c.os, hueOf(context, c.key), !homeAll && c.key == Nav.I.selected),
        PopupMenuItem(value: '+', height: 48, child: Row(children: [WIcon('plus', color: t.dim), const SizedBox(width: 10), Text('Add a computer', style: body(context, color: t.bright))])),
      ],
    );
    if (v == null) return;
    if (v == '+') return Nav.I.openPage('add_computer', aboutComputer: false);
    if (v == 'all') return _setAll(true);
    _setAll(false);
    if (v != Nav.I.selected) Nav.I.select(v);
  }

  PopupMenuItem<String> _item(BuildContext context, String v, String title, String sub, Color dot, bool on) {
    final t = Tok.of(context);
    return PopupMenuItem(
      value: v,
      height: 52,
      child: Row(children: [
        Container(width: 10, height: 10, decoration: BoxDecoration(color: dot, shape: BoxShape.circle)),
        const SizedBox(width: 12),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
            Text(title, style: body(context, w: FontWeight.w600, color: t.bright)),
            Text(sub, style: body(context, size: .6875, color: t.dim)),
          ]),
        ),
        if (on) WIcon('check', size: 18, color: t.accent),
      ]),
    );
  }

  Future<void> _more(BuildContext context, Computer? c) async {
    final t = Tok.of(context);
    final v = await showMenu<String>(
      context: context,
      color: t.s2,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(t.r3), side: BorderSide(color: t.line)),
      position: RelativeRect.fromLTRB(MediaQuery.of(context).size.width, 56 + MediaQuery.of(context).padding.top, 12, 0),
      items: [
        if (c != null) PopupMenuItem(enabled: false, height: 32, child: Text(c.name.toUpperCase(), style: body(context, size: .625, w: FontWeight.w700, color: t.dim).copyWith(letterSpacing: 1))),
        if (c != null) _row(context, 'page', 'monitor', 'Settings & connection'),
        _row(context, 'help', 'info', 'What can I do here?'),
      ],
    );
    if (!context.mounted) return;
    if (v == 'page' && c != null) Nav.I.openComputer(c.key);
    if (v == 'help') showCheatSheet(context, 'home');
  }

  PopupMenuItem<String> _row(BuildContext context, String v, String icon, String words) => PopupMenuItem(
        value: v,
        height: 48,
        child: Row(children: [WIcon(icon, color: Tok.of(context).dim), const SizedBox(width: 10), Text(words, style: body(context, color: Tok.of(context).bright))]),
      );
}

/// A 44 px-wide target inside the bar's plate, its icon in the chip's text colour.
class _BarBtn extends StatelessWidget {
  const _BarBtn({required this.icon, required this.label, required this.onTap});
  final String icon, label;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) => Press(
        onTap: onTap,
        label: label,
        child: SizedBox(width: 46, height: 40, child: Center(child: WIcon(icon, size: 20, color: Tok.of(context).bright))),
      );
}
