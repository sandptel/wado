// Game mode: a switch on one app of one computer (spec 2026-11-04-player-home-and-devices). Set up
// once — how it is played, then the session's rules — and every open of that app from this phone
// uses it. While it runs, its values shadow the phone's own settings (Settings.overlay): nothing
// the person chose for ordinary sessions is touched, and a change made in the game is kept for
// that game.
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';

import 'session.dart';
import 'settings.dart';

/// One app's Game mode, as stored under `game_modes` (`<computer>|<exec>` → this as JSON).
class GameProfile {
  GameProfile({
    required this.name,
    required this.exec,
    this.play = 'pad',
    this.layout = 'xbox',
    this.picture = 'smooth',
    this.input = true,
    this.calm = true,
    this.awake = true,
    this.x = false,
    Map<String, Object?>? kept,
  }) : kept = kept ?? {};

  final String name, exec;
  /// `pad` (on-screen), `controller` (one held by this phone), `touch`.
  String play;
  /// The on-screen pad's layout (gpad_layouts.dart).
  String layout;
  /// `smooth`, `balanced`, `sharp` — see [pictures].
  String picture;
  /// Never miss a press: inputs sent twice, sound kept tight.
  bool input;
  /// No stray gestures: three-finger and edge swipes off while playing.
  bool calm;
  /// Volume buttons go to the game and the screen stays awake.
  bool awake;
  /// The X server (Steam, X11-only games). Restart-only, so it shapes how the session starts.
  bool x;
  /// What the person changed while playing, kept for next time.
  final Map<String, Object?> kept;

  Map<String, Object?> toJson() => {
        'name': name,
        'exec': exec,
        'play': play,
        'layout': layout,
        'picture': picture,
        'input': input,
        'calm': calm,
        'awake': awake,
        'x': x,
        if (kept.isNotEmpty) 'kept': kept,
      };

  static GameProfile fromJson(Map m) => GameProfile(
        name: '${m['name'] ?? ''}',
        exec: '${m['exec'] ?? ''}',
        play: '${m['play'] ?? 'pad'}',
        layout: '${m['layout'] ?? 'xbox'}',
        picture: '${m['picture'] ?? 'smooth'}',
        input: m['input'] != false,
        calm: m['calm'] != false,
        awake: m['awake'] != false,
        x: m['x'] == true,
        kept: (m['kept'] as Map?)?.cast<String, Object?>(),
      );

  /// The settings this profile shadows while it runs.
  Map<String, Object?> overlay({int? refresh}) => {
        'cockpit': true,
        'console_open': 'full',
        if (play == 'pad') ...{'console_face': 'gamepad', 'pad_on': true, 'gpad_layout': layout},
        if (play == 'touch') ...{'console_face': 'touchpad', 'pad_on': false},
        if (play == 'controller') 'pad_on': false,
        ...pictures(picture, refresh ?? 60),
        if (input) ...{'redundant_input': true, 'low_latency_audio': true},
        if (calm) ...{'edge_swipe': false, 'swipe3': false},
        if (awake) ...{'keep_awake': true, 'volume_keys': true},
        if (x) 'x_server': true,
        ...kept,
      };
}

/// The Picture presets, each the exact values it sets (shown to the person in mono).
Map<String, Object?> pictures(String id, int refresh) => switch (id) {
      'balanced' => {'fps': 60, 'quality': 'balanced', 'fps_lock': true, 'keyframe_mode': 'on_request'},
      'sharp' => {'fps': 30, 'quality': 'quality', 'fps_lock': false, 'keyframe_mode': 'periodic'},
      _ => {'fps': refresh.clamp(60, 120), 'quality': 'reactivity', 'fps_lock': true, 'keyframe_mode': 'on_request'},
    };

String pictureWords(String id, int refresh) {
  final p = pictures(id, refresh);
  final kf = p['keyframe_mode'] == 'on_request' ? 'keyframes on demand' : 'keyframes every few s';
  return '${p['fps']} fps · ${p['quality'] == 'reactivity' ? 'latency' : p['quality']} · ${p['fps_lock'] == true ? 'locked' : 'adaptive'} · $kf';
}

class GameMode extends ChangeNotifier {
  GameMode._() {
    Settings.I.onOver = _kept;
    Session.I.shown.addListener(_follow);
  }
  static final I = GameMode._();

  /// The profile shadowing the settings now, if any.
  final active = ValueNotifier<String?>(null);

  static String keyOf(String computer, String exec) => '$computer|$exec';

  Map<String, Object?> get _all => (Settings.I.get<Object?>('game_modes') as Map?)?.cast<String, Object?>() ?? {};

  GameProfile? of(String computer, String exec) {
    final m = _all[keyOf(computer, exec)];
    return m is Map ? GameProfile.fromJson(m) : null;
  }

  /// Every app of [computer] with Game mode on, newest set up first.
  List<GameProfile> on(String computer) => [
        for (final e in _all.entries.toList().reversed)
          if (e.key.startsWith('$computer|') && e.value is Map) GameProfile.fromJson(e.value as Map),
      ];

  void save(String computer, GameProfile p) {
    final all = Map<String, Object?>.from(_all)
      ..remove(keyOf(computer, p.exec))
      ..[keyOf(computer, p.exec)] = p.toJson();
    Settings.I.set('game_modes', all);
    notifyListeners();
  }

  void off(String computer, String exec) {
    final all = Map<String, Object?>.from(_all)..remove(keyOf(computer, exec));
    Settings.I.set('game_modes', all);
    notifyListeners();
  }

  static int get refresh => ui.PlatformDispatcher.instance.views.first.display.refreshRate.round();

  /// Shadow the settings with this game's for the session about to start.
  void begin(String computer, GameProfile p) {
    active.value = keyOf(computer, p.exec);
    Settings.I.overlay(p.overlay(refresh: refresh));
  }

  void end() {
    if (active.value == null) return;
    active.value = null;
    Settings.I.overlay(const {});
  }

  /// The session view went away: ordinary settings again.
  void _follow() {
    if (!Session.I.shown.value) end();
  }

  /// A shadowed setting changed in the game: keep it for that game.
  void _kept(String k, Object? v) {
    final key = active.value;
    if (key == null) return;
    final i = key.indexOf('|');
    final p = of(key.substring(0, i), key.substring(i + 1));
    if (p == null) return;
    if (k == 'gpad_layout' && v is String) {
      p.layout = v;
    } else {
      p.kept[k] = v;
    }
    save(key.substring(0, i), p);
  }
}
