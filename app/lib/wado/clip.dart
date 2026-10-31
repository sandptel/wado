// The clipboard (port of js/cliphost.js + js/clip_share.js): the selected computer's clipboard
// history (`clip_list` → `clip_history`, refreshed by the daemon on every copy; one entry in full
// with `clip_get` → `clip_data`), and the clipboard shared both ways by itself.
//
// Computer → phone ("Copy from … automatically"): the link asks for every new copy (`clip_watch
// {live}`); each `clip_now` is written to this phone's clipboard at once (the app may write
// whenever).
// Phone → computer ("Send this phone's copies automatically"): when the app comes back to the
// front, this phone's clipboard is read (Android lets an app read only then) and sent if it is new.
// A copy a password manager marked sensitive is never sent by itself.
//
// Echo: every value is named by the daemon's hash (the first 16 bytes of SHA-256, hex). `last` is
// what this phone's clipboard is known to hold — written here, or read and sent — so a copy coming
// back the way it went stops at once, on either side.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart' as crypto;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/core_host.dart';
import 'host.dart';
import 'relay_link.dart';
import 'settings.dart';
import 'side_links.dart';

const _os = MethodChannel('wado/os');
const _maxText = 1 << 20;

/// What sharing did, said for a moment (`clipSaid`): n counts, so the same words say again.
class Said {
  const Said(this.n, this.text, this.tone);
  final int n;
  final String text, tone; // good · bad · dim
}

class Clip {
  Clip._() {
    final l = RelayLink.I;
    l.on('clip_history', (m) {
      available.value = m['available'] == true;
      entries.value = [for (final e in (m['entries'] as List? ?? [])) Map<String, dynamic>.from(e as Map)];
      error.value = (m['error'] ?? '') as String;
      loaded.value = true;
    });
    l.on('clip_data', (m) {
      final id = '${m['id']}';
      final c = (mime: '${m['mime']}', data: '${m['data']}');
      _full[id] = c;
      if (c.mime.startsWith('image/')) images.value = {...images.value, id: c.data};
      for (final f in _wait.remove(id) ?? const <void Function(({String mime, String data})?)>[]) {
        f(c);
      }
    });
    l.on('clip_error', (m) {
      final t = (m['message'] ?? "The computer's clipboard did not answer.") as String;
      if (_acks.isNotEmpty) say(t, 'bad');
    });
    l.on('clip_now', _now);
    l.on('clip_held', (m) {
      final id = m['id'] as String?;
      if (id != null) {
        _held[id] = (m['message'] ?? '') as String;
        for (final f in _wait.remove(id) ?? const <void Function(({String mime, String data})?)>[]) {
          f(null);
        }
      }
      final k = '$_key ${id ?? m['hash'] ?? ''}';
      if (_heldSaid.add(k)) say(_onIt((m['message'] ?? '') as String), 'dim');
    });
    l.on('clip_pushed', (m) {
      if (_acks.remove(m['hash']) != null) say('Sent to $_name');
    });
    l.on('__up', (_) {
      list();
      _watched = null;
      _sync();
    });
    l.on('__down', (_) => _watched = null);
    Host.I.config.addListener(_sync);
    _life = AppLifecycleListener(onResume: back, onHide: () => _hiddenAt = DateTime.now());
    try {
      prefs = (jsonDecode(Settings.I.s('clipShare').isEmpty ? '{}' : Settings.I.s('clipShare')) as Map).cast<String, dynamic>();
    } catch (_) {}
    _last = Settings.I.s('clipLast');
  }
  static final I = Clip._();

  final loaded = ValueNotifier<bool>(false);
  final available = ValueNotifier<bool>(false);
  final entries = ValueNotifier<List<Map<String, dynamic>>>([]);
  final error = ValueNotifier<String>('');
  /// Pictures by entry id, as data URLs, once fetched.
  final images = ValueNotifier<Map<String, String>>({});
  final said = ValueNotifier<Said>(const Said(0, '', ''));
  /// Each computer's two switches, as the person set them: key → {pull, push}.
  Map<String, dynamic> prefs = {};
  final changed = ValueNotifier<int>(0);

  final _full = <String, ({String mime, String data})>{};
  final _wait = <String, List<void Function(({String mime, String data})?)>>{};
  final _held = <String, String>{};
  final _heldSaid = <String>{};
  final _acks = <String, bool>{};
  String _last = '';
  bool? _watched;
  DateTime? _hiddenAt;
  DateTime _readAt = DateTime(2000);
  bool _reading = false;
  int _n = 0;
  // ignore: unused_field
  late final AppLifecycleListener _life;

  String get _key => RelayLink.I.key ?? '';
  String get _name => CoreHost.I.computer(_key)?.name ?? 'the computer';
  String _onIt(String why) => why.replaceAll('the computer', _name);
  bool get _may => (Host.I.config.value?['grants'] as Map?)?['clipboard'] != false;

  void say(String text, [String tone = 'good']) => said.value = Said(++_n, text, tone);

  /// This phone's switch for computer `key`: `pull` or `push`. On by default for a phone that may.
  bool pref(String key, String which) {
    final p = prefs[key];
    if (p is Map && p[which] is bool) return p[which] as bool;
    return true;
  }

  bool _on(String which) => _key.isNotEmpty && _may && pref(_key, which);

  void setPref(String key, String which, bool on) {
    prefs = {...prefs, key: {...?(prefs[key] as Map?)?.cast<String, dynamic>(), which: on}};
    Settings.I.set('clipShare', jsonEncode(prefs));
    if (which == 'pull') _watched = null;
    changed.value++;
    _sync();
  }

  void _remember(String h) {
    _last = h;
    Settings.I.set('clipLast', h);
  }

  static String hash(List<int> bytes) => crypto.sha256.convert(bytes).bytes.take(16).map((b) => b.toRadixString(16).padLeft(2, '0')).join();

  /// Tell the link what it should send — once, again on a change.
  void _sync() {
    if (!RelayLink.I.up.value || !_may) return;
    final live = pref(_key, 'pull');
    if (_watched == live) return;
    if (RelayLink.I.send({'type': 'clip_watch', 'live': live})) _watched = live;
  }

  void list() => RelayLink.I.send({'type': 'clip_list'});
  void pin(String id, bool on) => RelayLink.I.send({'type': 'clip_pin', 'id': id, 'on': on});
  void delete(String id) {
    _full.remove(id);
    _held.remove(id);
    RelayLink.I.send({'type': 'clip_delete', 'id': id});
  }

  /// Entry `id` in full; a throw with the daemon's words when it stays on the computer.
  Future<({String mime, String data})> fetch(String id) {
    final have = _full[id];
    if (have != null) return Future.value(have);
    if (_held.containsKey(id)) return Future.error(_Held(_held[id]!));
    final done = Completer<({String mime, String data})>();
    final first = !_wait.containsKey(id);
    (_wait[id] ??= []).add((c) {
      if (done.isCompleted) return;
      c == null ? done.completeError(_Held(_held[id] ?? '')) : done.complete(c);
    });
    if (first) RelayLink.I.send({'type': 'clip_get', 'id': id});
    Timer(const Duration(seconds: 8), () {
      if (!done.isCompleted) done.completeError('the computer did not answer');
    });
    return done.future;
  }

  /// An image tile came into view: fetch its picture once.
  void thumb(String id) {
    if (!_full.containsKey(id) && !_wait.containsKey(id) && !_held.containsKey(id)) fetch(id).catchError((_) => (mime: '', data: ''));
  }

  Future<bool> _write(String text) async {
    if (Platform.isAndroid) return await _os.invokeMethod<bool>('clipWrite', {'text': text}) ?? false;
    await Clipboard.setData(ClipboardData(text: text));
    return true;
  }

  /// Computer `key`'s entry `id` onto this phone (the selected one's link, or its light link).
  Future<bool> copyFrom(String key, String id, String kind) async {
    if (key == _key) return copy(id, kind);
    final name = CoreHost.I.computer(key)?.name ?? 'the computer';
    try {
      final c = await SideLinks.I.clipFetch(key, id);
      if (!c.mime.startsWith('text/')) {
        say('Pictures stay on $name — copy text, or open it in Files', 'dim');
        return false;
      }
      final ok = await _write(c.data);
      if (ok) {
        _remember(hash(utf8.encode(c.data)));
        say('Copied to this phone from $name');
      }
      return ok;
    } catch (e) {
      say('$e'.isEmpty || '$e' == 'no answer' ? '$name did not answer' : '$e'.replaceAll('the computer', name), 'dim');
      return false;
    }
  }

  /// Computer `key`'s entry `id` back onto its own clipboard.
  Future<bool> sendBackOn(String key, String id) async {
    if (key == _key) return sendBack(id);
    try {
      final c = await SideLinks.I.clipFetch(key, id);
      return SideLinks.I.send(key, {'type': 'clip_push', 'mime': c.mime, 'data': c.data});
    } catch (_) {
      say('${CoreHost.I.computer(key)?.name ?? 'The computer'} did not answer', 'bad');
      return false;
    }
  }

  /// Entry `id` onto this phone's clipboard; true when it took. Says what happened.
  Future<bool> copy(String id, String kind) async {
    if (_held.containsKey(id)) {
      say(_onIt(_held[id]!), 'dim');
      return false;
    }
    try {
      final c = await fetch(id);
      if (c.mime.startsWith('image/')) {
        // ponytail: pictures are not written to the phone's clipboard (Flutter's is text-only);
        // a platform image clip comes with the Kotlin ClipData path when wanted.
        say('Pictures stay on $_name — copy text, or open it in Files', 'dim');
        return false;
      }
      final ok = await _write(c.data);
      if (ok) {
        _remember(hash(utf8.encode(c.data)));
        say('Copied to this phone from $_name');
      } else {
        say('This phone would not take it — tap again', 'bad');
      }
      return ok;
    } catch (e) {
      say(e is _Held ? _onIt(e.why) : '$_name did not answer', e is _Held ? 'dim' : 'bad');
      return false;
    }
  }

  /// An entry of the history back onto the computer's clipboard.
  Future<bool> sendBack(String id) async {
    try {
      final c = await fetch(id);
      final h = hash(c.mime.startsWith('image/') ? _dataUrlBytes(c.data) : utf8.encode(c.data));
      if (!RelayLink.I.send({'type': 'clip_push', 'mime': c.mime, 'data': c.data})) throw 'down';
      _acks[h] = true;
      return true;
    } catch (e) {
      say(e is _Held ? _onIt(e.why) : '$_name did not answer', e is _Held ? 'dim' : 'bad');
      return false;
    }
  }

  static List<int> _dataUrlBytes(String u) {
    try {
      return base64Decode(u.split(',').last);
    } catch (_) {
      return const [];
    }
  }

  // ── computer → phone ──
  Future<void> _now(Map<String, dynamic> m) async {
    final h = (m['hash'] ?? '') as String;
    if (!_on('pull') || h.isEmpty || h == _last) return;
    final mime = (m['mime'] ?? 'text/plain') as String;
    if (!mime.startsWith('text/')) return;
    if (await _write((m['data'] ?? '') as String)) {
      _remember(h);
      say('Copied to this phone from $_name');
    }
  }

  // ── phone → computer ──
  Future<({String text, bool sensitive})?> _read() async {
    if (Platform.isAndroid) {
      final r = await _os.invokeMethod<Map>('clipRead');
      if (r == null) return null;
      return (text: '${r['text']}', sensitive: r['sensitive'] == true);
    }
    final d = await Clipboard.getData('text/plain');
    return d?.text == null || d!.text!.isEmpty ? null : (text: d.text!, sensitive: false);
  }

  /// This phone's clipboard to the computer. `manual`: a tap asked, so say what happened.
  Future<void> sendPhone({bool manual = true}) async {
    final got = await _read();
    if (got == null || got.text.isEmpty) {
      if (manual) say("This phone's clipboard is empty", 'dim');
      return;
    }
    if (got.sensitive && !manual) return; // a password manager's copy: never by itself
    final bytes = utf8.encode(got.text);
    if (bytes.length > _maxText) {
      if (manual) say('That text is too long to send', 'bad');
      return;
    }
    final h = hash(bytes);
    if (!manual && h == _last) return;
    if (!RelayLink.I.send({'type': 'clip_push', 'mime': 'text/plain', 'data': got.text, 'sensitive': got.sensitive})) {
      if (manual) say('$_name is not connected', 'bad');
      return;
    }
    _acks[h] = true;
    _remember(h);
  }

  /// The app came back to the front: offer this phone's copy (once for focus and visibility).
  Future<void> back() async {
    final t = DateTime.now();
    if (_reading || t.difference(_readAt).inMilliseconds < 800) return;
    if (!_on('push') || !RelayLink.I.up.value) return;
    _readAt = t;
    _reading = true;
    try {
      await sendPhone(manual: false);
    } catch (_) {
    } finally {
      _reading = false;
    }
  }

  /// When the app last went to the background (for the web's "came back after a while" rule).
  DateTime? get hiddenAt => _hiddenAt;
}

class _Held {
  _Held(this.why);
  final String why;
}
