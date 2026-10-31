// The sound card on the control centre's root (port of ui/cc/sound.rs): where the computer's
// sound plays, and how loud — on both ends. The Sound page has the per-app detail.
import 'package:flutter/material.dart';

import '../../../core/core_host.dart';
import '../../../wado/host.dart';
import '../../../wado/session.dart';
import '../../../wado/settings.dart';
import '../../computer/quick.dart' show speaker;
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';
import '../../widgets/slider.dart';

/// The output that plays on the device in hand: it says which device, not "This phone".
String get here => 'This device · ${CoreHost.deviceName.replaceAll(' · app', '')}';

class SoundCard extends StatelessWidget {
  const SoundCard({super.key, required this.open});
  final void Function(String page) open;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([Host.I.state, Settings.I, Session.I.on]),
      builder: (context, _) {
        final h = Host.I.state.value;
        if (h == null) return const SizedBox.shrink();
        final a = (h['audio'] as Map<String, dynamic>?) ?? const {};
        final phone = a['phone_sink'] as String?;
        final def = a['default_sink'];
        final onPhone = phone != null && phone == def;
        final sp = speaker(h);
        final playing = (a['streams'] as List? ?? const []).length;
        // The phone first, then the computer's own outputs.
        final outs = <(String, String, bool)>[
          if (phone != null) (phone, here, true),
          for (final s in (a['sinks'] as List? ?? const []).cast<Map<String, dynamic>>())
            if (s['name'] != phone) ('${s['name']}', '${s['label'] ?? s['name']}', false),
        ];
        final muted = Settings.I.b('muted');
        Widget mute(bool on, String icon, String label, VoidCallback tap) => Press(
              onTap: tap,
              label: label,
              child: AnimatedContainer(
                duration: t.dur,
                width: 44,
                height: 44,
                decoration: BoxDecoration(shape: BoxShape.circle, color: on ? fade(t.bad, 86) : null),
                child: Center(child: WIcon(icon, color: on ? t.bad : t.dim)),
              ),
            );
        return Container(
          padding: const EdgeInsets.all(14),
          decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(children: [
              Text('Sound', style: body(context, w: FontWeight.w600, color: t.bright)),
              Text(playing == 0 ? ' · nothing playing' : (playing == 1 ? ' · 1 app playing' : ' · $playing apps playing'), style: body(context, size: .75, color: t.dim)),
              const Spacer(),
              Press(
                onTap: () => open('sound_devices'),
                child: SizedBox(
                  height: 32,
                  child: Row(children: [
                    Text('Apps & outputs', style: body(context, size: .8125, w: FontWeight.w600, color: t.accent)),
                    WIcon('right', size: 16, color: t.accent),
                  ]),
                ),
              ),
            ]),
            const SizedBox(height: 10),
            // Every output, one tap each: this device, the computer's speakers, headphones, HDMI…
            Wrap(spacing: 6, runSpacing: 6, children: [
              for (final (name, label, isPhone) in outs)
                Press(
                  onTap: () {
                    Host.I.act({'do': 'all_to', 'sink': name});
                    // Sound on the phone needs a peer connection carrying it; off it, none.
                    isPhone ? Session.I.listen() : Session.I.listenStop();
                  },
                  label: label,
                  child: AnimatedContainer(
                    duration: t.dur,
                    height: 36,
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    decoration: BoxDecoration(
                      color: name == def ? t.accent : t.s0,
                      borderRadius: BorderRadius.circular(999),
                      border: Border.all(color: name == def ? Colors.transparent : t.line),
                    ),
                    child: Row(mainAxisSize: MainAxisSize.min, children: [
                      WIcon(isPhone ? 'phone' : 'monitor', size: 15, color: name == def ? t.onAccent : t.dim),
                      const SizedBox(width: 6),
                      Text(label, style: body(context, size: .8125, w: FontWeight.w600, color: name == def ? t.onAccent : t.dim)),
                    ]),
                  ),
                ),
            ]),
            if (sp != null) ...[
              const SizedBox(height: 10),
              Row(children: [
                mute(sp['muted'] == true, sp['muted'] == true ? 'mute' : 'monitor', sp['muted'] == true ? 'Unmute the computer' : 'Mute the computer',
                    () => Host.I.act({'do': 'sink_mute', 'id': sp['id'], 'muted': sp['muted'] != true})),
                const SizedBox(width: 8),
                Expanded(
                  child: ThickSlider(
                    label: 'Computer',
                    thin: true,
                    shown: sp['muted'] == true ? 'muted' : '',
                    value: ((sp['volume'] ?? 0) as num).toDouble().clamp(0, 1),
                    onChange: (v) => Host.I.act({'do': 'sink_volume', 'id': sp['id'], 'volume': v}),
                  ),
                ),
              ]),
            ],
            if (onPhone || Session.I.on.value) ...[
              const SizedBox(height: 10),
              Row(children: [
                mute(muted, muted ? 'mute' : 'phone', 'Mute this device', () {
                  Settings.I.set('muted', !muted);
                  Session.I.applyAudio();
                }),
                const SizedBox(width: 8),
                Expanded(
                  child: ThickSlider(
                    label: here,
                    thin: true,
                    shown: muted ? 'muted' : '',
                    value: Settings.I.d('volume').clamp(0, 1),
                    onInput: (v) {
                      Settings.I.set('volume', v);
                      if (v > 0) Settings.I.set('muted', false);
                      Session.I.applyAudio();
                    },
                  ),
                ),
              ]),
            ],
          ]),
        );
      },
    );
  }
}
