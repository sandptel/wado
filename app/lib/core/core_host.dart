// The Rust core (crates/app-bridge) as the app sees it: one event stream, decoded, fanned out to
// whoever opened a room, plus the saved computers and the pairing/answer events. The core is the
// secure pipe; everything above it is the web client's logic, ported (lib/wado/).
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:path_provider/path_provider.dart';

import '../bridge/api/core.dart';
import '../bridge/frb_generated.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart' show ExternalLibrary;

/// A saved computer (`computers` event).
class Computer {
  Computer(this.j);
  final Map<String, dynamic> j;
  String get key => j['key'] as String;
  String get name => j['name'] as String;
  String get remoteId => j['remote_id'] as String;
  String get relayUrl => j['relay_url'] as String;
  String get os => (j['os'] ?? '') as String;
  String get logo => (j['logo'] ?? 'linux') as String;
  String get desktop => (j['desktop'] ?? '') as String;
  int get hue => (j['hue'] ?? 0) as int;
}

/// One room's callbacks (`open`).
class RoomSink {
  RoomSink({required this.onState, required this.onMsg, required this.onRelay});
  final void Function(Map<String, dynamic> state) onState;
  final void Function(Map<String, dynamic> msg) onMsg;
  final void Function(Map<String, dynamic> frame) onRelay;
}

class CoreHost {
  CoreHost._(this.core);
  final AppCore core;
  static late CoreHost I;

  final computers = ValueNotifier<List<Computer>>([]);
  final _rooms = <String, RoomSink>{};
  final _pair = StreamController<Map<String, dynamic>>.broadcast();
  final _answers = StreamController<Map<String, dynamic>>.broadcast();
  Stream<Map<String, dynamic>> get pairSteps => _pair.stream;
  Stream<Map<String, dynamic>> get answers => _answers.stream;
  late final String clientKey = core.clientKey();
  late final bool keyReset = core.keyReset();
  static String deviceName = '${Platform.operatingSystem[0].toUpperCase()}${Platform.operatingSystem.substring(1)} · app';

  static Future<CoreHost> start() async {
    // The bundle's own copy: next to the executable on Linux, the APK's jniLibs on Android.
    final lib = Platform.isLinux ? '${File(Platform.resolvedExecutable).parent.path}/lib/libwado_app_bridge.so' : 'libwado_app_bridge.so';
    await WadoCore.init(externalLibrary: ExternalLibrary.open(lib));
    final dir = await getApplicationSupportDirectory();
    final key = await _boxKey();
    final core = await AppCore.start(dir: '${dir.path}/core', boxKey: key, deviceName: deviceName);
    final h = CoreHost._(core);
    I = h;
    h._load(core.computers());
    core.events().listen(h._on);
    return h;
  }

  /// The box key in the platform keystore (docs/app.md). Debug Linux may use a file instead.
  static Future<Uint8List> _boxKey() async {
    const dev = String.fromEnvironment('WADO_DEV_BOX');
    if (!kReleaseMode && dev.isNotEmpty) {
      final f = File(dev);
      if (!f.existsSync()) {
        f.writeAsBytesSync(_random32());
        await Process.run('chmod', ['600', dev]);
      }
      return f.readAsBytesSync();
    }
    const s = FlutterSecureStorage();
    final have = await s.read(key: 'wado.box.v1');
    if (have != null) return base64Decode(have);
    final k = _random32();
    await s.write(key: 'wado.box.v1', value: base64Encode(k));
    return k;
  }

  static Uint8List _random32() {
    final r = Random.secure();
    return Uint8List.fromList(List.generate(32, (_) => r.nextInt(256)));
  }

  void _load(String json) {
    computers.value = (jsonDecode(json) as List).map((e) => Computer(e as Map<String, dynamic>)).toList();
  }

  Computer? computer(String key) {
    for (final c in computers.value) {
      if (c.key == key) return c;
    }
    return null;
  }

  void _on(String raw) {
    final e = jsonDecode(raw) as Map<String, dynamic>;
    switch (e['t']) {
      case 'computers':
        computers.value = (e['list'] as List).map((x) => Computer(x as Map<String, dynamic>)).toList();
      case 'room_state':
        _rooms[e['id']]?.onState(e);
      case 'room_msg':
        final r = _rooms[e['id']];
        if (r != null) {
          final m = jsonDecode(e['json'] as String);
          if (m is Map<String, dynamic>) r.onMsg(m);
        }
      case 'room_relay':
        final r = _rooms[e['id']];
        if (r != null) {
          final m = jsonDecode(e['json'] as String);
          if (m is Map<String, dynamic>) r.onRelay(m);
        }
      case 'pair':
        _pair.add(e);
      case 'answer':
        _answers.add(e);
    }
  }

  /// Open a room: `kind` seat | control | view. Null if the computer is not saved here.
  String? open(String key, String kind, RoomSink sink, {String instance = '', bool takeover = false}) {
    final id = core.openRoom(key: key, kind: kind, instance: instance, takeover: takeover);
    if (id.isEmpty) return null;
    _rooms[id] = sink;
    return id;
  }

  bool send(String id, Map<String, dynamic> msg, {bool urgent = true, bool resend = false}) =>
      core.sendRoom(id: id, json: jsonEncode(msg), urgent: urgent, resend: resend);

  void close(String id) {
    _rooms.remove(id);
    core.closeRoom(id: id);
  }

  int pair(String link) => core.pair(link: link).toInt();
  int pairTyped(String relay, String rid, String code) =>
      core.pairTyped(relay: relay, remoteId: rid, code: code).toInt();
  int forget(String key) => core.forget(key: key).toInt();
}
