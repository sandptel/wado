// The touchpad's own link (port of js/pad_link.js + pad_touch.js + pad_keys.js's sends): pad
// messages ride the relay link (`pad_in` / `pad_out`) until a direct WebRTC path (`pad_offer`)
// is up, then two data channels — `pad-touch` (lossy, frames) and `pad-keys` (reliable).
// Frames: up to ten fingers, each 0..65535 in the pad's own box; compact binary once the daemon's
// pong says it reads them.
import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/foundation.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

import 'log.dart';
import 'relay_link.dart';

Uint8List encodeFrame(int seq, int t, List<List<int>> c) {
  final n = c.length.clamp(0, 10);
  final b = ByteData(8 + 5 * n);
  b.setUint32(0, seq & 0xffffffff);
  b.setUint32(4, t & 0xffffffff);
  for (var i = 0; i < n; i++) {
    b.setUint8(8 + 5 * i, c[i][0]);
    b.setUint16(9 + 5 * i, c[i][1]);
    b.setUint16(11 + 5 * i, c[i][2]);
  }
  return b.buffer.asUint8List();
}

class Pad {
  Pad._() {
    final l = RelayLink.I;
    l.on('pad_out', (m) => _deliver(m['msg']));
    l.on('pad_answer', (m) {
      final w = _answer;
      _answer = null;
      w?.complete(m);
    });
    l.on('__up', (_) {
      if (_want && state.value != 'direct') _tryDirect();
    });
  }
  static final I = Pad._();

  /// off · relayed · direct
  final state = ValueNotifier<String>('off');
  final rtt = ValueNotifier<int?>(null);
  /// The daemon's last error (`err`), for the why-card.
  final error = ValueNotifier<String?>(null);
  final actions = ValueNotifier<List<Map<String, dynamic>>>([]);
  bool bin = false;
  bool _want = false;
  RTCPeerConnection? _pc;
  RTCDataChannel? _touch, _keys;
  Completer<Map<String, dynamic>>? _answer;
  bool _offering = false;
  Timer? _ping;
  final _t0 = Stopwatch()..start();
  int get _now32 => _t0.elapsedMilliseconds & 0xffffffff;
  final _onMsg = <void Function(Map<String, dynamic>)>[];
  void onMsg(void Function(Map<String, dynamic>) f) => _onMsg.add(f);

  void _deliver(Object? raw) {
    if (raw is! Map<String, dynamic>) return;
    final m = raw;
    switch (m['k']) {
      case 'pong':
        rtt.value = (_now32 - ((m['t'] ?? 0) as int)) & 0xffffffff;
        if (m['bin'] == true && !bin) {
          bin = true;
          rlog('pad: compact touch frames on');
        }
      case 'err':
        error.value = (m['message'] ?? '') as String;
        if (m['fatal'] == true) _stop(false);
      case 'action_list':
        actions.value = (m['actions'] as List? ?? []).cast<Map<String, dynamic>>();
    }
    for (final f in [..._onMsg]) {
      f(m);
    }
  }

  bool send(Map<String, dynamic> msg, {bool lossy = false}) {
    if (state.value == 'off') return false;
    Uint8List? b;
    if (bin && msg['k'] == 'frame') {
      b = encodeFrame(msg['seq'] as int, msg['t'] as int, (msg['c'] as List).cast<List<int>>());
    }
    if (state.value == 'direct') {
      final dc = lossy ? _touch : _keys;
      if (dc != null && dc.state == RTCDataChannelState.RTCDataChannelOpen) {
        dc.send(b != null ? RTCDataChannelMessage.fromBinary(b) : RTCDataChannelMessage(jsonEncode(msg)));
        return true;
      }
    }
    final out = b != null ? {'k': 'bin', 'b': base64Encode(b)} : msg;
    return RelayLink.I.send({'type': 'pad_in', 'msg': out});
  }

  void start() {
    if (_want) return;
    _want = true;
    error.value = null;
    rtt.value = null;
    bin = false;
    state.value = 'relayed';
    void ping() => send({'k': 'ping', 't': _now32, 'bin': true});
    _ping = Timer.periodic(const Duration(seconds: 2), (_) => ping());
    ping();
    _tryDirect();
  }

  void stop() => _stop(true);

  void _stop(bool bye) {
    if (bye && _want) RelayLink.I.send({'type': 'pad_in', 'msg': {'k': 'bye'}});
    _want = false;
    _ping?.cancel();
    _ping = null;
    _pc?.close();
    _pc = _touch = _keys = null;
    final w = _answer;
    _answer = null;
    w?.complete({'err': ''});
    state.value = 'off';
  }

  void _drop(String why) {
    if (_pc == null) return;
    rlog('pad: direct link down — $why');
    _pc?.close();
    _pc = _touch = _keys = null;
    if (_want) {
      state.value = 'relayed';
      _resend?.call();
    }
  }

  /// What to send again when the path changes (the touch size, held modifiers).
  VoidCallback? _resend;
  set onResend(VoidCallback f) => _resend = f;

  Future<void> _tryDirect() async {
    if (_offering || !_want || _pc != null || !RelayLink.I.up.value) return;
    _offering = true;
    try {
      final me = await createPeerConnection({
        'iceServers': [
          {'urls': 'stun:stun.l.google.com:19302'},
          {'urls': 'stun:stun1.l.google.com:19302'},
        ],
        'candidateNetworkPolicy': 'all',
      });
      _pc = me;
      final touch = await me.createDataChannel('pad-touch', RTCDataChannelInit()
        ..ordered = false
        ..maxRetransmits = 0);
      final keys = await me.createDataChannel('pad-keys', RTCDataChannelInit()..ordered = true);
      _touch = touch;
      _keys = keys;
      for (final dc in [touch, keys]) {
        dc.onMessage = (m) {
          try {
            _deliver(jsonDecode(m.text));
          } catch (_) {}
        };
      }
      me.onConnectionState = (s) {
        if (_pc == me && (s == RTCPeerConnectionState.RTCPeerConnectionStateFailed || s == RTCPeerConnectionState.RTCPeerConnectionStateClosed)) {
          _drop(s.name);
        }
      };
      final gathered = Completer<void>();
      me.onIceGatheringState = (s) {
        if (s == RTCIceGatheringState.RTCIceGatheringStateComplete && !gathered.isCompleted) gathered.complete();
      };
      await me.setLocalDescription(await me.createOffer());
      await gathered.future.timeout(const Duration(seconds: 3), onTimeout: () {});
      if (_pc != me || !_want) return;
      final local = await me.getLocalDescription();
      _answer = Completer();
      if (!RelayLink.I.send({'type': 'pad_offer', 'sdp': jsonEncode({'type': 'offer', 'sdp': local?.sdp ?? ''})})) {
        _answer = null;
        return;
      }
      final a = await _answer!.future.timeout(const Duration(seconds: 15), onTimeout: () => {'err': ''});
      if (_pc != me) return;
      if ((a['err'] ?? '') != '') {
        _deliver({'k': 'err', 'message': a['err'], 'fatal': true});
        return;
      }
      final sdp = a['sdp'];
      if (sdp is! String) return _drop('no answer');
      final d = jsonDecode(sdp) as Map<String, dynamic>;
      await me.setRemoteDescription(RTCSessionDescription(d['sdp'] as String, (d['type'] ?? 'answer') as String));
      final deadline = DateTime.now().add(const Duration(seconds: 8));
      while (DateTime.now().isBefore(deadline)) {
        if (touch.state == RTCDataChannelState.RTCDataChannelOpen && keys.state == RTCDataChannelState.RTCDataChannelOpen) break;
        await Future.delayed(const Duration(milliseconds: 100));
      }
      if (_pc != me) return;
      if (touch.state != RTCDataChannelState.RTCDataChannelOpen) return _drop('no direct path (ICE)');
      state.value = 'direct';
      rlog('pad: direct link up');
      _resend?.call();
    } catch (e) {
      _drop('$e');
    } finally {
      _offering = false;
    }
  }
}

/// The fingers on the pad (pad_touch.js): ids 0..9, positions 0..65535 in the pad's box. A frame
/// goes on every change; while fingers rest, one at least every 100 ms.
class PadTouch {
  PadTouch._();
  static final I = PadTouch._();
  final _fingers = <int, List<int>>{}; // pointer → [id, x, y]
  int _seq = 0;
  bool _dirty = false;
  int _lastSent = 0;
  final _clock = Stopwatch()..start();
  Timer? _tick, _keep;
  (int, int) _size = (0, 0);

  List<List<int>> frame() => (_fingers.values.toList()..sort((a, b) => a[0] - b[0])).map((f) => [f[0], f[1], f[2]]).toList();

  void _send() {
    _seq = (_seq + 1) & 0xffffffff;
    _dirty = false;
    _lastSent = _clock.elapsedMilliseconds;
    Pad.I.send({'k': 'frame', 'seq': _seq, 't': _clock.elapsedMilliseconds & 0xffffffff, 'c': frame()}, lossy: true);
  }

  void _changed() {
    _send();
    if (Pad.I.state.value == 'direct') {
      for (final ms in [20, 60]) {
        Timer(Duration(milliseconds: ms), () {
          if (_tick != null) _send();
        });
      }
    }
  }

  void _moved() {
    _dirty = true;
    if (Pad.I.state.value == 'direct') _send();
  }

  int _free() {
    final used = _fingers.values.map((f) => f[0]).toSet();
    for (var i = 0; i <= 9; i++) {
      if (!used.contains(i)) return i;
    }
    return -1;
  }

  List<int> _place(double x, double y, double w, double h) =>
      [((x / w).clamp(0, 1) * 65535).round(), ((y / h).clamp(0, 1) * 65535).round()];

  void down(int pointer, double x, double y, double w, double h) {
    final id = _free();
    if (id < 0) return;
    _fingers[pointer] = [id, ..._place(x, y, w, h)];
    _changed();
  }

  void move(int pointer, double x, double y, double w, double h) {
    final f = _fingers[pointer];
    if (f == null) return;
    final p = _place(x, y, w, h);
    f[1] = p[0];
    f[2] = p[1];
    _moved();
  }

  void up(int pointer) {
    if (_fingers.remove(pointer) != null) _changed();
  }

  /// Lift these fingers on the computer (one empty-of-them frame); what the phone does with them
  /// next is not the computer's business (pad_gestures claims).
  void drop(Iterable<int> pointers) {
    var any = false;
    for (final p in pointers) {
      any = _fingers.remove(p) != null || any;
    }
    if (any) _changed();
  }

  void size(int w, int h) {
    if ((w, h) == _size) return;
    _size = (w, h);
    if (w > 0 && h > 0) Pad.I.send({'k': 'size', 'w': w, 'h': h});
  }

  void attach() {
    detach();
    Pad.I.onResend = () {
      if (_size.$1 > 0) Pad.I.send({'k': 'size', 'w': _size.$1, 'h': _size.$2});
      _send();
    };
    _tick = Timer.periodic(const Duration(milliseconds: 16), (_) {
      if (_dirty && Pad.I.state.value != 'direct') _send();
    });
    _keep = Timer.periodic(const Duration(milliseconds: 50), (_) {
      if (_fingers.isNotEmpty && _clock.elapsedMilliseconds - _lastSent >= 100) _send();
    });
  }

  void detach() {
    _tick?.cancel();
    _keep?.cancel();
    _tick = _keep = null;
    if (_fingers.isNotEmpty) {
      _fingers.clear();
      _send();
    }
    _size = (0, 0);
  }
}

/// Typing through the pad link (pad_keys.js): named keys, characters, text, held modifiers.
class PadKeys {
  static void key(String name, [List<String> mods = const []]) => Pad.I.send({'k': 'key', 'key': name, 'mods': mods});
  static void char(String ch, [List<String> mods = const []]) => Pad.I.send({'k': 'char', 'ch': ch, 'mods': mods});
  static void text(String t) => Pad.I.send({'k': 'text', 'text': t});
  static void hold(List<String> mods) => Pad.I.send({'k': 'hold', 'mods': mods});
}

/// pad_why.js: the daemon's words → what to tell the person.
({String cause, String title, String body, List<String> cmds, String alt}) padWhy(String m) {
  String grab(RegExp re) => re.firstMatch(m)?.group(1) ?? '';
  if (RegExp(r'passkey|unlock', caseSensitive: false).hasMatch(m)) {
    final none = m.startsWith('This needs a passkey');
    return (
      cause: none ? 'no-passkey' : 'locked',
      title: none ? 'The trackpad needs a passkey' : 'Locked',
      body: none
          ? 'Your fingerprint, face or PIN becomes the key for the trackpad and keyboard on this computer. Nothing about it leaves this phone.'
          : "Unlock with this phone's passkey to use the trackpad.",
      cmds: const ['wado passkey approve'],
      alt: 'Passkey not working on this phone? Someone at the computer can let it in — or stop asking this phone for one there, with wado passkey require <device> off.',
    );
  }
  if (m.contains('may not use the trackpad')) {
    final cmd = grab(RegExp(r'on the computer: (wado allow .+ input)\s*$'));
    return (
      cause: 'no-grant',
      title: 'This phone may not use the trackpad yet',
      body: 'The trackpad and typing are off for each phone until the computer allows them. On the computer, run:',
      cmds: [cmd.isEmpty ? 'wado allow <this device> input' : cmd],
      alt: 'Or open wado tui → Devices and tick input for this phone.',
    );
  }
  if (m.contains('trackpad is turned off on this computer')) {
    return (
      cause: 'off',
      title: 'The trackpad is turned off on this computer',
      body: 'Its config has trackpad { enabled #false }. To turn it on, run on the computer:',
      cmds: const ['wado msg set trackpad.enabled true'],
      alt: 'Or remove enabled #false from the trackpad section of ~/.config/wado/config.kdl.',
    );
  }
  if (m.contains('/dev/uinput')) {
    return (
      cause: 'uinput',
      title: "The computer can't make a trackpad yet",
      body: "wado moves the pointer and types through /dev/uinput, and this computer doesn't let it open that. On the computer, run these once:",
      cmds: const [
        'sudo modprobe uinput',
        'echo \'KERNEL=="uinput", GROUP="uinput", MODE="0660", OPTIONS+="static_node=uinput"\' | sudo tee /etc/udev/rules.d/60-wado-uinput.rules',
        'sudo groupadd -f uinput && sudo usermod -aG uinput \$USER',
        'sudo udevadm control --reload && sudo udevadm trigger',
      ],
      alt: 'Then log out and back in, so the new group counts, and restart wado. On NixOS instead: hardware.uinput.enable = true; and add "uinput" to your user\'s extraGroups.',
    );
  }
  if (RegExp("isn't supported on this computer|nothing on this computer provides it").hasMatch(m)) {
    return (
      cause: 'unsupported',
      title: 'Not on this computer yet',
      body: "wado can only drive the pointer and keyboard on Linux for now. On macOS and Windows the trackpad isn't supported yet — the screen and files still work.",
      cmds: const [],
      alt: '',
    );
  }
  return (cause: 'other', title: 'The trackpad stopped', body: m, cmds: const [], alt: '');
}
