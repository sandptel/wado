// Display & session (port of ui/pages/display.rs + computer/pages/display_session.rs): the
// session's shape, frame rate, scale and tuning. A running session changes in place — its apps
// stay open (`session_reconfigure`); the rest applies at the next start.
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import '../../wado/session.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';

class DisplaySession extends StatelessWidget {
  const DisplaySession({super.key});
  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Listenable.merge([Settings.I, Session.I.on]),
      builder: (context, _) {
        final t = Tok.of(context);
        final s = Settings.I;
        final view = ui.PlatformDispatcher.instance.views.first;
        final px = view.physicalSize;
        final opts = Session.resOptions(px.width.round(), px.height.round());
        var res = s.s('res');
        if (res.isEmpty) res = Session.defaultRes(px.width.round(), px.height.round());
        final running = Session.I.on.value;
        void set(String k, Object v) {
          s.set(k, v);
        }

        Widget chips(List<(String, String)> o, String value, ValueChanged<String> pick) => Wrap(spacing: 8, runSpacing: 8, children: [
              for (final (v, label) in o)
                Press(
                  onTap: () => pick(v),
                  label: label,
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                    decoration: BoxDecoration(color: v == value ? t.accent : t.s2, borderRadius: BorderRadius.circular(999)),
                    child: Text(label, style: body(context, size: .8125, w: FontWeight.w600, color: v == value ? t.onAccent : t.bright)),
                  ),
                ),
            ]);
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          if (running) ...[
            WCard(children: [
              Text('Running session', style: body(context, w: FontWeight.w600, color: t.bright)),
              const Why('Shape, frame rate, scale and tuning change it in place — its apps stay open.'),
              Btn('Apply to the running session', primary: true, onTap: Session.I.reconfigure),
            ]),
            const SizedBox(height: 4),
          ],
          const Sect('Shape'),
          const SizedBox(height: 8),
          WCard(children: [
            chips([for (final o in opts) (o, '${o.replaceAll('x', ' × ')}${o == opts.first ? ' · this screen' : ''}'), ('custom', 'Custom…')], res, (v) => set('res', v)),
            if (res == 'custom')
              Row(children: [
                Expanded(child: _Num(label: 'Width', value: s.i('custom_w'), onChanged: (v) => set('custom_w', v))),
                const SizedBox(width: 8),
                Expanded(child: _Num(label: 'Height', value: s.i('custom_h'), onChanged: (v) => set('custom_h', v))),
              ]),
            Text('Orientation', style: body(context, size: .75, color: t.dim)),
            Seg(options: const [('auto', 'Auto'), ('landscape', 'Landscape'), ('portrait', 'Portrait')], value: s.s('orientation'), onPick: (v) => set('orientation', v)),
          ]),
          const Sect('Frame rate'),
          const SizedBox(height: 8),
          Seg(options: const [('30', '30'), ('60', '60'), ('90', '90'), ('120', '120')], value: '${s.i('fps')}', onPick: (v) => set('fps', int.parse(v))),
          const SizedBox(height: 8),
          SwitchRow(title: 'Lock frame rate', sub: 'Stutter instead of slowing down', on: s.b('fps_lock'), onToggle: (v) => set('fps_lock', v)),
          const Sect('Scale'),
          const SizedBox(height: 8),
          chips(const [('1', '1×'), ('1.25', '1.25×'), ('1.5', '1.5×'), ('1.75', '1.75×'), ('2', '2×'), ('2.5', '2.5×'), ('3', '3×')],
              s.s('scale').isEmpty ? '${Session.defaultScale(view.devicePixelRatio)}'.replaceAll('.0', '') : s.s('scale'), (v) => set('scale', v)),
          const Sect('Tuning'),
          const SizedBox(height: 8),
          Seg(options: const [('reactivity', 'Reactive'), ('balanced', 'Balanced'), ('quality', 'Sharp'), ('custom', 'Custom')], value: s.s('quality'), onPick: (v) => set('quality', v)),
          if (s.s('quality') == 'custom') ...[
            const SizedBox(height: 8),
            _Num(label: 'Bitrate (kbps)', value: s.i('bitrate'), onChanged: (v) => set('bitrate', v)),
          ],
          const SizedBox(height: 8),
          WCard(children: [
            SwitchRow(title: 'Auto bitrate', sub: "Lowers the bitrate when the link can't carry it, then climbs back", on: s.b('auto_bitrate'), onToggle: (v) => set('auto_bitrate', v)),
            SwitchRow(
                title: 'Stream sound',
                sub: 'Off: no audio at all, so the picture is never held back to stay in sync with it — applies when you reconnect',
                on: s.b('stream_sound'),
                onToggle: (v) => set('stream_sound', v)),
            SwitchRow(title: 'Low-latency audio', sub: 'Smaller sound packets and no audio cushion — video waits for sound, so this speeds both', on: s.b('low_latency_audio'), onToggle: (v) => set('low_latency_audio', v)),
            SwitchRow(
                title: 'Redundant input',
                sub: 'Taps and keys also sent on a fast lane — the first copy to arrive counts, so a stuck packet never stalls touch',
                on: s.b('redundant_input'),
                onToggle: (v) => set('redundant_input', v)),
          ]),
          const Sect('Keyframes'),
          const SizedBox(height: 8),
          Seg(options: const [('periodic', 'Every 1–2 s'), ('on_request', 'On request')], value: s.s('keyframe_mode'), onPick: (v) => set('keyframe_mode', v)),
          const SizedBox(height: 6),
          const Why('A keyframe is ~8× an ordinary frame. On request sends one only when the phone asks (and every 10 s as a safety net) — fewer bursts on a jittery link.'),
          const Sect('Encoder'),
          const SizedBox(height: 8),
          Seg(options: const [('auto', 'Auto'), ('hardware', 'Hardware'), ('software', 'Software')], value: s.s('encoder_backend'), onPick: (v) => set('encoder_backend', v)),
          const Sect('Apps'),
          const SizedBox(height: 8),
          WCard(children: [
            SwitchRow(title: 'Isolate apps', sub: 'Own D-Bus, no host X11', when: When.restart, on: s.b('isolate_apps'), onToggle: (v) => set('isolate_apps', v)),
            SwitchRow(title: 'X server', sub: 'For Steam and X11-only apps', when: When.restart, on: s.b('x_server'), onToggle: (v) => set('x_server', v)),
          ]),
          const Sect('Windows'),
          const SizedBox(height: 8),
          chips(const [('auto', 'Automatic'), ('center', 'Centred'), ('maximized', 'Maximized'), ('tile', 'Tiled'), ('strip', 'Strip'), ('cascade', 'Cascade'), ('top_left', 'Top left')],
              s.s('placement'), (v) => set('placement', v)),
        ]);
      },
    );
  }
}

class _Num extends StatelessWidget {
  const _Num({required this.label, required this.value, required this.onChanged});
  final String label;
  final int value;
  final ValueChanged<int> onChanged;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return TextField(
      controller: TextEditingController(text: '$value'),
      keyboardType: TextInputType.number,
      style: body(context, size: 1, color: t.bright),
      decoration: InputDecoration(labelText: label, filled: true, fillColor: t.s0, border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1))),
      onSubmitted: (v) {
        final n = int.tryParse(v);
        if (n != null && n > 0) onChanged(n);
      },
    );
  }
}
