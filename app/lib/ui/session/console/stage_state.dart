// The session's state while there is no picture, in the picture's place (port of
// ui/surround/stage_state.rs): how far the connection got, what it waits for and for how long,
// and the two ways forward — Retry and Leave. A calm resting state, not a spinner: a session can
// sit here a long time. Over the picture's slot in the console, centred in the cockpit.
import 'dart:async';

import 'package:flutter/material.dart';

import '../../../core/core_host.dart';
import '../../../wado/link_state.dart';
import '../../../wado/nav.dart';
import '../../../wado/relay_link.dart';
import '../../../wado/session.dart';
import '../../motion/motion.dart';
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';

/// Whether the stage shows nothing but black: no frame yet, or no path that can carry one.
bool stageWaiting() => Session.I.on.value && (!Session.I.hasVideo.value || Link.I.now.value.state == 'relayed');

void leaveToComputer() {
  Session.I.leave();
  final k = Nav.I.selected;
  if (k.isNotEmpty) Nav.I.view.value = ComputerView(k);
}

/// `.hops`: four bars, done green, the live one breathing, a stopped one red.
class HopBar extends StatelessWidget {
  const HopBar({super.key, required this.names, required this.on, this.stopped = false});
  final List<String> names;
  final int on;
  final bool stopped;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Row(children: [
      for (var i = 0; i < names.length; i++) ...[
        if (i > 0) const SizedBox(width: 4),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Breathe(
              ms: 1200,
              on: i == on && !stopped,
              child: AnimatedContainer(
                duration: t.dur,
                height: 4,
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(2),
                  color: i < on ? t.good : (i == on ? (stopped ? t.bad : t.base[0xA]) : t.s3),
                ),
              ),
            ),
            const SizedBox(height: 4),
            Text(names[i].toUpperCase(), maxLines: 1, style: body(context, size: .625, color: i == on && stopped ? t.bad : t.dim).copyWith(letterSpacing: .6)),
          ]),
        ),
      ],
    ]);
  }
}

class StageState extends StatefulWidget {
  const StageState({super.key});
  @override
  State<StageState> createState() => _StageStateState();
}

class _StageStateState extends State<StageState> {
  // The clock (`js/elapsed.js`): restarts whenever the title changes.
  String _title = '';
  final _since = Stopwatch();
  Timer? _tick;

  @override
  void initState() {
    super.initState();
    _tick = Timer.periodic(const Duration(seconds: 1), (_) => mounted ? setState(() {}) : null);
  }

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Session.I;
    return ListenableBuilder(
      listenable: Listenable.merge([s.on, s.hasVideo, s.stage, s.stageError, s.alive, Link.I.now, RelayLink.I.seatBusy]),
      builder: (context, _) {
        if (!stageWaiting()) return const SizedBox.shrink();
        final name = CoreHost.I.computer(Nav.I.selected)?.name ?? 'the computer';
        final link = Link.I.now.value;
        final relayed = link.state == 'relayed';
        final err = s.stageError.value;
        final alive = s.alive.value;
        final seat = RelayLink.I.seatBusy.value;
        // The two choices a session can stop on before it starts: one already running there, or
        // the seat in use on another device.
        if (alive != null || seat != null) {
          return _Card(warn: false, children: [
            Text(alive != null ? 'A desktop is already running' : 'In use elsewhere', style: body(context, size: .9375, w: FontWeight.w600, color: t.bright)),
            Text(
              alive != null
                  ? 'Rejoin it, or end it and start a fresh one.'
                  : (seat!.idle == true ? 'Your other device, ${seat.holder.isEmpty ? 'another device' : seat.holder}, is connected — nothing is running there.' : 'This desktop is in use on ${seat.holder.isEmpty ? 'another device' : seat.holder}.'),
              style: body(context, size: .8125, color: t.dim).copyWith(height: 1.45),
            ),
            if (alive != null)
              Row(children: [Expanded(child: Btn('End it, start fresh', danger: true, onTap: s.dropAndStart)), const SizedBox(width: 8), Expanded(child: Btn('Rejoin', primary: true, onTap: s.rejoin))])
            else
              Row(children: [
                const Expanded(child: Btn('Leave', onTap: leaveToComputer)),
                const SizedBox(width: 8),
                Expanded(
                  child: Btn('Use it here', primary: true, onTap: () {
                    RelayLink.I.takeover();
                    s.start();
                  }),
                ),
              ]),
          ]);
        }
        final String title, said;
        if (relayed) {
          title = 'No picture — there is no direct path';
          said = link.detail;
        } else if (err.isNotEmpty) {
          title = '$name did not answer';
          said = err;
        } else if (link.state == 'reconnecting') {
          title = 'Getting the picture back…';
          said = link.detail;
        } else {
          title = switch (s.stage.value) { 0 || 1 => 'Reaching $name…', 2 => 'Starting the session on $name…', _ => 'Waiting for the picture…' };
          said = link.detail;
        }
        if (title != _title) {
          _title = title;
          _since
            ..reset()
            ..start();
        }
        final on = relayed ? 3 : s.stage.value.clamp(0, 3);
        final stopped = relayed || err.isNotEmpty;
        final secs = _since.elapsed.inSeconds;
        final short = MediaQuery.sizeOf(context).height <= 520;
        return _Card(warn: stopped, children: [
          if (!stopped) SizedBox(width: 22, height: 22, child: CircularProgressIndicator(strokeWidth: 2.5, color: t.accent, backgroundColor: t.s3)),
          Text(title, style: body(context, size: .9375, w: FontWeight.w600, color: t.bright).copyWith(height: 1.3)),
          HopBar(names: const ['Relay', 'Computer', 'Session', 'Picture'], on: on, stopped: stopped),
          // A short picture slot (landscape console): the words go, the hops and the buttons stay.
          if (!short) Text(said, style: body(context, size: .8125, color: t.dim).copyWith(height: 1.45)),
          if (secs > 0) Text('${secs ~/ 60}:${(secs % 60).toString().padLeft(2, '0')}', style: mono(context, size: .75, color: t.dim)),
          Row(children: [
            const Expanded(child: Btn('Leave', onTap: leaveToComputer)),
            const SizedBox(width: 8),
            Expanded(child: Btn(relayed ? 'Retry direct' : 'Retry', primary: true, onTap: () => err.isNotEmpty ? s.start() : s.negotiate())),
          ]),
        ]);
      },
    );
  }
}

class _Card extends StatelessWidget {
  const _Card({required this.warn, required this.children});
  final bool warn;
  final List<Widget> children;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Center(
      child: Enter(
        ms: 480,
        dy: 8,
        fadeOnClose: false,
        child: Container(
          constraints: const BoxConstraints(maxWidth: 340),
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            color: t.s1,
            borderRadius: BorderRadius.circular(t.r3),
            border: Border.all(color: warn ? fade(t.warn, 55) : t.line),
          ),
          child: SingleChildScrollView(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
              for (var i = 0; i < children.length; i++) ...[if (i > 0) const SizedBox(height: 10), children[i]],
            ]),
          ),
        ),
      ),
    );
  }
}

/// The network changed under a running picture (reconnecting.rs): the stage dims and says so,
/// never taking a touch.
class Reconnecting extends StatelessWidget {
  const Reconnecting({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return IgnorePointer(
      child: ListenableBuilder(
        listenable: Listenable.merge([Link.I.now, Session.I.on, Session.I.hasVideo]),
        builder: (context, _) {
          final on = Session.I.on.value && Session.I.hasVideo.value && Link.I.now.value.state == 'reconnecting';
          return AnimatedOpacity(
            opacity: on ? 1 : 0,
            duration: t.dur,
            child: !on
                ? const SizedBox.expand()
                : ColoredBox(
                    color: const Color(0x59000000),
                    child: Center(
                      child: Row(mainAxisSize: MainAxisSize.min, children: [
                        SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2, color: t.base[7], backgroundColor: fade(t.base[7], 70))),
                        const SizedBox(width: 10),
                        Text('Reconnecting…', style: body(context, size: 1, w: FontWeight.w600, color: t.base[7])),
                      ]),
                    ),
                  ),
          );
        },
      ),
    );
  }
}
