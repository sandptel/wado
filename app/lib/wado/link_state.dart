// Which path this device is on (port of js/link_state.js + link.rs's words): direct (LAN,
// internet or TURN), relayed (the limited lane only), connecting, reconnecting, waiting, or
// offline — ticked every 2 s, and said honestly: a green dot never claims more than it has.
import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

import 'relay_link.dart';
import 'session.dart';

class LinkNow {
  const LinkNow({this.state = 'offline', this.path = '', this.rtt, this.why = '', this.nat = '', this.turn = false});
  final String state, path, why, nat;
  final int? rtt;
  final bool turn;

  String get label => switch (state) {
        'direct' => switch (path) { 'lan' => 'Direct · LAN', 'turn' => 'Via TURN', _ => 'Direct · internet' },
        'ready' => 'Online',
        'connecting' => 'Connecting…',
        'relayed' => 'Relayed — limited',
        'reconnecting' => 'Reconnecting…',
        'computer_offline' => 'Computer offline',
        'waiting' => 'Waiting',
        _ => why == 'no_network' ? 'No network' : 'Offline',
      };

  /// ok · warn · bad
  String get tone => switch (state) { 'direct' || 'ready' => 'ok', 'relayed' || 'reconnecting' || 'waiting' || 'connecting' => 'warn', _ => 'bad' };

  String get detail {
    switch (state) {
      case 'direct':
        final how = switch (path) {
          'lan' => 'Straight across your local network.',
          'turn' => 'Through a TURN server — direct was not possible.',
          _ => 'Straight across the internet — the relay only introduced you.',
        };
        return '$how${rtt == null ? '' : ' $rtt ms round trip.'}';
      case 'ready':
        return 'Online: the relay sees your computer. The picture tries a direct path when you start.';
      case 'relayed':
        final cause = nat == 'symmetric' && !turn
            ? ' Your computer is behind a VPN or a strict NAT, which blocks direct connections — switching its VPN off is the likely fix.'
            : ' Switching off a VPN, or moving to mobile data, usually fixes it.';
        return "No direct path, so only the relay's limited lane is up — 64 KB/s. It carries small things: shells, clipboard, status, file lists and files up to 1 MB. Not the picture. This is the network, not wado.$cause";
      case 'connecting':
        return 'Setting up the direct path to your computer.';
      case 'reconnecting':
        return 'The network changed — getting the picture back. Your session is safe.';
      case 'computer_offline':
        return 'The relay is up, but your computer is not connected to it. Is `wado daemon` running there?';
      case 'waiting':
        return 'The relay is holding this connection: $why.';
      default:
        return why == 'no_network' ? 'This device has no network connection.' : "wado's relay is unreachable — not your computer. Retrying.";
    }
  }

  @override
  bool operator ==(Object o) => o is LinkNow && o.state == state && o.path == path && o.rtt == rtt && o.why == why && o.nat == nat && o.turn == turn;
  @override
  int get hashCode => Object.hash(state, path, rtt, why, nat, turn);
}

class Link {
  Link._() {
    final l = RelayLink.I;
    l.on('__waiting', (m) {
      _waiting = (m['reason'] ?? 'waiting') as String;
      tick();
    });
    l.on('__up', (_) {
      _waiting = '';
      l.send({'type': 'host_net_request'});
      tick();
    });
    l.on('__down', (_) {
      _waiting = '';
      tick();
    });
    l.on('host_net', (m) {
      final n = (m['net'] as Map?) ?? const {};
      _nat = (n['nat'] ?? '') as String;
      _turn = n['turn'] == true;
      tick();
    });
    Timer.periodic(const Duration(seconds: 2), (_) => tick());
  }
  static final I = Link._();

  final now = ValueNotifier<LinkNow>(const LinkNow());
  String _waiting = '', _nat = '';
  bool _turn = false;

  Future<({String path, int? rtt})?> _path(RTCPeerConnection pc) async {
    final st = await pc.getStats();
    final byId = {for (final r in st) r.id: r};
    StatsReport? pair;
    for (final r in st) {
      if (r.type == 'transport' && r.values['selectedCandidatePairId'] != null) pair = byId[r.values['selectedCandidatePairId']];
    }
    pair ??= st.where((r) => r.type == 'candidate-pair' && r.values['nominated'] == true && r.values['state'] == 'succeeded').firstOrNull;
    if (pair == null) return null;
    final lt = byId[pair.values['localCandidateId']]?.values['candidateType'], rt = byId[pair.values['remoteCandidateId']]?.values['candidateType'];
    final path = lt == 'relay' || rt == 'relay' ? 'turn' : (lt == 'host' && rt == 'host' ? 'lan' : 'internet');
    final rtt = pair.values['currentRoundTripTime'];
    return (path: path, rtt: rtt is num ? (rtt * 1000).round() : null);
  }

  Future<void> tick() async {
    final s = Session.I, l = RelayLink.I;
    final pc = s.pc;
    var state = 'offline', path = '', why = '';
    int? rtt;
    if (pc != null && pc.connectionState == RTCPeerConnectionState.RTCPeerConnectionStateConnected) {
      state = 'direct';
      try {
        final p = await _path(pc);
        if (p != null) {
          path = p.path;
          rtt = p.rtt;
        }
      } catch (_) {}
    } else if (l.up.value && !s.on.value) {
      state = 'ready';
    } else if (l.up.value) {
      final st = pc?.connectionState;
      state = st == null || st == RTCPeerConnectionState.RTCPeerConnectionStateNew || st == RTCPeerConnectionState.RTCPeerConnectionStateConnecting ? 'connecting' : 'relayed';
    } else if (_waiting.isNotEmpty) {
      state = RegExp('no computer', caseSensitive: false).hasMatch(_waiting) ? 'computer_offline' : 'waiting';
      why = _waiting;
    } else if (l.seatBusy.value != null) {
      final b = l.seatBusy.value!;
      final who = b.holder.isEmpty ? 'another device' : b.holder;
      state = 'waiting';
      why = b.idle == true ? 'your other device, $who, is connected — nothing is running there' : 'in use on $who';
    } else if (s.on.value) {
      state = 'reconnecting';
    }
    if (state == 'offline') {
      why = await _online() ? 'relay_unreachable' : 'no_network';
    }
    now.value = LinkNow(state: state, path: path, rtt: rtt, why: why, nat: _nat, turn: _turn);
  }

  /// Any network at all (navigator.onLine): a non-loopback interface with an address. Local only.
  Future<bool> _online() async {
    try {
      return (await NetworkInterface.list()).any((i) => i.addresses.any((a) => !a.isLoopback));
    } catch (_) {
      return true;
    }
  }
}
