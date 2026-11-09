// What the app adds to the hand (port of js/native_feel.js): the phone's own haptic effects by
// kind, and "active" — screen on, no system bars — while a session or the touchpad is up.
import 'dart:io';

import 'package:flutter/services.dart';

import '../../wado/pad_haptics.dart';

const _os = MethodChannel('wado/os');

/// tick · tap · claim · confirm · reject · toggle_on · toggle_off · heavy; or a plain pulse of
/// `ms` (the gamepad's strength).
void feel(String kind, {int ms = 0}) {
  if (Platform.isAndroid) {
    _os.invokeMethod('haptic', {'kind': kind, 'ms': ms});
  } else {
    HapticFeedback.selectionClick();
  }
}

/// A touchpad feel ([padFeels]): Android's composition primitives where the phone has them, else
/// the nearest predefined effect.
void feelPad(String name) {
  final prims = padFeels[name];
  if (prims == null) return;
  if (Platform.isAndroid) {
    _os.invokeMethod('haptic', {
      'kind': padFallback[name] ?? 'tick',
      'ms': 0,
      'prims': [for (final (p, scale, delay) in prims) {'p': p, 's': scale, 'd': delay}],
    });
  } else {
    final heavy = name == 'grab' || name == 'detent';
    heavy ? HapticFeedback.mediumImpact() : HapticFeedback.selectionClick();
  }
}

/// `navigator.vibrate(ms)` in the web: the nearest effect.
void buzz(int ms) => feel(ms <= 6 ? 'tick' : ms <= 15 ? 'tap' : 'heavy');

int _active = 0;

/// Counted: the session and the touchpad can both hold it.
void active(bool on) {
  final was = _active > 0;
  _active = (_active + (on ? 1 : -1)).clamp(0, 9);
  final now = _active > 0;
  if (was == now) return;
  if (Platform.isAndroid) {
    _os.invokeMethod('active', {'on': now});
  } else {
    SystemChrome.setEnabledSystemUIMode(now ? SystemUiMode.immersiveSticky : SystemUiMode.edgeToEdge);
  }
}
