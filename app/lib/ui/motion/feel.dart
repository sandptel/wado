// What the app adds to the hand (port of js/native_feel.js): the phone's own haptic effects by
// kind, and "active" — screen on, no system bars — while a session or the touchpad is up.
import 'dart:io';

import 'package:flutter/services.dart';

const _os = MethodChannel('wado/os');

/// tick · tap · claim · confirm · reject · toggle_on · toggle_off · heavy
void feel(String kind) {
  if (Platform.isAndroid) {
    _os.invokeMethod('haptic', {'kind': kind});
  } else {
    HapticFeedback.selectionClick();
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
