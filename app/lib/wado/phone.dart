// What the phone itself does for wado (port of js/native_sync.js + native_push.js and the native
// app's MainActivity): a session app's notification as the phone's own while wado is not in
// front; the computer's now-playing as the phone's media session (lock screen, shade, Bluetooth
// buttons); the volume keys turning the computer's sound; "Stay connected" holding the links.
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart' show AppLifecycleState, WidgetsBinding;

import 'host.dart';
import 'push.dart';
import 'relay_link.dart';
import 'settings.dart';

class Phone {
  Phone._() {
    final l = RelayLink.I;
    l.on('notification', (m) {
      if (WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed) return;
      final app = '${m['app'] ?? ''}', summary = '${m['summary'] ?? ''}';
      _os.invokeMethod('notify', {
        'kind': 'app',
        'id': '${m['id'] ?? 0}',
        'title': app.isEmpty ? summary : (summary.isEmpty ? app : '$app — $summary'),
        'body': '${m['body'] ?? ''}',
      });
    });
    l.on('notification_closed', (m) => _os.invokeMethod('notifyClear', {'kind': 'app', 'id': '${m['id'] ?? 0}'}));
    Host.I.state.addListener(_sync);
    Settings.I.addListener(_prefs);
    _prefs();
  }
  static final I = Phone._();
  static const _os = MethodChannel('wado/os');

  String _last = '';
  String? _art;
  int _pos = 0, _posAt = 0;
  bool _playing = false;
  final _clock = Stopwatch()..start();
  bool? _stay, _keys;

  /// The player a press goes to: the one playing, else the first.
  Map<String, dynamic>? get _player {
    final media = (Host.I.state.value?['media'] as List? ?? const []).cast<Map<String, dynamic>>();
    return media.where((p) => p['playing'] == true).firstOrNull ?? media.firstOrNull;
  }

  Map<String, dynamic>? get _speaker {
    final a = Host.I.state.value?['audio'] as Map<String, dynamic>?;
    final sinks = (a?['sinks'] as List? ?? const []).cast<Map<String, dynamic>>().where((s) => s['name'] != a?['phone_sink']).toList();
    return sinks.where((s) => s['name'] == a?['default_sink']).firstOrNull ?? sinks.firstOrNull;
  }

  void _sync() {
    _media();
    final keys = Settings.I.b('volume_keys') && _speaker != null;
    if (keys != _keys) {
      _keys = keys;
      _os.invokeMethod('volumeKeys', keys);
    }
  }

  void _media() {
    // The phone's media session, told only what changed. The position moved on every poll and
    // made each one a resend — the cover art across the channel, then a MediaSession metadata
    // and a notification post, each a binder call with the bitmap, all on the main thread every
    // 1.2 s while anything played. The phone runs the clock itself from a position and a speed,
    // so the position goes again only when it strays (a seek, a track change).
    final p = _player;
    if (p == null) {
      if (_last.isNotEmpty) _os.invokeMethod('nowPlaying', const <String, Object?>{});
      _last = '';
      _art = null;
      return;
    }
    final playing = p['playing'] == true;
    final pos = ((p['position_ms'] ?? 0) as num).toInt();
    final key = [p['title'], p['artist'], p['app'], playing, p['length_ms'], p['can_next'], p['can_prev'], p['can_seek']].join('|');
    final at = _clock.elapsedMilliseconds;
    final expect = _pos + (_playing ? at - _posAt : 0);
    final art = (p['art'] as String?) ?? '';
    final newArt = art != _art;
    if (key != _last || newArt || (pos - expect).abs() > 2500) {
      _last = key;
      _pos = pos;
      _posAt = at;
      _playing = playing;
      _os.invokeMethod('nowPlaying', {
        'title': p['title'] ?? '', 'artist': p['artist'] ?? '', 'app': p['app'] ?? '', //
        if (newArt) 'art': art,
        'playing': playing, 'position_ms': pos, 'length_ms': p['length_ms'] ?? 0,
        'can_next': p['can_next'] == true, 'can_prev': p['can_prev'] == true, 'can_seek': p['can_seek'] == true,
      });
      _art = art;
    }
  }

  void _prefs() {
    final stay = Settings.I.b('stay_connected');
    if (stay != _stay) {
      _stay = stay;
      _os.invokeMethod('stay', stay);
    }
    _sync();
  }

  /// A call from the phone's side of the channel.
  Future<void> call(MethodCall c) async {
    switch (c.method) {
      case 'media':
        final p = _player, a = c.arguments as Map;
        if (p != null) Host.I.act({'do': 'media', 'bus': p['bus'], 'op': {'op': a['op'], if (a['ms'] != null) 'ms': a['ms']}});
      case 'volume':
        final s = _speaker;
        if (s == null) return;
        final v = (((s['volume'] ?? 0) as num) + (c.arguments as num)).clamp(0.0, 1.0);
        s['volume'] = v;
        Host.I.act({'do': 'sink_volume', 'id': s['id'], 'volume': v});
      case 'pushEndpoint':
        Push.I.endpoint(c.arguments as Map);
      case 'stayOff':
        Settings.I.set('stay_connected', false);
    }
  }
}
