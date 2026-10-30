// Files on the computer (port of js/files_link.js): a `files` data channel on its own WebRTC
// connection (`files_offer`), or — with no direct path — the relay's limited lane
// (`files_in` / `files_out` / `files_chunk`). Requests are `{id, op, …}` JSON; bytes come back
// framed with their request id (4 bytes, big-endian) before the payload.
import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:crypto/crypto.dart' as crypto;
import 'package:flutter/foundation.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

import 'log.dart';
import 'relay_link.dart';

class _H {
  _H(this.msg, [this.bytes]);
  final bool Function(Map<String, dynamic>) msg; // true: done, forget the id
  final void Function(Uint8List)? bytes;
}

class Files {
  Files._() {
    final l = RelayLink.I;
    l.on('files_answer', (m) {
      final w = _answer;
      _answer = null;
      w?.complete(m);
    });
    l.on('__down', (_) {
      final w = _answer;
      _answer = null;
      w?.complete({'err': 'the link to the computer dropped'});
      if (lane) _lost('the relay link dropped');
    });
    l.on('files_out', (m) {
      if (!lane) return;
      try {
        _reply(jsonDecode(m['text'] as String) as Map<String, dynamic>);
      } catch (_) {}
    });
    l.on('files_chunk', (m) {
      if (!lane) return;
      final h = _handlers[m['id']];
      if (h?.bytes == null) return;
      h!.bytes!(base64Decode((m['b'] ?? '') as String));
    });
  }
  static final I = Files._();

  RTCPeerConnection? _pc;
  RTCDataChannel? _dc;
  bool ready = false;
  bool lane = false;
  Map<String, dynamic>? info; // hello: {roots, home, access, device, space, trash}
  int _next = 1;
  Future<Map<String, dynamic>>? _connecting;
  Completer<Map<String, dynamic>>? _answer;
  final _handlers = <int, _H>{};
  final laneNote = ValueNotifier<String>('');

  void _lost(String why) {
    if (_pc == null && !ready) return;
    rlog('files: link lost — $why');
    ready = false;
    lane = false;
    _pc?.close();
    _pc = null;
    _dc = null;
    _connecting = null;
    final hs = [..._handlers.values];
    _handlers.clear();
    for (final h in hs) {
      h.msg({'err': 'the connection to the computer was lost', 'lost': true});
    }
  }

  void close() => _lost('closed');

  void _reply(Map<String, dynamic> m) {
    if (m['lane'] == true) laneNote.value = (m['err'] ?? '') as String;
    final h = _handlers[m['id']];
    if (h != null && h.msg(m)) _handlers.remove(m['id']);
  }

  Future<Map<String, dynamic>> _openLane(String why) async {
    rlog("files: on the relay's limited lane — $why");
    lane = true;
    ready = true;
    try {
      info = await req('hello').timeout(const Duration(seconds: 8));
    } catch (e) {
      lane = false;
      ready = false;
      rethrow;
    }
    laneNote.value = 'Files are on the relay’s limited lane: small files only, until a direct path is found.';
    return info!;
  }

  Future<Map<String, dynamic>> _open() async {
    if (!RelayLink.I.up.value) throw Exception('not connected to the computer yet');
    final pc = await createPeerConnection({
      'iceServers': [
        {'urls': 'stun:stun.l.google.com:19302'},
        {'urls': 'stun:stun1.l.google.com:19302'},
      ],
      'candidateNetworkPolicy': 'all',
    });
    final dc = await pc.createDataChannel('files', RTCDataChannelInit()..ordered = true);
    _pc = pc;
    _dc = dc;
    final opened = Completer<bool>();
    pc.onConnectionState = (s) {
      if (pc == _pc && (s == RTCPeerConnectionState.RTCPeerConnectionStateFailed || s == RTCPeerConnectionState.RTCPeerConnectionStateClosed)) _lost(s.name);
    };
    dc.onDataChannelState = (s) {
      if (s == RTCDataChannelState.RTCDataChannelOpen && !opened.isCompleted) opened.complete(true);
      if (s == RTCDataChannelState.RTCDataChannelClosed && dc == _dc) _lost('channel closed');
    };
    dc.onMessage = (m) {
      if (!m.isBinary) {
        try {
          _reply(jsonDecode(m.text) as Map<String, dynamic>);
        } catch (_) {}
        return;
      }
      final u8 = m.binary;
      if (u8.length < 4) return;
      final id = ByteData.sublistView(u8, 0, 4).getUint32(0);
      _handlers[id]?.bytes?.call(Uint8List.sublistView(u8, 4));
    };
    final gathered = Completer<void>();
    pc.onIceGatheringState = (s) {
      if (s == RTCIceGatheringState.RTCIceGatheringStateComplete && !gathered.isCompleted) gathered.complete();
    };
    await pc.setLocalDescription(await pc.createOffer());
    await gathered.future.timeout(const Duration(seconds: 3), onTimeout: () {});
    final local = await pc.getLocalDescription();
    _answer = Completer();
    if (!RelayLink.I.send({'type': 'files_offer', 'sdp': jsonEncode({'type': 'offer', 'sdp': local?.sdp ?? ''})})) {
      _answer = null;
      _lost('refused');
      throw Exception('not connected to the computer');
    }
    final reply = await _answer!.future.timeout(const Duration(seconds: 15),
        onTimeout: () => {'err': 'the computer did not answer — its wado may be older than this app'});
    if ((reply['err'] ?? '') != '') {
      _lost('refused');
      throw Exception(reply['err']);
    }
    final d = jsonDecode(reply['sdp'] as String) as Map<String, dynamic>;
    await pc.setRemoteDescription(RTCSessionDescription(d['sdp'] as String, (d['type'] ?? 'answer') as String));
    final direct = await opened.future.timeout(const Duration(seconds: 8), onTimeout: () => false);
    if (!direct) {
      pc.close();
      if (_pc == pc) {
        _pc = null;
        _dc = null;
      }
      return _openLane('no direct path (ICE did not connect in 8 s)');
    }
    ready = true;
    info = await req('hello');
    laneNote.value = '';
    rlog('files: open — access ${info?['access']}');
    return info!;
  }

  Future<Map<String, dynamic>> connect() {
    if (ready && info != null) return Future.value(info!);
    return _connecting ??= _open().catchError((Object e) {
      _connecting = null;
      _lost('$e');
      throw e;
    });
  }

  bool _send(Map<String, dynamic> obj) {
    if (lane) return RelayLink.I.send({'type': 'files_in', 'text': jsonEncode(obj)});
    final dc = _dc;
    if (dc == null || dc.state != RTCDataChannelState.RTCDataChannelOpen) return false;
    dc.send(RTCDataChannelMessage(jsonEncode(obj)));
    return true;
  }

  Future<Map<String, dynamic>> req(String op, [Map<String, dynamic> args = const {}]) {
    final c = Completer<Map<String, dynamic>>();
    final id = _next++;
    _handlers[id] = _H((m) {
      if ((m['err'] ?? '') != '') {
        c.completeError(Exception(m['err']));
      } else {
        c.complete(m);
      }
      return true;
    });
    if (!_send({'id': id, 'op': op, ...args})) {
      _handlers.remove(id);
      c.completeError(Exception('not connected'));
    }
    return c.future;
  }

  /// A request whose answer comes in parts: `onMsg` returns true when it is done.
  int stream(String op, Map<String, dynamic> args, bool Function(Map<String, dynamic>) onMsg, void Function(Uint8List) onBytes) {
    final id = _next++;
    _handlers[id] = _H(onMsg, onBytes);
    if (!_send({'id': id, 'op': op, ...args})) {
      _handlers.remove(id);
      onMsg({'err': 'not connected', 'lost': true});
    }
    return id;
  }

  /// Bytes of an upload (`put`), framed with its transfer id.
  void bytes(int id, Uint8List u8) {
    if (lane) {
      RelayLink.I.send({'type': 'files_chunk', 'id': id, 'b': base64Encode(u8)}, urgent: false);
      return;
    }
    final f = Uint8List(4 + u8.length);
    ByteData.sublistView(f).setUint32(0, id);
    f.setRange(4, f.length, u8);
    _dc?.send(RTCDataChannelMessage.fromBinary(f));
  }

  /// Fetch a whole file (`get`) into memory, with progress 0..1.
  Future<Uint8List> get(String path, {void Function(double)? progress}) {
    final c = Completer<Uint8List>();
    final b = BytesBuilder(copy: false);
    var size = 0;
    stream('get', {'path': path}, (m) {
      if ((m['err'] ?? '') != '') {
        c.completeError(Exception(m['err']));
        return true;
      }
      if (m['size'] != null) size = (m['size'] as num).toInt();
      if (m['done'] == true) {
        c.complete(b.takeBytes());
        return true;
      }
      return false;
    }, (u8) {
      b.add(u8);
      if (size > 0) progress?.call(b.length / size);
    });
    return c.future;
  }
}

/// Upload one file (files_xfer.js): `put` → bytes in chunks under a buffer ceiling → `put_end`
/// with its SHA-256. Progress 0..1. Throws "exists" when the name is taken and `clash` is fail.
Future<String> upload(String dir, String name, Uint8List data, {String clash = 'rename', void Function(double)? progress}) async {
  final f = Files.I;
  final started = Completer<Map<String, dynamic>>();
  final ended = Completer<void>();
  final id = f.stream('put', {'dir': dir, 'name': name, 'size': data.length, 'clash': clash}, (m) {
    if (!started.isCompleted) {
      started.complete(m);
      return false;
    }
    if ((m['err'] ?? '') != '') {
      if (!ended.isCompleted) ended.completeError(Exception(m['err']));
      return true;
    }
    if (!ended.isCompleted) ended.complete();
    return true;
  }, (_) {});
  final put = await started.future;
  if ((put['err'] ?? '') != '') throw Exception(put['err']);
  const chunk = 16 * 1024;
  var pos = (put['offset'] ?? 0) as int;
  final sha = sha256Sink();
  if (pos > 0) sha.add(data.sublist(0, pos));
  while (pos < data.length) {
    final end = (pos + chunk).clamp(0, data.length);
    final part = Uint8List.sublistView(data, pos, end);
    sha.add(part);
    f.bytes(id, part);
    pos = end;
    progress?.call(pos / data.length);
    // Let the channel drain: a phone's radio is slower than this loop.
    if (pos % (chunk * 16) == 0) await Future.delayed(const Duration(milliseconds: 8));
  }
  final hex = sha.close();
  final done = await f.req('put_end', {'xfer': id, 'sha256': hex});
  return (done['name'] ?? put['name'] ?? name) as String;
}

class _Sha {
  final _out = _Capture();
  late final ByteConversionSink _in = crypto.sha256.startChunkedConversion(_out);
  void add(List<int> b) => _in.add(b);
  String close() {
    _in.close();
    return _out.digest.toString();
  }
}

class _Capture implements Sink<crypto.Digest> {
  late crypto.Digest digest;
  @override
  void add(crypto.Digest d) => digest = d;
  @override
  void close() {}
}

_Sha sha256Sink() => _Sha();
