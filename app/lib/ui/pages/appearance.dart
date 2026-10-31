// Appearance (port of home/pages/appearance.rs + pages/size.rs + pages/appearance.rs): how big
// this phone draws wado (text size, interface size), then the scheme, accent, corners and motion,
// and a pasted base16 scheme. The text slider previews in its sample while dragged and applies
// on release: the whole page re-flowing under the finger would move the slider away from it.
import 'package:flutter/material.dart';

import '../../wado/settings.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/slider.dart';
import 'display_session.dart' show HeadCard;
import 'dock_switcher.dart';

const _textMin = 85, _textMax = 150, _textStep = 5;

class Appearance extends StatefulWidget {
  const Appearance({super.key});
  @override
  State<Appearance> createState() => _AppearanceState();
}

class _AppearanceState extends State<Appearance> {
  int? _drag;
  late final _custom = TextEditingController(text: Settings.I.s('theme_custom'));

  @override
  void dispose() {
    _custom.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Settings.I,
      builder: (context, _) {
        final t = Tok.of(context);
        final s = Settings.I;
        final ts = s.d('text_scale');
        final follow = ts <= 0;
        final text = follow ? (deviceText * 100).round() : (ts * 100).round();
        final shown = _drag ?? text;
        int step(int v, bool up) => (up ? v + _textStep : v - _textStep).clamp(_textMin, _textMax);
        void setText(int v) => s.set('text_scale', v / 100);
        final iface = s.s('ui_size').isEmpty ? 'cozy' : s.s('ui_size');
        final customOk = parseScheme(s.s('theme_custom')) != null;
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          HeadCard(title: 'Text size', children: [
            ThickSlider(
              label: 'Text size',
              value: shown.toDouble(),
              min: _textMin.toDouble(),
              max: _textMax.toDouble(),
              step: _textStep.toDouble(),
              ticks: (_textMax - _textMin) ~/ _textStep,
              shown: '$shown%',
              disabled: follow,
              onInput: (v) => setState(() => _drag = v.round()),
              onChange: (v) {
                setState(() => _drag = null);
                setText(v.round());
              },
            ),
            Row(children: [
              Expanded(
                child: Btn('Smaller', onTap: follow || text <= _textMin ? null : () => setText(step(text, false)), icon: null),
              ),
              const SizedBox(width: 8),
              Expanded(child: Btn('Larger', onTap: follow || text >= _textMax ? null : () => setText(step(text, true)))),
            ]),
            // The sample, at the size being dragged to.
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
              decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2)),
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text('Living-room PC', style: display(context, 17 * shown / 100, ls: -.17).copyWith(height: 1.2)),
                Text('Connected over your network. Text reads like this.', style: TextStyle(fontFamily: Tokens.body, fontSize: 14 * shown / 100, height: 1.45, color: t.text)),
                Text('NixOS · Hyprland · 1080 × 2400', style: TextStyle(fontFamily: Tokens.mono, fontSize: 12 * shown / 100, color: t.dim)),
              ]),
            ),
            SwitchRow(
              title: 'Follow phone',
              sub: "Use the phone's own font size, now ${(deviceText * 100).round()}%",
              on: follow,
              onToggle: (v) => s.set('text_scale', v ? 0.0 : text / 100),
            ),
            if (text != 100 && !follow)
              Align(
                alignment: Alignment.centerLeft,
                child: Press(
                  onTap: () => setText(100),
                  child: Padding(padding: const EdgeInsets.symmetric(vertical: 12, horizontal: 4), child: Text('Back to 100%', style: body(context, w: FontWeight.w600, color: t.accent))),
                ),
              ),
          ]),
          const SizedBox(height: 12),
          HeadCard(title: 'Interface size', children: [
            Seg(options: const [('compact', 'Compact'), ('cozy', 'Cozy'), ('comfy', 'Comfy'), ('large', 'Large')], value: iface, onPick: (v) => s.set('ui_size', v)),
            Hint(switch (iface) {
              'compact' => 'Smaller controls and tighter spacing: more on the screen at once.',
              'comfy' => 'Roomier controls and spacing, and the text grows with them.',
              'large' => 'The largest controls and spacing, and the text grows with them.',
              _ => 'The size wado is designed at. Controls, spacing and text grow or shrink together.',
            }),
          ]),
          const Sect('Colour & shape'),
          const SizedBox(height: 8),
          // Each scheme as a little window in its own colours, with its eight accents under it.
          LayoutBuilder(builder: (context, box) {
            final w = (box.maxWidth - 8) / 2;
            return Wrap(spacing: 8, runSpacing: 8, children: [
              for (final sc in Look.I.schemes)
                SizedBox(width: w, child: _ThemeCard(sc: sc, on: sc.name == s.s('theme') && !customOk, onTap: () {
                  s.set('theme', sc.name);
                  s.set('theme_custom', '');
                  _custom.clear();
                })),
            ]);
          }),
          const SizedBox(height: 12),
          HeadCard(title: 'Accent', children: [
            Wrap(spacing: 10, runSpacing: 10, children: [
              for (final slot in const ['08', '09', '0A', '0B', '0C', '0D', '0E', '0F'])
                Press(
                  onTap: () => s.set('accent', slot),
                  label: 'base$slot',
                  scale: 1,
                  child: AnimatedScale(
                    scale: s.s('accent') == slot ? 1.08 : 1,
                    duration: const Duration(milliseconds: 200),
                    curve: t.pop,
                    child: Container(
                      width: 44,
                      height: 44,
                      padding: const EdgeInsets.all(2),
                      decoration: BoxDecoration(shape: BoxShape.circle, border: Border.all(color: s.s('accent') == slot ? slot2(t, slot) : Colors.transparent, width: 2)),
                      child: Container(decoration: BoxDecoration(shape: BoxShape.circle, color: slot2(t, slot), border: Border.all(color: t.s1, width: 3))),
                    ),
                  ),
                ),
            ]),
            Text('Corners', style: body(context, w: FontWeight.w600, color: t.bright)),
            Seg(options: const [('sharp', 'Sharp'), ('round', 'Round'), ('pill', 'Pill')], value: s.s('radius'), onPick: (v) => s.set('radius', v)),
            Text('Motion', style: body(context, w: FontWeight.w600, color: t.bright)),
            Seg(options: const [('full', 'Full'), ('reduced', 'Reduced'), ('off', 'Off')], value: s.s('motion'), onPick: (v) => s.set('motion', v)),
          ]),
          const SizedBox(height: 12),
          HeadCard(title: 'Paste a base16 scheme', children: [
            const Why('Any published scheme — the YAML, or its 16 hex values in order. A pasted scheme wins; clear the box to go back.'),
            TextField(
              controller: _custom,
              maxLines: 5,
              minLines: 5,
              onChanged: (v) => s.set('theme_custom', v),
              style: mono(context, size: .8125, color: t.bright).copyWith(height: 1.5),
              decoration: InputDecoration(
                hintText: 'base00: "1d2021"\nbase01: "3c3836"\n…',
                hintStyle: mono(context, size: .8125, color: t.dim),
                filled: true,
                fillColor: t.s0,
                border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1), borderSide: BorderSide(color: t.line)),
              ),
            ),
            if (s.s('theme_custom').trim().isNotEmpty)
              Why(customOk ? 'Applied — 16 colours parsed.' : 'Not applied — could not find 16 colours.', tone: customOk ? 'ok' : 'bad'),
          ]),
          const Sect('Dock & switcher'),
          const SizedBox(height: 8),
          const DockSwitcher(),
        ]);
      },
    );
  }
}

Color slot2(Tokens t, String slot) => t.base[int.parse(slot, radix: 16)];

class _ThemeCard extends StatelessWidget {
  const _ThemeCard({required this.sc, required this.on, required this.onTap});
  final Scheme sc;
  final bool on;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = sc.colors;
    return Press(
      onTap: onTap,
      label: sc.name,
      child: AnimatedContainer(
        duration: t.dur,
        padding: const EdgeInsets.all(8),
        decoration: BoxDecoration(color: c[0], borderRadius: BorderRadius.circular(t.r3), border: Border.all(color: on ? t.accent : Colors.transparent, width: 2)),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          // `.pv`: a window: a title bar, a line of text, a button.
          Container(
            height: 54,
            decoration: BoxDecoration(color: c[1], borderRadius: BorderRadius.circular(t.r2)),
            child: LayoutBuilder(builder: (context, b) {
              return Stack(children: [
                Positioned(left: 8, top: 8, width: b.maxWidth * .4, height: 7, child: _bar(c[5])),
                Positioned(left: 8, top: 21, width: b.maxWidth * .6, height: 5, child: _bar(c[3])),
                Positioned(right: 8, bottom: 8, width: 24, height: 14, child: _bar(c[13])),
              ]);
            }),
          ),
          const SizedBox(height: 8),
          ClipRRect(
            borderRadius: BorderRadius.circular(4),
            child: SizedBox(height: 10, child: Row(children: [for (final x in c.sublist(8)) Expanded(child: ColoredBox(color: x))])),
          ),
          const SizedBox(height: 8),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 2),
            child: Text(sc.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, w: FontWeight.w600, color: c[5])),
          ),
        ]),
      ),
    );
  }

  Widget _bar(Color c) => DecoratedBox(decoration: BoxDecoration(color: c, borderRadius: BorderRadius.circular(6)));
}

/// A pasted scheme (theme.js `parse`): the YAML or 16 hex values, in order. Null unless 16.
List<String>? parseScheme(String raw) {
  final hex = RegExp(r'(?<![0-9a-fA-F])([0-9a-fA-F]{6})(?![0-9a-fA-F])').allMatches(raw).map((m) => m[1]!.toLowerCase()).toList();
  return hex.length >= 16 ? hex.sublist(0, 16) : null;
}
