// The passkey card (port of ui/passkey/card.rs + approve.rs): locked or unlocked, how long it
// stays unlocked, lock when leaving the app, Unlock / Add a passkey, and Approve on the computer
// instead. Used on the computer's page and on the touchpad when it is locked.
import 'package:flutter/material.dart';

import '../../wado/passkey.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';

String stayWords(String stay, bool leave) => switch (stay) {
      'lock' => leave ? 'It stays unlocked until you lock it, or leave the app.' : 'It stays unlocked until you lock it.',
      '1h' => leave ? 'It locks again after 1 hour without use, and whenever you leave the app.' : 'It locks again after 1 hour without use.',
      '8h' => leave ? 'It locks again after 8 hours without use, and whenever you leave the app.' : 'It locks again after 8 hours without use.',
      _ => leave ? 'It locks again after 15 minutes without use, and whenever you leave the app.' : 'It locks again after 15 minutes without use.',
    };

class PasskeyCard extends StatelessWidget {
  const PasskeyCard({super.key, this.need = '', this.onUnlocked});
  /// What it unlocks, in the heading ("the touchpad").
  final String need;
  final VoidCallback? onUnlocked;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder(
      valueListenable: Passkey.I.view,
      builder: (context, v, _) {
        final t = Tok.of(context);
        final p = Passkey.I;
        final ap = v.approve;
        if (v.unlocked) {
          final stay = (v.state['stay'] ?? '15m') as String;
          final leave = v.state['lock_on_leave'] != false;
          return WCard(children: [
            Row(children: [
              WIcon('check', color: t.good),
              const SizedBox(width: 8),
              Expanded(child: Text('Unlocked', style: body(context, w: FontWeight.w600, color: t.good))),
              Press(onTap: () => p.lockNow('Lock now'), child: Text('Lock now', style: body(context, size: .8125, w: FontWeight.w600, color: t.accent))),
            ]),
            Text('Stay unlocked', style: body(context, size: .75, color: t.dim)),
            Seg(options: const [('15m', '15 min'), ('1h', '1 hour'), ('8h', '8 hours'), ('lock', 'Until I lock')], value: stay, onPick: (s) => p.stay(s)),
            Why(stayWords(stay, leave)),
            SwitchRow(title: 'Lock when I leave the app', sub: 'Going to another app or the home screen locks it at once.', on: leave, onToggle: (b) => p.stay(stay, lockOnLeave: b)),
          ]);
        }
        return WCard(children: [
          Row(children: [
            Disc(hue: t.base[0xA], child: WIcon('lock', color: t.base[0xA])),
            const SizedBox(width: 10),
            Expanded(
              child: Text(
                v.registered ? 'Unlock with your passkey' : (need.isEmpty ? 'One last step: a passkey' : '$need needs a passkey'),
                style: body(context, w: FontWeight.w600, color: t.bright),
              ),
            ),
          ]),
          const Why('Your fingerprint, face or PIN becomes the key. Nothing about it leaves this phone; the computer keeps only a public key.'),
          if (v.note.isNotEmpty) Why(v.note, tone: 'bad'),
          if (ap != null)
            Container(
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2)),
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                Text(
                  switch (ap['state']) {
                    'waiting' => 'Waiting for someone at the computer… (${ap['left_s']} s)',
                    'approved' => 'Approved on the computer.',
                    'denied' => 'Not approved on the computer.',
                    _ => '${ap['message'] ?? ''}'.isEmpty ? 'Nobody answered on the computer.' : '${ap['message']}',
                  },
                  style: body(context, size: .8125, color: t.bright),
                ),
                if (ap['state'] == 'waiting') ...[
                  const SizedBox(height: 4),
                  Row(children: [
                    Text('There, run ', style: body(context, size: .75, color: t.dim)),
                    Text('wado passkey approve', style: mono(context, color: t.bright)),
                  ]),
                ],
              ]),
            ),
          if (p.native)
            Btn(v.busy ? 'Waiting for your passkey…' : (v.registered ? 'Unlock' : 'Add a passkey'), primary: true, icon: 'lock', busy: v.busy, onTap: () => p.go(then: onUnlocked)),
          Row(children: [
            Expanded(child: Btn('Approve on the computer instead', onTap: ap?['state'] == 'waiting' ? null : p.approveOnComputer)),
            if (v.busy || ap?['state'] == 'waiting') ...[const SizedBox(width: 8), Btn('Not now', expand: false, onTap: p.cancel)],
          ]),
        ]);
      },
    );
  }
}
