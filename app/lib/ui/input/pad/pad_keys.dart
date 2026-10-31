// The touchpad's keys (port of pad_keys.js + pad_rail.js `keysRail`): Super, Ctrl, Alt and Shift
// latch — tap for the next key, tap again within 300 ms to lock (held on the computer until tapped
// off, so Super + drag moves a window) — and Esc, Tab and the arrows go out wrapped in what is
// latched. Shown as the landscape rail's two-column grid, or the portrait row over the keyboard.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../../wado/pad.dart';
import '../../motion/motion.dart';
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';

class PadMods {
  PadMods._();
  static final I = PadMods._();
  static const names = ['super', 'ctrl', 'alt', 'shift'];
  final mods = ValueNotifier<Map<String, String>>({for (final n in names) n: 'off'});
  final _last = <String, int>{};
  final _sw = Stopwatch()..start();

  List<String> _in(String st) => [for (final n in names) if (mods.value[n] == st) n];

  void tap(String name) {
    final now = _sw.elapsedMilliseconds;
    final double = now - (_last[name] ?? -100000) < 300;
    _last[name] = now;
    final m = {...mods.value};
    final st = m[name];
    if (st == 'locked') {
      m[name] = 'off';
      mods.value = m;
      Pad.I.send({'k': 'hold', 'mods': _in('locked')});
      return;
    }
    if (st == 'latched' && double) {
      m[name] = 'locked';
      mods.value = m;
      Pad.I.send({'k': 'hold', 'mods': _in('locked')});
      return;
    }
    m[name] = st == 'latched' ? 'off' : 'latched';
    mods.value = m;
  }

  /// The latched modifiers for the next key or character — and they let go.
  List<String> take() {
    final l = _in('latched');
    if (l.isNotEmpty) mods.value = {for (final n in names) n: mods.value[n] == 'latched' ? 'off' : mods.value[n]!};
    return l;
  }

  void key(String name) => PadKeys.key(name, take());

  /// The app went away: nothing stays held on the computer.
  void releaseAll() {
    if (_in('locked').isNotEmpty) Pad.I.send({'k': 'hold', 'mods': <String>[]});
    mods.value = {for (final n in names) n: 'off'};
  }
}

const _keys = [('esc', 'Esc', 'Escape'), ('tab', 'Tab', 'Tab'), ('left', '←', 'Left arrow'), ('right', '→', 'Right arrow'), ('up', '↑', 'Up arrow'), ('down', '↓', 'Down arrow')];

/// `.tpkey`: mono, latched ringed in accent, locked filled as a pill; both pop on.
class PadKey extends StatelessWidget {
  const PadKey({super.key, required this.label, required this.onTap, this.st = 'off', this.name, this.wide = false});
  final String label, st;
  final String? name;
  final bool wide;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final locked = st == 'locked', latched = st == 'latched';
    return Press(
      onTap: () {
        HapticFeedback.selectionClick();
        onTap();
      },
      scale: .92,
      label: name ?? label,
      child: Bump(
        on: st != 'off',
        child: AnimatedContainer(
          duration: t.dur,
          curve: Tokens.spring,
          height: 44,
          constraints: BoxConstraints(minWidth: wide ? 0 : 44),
          padding: EdgeInsets.symmetric(horizontal: wide ? 0 : 12),
          decoration: BoxDecoration(
            color: locked ? t.accent : (latched ? Color.alphaBlend(fade(t.accent, 88), t.s2) : t.s2),
            borderRadius: BorderRadius.circular(locked ? 999 : t.r2),
            border: latched ? Border.all(color: t.accent, width: 2) : null,
          ),
          child: Center(
            widthFactor: wide ? null : 1,
            child: Text(label, style: mono(context, size: .8125, w: FontWeight.w600, color: locked ? t.onAccent : (latched ? t.accent : t.bright))),
          ),
        ),
      ),
    );
  }
}

/// The landscape rail's keys: the modifiers and the six keys, two columns, stepping in.
class PadKeysGrid extends StatelessWidget {
  const PadKeysGrid({super.key});
  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder(
      valueListenable: PadMods.I.mods,
      builder: (context, m, _) {
        final all = <Widget>[
          for (final n in PadMods.names) PadKey(label: n[0].toUpperCase() + n.substring(1), st: m[n]!, wide: true, onTap: () => PadMods.I.tap(n)),
          for (final (id, l, name) in _keys) PadKey(label: l, name: name, wide: true, onTap: () => PadMods.I.key(id)),
        ];
        return Column(children: [
          for (var i = 0; i < all.length; i += 2) ...[
            if (i > 0) const SizedBox(height: 6),
            Row(children: [
              Expanded(child: Enter(delay: Duration(milliseconds: i * 26), ms: 420, dy: 0, dx: -14, scale: .94, fadeOnClose: false, child: all[i])),
              const SizedBox(width: 6),
              Expanded(child: Enter(delay: Duration(milliseconds: (i + 1) * 26), ms: 420, dy: 0, dx: -14, scale: .94, fadeOnClose: false, child: all[i + 1])),
            ]),
          ],
        ]);
      },
    );
  }
}

/// The portrait row over the keyboard: the modifiers, a rule, then the keys.
class PadKeysRow extends StatelessWidget {
  const PadKeysRow({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ValueListenableBuilder(
      valueListenable: PadMods.I.mods,
      builder: (context, m, _) => SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: Row(children: [
          for (final n in PadMods.names) ...[PadKey(label: n[0].toUpperCase() + n.substring(1), st: m[n]!, onTap: () => PadMods.I.tap(n)), const SizedBox(width: 6)],
          Container(width: 1, height: 28, margin: const EdgeInsets.symmetric(horizontal: 2), color: t.line),
          const SizedBox(width: 6),
          for (final (id, l, name) in _keys) ...[PadKey(label: l, name: name, onTap: () => PadMods.I.key(id)), const SizedBox(width: 6)],
        ]),
      ),
    );
  }
}
