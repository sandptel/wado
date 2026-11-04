// Unit tests run the app's shared rules against the real thing: the bridge built for this host
// (`cargo build -p wado-app-bridge`), loaded once.
import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:wado/bridge/frb_generated.dart';

Future<void> loadUx() async {
  final lib = File('../target/debug/libwado_app_bridge.so');
  if (!lib.existsSync()) throw StateError('build it first: cargo build -p wado-app-bridge');
  await WadoCore.init(externalLibrary: ExternalLibrary.open(lib.absolute.path));
}
