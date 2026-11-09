// A control a Mac does not have yet, drawn by [HostCaps]: dimmed with a "Coming to macOS" badge,
// and a tap only says why. A feature missing for good is not drawn at all — "On this Mac" on the
// Computer page says it once.
import 'package:flutter/material.dart';

import '../../wado/host_caps.dart';
import '../../wado/multi.dart';
import 'basics.dart';

/// [child] as the selected computer has [cap]: as it is, dimmed and coming, or gone (null).
Widget? gated(String cap, Widget child) {
  final s = HostCaps.now.state(cap);
  return switch (s.cap) { Cap.on => child, Cap.later => Soon(why: s.why, child: child), Cap.off => null };
}

/// The computer has [cap]; if not, says why in a note and answers false.
bool may(String cap) {
  final s = HostCaps.now.state(cap);
  if (!s.on) Multi.I.note(s.why);
  return s.on;
}

class Soon extends StatelessWidget {
  const Soon({super.key, required this.why, required this.child});
  final String why;
  final Widget child;
  @override
  Widget build(BuildContext context) => Press(
        onTap: () => Multi.I.note(why),
        label: why,
        scale: 1,
        child: Stack(children: [
          IgnorePointer(child: Opacity(opacity: .45, child: child)),
          const Positioned(top: 8, right: 8, child: WhenBadge(When.soon)),
        ]),
      );
}
