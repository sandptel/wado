// How a computer can be reached, in words and a tone (port of reach.rs): the selected one from
// its link, the others from the relay's online probe.
import '../core/core_host.dart';
import 'host.dart';
import 'nav.dart';
import 'online.dart';
import 'relay_link.dart';
import 'session.dart';

class Reach {
  const Reach({required this.tone, required this.words, required this.online, this.busy = false, this.selected = false});
  final String tone; // ok · warn · off · bad
  final String words;
  final bool online, busy, selected;

  static Reach of(Computer c) {
    final l = RelayLink.I;
    if (c.key == Nav.I.selected && l.key == c.key) {
      final seat = l.seatBusy.value;
      if (seat != null) {
        final other = seat.holder.isEmpty ? 'another device' : seat.holder;
        if (seat.idle == true) return Reach(tone: 'ok', words: 'Online · $other is connected, nothing running', online: true, busy: true, selected: true);
        return const Reach(tone: 'warn', words: 'In use on another device', online: true, busy: true, selected: true);
      }
      if (l.up.value) {
        if (Session.I.on.value && Session.I.stage.value >= 4) return const Reach(tone: 'ok', words: 'Streaming', online: true, selected: true);
        return Reach(tone: 'ok', words: Host.I.state.value != null ? 'Online' : 'Online', online: true, selected: true);
      }
      if (l.takenOver) return const Reach(tone: 'warn', words: 'In use on another device', online: true, busy: true, selected: true);
      final w = l.words.value;
      return Reach(
        tone: w.startsWith('Not ') || w.startsWith('Removed') ? 'bad' : (l.phase.value >= 1 ? 'warn' : 'off'),
        words: w.isEmpty ? 'Connecting…' : w,
        online: false,
        selected: true,
      );
    }
    final n = Online.I.map.value['${c.relayUrl}|${c.remoteId}'];
    return switch (n) {
      null => const Reach(tone: 'off', words: 'Checking…', online: false),
      -1 => const Reach(tone: 'off', words: "Can't reach its relay", online: false),
      0 => const Reach(tone: 'off', words: 'Offline', online: false),
      _ => const Reach(tone: 'ok', words: 'Online', online: true),
    };
  }
}
