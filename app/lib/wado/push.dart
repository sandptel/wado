// Push from the selected computer (port of js/push.js, the app's UnifiedPush branch): the computer
// says its key and what this device subscribed to (`push_info`); turning it on registers with the
// phone's UnifiedPush distributor for that computer and hands the endpoint to it.
import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'relay_link.dart';

class Push {
  Push._() {
    final l = RelayLink.I;
    l.on('push_info', (m) {
      info.value = (m['info'] as Map?)?.cast<String, dynamic>();
      busy.value = false;
      error.value = '${info.value?['error'] ?? ''}';
    });
    l.on('__up', (_) => l.send({'type': 'push_get'}));
  }
  static final I = Push._();
  static const _os = MethodChannel('wado/os');

  /// {key, subscribed, kinds: {claude_permission, claude_done, online, transfer}, error}
  final info = ValueNotifier<Map<String, dynamic>?>(null);
  final busy = ValueNotifier(false);
  final error = ValueNotifier('');
  Completer<Map>? _wait;

  Map<String, dynamic> get kinds => (info.value?['kinds'] as Map?)?.cast<String, dynamic>() ?? const {};

  Future<void> on() async {
    final key = '${info.value?['key'] ?? ''}';
    if (key.isEmpty) {
      error.value = "The computer hasn't said its notification key yet — try again.";
      return;
    }
    busy.value = true;
    error.value = '';
    await _os.invokeMethod<bool>('notifications');
    final w = _wait = Completer<Map>();
    await _os.invokeMethod('pushSubscribe', {'vapid': key, 'rid': RelayLink.I.remoteId});
    final j = await w.future.timeout(const Duration(seconds: 30), onTimeout: () => {'error': 'The UnifiedPush app did not answer.'});
    if (j['error'] != null) {
      busy.value = false;
      error.value = '${j['error']}';
      return;
    }
    final k = (j['keys'] as Map?) ?? const {};
    RelayLink.I.send({
      'type': 'push_subscribe',
      'sub': {'endpoint': j['endpoint'], 'p256dh': k['p256dh'], 'auth': k['auth'], 'kind': 'unifiedpush'},
      'kinds': kinds,
    });
  }

  void off() {
    busy.value = true;
    RelayLink.I.send({'type': 'push_unsubscribe'});
  }

  void setKinds(Map<String, dynamic> k) => RelayLink.I.send({'type': 'push_kinds_set', 'kinds': k});

  void test() {
    busy.value = true;
    RelayLink.I.send({'type': 'push_test'});
  }

  /// The distributor's answer, via the activity.
  void endpoint(Map m) {
    final w = _wait;
    _wait = null;
    if (w != null && !w.isCompleted) w.complete(m);
  }
}
