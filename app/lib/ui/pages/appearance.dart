// Appearance (port of ui/pages/appearance.rs + size.rs): scheme, accent, corners, motion, a
// pasted base16 scheme, and Size — text size and interface size, with a live sample.
import 'package:flutter/material.dart';

import '../../wado/settings.dart';
import '../home/cards.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/slider.dart';

class Appearance extends StatelessWidget {
  const Appearance({super.key});
  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Settings.I,
      builder: (context, _) {
        final t = Tok.of(context);
        final s = Settings.I;
        final custom = TextEditingController(text: s.s('theme_custom'));
        final textScale = s.d('text_scale');
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          const Sect('Size'),
          const SizedBox(height: 8),
          WCard(children: [
            // The live sample: a computer card at the chosen size.
            Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r3), border: Border.all(color: t.line)),
              child: Row(children: [
                OsDisc(logo: 'nixos', hue: t.accent),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text('Living-room PC', style: display(context, rem(context, 1.0625))),
                    Text('NixOS · Hyprland · 1080 × 2400', style: mono(context, size: .7188, color: t.dim)),
                    Text('Connected over your network. Text reads like this.', style: body(context, size: .75)),
                  ]),
                ),
              ]),
            ),
            SwitchRow(
              title: 'Follow phone',
              sub: "Use the phone's own text size",
              on: textScale <= 0,
              onToggle: (v) => s.set('text_scale', v ? 0.0 : 1.0),
            ),
            if (textScale > 0)
              ThickSlider(
                label: 'Text size',
                value: textScale,
                min: .85,
                max: 1.5,
                step: .05,
                shown: '${(textScale * 100).round()}%',
                onChange: (v) => s.set('text_scale', double.parse(v.toStringAsFixed(2))),
              ),
            Text('Interface size', style: body(context, w: FontWeight.w600, color: t.bright)),
            Seg(
              options: const [('compact', 'Compact'), ('cozy', 'Cozy'), ('comfy', 'Comfy'), ('large', 'Large')],
              value: s.s('ui_size'),
              onPick: (v) => s.set('ui_size', v),
            ),
            Why(switch (s.s('ui_size')) {
              'compact' => 'Smaller controls and tighter spacing: more on the screen at once.',
              'comfy' => 'Roomier controls and spacing, and the text grows with them.',
              'large' => 'The largest controls and spacing, and the text grows with them.',
              _ => 'The size wado is designed at. Controls, spacing and text grow or shrink together.',
            }),
          ]),
          const Sect('Scheme'),
          const SizedBox(height: 8),
          WCard(children: [
            Wrap(spacing: 8, runSpacing: 8, children: [
              for (final sc in Look.I.schemes)
                Press(
                  onTap: () {
                    s.set('theme_custom', '');
                    s.set('theme', sc.name);
                  },
                  label: sc.name,
                  child: Container(
                    padding: const EdgeInsets.all(8),
                    decoration: BoxDecoration(
                      color: sc.colors[0],
                      borderRadius: BorderRadius.circular(t.r2),
                      border: Border.all(color: s.s('theme') == sc.name && s.s('theme_custom').isEmpty ? t.accent : t.line, width: 2),
                    ),
                    child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Row(mainAxisSize: MainAxisSize.min, children: [
                        for (final i in [8, 9, 0xA, 0xB, 0xC, 0xD, 0xE]) Container(width: 10, height: 10, margin: const EdgeInsets.only(right: 2), decoration: BoxDecoration(shape: BoxShape.circle, color: sc.colors[i])),
                      ]),
                      const SizedBox(height: 4),
                      Text(sc.name, style: TextStyle(fontFamily: Tokens.body, fontSize: 11, color: sc.colors[5])),
                    ]),
                  ),
                ),
            ]),
          ]),
          const Sect('Accent'),
          const SizedBox(height: 8),
          WCard(children: [
            Wrap(spacing: 10, runSpacing: 10, children: [
              for (final h in ['08', '09', '0A', '0B', '0C', '0D', '0E', '0F'])
                Press(
                  onTap: () => s.set('accent', h),
                  label: 'Accent $h',
                  child: Container(
                    width: 44,
                    height: 44,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: slot(context, h),
                      border: Border.all(color: s.s('accent') == h ? t.bright : Colors.transparent, width: 3),
                    ),
                  ),
                ),
            ]),
          ]),
          const Sect('Corners'),
          const SizedBox(height: 8),
          Seg(options: const [('sharp', 'Sharp'), ('round', 'Round'), ('pill', 'Pill')], value: s.s('radius'), onPick: (v) => s.set('radius', v)),
          const Sect('Motion'),
          const SizedBox(height: 8),
          Seg(options: const [('full', 'Full'), ('reduced', 'Reduced'), ('off', 'Off')], value: s.s('motion'), onPick: (v) => s.set('motion', v)),
          const Sect('Paste a base16 scheme'),
          const SizedBox(height: 8),
          WCard(children: [
            const Why('Any published scheme — the YAML, or its 16 hex values in order. A pasted scheme wins; clear the box to go back.'),
            TextField(
              controller: custom,
              maxLines: 4,
              style: mono(context, size: .8125, color: t.bright),
              decoration: InputDecoration(filled: true, fillColor: t.s0, border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1))),
              onChanged: (v) {
                final hex = RegExp(r'(?<![0-9a-fA-F])[0-9a-fA-F]{6}(?![0-9a-fA-F])').allMatches(v).map((m) => m.group(0)!).toList();
                s.set('theme_custom', hex.length >= 16 ? hex.take(16).join(',') : '');
              },
            ),
            if (s.s('theme_custom').isNotEmpty) const Why('Applied — 16 colours parsed.', tone: 'ok'),
          ]),
        ]);
      },
    );
  }
}
