// Notifications & background (port of the native app's BackgroundScreen): what only the phone
// app has — staying connected while closed, the notification permission, and the volume keys.
import 'package:flutter/services.dart';
import 'package:flutter/material.dart';

import '../../wado/settings.dart';
import '../widgets/basics.dart';
import 'display_session.dart' show HeadCard;

const _os = MethodChannel('wado/os');

class BackgroundPage extends StatelessWidget {
  const BackgroundPage({super.key});
  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Settings.I,
      builder: (context, _) {
        final s = Settings.I;
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          HeadCard(children: [
            SwitchRow(
              title: 'Stay connected',
              sub: "Keeps wado's link to your computer open while the app is closed, so its notifications still arrive. Shows a quiet, permanent notification and uses some battery.",
              on: s.b('stay_connected'),
              onToggle: (v) async {
                // Android 13 asks for the notification first: the service lives in one.
                if (v) await _os.invokeMethod<bool>('notifications');
                s.set('stay_connected', v);
              },
            ),
            SwitchRow(
              title: 'Volume keys control the computer',
              sub: "While wado is open, this phone's volume keys turn the computer's sound up and down.",
              on: s.b('volume_keys'),
              onToggle: (v) => s.set('volume_keys', v),
            ),
          ]),
          const SizedBox(height: 12),
          HeadCard(title: 'Notifications', children: [
            const Hint("Your computer's apps, Claude asking and finished transfers show as this phone's own notifications while wado is not in front; what plays on the computer shows on the lock screen."),
            Btn('Allow notifications', icon: 'bell', onTap: () => _os.invokeMethod<bool>('notifications')),
          ]),
        ]);
      },
    );
  }
}
