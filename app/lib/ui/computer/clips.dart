// Clipboard on the computer's page (port of ui/computer/clips.rs + ui/clipboard/*): send this
// phone's clipboard, follow the computer's live, and its history to copy from.
import 'package:flutter/material.dart';

import '../../wado/clip.dart';
import '../../wado/host.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';

class Clips extends StatefulWidget {
  const Clips({super.key});
  @override
  State<Clips> createState() => _ClipsState();
}

class _ClipsState extends State<Clips> {
  @override
  void initState() {
    super.initState();
    Clip.I.list();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = Clip.I;
    return ListenableBuilder(
      listenable: Listenable.merge([c.entries, c.available, c.error, c.said, c.live, Host.I.config]),
      builder: (context, _) {
        final g = Host.I.config.value?['grants'] as Map<String, dynamic>?;
        if (g != null && g['clipboard'] == false) {
          return WCard(children: [
            Text("This phone can't use this computer's clipboard.", style: body(context)),
            const Why('Allow it on the computer: wado tui → Devices, or `wado allow <this phone> clipboard`.'),
          ]);
        }
        return WCard(children: [
          Row(children: [
            Expanded(child: Btn("Send this phone's clipboard", icon: 'paste', onTap: c.sendPhone)),
          ]),
          SwitchRow(title: 'Follow its clipboard', sub: "What you copy on the computer is copied here too, while this app is open", on: c.live.value, onToggle: c.watch),
          if (c.said.value.isNotEmpty) Why(c.said.value, tone: 'ok'),
          if (c.error.value.isNotEmpty) Why(c.error.value, tone: 'bad'),
          if (!c.available.value && c.error.value.isEmpty) const Why('Its clipboard history shows here once the computer answers.'),
          for (final e in c.entries.value.take(12))
            Press(
              onTap: () => c.get('${e['id']}'),
              label: 'Copy to this phone',
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 6),
                child: Row(children: [
                  WIcon(e['kind'] == 'image' ? 'image' : (e['kind'] == 'link' ? 'link' : 'text'), color: e['pinned'] == true ? t.accent : t.dim),
                  const SizedBox(width: 10),
                  Expanded(child: Text('${e['preview']}', maxLines: 2, overflow: TextOverflow.ellipsis, style: body(context, size: .8125, color: t.bright))),
                  IconBtn('copy', label: 'Copy', onTap: () => c.get('${e['id']}')),
                ]),
              ),
            ),
        ]);
      },
    );
  }
}
