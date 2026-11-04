// Virtual input (port of ui/computer/input/hub.rs): Touchpad, Keyboard and Gamepad, each
// opening its surface; each works without a session. Their settings live under each door.
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/online.dart';
import '../../wado/reach.dart';
import '../../wado/side_links.dart';
import '../computer/actions.dart' show notAllowed;
import '../widgets/sheet.dart';
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
      // Reach, not the seat link: each door opens a link of its own, so a session elsewhere (or
      // files downloading) holding the seat never makes the computer "Offline" here.
      listenable: Listenable.merge([RelayLink.I.up, RelayLink.I.seatBusy, Host.I.config, Online.I.map, SideLinks.I.state]),
      builder: (context, _) {
        final c = CoreHost.I.computer(Nav.I.selected);
        final r = c == null ? null : Reach.of(c);
        // Still connecting counts as reachable: each door's own link says if it is really refused.
        final up = r == null ? RelayLink.I.up.value : (r.online || (r.selected && r.tone == 'warn'));
        final name = c?.name ?? 'The computer';
        final g = Host.I.config.value?['grants'] as Map<String, dynamic>?;
        final input = g == null || g['input'] == true;
        String why(bool needsInput) => !up ? 'Offline · why?' : (needsInput && !input ? 'Not allowed for this phone · why?' : '');
        // A shut door answers its tap: what is wrong, and what would fix it.
        void explain(bool needsInput) => (!up
                ? Locked(
                    '$name isn\'t answering',
                    'It is turned off, asleep, or its wado is not running. This page fills in by itself when it is back.',
                    fix: 'On $name, check it is awake and wado daemon is running.',
                  )
                : notAllowed('The touchpad and keyboard', 'input'))
            .show(context);
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
                Expanded(child: Btn('Open', primary: note.isEmpty, icon: note.isEmpty ? 'play' : 'lock', onTap: note.isEmpty ? onOpen : () => explain(note.startsWith('Not')))),
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
