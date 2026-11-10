// Shell (port of ui/console.rs's shells tab + pty.js): this phone's shells on the computer, made
// for a thumb. The page rides above the phone's keyboard — the terminal gives up rows, the shell
// redraws to fit, so the prompt is never under the keys — and the tabs fold away while typing.
// Pinch the terminal (or A− / A+) to zoom; the size is kept. Under it, two rows: what you do to a
// shell (type, paste, copy, stop, clear) and the keys a phone keyboard lacks — Ctrl and Alt latch
// until the next key, lit while they do.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:xterm/xterm.dart';

import '../../wado/multi.dart';
import '../../wado/passkey.dart';
import '../../wado/relay_link.dart';
import '../../wado/settings.dart';
import '../../wado/shells.dart';
import '../motion/feel.dart';
import '../passkey/passkey_card.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/lane_card.dart';

TerminalTheme termTheme(Tokens t) => TerminalTheme(
      cursor: t.accent,
      selection: fade(t.accent, 65),
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

const _minFont = 8.0, _maxFont = 24.0;

/// The keys row: what a phone keyboard lacks, most used first. (key, label)
const _keys = [
  ('esc', 'Esc'), ('tab', 'Tab'), ('ctrl', 'Ctrl'), ('alt', 'Alt'), //
  ('left', '←'), ('down', '↓'), ('up', '↑'), ('right', '→'),
  ('^d', '^D'), ('^z', '^Z'), ('^r', '^R'), ('^a', '^A'), ('^e', '^E'), ('^w', '^W'), ('^u', '^U'),
  ('home', 'Home'), ('end', 'End'), ('pgup', 'PgUp'), ('pgdn', 'PgDn'),
  ('|', '|'), ('~', '~'), ('/', '/'), ('-', '-'), ('_', '_'), ('*', '*'), ('>', '>'), ('&', '&'), (r'$', r'$'), (r'\', r'\'), ('`', '`'),
  ("'", "'"), ('"', '"'), ('{', '{'), ('}', '}'), ('[', '['), (']', ']'), ('(', '('), (')', ')'), (';', ';'), (':', ':'), ('=', '='),
];

class ShellPage extends StatefulWidget {
  const ShellPage({super.key});
  @override
  State<ShellPage> createState() => _ShellPageState();
}

class _ShellPageState extends State<ShellPage> {
  final _view = GlobalKey<TerminalViewState>();
  final _focus = FocusNode();
  final _sel = TerminalController();
  // Pinch: the fingers on the terminal, and the size and spread when the second one landed.
  final _fingers = <int, Offset>{};
  double? _pinchFrom, _spreadFrom;
  bool _zooming = false;
  late double _font = Settings.I.d('shell_font').clamp(_minFont, _maxFont);

  @override
  void initState() {
    super.initState();
    Shells.I.request();
    _sel.addListener(_selChanged);
  }

  @override
  void dispose() {
    _sel.removeListener(_selChanged);
    _sel.dispose();
    _focus.dispose();
    super.dispose();
  }

  bool _hadSel = false;
  void _selChanged() {
    final has = _sel.selection != null;
    if (has != _hadSel) setState(() => _hadSel = has);
  }

  void _setFont(double f, {bool keep = true}) {
    f = (f * 2).roundToDouble() / 2; // half points: steady, not jittering with the fingers
    f = f.clamp(_minFont, _maxFont);
    if (f != _font) {
      if ((f - _font).abs() >= 1 || f == _minFont || f == _maxFont) feel('tick');
      setState(() => _font = f);
    }
    if (keep) Settings.I.set('shell_font', _font);
  }

  double get _spread {
    final p = _fingers.values.toList();
    return (p[0] - p[1]).distance;
  }

  void _down(PointerDownEvent e) {
    _fingers[e.pointer] = e.localPosition;
    if (_fingers.length == 2) {
      _pinchFrom = _font;
      _spreadFrom = _spread;
      setState(() => _zooming = true);
    }
  }

  void _move(PointerMoveEvent e) {
    if (!_fingers.containsKey(e.pointer)) return;
    _fingers[e.pointer] = e.localPosition;
    if (_fingers.length == 2 && _spreadFrom != null && _spreadFrom! > 0) {
      _setFont(_pinchFrom! * _spread / _spreadFrom!, keep: false);
    }
  }

  void _up(PointerEvent e) {
    _fingers.remove(e.pointer);
    if (_fingers.length < 2 && _zooming) {
      _spreadFrom = null;
      Settings.I.set('shell_font', _font);
      setState(() => _zooming = false);
    }
  }

  void _keyboard(bool kb) {
    feel('tap');
    if (kb) {
      _view.currentState?.closeKeyboard();
      _focus.unfocus();
    } else {
      _focus.requestFocus();
      _view.currentState?.requestKeyboard();
    }
  }

  Future<void> _paste() async {
    final id = Shells.I.active.value;
    final text = (await Clipboard.getData(Clipboard.kTextPlain))?.text ?? '';
    if (id == 0 || text.isEmpty) {
      Multi.I.note('Nothing to paste — the clipboard is empty.');
      return;
    }
    feel('tick');
    Shells.I.term(id).paste(text);
  }

  Future<void> _copy() async {
    final id = Shells.I.active.value;
    final r = _sel.selection;
    if (id == 0 || r == null) {
      Multi.I.note('Hold on the text to select it, then Copy.');
      return;
    }
    final text = Shells.I.term(id).buffer.getText(r).trimRight();
    await Clipboard.setData(ClipboardData(text: text));
    _sel.clearSelection();
    feel('tick');
    Multi.I.note('Copied ${text.length} characters.');
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Shells.I;
    final mq = MediaQuery.of(context);
    // The app's Scaffold does not shrink for the keyboard (the session needs it so); this page does.
    final kbInset = mq.viewInsets.bottom;
    final kb = kbInset > 0;
    return ListenableBuilder(
      listenable: Listenable.merge([s.shells, s.active, s.mods, s.refused, RelayLink.I.up, Passkey.I.view]),
      builder: (context, _) {
        final list = s.shells.value;
        final id = s.active.value;
        final locked = Passkey.I.view.value.state.isNotEmpty && Passkey.I.view.value.required && !Passkey.I.view.value.unlocked;
        final live = id != 0 && !locked;
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          // Tabs fold away while typing: every row is the terminal's then.
          AnimatedSize(
            duration: t.dur,
            curve: Tokens.spring,
            alignment: Alignment.topCenter,
            child: kb && list.isNotEmpty ? const SizedBox(width: double.infinity) : _Tabs(list: list, active: id),
          ),
          if (locked)
            Padding(padding: const EdgeInsets.all(12), child: PasskeyCard(need: 'The shell', onUnlocked: () => s.request()))
          else if (s.refused.value.isNotEmpty)
            Padding(padding: const EdgeInsets.all(12), child: WCard(alert: true, children: [Text(s.refused.value, style: body(context, color: t.bright))])),
          // Shells always ride the relay: a big dump arrives at its pace, and says so.
          const LaneCard(spot: 'shell', margin: EdgeInsets.fromLTRB(12, 8, 12, 0)),
          Expanded(
            child: !live
                ? _Empty(none: list.isEmpty, up: RelayLink.I.up.value, locked: locked)
                : Listener(
                    onPointerDown: _down,
                    onPointerMove: _move,
                    onPointerUp: _up,
                    onPointerCancel: _up,
                    child: Stack(children: [
                      Positioned.fill(
                        child: ColoredBox(
                          color: t.base[0],
                          child: TerminalView(
                            s.term(id),
                            key: _view,
                            controller: _sel,
                            focusNode: _focus,
                            theme: termTheme(t),
                            textStyle: TerminalStyle(fontFamily: Tokens.mono, fontSize: _font),
                            padding: const EdgeInsets.fromLTRB(10, 8, 10, 8),
                            autofocus: true,
                            keyboardType: TextInputType.visiblePassword,
                            keyboardAppearance: t.light ? Brightness.light : Brightness.dark,
                            cursorType: TerminalCursorType.block,
                          ),
                        ),
                      ),
                      // The size while pinching: pops in, leaves quickly.
                      Positioned(
                        top: 12,
                        right: 12,
                        child: IgnorePointer(
                          child: AnimatedScale(
                            scale: _zooming ? 1 : .8,
                            duration: _zooming ? t.dur : t.durPress,
                            curve: _zooming ? t.pop : Curves.easeIn,
                            child: AnimatedOpacity(
                              opacity: _zooming ? 1 : 0,
                              duration: _zooming ? t.dur : t.durPress,
                              child: Container(
                                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
                                decoration: BoxDecoration(color: t.s3, borderRadius: BorderRadius.circular(999)),
                                child: Text('${_font % 1 == 0 ? _font.toInt() : _font} pt', style: body(context, size: .8125, w: FontWeight.w600, color: t.bright)),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ]),
                  ),
          ),
          if (live)
            _Dock(
              kb: kb,
              font: _font,
              canCopy: _hadSel,
              onKeyboard: () => _keyboard(kb),
              onPaste: _paste,
              onCopy: _copy,
              onZoom: (d) => _setFont(_font + d),
              bottom: kb ? 0 : mq.padding.bottom,
            ),
          // Lifts everything above the phone's keyboard.
          SizedBox(height: kbInset),
        ]);
      },
    );
  }
}

/// The shells this phone has open, and New.
class _Tabs extends StatelessWidget {
  const _Tabs({required this.list, required this.active});
  final List<Map<String, dynamic>> list;
  final int active;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Shells.I;
    return SizedBox(
      height: 48,
      child: ListView(scrollDirection: Axis.horizontal, padding: const EdgeInsets.fromLTRB(10, 4, 10, 4), children: [
        for (final sh in list)
          Padding(
            padding: const EdgeInsets.only(right: 6),
            child: Press(
              onTap: () {
                feel('tick');
                s.active.value = sh['id'] as int;
              },
              label: '${sh['title'] ?? 'shell'}',
              child: AnimatedContainer(
                duration: t.dur,
                curve: Tokens.spring,
                padding: const EdgeInsets.fromLTRB(12, 0, 6, 0),
                decoration: BoxDecoration(color: sh['id'] == active ? t.s3 : t.s1, borderRadius: BorderRadius.circular(999)),
                child: Row(children: [
                  // Alive: a dot in the accent on the one shown, quiet on the rest. Exited: none.
                  if (sh['alive'] != false)
                    Container(width: 6, height: 6, margin: const EdgeInsets.only(right: 8), decoration: BoxDecoration(shape: BoxShape.circle, color: sh['id'] == active ? t.accent : t.dim)),
                  Text('${sh['title'] ?? 'shell'}${sh['alive'] == false ? ' · exited' : ''}',
                      style: body(context, size: .8125, w: FontWeight.w600, color: sh['id'] == active ? t.bright : t.text)),
                  Press(
                    onTap: () => s.close(sh['id'] as int),
                    label: 'Close ${sh['title'] ?? 'shell'}',
                    child: SizedBox(width: 32, height: 40, child: Center(child: WIcon('x', size: 15, color: t.dim))),
                  ),
                ]),
              ),
            ),
          ),
        Press(
          onTap: RelayLink.I.up.value ? () => s.open() : () => Multi.I.note('Not connected — a shell opens once the computer is back.'),
          label: 'New shell',
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 14),
            decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(999)),
            child: Row(children: [WIcon('plus', size: 16, color: t.accent), const SizedBox(width: 6), Text('New', style: body(context, size: .8125, w: FontWeight.w600, color: t.bright))]),
          ),
        ),
      ]),
    );
  }
}

class _Empty extends StatelessWidget {
  const _Empty({required this.none, required this.up, required this.locked});
  final bool none, up, locked;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    if (locked) return const SizedBox.shrink();
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(28),
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          WIcon('term', size: 36, color: t.dim),
          const SizedBox(height: 14),
          Text(none ? 'No shell open yet' : 'Pick a shell above', style: display(context, rem(context, 1.125))),
          const SizedBox(height: 6),
          Text(up ? 'A terminal on the computer, no video — it keeps running when you leave.' : 'Not connected — shells open once the computer is back.',
              textAlign: TextAlign.center, style: body(context, size: .875, color: t.dim)),
          if (none && up) ...[
            const SizedBox(height: 18),
            Btn('Open a shell', icon: 'plus', primary: true, expand: false, onTap: () => Shells.I.open()),
          ],
        ]),
      ),
    );
  }
}

/// Under the terminal: what you do to a shell, then the keys a phone keyboard lacks.
class _Dock extends StatelessWidget {
  const _Dock({
    required this.kb,
    required this.font,
    required this.canCopy,
    required this.onKeyboard,
    required this.onPaste,
    required this.onCopy,
    required this.onZoom,
    required this.bottom,
  });
  final bool kb, canCopy;
  final double font, bottom;
  final VoidCallback onKeyboard, onPaste, onCopy;
  final ValueChanged<double> onZoom;

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Shells.I;
    final (ctrl, alt) = s.mods.value;
    void send(String k) {
      feel('tick');
      s.key(k);
    }

    return Container(
      color: t.s1,
      padding: EdgeInsets.fromLTRB(0, 6, 0, 6 + bottom),
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        SizedBox(
          height: 44,
          child: ListView(scrollDirection: Axis.horizontal, padding: const EdgeInsets.symmetric(horizontal: 8), children: [
            _Action(icon: 'kbd', label: kb ? 'Hide' : 'Type', lit: kb, onTap: onKeyboard),
            _Action(icon: 'paste', label: 'Paste', onTap: onPaste),
            _Action(icon: 'copy', label: 'Copy', dim: !canCopy, onTap: onCopy),
            _Action(icon: 'pause', label: 'Stop', sub: '^C', onTap: () => send('^c')),
            _Action(icon: 'trash', label: 'Clear', sub: '^L', onTap: () => send('^l')),
            const SizedBox(width: 6),
            _Action(icon: 'minus', label: 'A', dim: font <= _minFont, onTap: () => onZoom(-1)),
            _Action(icon: 'plus', label: 'A', dim: font >= _maxFont, onTap: () => onZoom(1)),
          ]),
        ),
        const SizedBox(height: 6),
        SizedBox(
          height: 42,
          child: ListView(scrollDirection: Axis.horizontal, padding: const EdgeInsets.symmetric(horizontal: 8), children: [
            for (final (k, label) in _keys)
              _Key(label: label, lit: (k == 'ctrl' && ctrl) || (k == 'alt' && alt), wide: label.length > 1, onTap: () => send(k)),
          ]),
        ),
      ]),
    );
  }
}

class _Action extends StatelessWidget {
  const _Action({required this.icon, required this.label, required this.onTap, this.sub, this.lit = false, this.dim = false});
  final String icon, label;
  final String? sub;
  final bool lit, dim;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final fg = lit ? t.onAccent : (dim ? t.dim : t.bright);
    return Padding(
      padding: const EdgeInsets.only(right: 6),
      child: Press(
        onTap: onTap,
        label: sub == null ? label : '$label ($sub)',
        child: AnimatedContainer(
          duration: t.dur,
          curve: Tokens.spring,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          decoration: BoxDecoration(color: lit ? t.accent : t.s2, borderRadius: BorderRadius.circular(t.r1)),
          child: Row(children: [
            WIcon(icon, size: 17, color: fg),
            const SizedBox(width: 6),
            Text(label, style: body(context, size: .8125, w: FontWeight.w600, color: fg)),
            if (sub != null) ...[const SizedBox(width: 6), Text(sub!, style: mono(context, size: .6875, color: lit ? fg : t.dim))],
          ]),
        ),
      ),
    );
  }
}

class _Key extends StatelessWidget {
  const _Key({required this.label, required this.onTap, this.lit = false, this.wide = false});
  final String label;
  final bool lit, wide;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Padding(
      padding: const EdgeInsets.only(right: 4),
      child: Press(
        onTap: onTap,
        label: label,
        scale: .9,
        child: AnimatedContainer(
          duration: t.durPress,
          constraints: BoxConstraints(minWidth: wide ? 48 : 38),
          padding: const EdgeInsets.symmetric(horizontal: 9),
          alignment: Alignment.center,
          decoration: BoxDecoration(color: lit ? t.accent : t.s2, borderRadius: BorderRadius.circular(t.r1)),
          child: Text(label, style: mono(context, size: .875, w: FontWeight.w600, color: lit ? t.onAccent : t.bright)),
        ),
      ),
    );
  }
}
