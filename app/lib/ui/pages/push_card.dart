// Notifications from the computer (port of ui/push.rs): on or off for this computer, which kinds,
// and a test. Through the phone's UnifiedPush app, so they arrive while wado is closed.
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/nav.dart';
import '../../wado/push.dart';
import '../widgets/basics.dart';
import 'display_session.dart' show HeadCard;

class PushCard extends StatelessWidget {
  const PushCard({super.key});
  @override
  Widget build(BuildContext context) {
    final p = Push.I;
    return ListenableBuilder(
      listenable: Listenable.merge([p.info, p.busy, p.error]),
      builder: (context, _) {
        final name = CoreHost.I.computer(Nav.I.selected)?.name ?? 'the computer';
        final info = p.info.value;
        final on = info?['subscribed'] == true;
        final k = p.kinds;
        void set(String id, bool v) => p.setKinds({...k, id: v});
        return HeadCard(title: 'Notifications', children: [
          SwitchRow(
            title: 'From $name',
            sub: on ? 'On — even when wado is closed' : 'Off',
            on: on,
            disabled: info == null || p.busy.value,
            onToggle: (v) => v ? p.on() : p.off(),
          ),
          if (on) ...[
            SwitchRow(title: 'Claude needs permission', sub: 'With Allow and Deny right on it', on: k['claude_permission'] == true, onToggle: (v) => set('claude_permission', v)),
            SwitchRow(title: 'Claude finished', sub: 'When a session is done and waiting', on: k['claude_done'] == true, onToggle: (v) => set('claude_done', v)),
            SwitchRow(title: '$name is back online', sub: 'After it was off or unreachable', on: k['online'] == true, onToggle: (v) => set('online', v)),
            SwitchRow(title: 'Transfer finished', sub: 'A big upload or download is done', on: k['transfer'] == true, onToggle: (v) => set('transfer', v)),
            Btn('Send a test', onTap: p.busy.value ? null : p.test),
          ],
          if (p.error.value.isNotEmpty) Why(p.error.value, tone: 'bad'),
          Hint("wado can't tell you when $name goes offline — nothing is left running to say so — only when it is back."),
        ]);
      },
    );
  }
}
