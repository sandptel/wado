// Turning Game mode on for an app: two questions, asked once — how it is played, then the session
// rules — in one sheet whose steps slide on the spring. Returns the profile, or null if put away.
import 'package:flutter/material.dart';

import '../../wado/game_mode.dart';
import '../../wado/gpad_layouts.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/sheet.dart';
import 'poster.dart';

/// Launchers and Windows-game runners need the X server.
bool needsX(String exec) => RegExp(r'steam|lutris|heroic|wine|bottles|itch', caseSensitive: false).hasMatch(exec);

Future<GameProfile?> setUpGame(BuildContext context, {required String name, required String exec, String? icon, GameProfile? from}) =>
    openSheet<GameProfile>(context, (ctx) => _Setup(from ?? GameProfile(name: name, exec: exec, x: needsX(exec)), icon: icon, edit: from != null));

class _Setup extends StatefulWidget {
  const _Setup(this.p, {this.icon, this.edit = false});
  final GameProfile p;
  final String? icon;
  final bool edit;
  @override
  State<_Setup> createState() => _SetupState();
}

class _SetupState extends State<_Setup> {
  int _step = 0;
  GameProfile get p => widget.p;

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final lead = PosterGlyph(name: p.name, icon: widget.icon, size: 44);
    final steps = [
      Column(key: const ValueKey(0), crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        SheetHead('How will you play ${p.name}?', sub: widget.edit ? 'Game mode settings · 1 of 2' : 'Game mode setup · 1 of 2', lead: lead),
        for (final (id, icon, title, sub) in const [
          ('pad', 'pad', 'On-screen pad', 'Buttons over the game'),
          ('controller', 'target', 'My controller', 'A Bluetooth or USB pad on this phone'),
          ('touch', 'hand', 'Touch & mouse', 'Tap to click, like the desktop'),
        ])
          _Option(icon: icon, title: title, sub: sub, on: p.play == id, onTap: () => setState(() => p.play = id)),
        AnimatedSize(
          duration: t.dur,
          curve: Tokens.spring,
          alignment: Alignment.topCenter,
          child: p.play != 'pad'
              ? const SizedBox(width: double.infinity)
              : Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Wrap(spacing: 6, runSpacing: 6, children: [
                    for (final l in layouts)
                      _Chip(l.name, on: p.layout == l.id, onTap: () => setState(() => p.layout = l.id)),
                  ]),
                ),
        ),
        const SizedBox(height: 14),
        Btn('Next', primary: true, onTap: () => setState(() => _step = 1)),
      ]),
      Column(key: const ValueKey(1), crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        SheetHead('Game mode for ${p.name}', sub: '2 of 2 · every open from this phone uses these', lead: lead),
        Text('PICTURE', style: body(context, size: .6875, w: FontWeight.w600, color: t.dim).copyWith(letterSpacing: 1.1)),
        const SizedBox(height: 6),
        for (final (id, title) in const [('smooth', 'Smoothest'), ('balanced', 'Balanced'), ('sharp', 'Sharpest')])
          _Option(title: title, sub: pictureWords(id, GameMode.refresh), mono: true, on: p.picture == id, onTap: () => setState(() => p.picture = id)),
        const SizedBox(height: 8),
        ListBox(children: [
          SwitchRow(title: 'Never miss a press', sub: 'Buttons sent twice, sound kept tight', on: p.input, onToggle: (v) => setState(() => p.input = v)),
          SwitchRow(title: 'No stray gestures', sub: 'Three-finger and edge swipes off while playing', on: p.calm, onToggle: (v) => setState(() => p.calm = v)),
          SwitchRow(title: 'Volume buttons and screen', sub: 'Volume keys go to the game; the screen stays awake', on: p.awake, onToggle: (v) => setState(() => p.awake = v)),
          SwitchRow(
            title: 'Older-games mode',
            sub: 'Steam and X11-only games need it. Their windows can see each other\'s typing.',
            on: p.x,
            onToggle: (v) => setState(() => p.x = v),
          ),
        ]),
        const SizedBox(height: 14),
        Row(children: [
          Expanded(child: Btn('Back', onTap: () => setState(() => _step = 0))),
          const SizedBox(width: 8),
          Expanded(flex: 2, child: Btn(widget.edit ? 'Save' : 'Save and play', primary: true, onTap: () => Navigator.of(context).pop(p))),
        ]),
      ]),
    ];
    return AnimatedSwitcher(
      duration: t.dur,
      switchInCurve: Tokens.spring,
      switchOutCurve: Curves.easeIn,
      transitionBuilder: (child, a) => FadeTransition(
        opacity: a,
        child: SlideTransition(position: Tween(begin: Offset(child.key == const ValueKey(1) ? .06 : -.06, 0), end: Offset.zero).animate(a), child: child),
      ),
      layoutBuilder: (cur, prev) => Stack(alignment: Alignment.topCenter, children: [...prev, ?cur]),
      child: steps[_step],
    );
  }
}

class _Option extends StatelessWidget {
  const _Option({this.icon, required this.title, required this.sub, required this.on, required this.onTap, this.mono = false});
  final String? icon;
  final String title, sub;
  final bool on, mono;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final fg = on ? t.onAccent : t.bright;
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Press(
        onTap: onTap,
        label: title,
        child: AnimatedContainer(
          duration: t.dur,
          curve: Tokens.spring,
          constraints: const BoxConstraints(minHeight: 56),
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
          decoration: BoxDecoration(color: on ? t.accent : t.s2, borderRadius: BorderRadius.circular(on ? t.r3 : t.r2)),
          child: Row(children: [
            if (icon != null) ...[WIcon(icon!, color: fg), const SizedBox(width: 12)],
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                Text(title, style: body(context, w: FontWeight.w600, color: fg)),
                Text(sub, style: mono ? mono_(context, on ? fade(t.onAccent, 25) : t.dim) : body(context, size: .75, color: on ? fade(t.onAccent, 25) : t.dim)),
              ]),
            ),
          ]),
        ),
      ),
    );
  }
}

TextStyle mono_(BuildContext c, Color color) => mono(c, size: .6875, color: color);

class _Chip extends StatelessWidget {
  const _Chip(this.label, {required this.on, required this.onTap});
  final String label;
  final bool on;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Press(
      onTap: onTap,
      label: label,
      child: AnimatedContainer(
        duration: t.dur,
        curve: Tokens.spring,
        height: 36,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        alignment: Alignment.center,
        decoration: BoxDecoration(color: on ? t.accent : t.s2, borderRadius: BorderRadius.circular(999)),
        child: Text(label, style: body(context, size: .75, w: FontWeight.w600, color: on ? t.onAccent : t.text)),
      ),
    );
  }
}
