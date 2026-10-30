// Virtual input (port of ui/computer/input/hub.rs): Touchpad, Keyboard and Gamepad, each
// opening its surface; each works without a session. Their settings live under each door.
import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/relay_link.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';
import 'pad_screen.dart';

class InputHub extends StatelessWidget {
  const InputHub({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([RelayLink.I.up, Host.I.config]),
      builder: (context, _) {
        final up = RelayLink.I.up.value;
        final g = Host.I.config.value?['grants'] as Map<String, dynamic>?;
        final input = g == null || g['input'] == true;
        String why(bool needsInput) => !up ? 'Offline' : (needsInput && !input ? 'Not allowed for this phone' : '');
        Widget door(String icon, String hue, String title, String sub, String note, VoidCallback onOpen, String settings) => WCard(children: [
              Row(children: [
                Disc(hue: slot(context, hue), size: 46, child: WIcon(icon, color: slot(context, hue))),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text(title, style: display(context, rem(context, 1.0625))),
                    Text(note.isEmpty ? sub : note, style: body(context, size: .75, color: note.isEmpty ? t.dim : t.warn)),
                  ]),
                ),
              ]),
              Row(children: [
                Expanded(child: Btn('Open', primary: true, icon: 'play', onTap: note.isEmpty ? onOpen : null)),
                const SizedBox(width: 8),
                Expanded(child: Btn('Settings', icon: 'sliders', onTap: () => Nav.I.openPage(settings))),
              ]),
            ]);
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          door('trackpad', '0D', 'Touchpad', 'Move the pointer, click, scroll', why(true), () => openPad(context, keyboard: false), 'trackpad'),
          const SizedBox(height: 10),
          door('kbd', '0E', 'Keyboard', "Type with this phone's keyboard, voice too", why(true), () => openPad(context, keyboard: true), 'keyboard'),
          const SizedBox(height: 10),
          door('pad', '0B', 'Gamepad', 'A controller on the computer, one per phone', why(false), () => openGamepad(context), 'gamepad'),
        ]);
      },
    );
  }
}
