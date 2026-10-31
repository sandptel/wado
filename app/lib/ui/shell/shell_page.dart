// Shell (port of ui/console.rs's shells tab + pty.js): tabs for this device's shells, the
// terminal in base16 and JetBrains Mono, and the keys a phone keyboard lacks.
import 'package:flutter/material.dart';
import 'package:xterm/xterm.dart';

import '../../wado/relay_link.dart';
import '../../wado/shells.dart';
import '../../wado/passkey.dart';
import '../passkey/passkey_card.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/lane_card.dart';

TerminalTheme termTheme(Tokens t) => TerminalTheme(
      cursor: t.base[5],
      selection: t.base[2],
      foreground: t.base[5],
      background: t.base[0],
      black: t.base[0],
      red: t.base[8],
      green: t.base[0xB],
      yellow: t.base[0xA],
      blue: t.base[0xD],
      magenta: t.base[0xE],
      cyan: t.base[0xC],
      white: t.base[5],
      brightBlack: t.base[3],
      brightRed: t.base[8],
      brightGreen: t.base[0xB],
      brightYellow: t.base[0xA],
      brightBlue: t.base[0xD],
      brightMagenta: t.base[0xE],
      brightCyan: t.base[0xC],
      brightWhite: t.base[7],
      searchHitBackground: t.base[0xA],
      searchHitBackgroundCurrent: t.base[9],
      searchHitForeground: t.base[0],
    );

class ShellPage extends StatefulWidget {
  const ShellPage({super.key});
  @override
  State<ShellPage> createState() => _ShellPageState();
}

class _ShellPageState extends State<ShellPage> {
  @override
  void initState() {
    super.initState();
    Shells.I.request();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Shells.I;
    return ListenableBuilder(
      listenable: Listenable.merge([s.shells, s.active, s.mods, s.refused, RelayLink.I.up, Passkey.I.view]),
      builder: (context, _) {
        final list = s.shells.value;
        final id = s.active.value;
        return Column(children: [
          SizedBox(
            height: 48,
            child: ListView(scrollDirection: Axis.horizontal, padding: const EdgeInsets.symmetric(horizontal: 8), children: [
              for (final sh in list)
                Padding(
                  padding: const EdgeInsets.all(4),
                  child: Press(
                    onTap: () => s.active.value = sh['id'] as int,
                    child: Container(
                      padding: const EdgeInsets.symmetric(horizontal: 12),
                      decoration: BoxDecoration(color: sh['id'] == id ? t.accent : t.s2, borderRadius: BorderRadius.circular(999)),
                      child: Row(children: [
                        Text('${sh['title'] ?? 'shell'}${sh['alive'] == false ? ' (exited)' : ''}',
                            style: body(context, size: .8125, w: FontWeight.w600, color: sh['id'] == id ? t.onAccent : t.bright)),
                        const SizedBox(width: 4),
                        GestureDetector(onTap: () => s.close(sh['id'] as int), child: WIcon('x', size: 16, color: sh['id'] == id ? t.onAccent : t.dim)),
                      ]),
                    ),
                  ),
                ),
              Padding(
                padding: const EdgeInsets.all(4),
                child: Press(
                  onTap: RelayLink.I.up.value ? () => s.open() : null,
                  label: 'New shell',
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(999), border: Border.all(color: t.line)),
                    child: Row(children: [WIcon('plus', size: 16, color: t.accent), const SizedBox(width: 4), Text('New', style: body(context, w: FontWeight.w600, color: t.accent))]),
                  ),
                ),
              ),
            ]),
          ),
          if (Passkey.I.view.value.state.isNotEmpty && Passkey.I.view.value.required && !Passkey.I.view.value.unlocked)
            Padding(padding: const EdgeInsets.all(12), child: PasskeyCard(need: 'The shell', onUnlocked: () => s.request()))
          else if (s.refused.value.isNotEmpty)
            Padding(padding: const EdgeInsets.all(12), child: WCard(alert: true, children: [Text(s.refused.value, style: body(context, color: t.bright))])),
          // Shells always ride the relay: a big dump arrives at its pace, and says so.
          const Padding(padding: EdgeInsets.fromLTRB(12, 8, 12, 0), child: LaneCard(spot: 'shell')),
          Expanded(
            child: id == 0
                ? Center(child: Padding(padding: const EdgeInsets.all(24), child: Text(list.isEmpty ? 'No shell open on this computer for this phone. Tap New.' : '', style: body(context, color: t.dim))))
                : Container(
                    color: t.base[0],
                    padding: const EdgeInsets.all(4),
                    child: TerminalView(
                      s.term(id),
                      key: ValueKey(id),
                      theme: termTheme(t),
                      textStyle: const TerminalStyle(fontFamily: Tokens.mono, fontSize: 13),
                      autofocus: true,
                      keyboardType: TextInputType.text,
                    ),
                  ),
          ),
          // The keys a phone keyboard does not have.
          Container(
            color: t.s1,
            padding: EdgeInsets.fromLTRB(4, 4, 4, 4 + MediaQuery.of(context).padding.bottom),
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Row(children: [
                for (final (k, label) in const [
                  ('esc', 'Esc'), ('tab', 'Tab'), ('ctrl', 'Ctrl'), ('alt', 'Alt'), ('up', '↑'), ('down', '↓'), ('left', '←'), ('right', '→'), //
                  ('home', 'Home'), ('end', 'End'), ('pgup', 'PgUp'), ('pgdn', 'PgDn'), ('pipe', '|'), ('tilde', '~'), ('slash', '/'), ('dash', '-'),
                ])
                  Padding(
                    padding: const EdgeInsets.all(2),
                    child: Press(
                      onTap: () => s.key(k),
                      label: label,
                      child: Container(
                        constraints: const BoxConstraints(minWidth: 44),
                        height: 40,
                        padding: const EdgeInsets.symmetric(horizontal: 10),
                        alignment: Alignment.center,
                        decoration: BoxDecoration(
                          color: (k == 'ctrl' && s.mods.value.$1) || (k == 'alt' && s.mods.value.$2) ? t.accent : t.s2,
                          borderRadius: BorderRadius.circular(t.r1),
                        ),
                        child: Text(label, style: mono(context, size: .8125, w: FontWeight.w600, color: t.bright)),
                      ),
                    ),
                  ),
              ]),
            ),
          ),
        ]);
      },
    );
  }
}
