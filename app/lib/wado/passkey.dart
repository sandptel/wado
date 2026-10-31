// The passkey (port of js/passkey.js + passkey_approve.js): what unlocks the touchpad and the
// keyboard on a computer that asks for one. The phone's own authenticator (a Keystore key behind
// the screen lock, kotlin/…/passkey) answers the computer's challenge; or someone at the
// computer approves this phone (`wado passkey approve`). Every prompt ends, and says why.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart' show AppLifecycleListener;
import 'package:flutter/services.dart';

import '../bridge/api/webauthn.dart';
import 'log.dart';
import 'relay_link.dart';

const _rpId = 'appassets.androidplatform.net';
const _origin = 'https://$_rpId';
const _ch = MethodChannel('wado/passkey');

String _enc(List<int> b) => base64Url.encode(b).replaceAll('=', '');
Uint8List _dec(String s) => base64Url.decode(s.padRight((s.length + 3) ~/ 4 * 4, '='));

class PasskeyView {
  const PasskeyView({this.state = const {}, this.busy = false, this.note = '', this.approve});
  final Map<String, dynamic> state;
  final bool busy;
  final String note;
  /// Approval on the computer: {state: waiting|approved|denied|expired, left_s, message}.
  final Map<String, dynamic>? approve;
  bool get registered => state['registered'] == true;
  bool get unlocked => state['unlocked'] == true;
  bool get required => state['required'] != false;
}

class Passkey {
  Passkey._() {
    final l = RelayLink.I;
    l.on('passkey_state', (m) {
      final s = (m['state'] as Map?)?.cast<String, dynamic>() ?? view.value.state;
      _settle();
      final err = (s['error'] ?? '') as String;
      view.value = PasskeyView(state: s, busy: false, note: err, approve: s['unlocked'] == true ? null : view.value.approve);
      if (s['unlocked'] == true && need.value.isNotEmpty) {
        // The sheet says so for a moment, then goes (`cheer`).
        opened.value++;
        need.value = '';
      }
      if (s['unlocked'] == true) {
        final f = _then;
        _then = null;
        f?.call();
      }
    });
    l.on('passkey_challenge', _challenge);
    l.on('passkey_approve_state', (m) {
      final st = '${m['state'] ?? ''}';
      _approveLimit?.cancel();
      view.value = PasskeyView(state: view.value.state, approve: st == 'cancelled' ? null : {'state': st, 'left_s': m['left_s'] ?? 0, 'message': m['message'] ?? ''});
    });
    // Leaving the app locks it, unless the person chose otherwise; idle locks it too.
    AppLifecycleListener(onHide: () {
      final st = view.value.state;
      if (view.value.unlocked && st['required'] != false && st['lock_on_leave'] != false) lockNow('the app went to the background');
    });
    Timer.periodic(const Duration(seconds: 15), (_) {
      final st = view.value.state;
      if (!view.value.unlocked || st['stay'] == 'lock' || st['required'] == false) return;
      final idle = Duration(seconds: (st['idle_s'] ?? 900) as int);
      if (DateTime.now().difference(lastAct) > idle) lockNow('idle for ${idle.inMinutes} min');
    });
    l.on('__up', (_) {
      final kept = view.value.unlocked && (view.value.state['stay'] ?? '15m') != '15m';
      if (!kept) view.value = PasskeyView(state: {...view.value.state, 'unlocked': false, 'idle_left_s': 0});
      l.send({'type': 'passkey_get'});
    });
  }
  static final I = Passkey._();

  final view = ValueNotifier<PasskeyView>(const PasskeyView());
  /// The person last touched the app (a pointer anywhere; lib/app.dart).
  DateTime lastAct = DateTime.now();
  DateTime _lastKeep = DateTime.now();
  void touched() {
    lastAct = DateTime.now();
    if (view.value.unlocked && lastAct.difference(_lastKeep).inSeconds > 60) {
      _lastKeep = lastAct;
      keepalive();
    }
  }
  VoidCallback? _then;
  Timer? _wait, _approveLimit;
  int _seq = 0;
  DateTime? _wanted;

  bool get native => Platform.isAndroid;

  void _say({bool? busy, String? note}) =>
      view.value = PasskeyView(state: view.value.state, busy: busy ?? view.value.busy, note: note ?? view.value.note, approve: view.value.approve);

  void _settle() {
    _wait?.cancel();
    _wait = null;
  }

  bool _ask(Map<String, dynamic> m) {
    if (!RelayLink.I.send(m)) {
      _settle();
      _say(busy: false, note: 'Not connected to the computer right now — try again when it is back.');
      return false;
    }
    _settle();
    _wait = Timer(const Duration(seconds: 20), () => _say(busy: false, note: "The computer didn't answer — check that it is online, then try again."));
    return true;
  }

  /// What the step-up sheet is up for: shell · pad · files · allow · claude · new · again ·
  /// paired · add; empty when it is down.
  final need = ValueNotifier<String>('');
  /// Bumped on each unlock the sheet asked for.
  final opened = ValueNotifier<int>(0);

  /// An action that needs the passkey (`W.passkeyNeed`): runs at once when unlocked, otherwise
  /// raises the sheet, which says what it is for and runs `then` once it is unlocked.
  void ask(String why, {VoidCallback? then}) {
    if (view.value.unlocked || (view.value.state.isNotEmpty && !view.value.required)) {
      then?.call();
      return;
    }
    _then = then;
    view.value = PasskeyView(state: view.value.state, approve: view.value.approve);
    need.value = why;
  }

  /// Unlock (or add a passkey first), then run `then`.
  void go({VoidCallback? then}) {
    // Already unlocked (or not asked for here): the action goes at once (passkeyThen).
    if (view.value.unlocked || (view.value.state.isNotEmpty && !view.value.required)) {
      then?.call();
      return;
    }
    if (view.value.busy) return;
    if (then != null) _then = then;
    _wanted = DateTime.now();
    _say(busy: true, note: '');
    _ask({'type': 'passkey_begin', 'purpose': view.value.registered ? 'unlock' : 'register'});
  }

  void add() {
    _wanted = DateTime.now();
    _say(busy: true, note: '');
    _ask({'type': 'passkey_begin', 'purpose': 'register'});
  }

  void cancel() {
    _settle();
    if (native) _ch.invokeMethod('cancel', {'id': _seq});
    if (view.value.approve?['state'] == 'waiting') RelayLink.I.send({'type': 'passkey_approve_cancel'});
    _then = null;
    need.value = '';
    view.value = PasskeyView(state: view.value.state);
  }

  void lockNow(String why) {
    if (!view.value.unlocked) return;
    rlog('passkey: locked here — $why');
    RelayLink.I.send({'type': 'passkey_lock'});
    view.value = PasskeyView(state: {...view.value.state, 'unlocked': false, 'idle_left_s': 0});
  }

  void require(bool on) => RelayLink.I.send({'type': 'passkey_require', 'on': on});

  void stay(String stay, {bool? lockOnLeave}) {
    final s = view.value.state;
    final lol = lockOnLeave ?? (stay == 'lock' ? false : s['lock_on_leave'] != false);
    view.value = PasskeyView(state: {...s, 'stay': stay, 'lock_on_leave': lol});
    RelayLink.I.send({'type': 'passkey_stay', 'stay': stay, 'lock_on_leave': lol});
  }

  void keepalive() => RelayLink.I.send({'type': 'passkey_keepalive'});

  /// Ask someone at the computer to let this phone in (passkey_approve.js).
  void approveOnComputer() {
    // The sheet stays up, saying it waits for the computer; the action still runs on yes.
    final keep = need.value, then = _then;
    cancel();
    need.value = keep;
    _then = then;
    if (!RelayLink.I.send({'type': 'passkey_approve'})) {
      _say(note: 'Not connected to the computer right now — try again when it is back.');
      return;
    }
    view.value = PasskeyView(state: view.value.state, approve: {'state': 'waiting', 'left_s': 120, 'message': ''});
    _approveLimit = Timer(const Duration(seconds: 135), () {
      if (view.value.approve?['state'] == 'waiting') {
        view.value = PasskeyView(state: view.value.state, approve: {'state': 'expired', 'left_s': 0, 'message': 'Nobody answered on the computer — try again when someone is there.'});
      }
    });
  }

  Future<void> _challenge(Map<String, dynamic> m) async {
    if (_wanted == null || DateTime.now().difference(_wanted!).inSeconds > 25) {
      rlog('passkey: a challenge nobody is waiting for — not shown');
      return;
    }
    _wanted = null;
    _settle();
    if (!native) {
      _say(busy: false, note: 'This device has no passkey of its own — approve it on the computer instead.');
      return;
    }
    final id = ++_seq;
    final challenge = _dec(m['challenge'] as String);
    try {
      if (m['purpose'] == 'register') {
        final r = (await _ch.invokeMapMethod<String, dynamic>('create', {'id': id, 'rpId': _rpId, 'timeoutMs': 60000}))!;
        if (r['error'] != null) throw PlatformException(code: '${r['code']}', message: '${r['message']}');
        final cred = _dec(r['cred'] as String);
        final cd = webauthnClientData(create: true, challenge: challenge, origin: _origin);
        final att = webauthnAttestation(rpId: _rpId, credId: cred, publicKey: r['point'] as Uint8List);
        _ask({'type': 'passkey_register', 'client_data': _enc(cd), 'attestation': _enc(att)});
      } else {
        final allow = m['cred_id'] is String ? [m['cred_id'] as String] : <String>[];
        final cred = await _ch.invokeMethod<String>('pick', {'rpId': _rpId, 'allow': allow});
        if (cred == null) {
          throw PlatformException(code: 'not-held', message: "This phone doesn't hold this computer's passkey. Add one here, or approve this phone on the computer.");
        }
        final cd = webauthnClientData(create: false, challenge: challenge, origin: _origin);
        final ad = webauthnAuthData(rpId: _rpId, count: 0);
        final data = webauthnSigned(authData: ad, clientData: cd);
        final r = (await _ch.invokeMapMethod<String, dynamic>('get', {'id': id, 'cred': cred, 'data': data, 'timeoutMs': 60000}))!;
        if (r['error'] != null) throw PlatformException(code: '${r['code']}', message: '${r['message']}');
        _ask({
          'type': 'passkey_unlock',
          'cred_id': cred,
          'client_data': _enc(cd),
          'auth_data': _enc(ad),
          'signature': _enc(r['signature'] as Uint8List),
        });
      }
    } on PlatformException catch (e) {
      rlog('passkey: ${e.code} ${e.message}');
      if (e.code == 'cancelled') return _say(busy: false, note: '');
      _say(busy: false, note: e.message ?? 'The passkey prompt did not finish.');
    } catch (e) {
      _say(busy: false, note: '$e');
    }
  }
}
