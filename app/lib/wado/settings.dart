// This device's settings (port of state.rs `Settings` + persist.rs): one JSON blob, saved on any
// change. Defaults are the web client's. Profiles are not here: the core holds saved computers.
import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart' show AppLifecycleListener;
import 'package:shared_preferences/shared_preferences.dart';

const defaults = <String, Object>{
  'res': '',
  'custom_w': 1280,
  'custom_h': 720,
  'scale': '',
  'fps': 60,
  'fps_lock': false,
  'quality': 'balanced',
  'bitrate': 4000,
  'encoder_backend': 'auto',
  'placement': 'auto',
  'focus_follows': false,
  'repeat_rate': 25,
  'repeat_delay': 200,
  'preset': '',
  'keyframe': '',
  'isolate_apps': true,
  'x_server': false,
  'command': '',
  'recent': <String>[],
  'show_hidden': false,
  'move_mode': false,
  'scroll_speed': 0.35,
  'orientation': 'auto',
  'natural_scroll': false,
  'touch_mode': 'pointer',
  'auto_bitrate': true,
  'low_latency_audio': false,
  'keyframe_mode': 'periodic',
  'redundant_input': false,
  'stream_sound': true,
  'dock_pin': false,
  'ws_bar': true,
  'rail': 'right',
  'edge_swipe': true,
  'pad_on': false,
  'pad_mode': 'keys',
  'pad_scale': 1.0,
  'pad_opacity': 0.5,
  'pad_inset_x': 0.0,
  'pad_inset_y': 0.0,
  'radius': 'round',
  'motion': 'full',
  'accent': '0D',
  'tiles': <String>['kbd', 'sound', 'playhere', 'pcmute', 'wifi', 'bluetooth', 'pcawake', 'fullscreen'],
  'selected': '',
  'widgets': <Object>[],
  'keep_awake': true,
  'dnd': false,
  'volume': 1.0,
  'muted': false,
  'theme': 'default-dark',
  'theme_custom': '',
  'text_scale': 0.0, // 0 = follow the phone
  'ui_size': 'cozy',
  'debug_master': true,
  'cockpit': false,
  'console_face': 'auto',
  'console_open': 'console',
  'console_deck': true,
  'pad_coach_seen': false,
  'gpad_layout': 'xbox',
  // The gamepad editor's changes: {"<layout>:<l|p>": {"<control>": [dx, dy, scale, hidden]}}.
  'gpad_edits': '{}',
  'stay_connected': false,
  'max_refresh': false,
  'volume_keys': true,
  'pad_marks_off': false,
  'game_modes': <String, Object?>{},
  'recent_play': <Object>[],
  'swipe3': true,
  'tips_seen': <String>[],
  'home_all': false,
};

class Settings extends ChangeNotifier {
  Settings._();
  static final I = Settings._();
  final _v = <String, Object?>{};
  SharedPreferences? _p;

  Future<void> load() async {
    _p = await SharedPreferences.getInstance();
    final raw = _p!.getString('wado.settings');
    if (raw != null) {
      try {
        _v.addAll((jsonDecode(raw) as Map).cast<String, Object?>());
      } catch (_) {}
    }
  }

  /// Values that shadow the stored ones for now — a Game-mode session's (game_mode.dart). Never
  /// saved with the rest; a change to a shadowed key goes to [onOver] instead.
  final _over = <String, Object?>{};
  void Function(String k, Object? v)? onOver;

  /// Shadow [values] (empty: stop shadowing). One notification, so the screen moves once.
  void overlay(Map<String, Object?> values) {
    if (mapEquals(_over, values)) return;
    _over
      ..clear()
      ..addAll(values);
    notifyListeners();
  }

  bool get overlaid => _over.isNotEmpty;

  T get<T>(String k) {
    final v = _over.containsKey(k) ? _over[k] : (_v[k] ?? defaults[k]);
    // A key nobody wrote and with no default reads as its type's empty value, never a crash.
    if (v == null) {
      if (T == bool) return false as T;
      if (T == String) return '' as T;
      if (T == int) return 0 as T;
      if (T == double) return 0.0 as T;
      return <String>[] as T;
    }
    if (T == double && v is int) return v.toDouble() as T;
    if (v is List && v is! List<String> && <String>[] is T) return v.cast<String>() as T;
    return v as T;
  }

  String s(String k) => get<String>(k);
  bool b(String k) => get<bool>(k);
  int i(String k) => get<int>(k);
  double d(String k) => get<double>(k);

  void set(String k, Object? v) {
    if (_over.containsKey(k)) {
      if (_over[k] == v) return;
      _over[k] = v;
      onOver?.call(k, v);
      notifyListeners();
      return;
    }
    if (_v[k] == v) return;
    _v[k] = v;
    // Written once things settle: a slider sets on every tick, and encoding every setting each
    // time was work done on the frame.
    _flush;
    _save?.cancel();
    _save = Timer(const Duration(milliseconds: 400), () => _p?.setString('wado.settings', jsonEncode(_v)));
    notifyListeners();
  }

  /// A signal for [keys] alone: it fires when one of them changes, not on every setting — so a
  /// slider dragged elsewhere never rebuilds what does not show it. One per caller, kept for
  /// the widget's life (made in build, it would be a new listener every frame).
  Listenable keys(List<String> keys) => _Keys(this, keys);

  Timer? _save;
  // ignore: unused_field
  late final _flush = AppLifecycleListener(onHide: () {
    if (_save?.isActive ?? false) {
      _save!.cancel();
      _p?.setString('wado.settings', jsonEncode(_v));
    }
  });
}

class _Keys extends ChangeNotifier {
  _Keys(this.s, this.keys) : _was = _now(s, keys) {
    s.addListener(_check);
  }
  final Settings s;
  final List<String> keys;
  String _was;

  static String _now(Settings s, List<String> keys) => [for (final k in keys) s.get<Object?>(k)].join('\u0000');

  void _check() {
    final now = _now(s, keys);
    if (now == _was) return;
    _was = now;
    notifyListeners();
  }

  @override
  void dispose() {
    s.removeListener(_check);
    super.dispose();
  }
}
