// The session (port of ui/stage.rs + dock.rs + surround): the picture on Stage Black, its
// connecting and no-picture states drawn in the stage itself with Leave and Retry, and the dock.
// Back (or ○) leaves the session running and goes to the computer's page.
import 'package:flutter/material.dart';

import '../../wado/nav.dart';
import '../../wado/relay_link.dart';
import '../../wado/session.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'console.dart';
import 'control_centre.dart';
import 'input_layer.dart';
import '../motion/feel.dart';


void leaveToComputer() {
  Session.I.leave();
  final k = Nav.I.selected;
  if (k.isNotEmpty) Nav.I.view.value = ComputerView(k);
}

class SessionView extends StatefulWidget {
  const SessionView({super.key});
  @override
  State<SessionView> createState() => _SessionViewState();
}

class _SessionViewState extends State<SessionView> {
  // Screen on, no system bars, for as long as the session is shown (js/native_feel.js `active`).
  @override
  void initState() {
    super.initState();
    active(true);
  }

  @override
  void dispose() {
    active(false);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Session.I;
    return ColoredBox(
      color: Tokens.stage,
      child: ListenableBuilder(
        listenable: Listenable.merge([s.hasVideo, s.stage, s.stageError, s.alive, s.startFailed, RelayLink.I.up, RelayLink.I.seatBusy]),
        builder: (context, _) {
          final pic = s.hasVideo.value;
          return Stack(children: [
            if (pic) const Positioned.fill(child: Console()),
            if (!pic) ...[Positioned.fill(child: _Connecting(t: t)), const Positioned(right: 8, bottom: 8, child: Dock())],
          ]);
        },
      ),
    );
  }
}

class _Connecting extends StatelessWidget {
  const _Connecting({required this.t});
  final Tokens t;
  @override
  Widget build(BuildContext context) {
    final s = Session.I;
    final err = s.stageError.value;
    final alive = s.alive.value;
    final seat = RelayLink.I.seatBusy.value;
    final words = switch (s.stage.value) {
      0 || 1 => RelayLink.I.up.value ? 'Asking for a desktop…' : 'Reaching the computer…',
      2 => 'Starting the desktop…',
      3 => 'Finding a path for the picture…',
      _ => 'Picture on its way…',
    };
    return SafeArea(
      child: Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 420),
            child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              if (alive != null) ...[
                Text('A desktop is already running', style: display(context, rem(context, 1.25), color: Colors.white)),
                const SizedBox(height: 8),
                Text('Rejoin it, or end it and start a fresh one.', style: body(context, color: Colors.white70)),
                const SizedBox(height: 16),
                Btn('Rejoin', primary: true, icon: 'play', onTap: s.rejoin),
                const SizedBox(height: 8),
                Btn('End it and start fresh', danger: true, onTap: s.dropAndStart),
              ] else if (seat != null) ...[
                Text(seat.idle == true ? 'Your other device, ${seat.holder.isEmpty ? 'another device' : seat.holder}, is connected — nothing is running.' : 'This desktop is in use on ${seat.holder.isEmpty ? 'another device' : seat.holder}.',
                    style: body(context, size: 1, color: Colors.white)),
                const SizedBox(height: 16),
                Btn('Use it here', primary: true, onTap: () {
                  RelayLink.I.takeover();
                  s.start();
                }),
              ] else if (err.isNotEmpty) ...[
                Text("The picture didn't come", style: display(context, rem(context, 1.25), color: Colors.white)),
                const SizedBox(height: 8),
                Text(err, style: body(context, color: Colors.white70)),
                const SizedBox(height: 16),
                Btn('Try again', primary: true, onTap: s.start),
              ] else ...[
                const Center(child: SizedBox(width: 28, height: 28, child: CircularProgressIndicator(strokeWidth: 2.5, color: Colors.white70))),
                const SizedBox(height: 16),
                Text(words, textAlign: TextAlign.center, style: body(context, size: 1, color: Colors.white)),
              ],
              const SizedBox(height: 12),
              Btn('Leave', onTap: leaveToComputer),
            ]),
          ),
        ),
      ),
    );
  }
}

/// `dock.rs`: ⋯ (menu) ○ (leave) ◁ (back) over the picture.
class Dock extends StatelessWidget {
  const Dock({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return SafeArea(
      child: Container(
        decoration: BoxDecoration(color: fade(t.s1, 20), borderRadius: BorderRadius.circular(999), border: Border.all(color: fade(t.line, 40))),
        padding: const EdgeInsets.symmetric(horizontal: 4),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          IconBtn('dots', label: 'Control centre', color: t.bright, onTap: () => openControlCentre(context)),
          IconBtn('kbd', label: 'Keyboard', color: t.bright, onTap: () => KeyboardSheet.toggle(context)),
          IconBtn('home', label: 'Leave the session running', color: t.bright, onTap: leaveToComputer),
          IconBtn('power', label: 'End the session', color: t.bad, onTap: () async {
            final ok = await showDialog<bool>(
              context: context,
              builder: (c) => AlertDialog(
                title: const Text('End this session?'),
                content: const Text('Its apps are closed. Unsaved work in them is lost.'),
                actions: [TextButton(onPressed: () => Navigator.pop(c, false), child: const Text('Keep it')), TextButton(onPressed: () => Navigator.pop(c, true), child: const Text('End it'))],
              ),
            );
            if (ok == true) {
              Session.I.stop();
              final k = Nav.I.selected;
              if (k.isNotEmpty) Nav.I.view.value = ComputerView(k);
            }
          }),
        ]),
      ),
    );
  }
}
