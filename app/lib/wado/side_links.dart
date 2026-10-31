// Light links (port of js/side_link.js): a control-only connection to each computer home has a
// widget for, other than the selected one (that one has the full link). No picture, no sound, no
// session: the sealed relay link alone (a core control room: `control=1&strict=1`), asking for the
// computer's state, its config (what this phone may do there), its clipboard and its actions — so
// a widget shows live values and its controls act on the right computer. Dropped while a desktop
// session streams: the downlink then belongs to the picture.
//
// **A light link takes no seat.** Before every dial the relay is asked (`/online/:id`) whether it
// and the computer serve control links; an older relay never says so and is never dialled — it
// would read the join as an ordinary one and give the widget a seat.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../core/core_host.dart';
import 'log.dart';

/// One link's state, as the widgets read it. `state`: connecting · up · offline · refused ·
/// unreachable · old_relay · old_daemon.
class Side {
  const Side({this.state = '', this.host, this.config, this.clips, this.actions = const []});
  final String state;
  final Map<String, dynamic>? host, config;
  final List<Map<String, dynamic>>? clips;
  final List<Map<String, dynamic>> actions;
  Side copy({String? state, Map<String, dynamic>? host, Map<String, dynamic>? config, List<Map<String, dynamic>>? clips, List<Map<String, dynamic>>? actions}) =>
      Side(state: state ?? this.state, host: host ?? this.host, config: config ?? this.config, clips: clips ?? this.clips, actions: actions ?? this.actions);
}

class _Link {
  _Link(this.key, this.relay, this.id, this.wants);
  final String key, relay, id;
  List<String> wants;
  String? room;
  bool up = false, dead = false;
  Timer? retry, poll;
  int backoff = 0;
  final pics = <String, Map<String, dynamic>>{};
  final full = <String, ({String mime, String data})>{};
  final wait = <String, List<void Function(({String mime, String data})?)>>{};
  final held = <String, String>{};
}

class SideLinks {
  SideLinks._();
  static final I = SideLinks._();

  final state = ValueNotifier<Map<String, Side>>({});
  /// A computer's clip_now / clip_pushed / clip_held, for the clipboard share (wado/clip.dart).
  final clipEvents = StreamController<(String key, Map<String, dynamic> m)>.broadcast();
  final _links = <String, _Link>{};

  Side of(String key) => state.value[key] ?? const Side();
  void _say(_Link l, Side Function(Side) f) => state.value = {...state.value, l.key: f(of(l.key))};

  /// Hold exactly these links; drop the rest. `wants`: "clips" and/or "actions".
  void watch(List<(String key, String relay, String id, List<String> wants)> list) {
    final want = {for (final w in list) w.$1: w};
    for (final k in [..._links.keys]) {
      final w = want[k], l = _links[k]!;
      if (w == null || w.$2 != l.relay || w.$3 != l.id) drop(k);
    }
    for (final w in want.values) {
      final l = _links[w.$1];
      if (l != null) {
        final more = w.$4.where((x) => !l.wants.contains(x)).isNotEmpty;
        l.wants = w.$4;
        if (more && l.up) _askAll(l);
        continue;
      }
      final nl = _Link(w.$1, w.$2, w.$3, w.$4);
      _links[w.$1] = nl;
      _dial(nl);
    }
  }

  void drop(String key) {
    final l = _links.remove(key);
    if (l == null) return;
    l.dead = true;
    l.retry?.cancel();
    l.poll?.cancel();
    if (l.room != null) CoreHost.I.close(l.room!);
  }

  bool up(String key) => _links[key]?.up ?? false;

  bool send(String key, Map<String, dynamic> msg) {
    final l = _links[key];
    if (l == null || !l.up || l.room == null) return false;
    return CoreHost.I.send(l.room!, msg);
  }

  /// Does this relay, for this computer, serve control-only links? "" to go ahead.
  Future<String> _ask(_Link l) async {
    try {
      final c = HttpClient()..connectionTimeout = const Duration(seconds: 8);
      final rq = await c.getUrl(Uri.parse('${l.relay.replaceAll(RegExp(r'/+$'), '')}/online/${Uri.encodeComponent(l.id.replaceAll(RegExp(r'[\s-]'), ''))}'));
      final rs = await rq.close().timeout(const Duration(seconds: 8));
      final j = jsonDecode(await rs.transform(utf8.decoder).join()) as Map<String, dynamic>;
      c.close();
      if (((j['daemons'] ?? 0) as int) <= 0) return 'offline';
      if (j['control'] is! int) return 'old_relay';
      if ((j['control'] as int) <= 0) return 'old_daemon';
      return '';
    } catch (_) {
      return 'unreachable';
    }
  }

  // Ask first, then dial — or wait and ask again. A relay or computer that cannot serve a control
  // link is asked again only now and then: an update is not something to poll for.
  Future<void> _dial(_Link l) async {
    if (l.dead) return;
    l.retry?.cancel();
    final why = await _ask(l);
    if (l.dead) return;
    if (why.isEmpty) return _open(l);
    _say(l, (s) => s.copy(state: why));
    l.retry = Timer(Duration(seconds: why.startsWith('old_') ? 120 : (why == 'offline' ? 15 : 20)), () => _dial(l));
  }

  void _open(_Link l) {
    if (l.dead) return;
    if (of(l.key).state != 'up') _say(l, (s) => s.copy(state: 'connecting'));
    l.room = CoreHost.I.open(l.key, 'control', RoomSink(onState: (s) => _state(l, s), onMsg: (m) => _msg(l, m), onRelay: (m) => _relay(l, m)));
  }

  void _state(_Link l, Map<String, dynamic> s) {
    final st = s['state'] as String;
    if (st == 'open') {
      if (l.up) return;
      l.up = true;
      l.backoff = 0;
      _say(l, (x) => x.copy(state: 'up'));
      _askAll(l);
      l.poll?.cancel();
      var tick = 0;
      l.poll = Timer.periodic(const Duration(seconds: 3), (_) {
        send(l.key, {'type': 'host_get'});
        if (l.wants.contains('clips') && ++tick % 4 == 0) send(l.key, {'type': 'clip_list'});
      });
      return;
    }
    if (l.up) {
      l.up = false;
      l.poll?.cancel();
    }
    final mapped = switch (st) { 'refused' => 'refused', 'unreachable' => 'unreachable', 'dialing' || 'securing' || 'reconnecting' => 'connecting', _ => null };
    if (mapped != null) _say(l, (x) => x.copy(state: mapped));
  }

  void _relay(_Link l, Map<String, dynamic> m) {
    switch (m['type']) {
      case 'waiting':
        _say(l, (x) => x.copy(state: 'offline'));
      case 'join_denied':
        _say(l, (x) => x.copy(state: RegExp('older than live widgets', caseSensitive: false).hasMatch('${m['reason'] ?? ''}') ? 'old_daemon' : 'offline'));
      case 'join_accepted':
        // Belt and braces: a relay that answered with a seat did not make a control link.
        if (m['assignment'] != null && m['assignment'] != 'control') {
          rlog('light link: ${l.id} got a seat — leaving');
          _say(l, (x) => x.copy(state: 'old_relay'));
          drop(l.key);
        }
    }
  }

  void _askAll(_Link l) {
    send(l.key, {'type': 'host_get'});
    send(l.key, {'type': 'config_get'});
    if (l.wants.contains('clips')) send(l.key, {'type': 'clip_list'});
    if (l.wants.contains('actions')) send(l.key, {'type': 'pad_in', 'msg': {'k': 'actions'}});
  }

  void _msg(_Link l, Map<String, dynamic> m) {
    switch (m['type']) {
      case 'host_state':
        final st = m['state'] as Map<String, dynamic>?;
        for (final p in (st?['media'] as List? ?? []).cast<Map<String, dynamic>>()) {
          final had = l.pics['${p['bus']}'] ?? {};
          if (p['art_same'] == true) p['art'] = had['art'];
          if (p['icon_same'] == true) p['icon'] = had['icon'];
          l.pics['${p['bus']}'] = {'art': p['art'], 'icon': p['icon']};
        }
        if (st != null) _say(l, (x) => x.copy(host: st));
      case 'config_state':
        final c = m['state'] as Map<String, dynamic>?;
        if (c != null) _say(l, (x) => x.copy(config: c));
      case 'clip_history':
        _say(l, (x) => x.copy(clips: [for (final e in (m['entries'] as List? ?? [])) Map<String, dynamic>.from(e as Map)]));
      case 'clip_data':
        final id = '${m['id']}';
        final c = (mime: '${m['mime']}', data: '${m['data']}');
        l.full[id] = c;
        for (final f in l.wait.remove(id) ?? const []) {
          f(c);
        }
      case 'clip_held':
        final id = m['id'] as String?;
        if (id != null) {
          l.held[id] = '${m['message'] ?? ''}';
          for (final f in l.wait.remove(id) ?? const []) {
            f(null);
          }
        }
        clipEvents.add((l.key, m));
      case 'clip_now' || 'clip_pushed' || 'clip_error':
        clipEvents.add((l.key, m));
      case 'pad_out':
        final msg = m['msg'] as Map?;
        if (msg?['k'] == 'action_list') _say(l, (x) => x.copy(actions: [for (final a in (msg!['actions'] as List? ?? [])) Map<String, dynamic>.from(a as Map)]));
    }
  }

  /// Entry `id` of that computer's clipboard history in full.
  Future<({String mime, String data})> clipFetch(String key, String id) {
    final l = _links[key];
    if (l == null) return Future.error('no link');
    final have = l.full[id];
    if (have != null) return Future.value(have);
    if (l.held.containsKey(id)) return Future.error(l.held[id]!);
    final done = Completer<({String mime, String data})>();
    final first = !l.wait.containsKey(id);
    (l.wait[id] ??= []).add((c) {
      if (done.isCompleted) return;
      c == null ? done.completeError(l.held[id] ?? '') : done.complete(c);
    });
    if (first) send(key, {'type': 'clip_get', 'id': id});
    Timer(const Duration(seconds: 8), () {
      if (!done.isCompleted) done.completeError('no answer');
    });
    return done.future;
  }
}
