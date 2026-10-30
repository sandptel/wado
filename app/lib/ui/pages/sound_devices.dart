// Sound & devices (port of ui/pages/sound.rs + host.rs): the computer's outputs (volume, mute,
// which one plays), what is playing where, and Bluetooth (power, connect).
import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';
import '../widgets/slider.dart';

class SoundDevices extends StatelessWidget {
  const SoundDevices({super.key});
  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Host.I.state,
      builder: (context, _) {
        final t = Tok.of(context);
        final h = Host.I.state.value;
        if (h == null) return WCard(children: [Text('Asking the computer…', style: body(context, color: t.dim))]);
        final a = h['audio'] as Map<String, dynamic>? ?? {};
        final sinks = (a['sinks'] as List? ?? []).cast<Map<String, dynamic>>();
        final streams = (a['streams'] as List? ?? []).cast<Map<String, dynamic>>();
        final def = a['default_sink'];
        final bt = h['bluetooth'] as Map<String, dynamic>?;
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          const Sect('Outputs'),
          const SizedBox(height: 8),
          for (final s in sinks) ...[
            WCard(children: [
              Row(children: [
                Expanded(child: Text('${s['label']}', style: body(context, w: FontWeight.w600, color: t.bright))),
                if (s['name'] == def)
                  Text('Plays here', style: body(context, size: .75, w: FontWeight.w600, color: t.accent))
                else
                  Press(onTap: () => Host.I.act({'do': 'default_sink', 'name': s['name']}), child: Text('Play here', style: body(context, size: .8125, w: FontWeight.w600, color: t.accent))),
              ]),
              Row(children: [
                IconBtn(s['muted'] == true ? 'mute' : 'sound', label: 'Mute', color: s['muted'] == true ? t.bad : t.bright, onTap: () => Host.I.act({'do': 'sink_mute', 'id': s['id'], 'muted': s['muted'] != true})),
                Expanded(
                  child: ThickSlider(label: 'Volume', thin: true, value: ((s['volume'] ?? 0) as num).toDouble().clamp(0, 1), onChange: (v) => Host.I.act({'do': 'sink_volume', 'id': s['id'], 'volume': v})),
                ),
              ]),
            ]),
            const SizedBox(height: 8),
          ],
          if (streams.isNotEmpty) ...[
            const Sect('Playing'),
            const SizedBox(height: 8),
            for (final s in streams) ...[
              WCard(children: [
                Text('${s['app']}', style: body(context, w: FontWeight.w600, color: t.bright)),
                if ('${s['title']}'.isNotEmpty) Text('${s['title']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, color: t.dim)),
                ThickSlider(label: 'Volume', thin: true, value: ((s['volume'] ?? 0) as num).toDouble().clamp(0, 1), onChange: (v) => Host.I.act({'do': 'stream_volume', 'id': s['id'], 'volume': v})),
              ]),
              const SizedBox(height: 8),
            ],
          ],
          const Sect('Bluetooth'),
          const SizedBox(height: 8),
          if (bt == null)
            WCard(children: [Text('This computer has no Bluetooth wado can use.', style: body(context, color: t.dim))])
          else
            WCard(children: [
              SwitchRow(title: 'Bluetooth', on: bt['powered'] == true, onToggle: (v) => Host.I.act({'do': 'bt_power', 'on': v})),
              for (final d in (bt['devices'] as List? ?? []).cast<Map<String, dynamic>>())
                Row(children: [
                  WIcon('bt', color: d['connected'] == true ? t.accent : t.dim),
                  const SizedBox(width: 10),
                  Expanded(child: Text('${d['name']}', style: body(context, color: t.bright))),
                  Press(
                    onTap: () => Host.I.act({'do': 'bt_connect', 'addr': d['addr'], 'connect': d['connected'] != true}),
                    child: Padding(
                      padding: const EdgeInsets.all(8),
                      child: Text(d['connected'] == true ? 'Disconnect' : 'Connect', style: body(context, size: .8125, w: FontWeight.w600, color: t.accent)),
                    ),
                  ),
                ]),
            ]),
        ]);
      },
    );
  }
}
