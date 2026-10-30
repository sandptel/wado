// Diagnostics (port of ui/pages/diagnostics.rs): the link, this device, and the log.
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/log.dart';
import '../../wado/relay_link.dart';
import '../../wado/session.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';

class Diagnostics extends StatelessWidget {
  const Diagnostics({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([logs, RelayLink.I.words, RelayLink.I.pool, RelayLink.I.caps, Session.I.encoder, status]),
      builder: (context, _) {
        final l = RelayLink.I;
        final p = l.pool.value;
        Widget kv(String k, String v) => Padding(
              padding: const EdgeInsets.symmetric(vertical: 2),
              child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                SizedBox(width: 110, child: Text(k, style: body(context, size: .75, color: t.dim))),
                Expanded(child: Text(v, style: mono(context))),
              ]),
            );
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          const Sect('Readouts'),
          const SizedBox(height: 8),
          WCard(children: [
            kv('Link', l.words.value.isEmpty ? '—' : l.words.value),
            kv('Daemon', p.instance.isEmpty ? '—' : '${p.tag} · ${p.busy} of ${p.size} busy · ${p.assignment}'),
            kv('Relay caps', l.caps.value.join(' ')),
            kv('Encoder', '${Session.I.encoder.value.$1} ${Session.I.encoder.value.$2}'.trim()),
            kv('Status', status.value),
            kv('This device', '${CoreHost.deviceName} · ${CoreHost.I.clientKey}'),
          ]),
          const Sect('Log'),
          const SizedBox(height: 8),
          Container(
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
            child: SelectableText(
              logs.value.reversed.take(200).map((x) => '${x.at.toIso8601String().substring(11, 19)}  ${x.text}').join('\n'),
              style: mono(context, size: .6875),
            ),
          ),
        ]);
      },
    );
  }
}
