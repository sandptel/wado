// Diagnostics (port of ui/pages/diagnostics.rs): the link, this device, and the log.
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/log.dart';
import '../../wado/nav.dart';
import '../../wado/relay_link.dart';
import '../../wado/session.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';

/// The readouts drawn over the picture (debug.rs `ITEMS`): id, label, on by default.
const readouts = [
  ('statusbar', 'Status overlay (session, encoder, stats)', false),
  ('health', 'Health verdict (who is at fault)', true),
  ('fps', 'Show FPS', true),
  ('ping', 'Show ping', true),
  ('touches', 'Show touches / clicks', false),
  ('latency', 'Latency breakdown (per stage)', false),
];

/// A readout is shown: the master switch is on and so is it.
bool readout(String id) {
  final s = Settings.I;
  if (!s.b('debug_master')) return false;
  final v = s.get<Object>('debug_$id');
  return v is bool ? v : readouts.firstWhere((r) => r.$1 == id).$3;
}

/// Diagnostics (port of ui/pages/diagnostics.rs + about.rs): this app's version, the readouts
/// drawn over the picture, and the log.
class Diagnostics extends StatelessWidget {
  const Diagnostics({super.key});
  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Settings.I,
      builder: (context, _) {
        final t = Tok.of(context);
        final s = Settings.I;
        final master = s.b('debug_master');
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          WCard(children: [
            Row(children: [
              Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: kReleaseMode ? t.good : t.dim)),
              const SizedBox(width: 8),
              Text('wado next $appVersion', style: mono(context, size: .875, w: FontWeight.w600, color: t.bright)),
            ]),
            Text(kReleaseMode ? 'The native app, installed beside the old one.' : 'A development build: profile or debug, for testing.', style: body(context, size: .75, color: t.dim)),
          ]),
          const SizedBox(height: 12),
          WCard(children: [
            SwitchRow(title: 'Readouts over the picture', sub: 'Off also stops the work behind them', on: master, onToggle: (v) => s.set('debug_master', v)),
            for (final (id, label, _) in readouts)
              SwitchRow(title: label, on: readout(id) || (!master && s.get<Object>('debug_$id') == true), disabled: !master, onToggle: (v) => s.set('debug_$id', v)),
          ]),
          const SizedBox(height: 12),
          WCard(children: [Btn('Open the server log', onTap: () => Nav.I.openPage('logs'))]),
        ]);
      },
    );
  }
}

const appVersion = '0.1.0';

/// The log: this app's and the computer's lines (the console's Logs tab).
class LogsPage extends StatelessWidget {
  const LogsPage({super.key});
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
        final (mode, pipe) = Session.I.encoder.value;
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          WCard(children: [
            kv('Link', l.words.value),
            kv('Daemon', p.size > 0 ? '${p.tag} · ${p.busy} of ${p.size} busy · ${p.assignment}' : '—'),
            kv('Relay caps', l.caps.value.join(' ')),
            kv('Encoder', [mode, pipe].where((x) => x.isNotEmpty).join(' · ')),
            kv('Status', status.value),
            kv('This device', '${CoreHost.deviceName} · ${CoreHost.I.clientKey}'),
          ]),
          const SizedBox(height: 12),
          Container(
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
            child: SelectableText([for (final x in logs.value) '${x.at.toIso8601String().substring(11, 19)}  ${x.text}'].join('\n'), style: mono(context, size: .6875).copyWith(height: 1.45)),
          ),
        ]);
      },
    );
  }
}
