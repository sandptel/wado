// The session (port of js/relay.js + actions.rs + cfg.rs): ask the selected computer for a
// desktop, rejoin or drop one that is running, and negotiate the WebRTC picture over the relay
// link. Entering the session view is only ever a person's pick (session_intent.js); a late
// `session_started` with nobody waiting is detached, never shown.
import 'dart:async';
import 'dart:convert';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/painting.dart' show EdgeInsets;
import 'package:flutter/widgets.dart' show AppLifecycleListener;
import 'package:flutter_webrtc/flutter_webrtc.dart';

import '../ui/theme/look.dart';
import 'apps.dart';
import 'console_layout.dart' show pictureRoom;
import 'latency.dart';
import 'log.dart';
import 'relay_link.dart';
import 'multi.dart';
import 'presence.dart';
import 'settings.dart';
import 'stats.dart';

const inputChannel = 'wado-input';
const motionChannel = 'wado-motion';
const fastChannel = 'wado-fast';

class Session {
  Session._() {
    final l = RelayLink.I;
    // The cheapest large win in the stack (lifecycle.js): a hidden app still receives RTP and
    // throws it away, so tell the daemon to stop rendering for it until it is back.
    AppLifecycleListener(onHide: () => _visible(false), onShow: () => _visible(true));
    l.on('__up', (_) => _onLinkUp());
    l.on('__down', (_) => _onDown());
    l.on('session_started', _started);
    Stats.I.sample.addListener(_sampled);
    // Auto bitrate moved the cap: rebuild only the encoder (autorate.js → actions::apply).
    Stats.I.onCap = (_) => reconfigure();
    l.on('session_alive', _alive);
    l.on('session_reconfigured', _reconfigured);
    l.on('session_error', _error);
    l.on('session_stopped', (_) => _stopped());
    l.on('sdp_answer', (m) {
      final a = _answer;
      _answer = null;
      a?.complete(m['sdp'] as String);
    });
    l.on('ice_candidate', (m) async {
      final c = m['candidate'];
      if (pc == null || c is! String) return;
      try {
        final j = jsonDecode(c) as Map<String, dynamic>;
        await pc!.addCandidate(RTCIceCandidate(j['candidate'] as String?, j['sdpMid'] as String?, j['sdpMLineIndex'] as int?));
      } catch (_) {}
    });
    l.on('apps_list', (m) => apps.value = (m['apps'] as List? ?? []).cast<Map<String, dynamic>>());
    l.on('log', (m) {
      if (m['line'] is String) rlog(m['line'] as String);
    });
    l.on('error', (m) => setStatus('relay error: ${m['message'] ?? '?'}'));
    l.on('text_input', (m) => textWanted.value = m['active'] == true);
    l.on('windows', (m) {
      windows.value = (m['windows'] as List? ?? []).cast<Map<String, dynamic>>();
      workspace.value = (m['workspace'] ?? 1) as int;
    });
  }

  final windows = ValueNotifier<List<Map<String, dynamic>>>([]);
  final workspace = ValueNotifier<int>(1);
  /// fps, round trip (ms), decoder, from the picture's stats (stats.js).
  final stats = ValueNotifier<({double fps, double rtt, String decoder, double kbps})?>(null);
  /// The control centre's sparklines: the last 30 readings of each.
  final spark = (fps: <double>[], ping: <double>[], kbps: <double>[]);

  /// Launch an app or command in the session (`session_launch`).
  void launch(String command) {
    final c = command.trim();
    if (c.isEmpty) return;
    RelayLink.I.send({'type': 'session_launch', 'command': c});
    final r = Settings.I.get<List<String>>('recent').where((x) => x != c).toList()..insert(0, c);
    Settings.I.set('recent', r.take(12).toList());
  }

  /// Act on the session's windows (`session_window`): close, maximize, focus {id}, workspace {n}…
  void window(Object action) => RelayLink.I.send({'type': 'session_window', 'action': action});
  static final I = Session._();

  /// A session is running and this device is watching it.
  final on = ValueNotifier<bool>(false);
  /// The session view is up (only ever by a person's pick).
  final shown = ValueNotifier<bool>(false);
  /// 0 none · 1 relay · 2 joined · 3 session ready · 4 picture path up (`phase`).
  final stage = ValueNotifier<int>(0);
  final stageError = ValueNotifier<String>('');
  final stagebar = ValueNotifier<String>('No session.');
  final encoder = ValueNotifier<(String, String)>(('', ''));
  /// A session already runs: rejoin it or drop it (`sessionAlive`).
  final alive = ValueNotifier<(String, String)?>(null);
  final apps = ValueNotifier<List<Map<String, dynamic>>>([]);
  final textWanted = ValueNotifier<bool>(false);
  final startFailed = ValueNotifier<int>(0);
  final renderer = RTCVideoRenderer();
  final hasVideo = ValueNotifier<bool>(false);
  bool _rendererReady = false;

  RTCPeerConnection? pc;
  RTCDataChannel? inputDC, motionDC, fastDC;
  Completer<String>? _answer;
  bool _wanted = false, _resuming = false, _dropPending = false, _left = false;
  Map<String, dynamic>? _config;
  Timer? _wait;

  void _phase(int s, [String err = '']) {
    stage.value = s;
    stageError.value = err;
  }

  void _armWait([String kind = 'start']) {
    _wait?.cancel();
    _wait = Timer(const Duration(seconds: 20), () {
      _wait = null;
      if (kind == 'change') {
        rlog('the daemon did not answer the change request within 20 s');
        setStatus('relay: no answer to that change — the session is still running');
        return;
      }
      _phase(2, 'the daemon is online but the session never started');
      setStatus('relay: the daemon did not answer the session request');
      startFailed.value++;
    });
  }

  void _clearWait() {
    _wait?.cancel();
    _wait = null;
  }

  /// A phone (shortest side under 600 logical px), as viewport.js `isPhone`.
  static bool get phone {
    final view = ui.PlatformDispatcher.instance.views.first;
    return view.display.size.shortestSide / view.devicePixelRatio < 600;
  }

  /// viewport.js `reportScreen`: the panel in real pixels, the way round the session will be.
  /// A phone at rest is held upright, so a session started without thinking would give desktop
  /// apps a 360 px-wide screen: "auto" is landscape on a phone, native everywhere else. The
  /// output's size is fixed at Start (invariant #8), so this is the one moment the choice exists.
  static (int, int) screen() {
    final view = ui.PlatformDispatcher.instance.views.first;
    final d = view.display.size;
    var (w, h) = (d.width.round(), d.height.round());
    final pref = Settings.I.s('orientation');
    final land = pref == 'landscape' ? true : (pref == 'portrait' ? false : (phone ? true : w >= h));
    if ((land && h > w) || (!land && w > h)) (w, h) = (h, w);
    return (w, h);
  }

  /// The config the web client sends (cfg.rs `build`).
  static Map<String, dynamic> config() {
    final s = Settings.I;
    final view = ui.PlatformDispatcher.instance.views.first;
    var res = s.s('res');
    if (res.isEmpty) res = fitRes();
    int w, h;
    if (res == 'custom') {
      w = s.i('custom_w');
      h = s.i('custom_h');
    } else {
      final p = res.split('x');
      w = int.tryParse(p.first.trim()) ?? 1280;
      h = int.tryParse(p.length > 1 ? p[1].trim() : '') ?? 720;
    }
    final q = s.s('quality');
    final quality = switch (q) {
      'reactivity' => 'reactivity',
      'quality' => 'quality',
      'custom' => {'custom': {'bitrate_kbps': s.i('bitrate')}},
      _ => 'balanced',
    };
    final placement = switch (s.s('placement')) {
      'top_left' || 'cascade' || 'maximized' || 'tile' || 'strip' || 'center' => s.s('placement'),
      _ => phone ? 'strip' : 'center',
    };
    final backend = switch (s.s('encoder_backend')) { 'hardware' => 'hardware', 'software' => 'software', _ => 'auto' };
    var scale = double.tryParse(s.s('scale').trim()) ?? 0;
    if (!(scale > 0)) scale = defaultScale(view.devicePixelRatio);
    final kf = int.tryParse(s.s('keyframe').trim()) ?? (s.s('keyframe_mode') == 'on_request' ? (s.i('fps').clamp(1, 1000) * 10) : null);
    final t = Look.I.tokens.value;
    return {
      'width': w,
      'height': h,
      'fps': s.i('fps'),
      'scale': scale,
      'quality': quality,
      if (s.s('preset').isNotEmpty) 'preset': s.s('preset'),
      'keyframe_interval': ?kf,
      'input': {'repeat_rate': s.i('repeat_rate'), 'repeat_delay': s.i('repeat_delay'), 'focus_follows_pointer': s.b('focus_follows')},
      'window': {'placement': placement},
      'encoder': {'backend': backend},
      'isolate_apps': s.b('isolate_apps'),
      'x_server': s.b('x_server'),
      'dark': !t.light,
      'background': _desktop(t.base, s.s('accent')),
      'low_latency_audio': s.b('low_latency_audio'),
      if (s.b('auto_bitrate') && Stats.I.rate.cap != null) 'bitrate_cap_kbps': Stats.I.rate.cap,
    };
  }

  static String _desktop(List<ui.Color> base, String accent) {
    final i = (int.tryParse(accent, radix: 16) ?? 13).clamp(0, 15);
    final g = base[1], a = base[i];
    String ch(double x, double y) => ((x * 255) * .78 + (y * 255) * .22).round().toRadixString(16).padLeft(2, '0');
    return ch(g.r, a.r) + ch(g.g, a.g) + ch(g.b, a.b);
  }

  /// res/device.rs `options`: the device's own aspect at each target that fits the screen.
  static List<String> resOptions(int sw, int sh) {
    (int, int) scaled(int t) {
      if (sw == 0 || sh == 0) return (t, t);
      final (w, h) = sw <= sh ? (t, (sh * t / sw).round()) : ((sw * t / sh).round(), t);
      return (w.clamp(16, 1 << 30) & ~1, h.clamp(16, 1 << 30) & ~1);
    }

    final out = <String>[];
    for (final t in const [1440, 1080, 900, 720, 540]) {
      final (w, h) = scaled(t);
      if (w > sw.clamp(16, 1 << 30) || h > sh.clamp(16, 1 << 30)) continue;
      if (!out.contains('${w}x$h')) out.add('${w}x$h');
    }
    if (out.isEmpty) out.add('${sw.clamp(16, 1 << 30) & ~1}x${sh.clamp(16, 1 << 30) & ~1}');
    return out;
  }

  /// The default shape: the console's picture room, the way round the session will be, so the
  /// picture fills it edge to edge (full screen letterboxes a little instead). 720 on the short side.
  static String fitRes() {
    final view = ui.PlatformDispatcher.instance.views.first;
    final (sw, sh) = screen();
    final k = view.devicePixelRatio;
    final p = view.viewPadding;
    var inset = EdgeInsets.fromLTRB(p.left / k, p.top / k, p.right / k, p.bottom / k);
    // Held the other way round from the session: the bars turn with the phone.
    // ponytail: a quarter turn of today's insets (the status bar's side goes left), not the OS's own.
    final held = view.physicalSize;
    if ((held.width >= held.height) != (sw >= sh)) inset = EdgeInsets.fromLTRB(inset.top, inset.left, inset.bottom, inset.right);
    final room = pictureRoom(sw / k, sh / k, inset);
    if (room.width < 1 || room.height < 1) return defaultRes(sw, sh);
    final a = room.width / room.height;
    final (w, h) = a >= 1 ? ((720 * a).round() & ~1, 720) : (720, (720 / a).round() & ~1);
    return '${w}x$h';
  }

  /// res/device.rs `default_value`: the 720p option, else the first that fits.
  static String defaultRes(int sw, int sh) {
    if (sw <= 0 || sh <= 0) return '1280x720';
    final opts = resOptions(sw, sh);
    final (w, h) = sw <= sh ? (720, (sh * 720 / sw).round() & ~1) : ((sw * 720 / sh).round() & ~1, 720);
    return opts.contains('${w}x$h') ? '${w}x$h' : opts.first;
  }

  /// res/scale.rs `default_scale`.
  static double defaultScale(double dpr) {
    const scales = [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];
    var best = scales.first;
    for (final s in scales) {
      if ((s - dpr).abs() < (best - dpr).abs()) best = s;
    }
    return best;
  }

  /// Start (or rejoin) a desktop on the selected computer — a person's tap.
  void start() {
    _config = config();
    _wanted = true;
    _left = false;
    shown.value = true;
    _phase(0);
    setStatus('relay: connecting…');
    final l = RelayLink.I;
    if (l.seatBusy.value != null || l.takenOver) {
      l.takeover();
      return;
    }
    if (l.up.value) _onLinkUp();
  }

  bool _ask() {
    if (_config == null) {
      rlog('cannot start a session: this page has no settings yet');
      startFailed.value++;
      return false;
    }
    _armWait();
    final verb = _resuming ? {'type': 'session_rejoin'} : {'type': 'session_start', 'config': _config};
    if (!RelayLink.I.send(verb)) {
      _clearWait();
      setStatus('relay: link is down — the request will go when it reconnects');
      return false;
    }
    return true;
  }

  void _onLinkUp() {
    // A joined room is multi.dart's: it says join / take over / end itself.
    if (RelayLink.I.view != null) return;
    if (on.value && (!_wanted || _config == null)) {
      _resuming = true;
      rlog('link back — taking back the session this page joined');
      RelayLink.I.send({'type': 'session_rejoin'});
      return;
    }
    if (!_wanted || _config == null) return;
    if (apps.value.isEmpty) Apps.I.request();
    if (on.value) {
      _resuming = true;
      rlog('link back — checking whether the session survived');
    } else {
      setStatus('relay: joined — starting session…');
    }
    _ask();
  }

  void _onDown() {
    _dropPending = false;
    _resuming = false;
    _clearWait();
    alive.value = null;
    if (on.value) setStatus('relay: link lost — holding the session, reconnecting…');
  }

  Future<void> _started(Map<String, dynamic> msg) async {
    _clearWait();
    _resuming = false;
    // Nobody here asked for it (a late answer after Leave): detach, never show (session_intent).
    if (!_wanted && !on.value) {
      rlog('a session started with nobody waiting here — detaching');
      RelayLink.I.send({'type': 'session_detach'});
      return;
    }
    final enc = (msg['info'] as Map?)?['encoder'] as Map?;
    if (enc != null) encoder.value = ((enc['mode'] ?? '') as String, (enc['pipeline'] ?? '') as String);
    Stats.I.rate.reset();
    _yardsticks(enc);
    on.value = true;
    _phase(3);
    rlog('session ready — encoder ${enc?['mode'] ?? '?'}');
    stagebar.value = 'Session running — negotiating WebRTC…';
    try {
      if (pc != null && pc!.connectionState == RTCPeerConnectionState.RTCPeerConnectionStateConnected) {
        stagebar.value = 'Streaming (relay).';
      } else {
        await negotiate();
      }
    } catch (e) {
      rlog('negotiation failed: $e');
      setStatus('relay: $e');
    }
  }

  void _alive(Map<String, dynamic> msg) {
    _clearWait();
    _phase(2);
    if (_resuming) {
      _resuming = false;
      rlog('the session survived the outage — rejoining');
      _armWait('change');
      RelayLink.I.send({'type': 'session_rejoin'});
      return;
    }
    final enc = (msg['info'] as Map?)?['encoder'] as Map?;
    rlog('a session is already running — waiting for rejoin or drop');
    alive.value = ((enc?['mode'] ?? '') as String, (enc?['pipeline'] ?? '') as String);
  }

  void _reconfigured(Map<String, dynamic> msg) {
    _clearWait();
    final enc = (msg['info'] as Map?)?['encoder'] as Map?;
    if (enc != null) encoder.value = ((enc['mode'] ?? '') as String, (enc['pipeline'] ?? '') as String);
    rlog('session reconfigured — ${enc?['bitrate_kbps'] ?? '?'} kbps');
    _yardsticks(enc);
    setStatus('applied');
  }

  void _error(Map<String, dynamic> msg) {
    _clearWait();
    final why = (msg['message'] ?? 'unknown') as String;
    if (RegExp('could not understand that message', caseSensitive: false).hasMatch(why)) {
      rlog('the daemon did not understand one of our messages (it is older than this client): $why');
      // A speed test or a "new session" question it cannot answer: go on without one.
      Multi.I.abortTest();
      return;
    }
    if (_resuming) {
      _resuming = false;
      on.value = false;
      if (!_wanted || _config == null) {
        rlog('nothing left to rejoin: $why');
        return;
      }
      rlog('the session did not survive: $why — starting a fresh one');
      _ask();
      return;
    }
    // A join, take over or end refused on a joined room says so and goes home.
    if (Multi.I.error(why)) return;
    _phase(2, why);
    setStatus('relay: session error — $why');
    startFailed.value++;
  }

  void _stopped() {
    if (_dropPending) {
      _dropPending = false;
      rlog('previous session dropped — starting a new one');
      _ask();
      return;
    }
    if (on.value) {
      on.value = false;
      stagebar.value = 'Session stopped.';
      _closePc();
      shown.value = false;
    }
  }

  /// Wanted and shown, waiting for an answer someone else asks for (a join, a resume elsewhere).
  void prepare() {
    _config = config();
    _wanted = true;
    _left = false;
    shown.value = true;
    _phase(0);
  }

  /// Rejoin the session on this link's daemon (sessions.js `act("resume")`).
  void resume() {
    prepare();
    _resuming = true;
    _armWait('change');
    RelayLink.I.send({'type': 'session_rejoin'});
  }

  /// Stop watching without telling the computer anything (multi.js `stopStream`).
  void dropStream() {
    _clearWait();
    _wanted = false;
    _resuming = false;
    _closePc();
    on.value = false;
    shown.value = false;
  }

  /// A running session was found: take it back.
  bool rejoin() {
    if (alive.value == null) return false;
    if (!RelayLink.I.send({'type': 'session_rejoin'})) return false;
    alive.value = null;
    _armWait('change');
    return true;
  }

  /// A running session was found: stop it and start a fresh one.
  bool dropAndStart() {
    if (alive.value == null || _config == null) return false;
    rlog('sending session_stop (drop-and-restart chosen)');
    if (!RelayLink.I.send({'type': 'session_stop'})) return false;
    alive.value = null;
    _dropPending = true;
    return true;
  }

  /// Apply changed settings to the running session.
  bool reconfigure() {
    if (!on.value) return false;
    _config = config();
    _armWait('change');
    return RelayLink.I.send({'type': 'session_reconfigure', 'config': _config});
  }

  /// Leave the session running on the computer; back to its page.
  void leave() {
    _wanted = false;
    _left = true;
    _resuming = false;
    shown.value = false;
    RelayLink.I.send({'type': 'session_detach'});
    _closePc();
    on.value = false;
    setStatus('left the session running');
  }

  /// End the session on the computer.
  void stop() {
    _clearWait();
    _wanted = false;
    _resuming = false;
    _dropPending = false;
    rlog('sending session_stop (Stop pressed)');
    RelayLink.I.send({'type': 'session_stop'});
    _closePc();
    on.value = false;
    shown.value = false;
    stagebar.value = 'No session.';
  }

  bool get left => _left;

  void _visible(bool v) {
    if (!on.value || pc == null) return;
    rlog('app is now ${v ? 'visible' : 'hidden'}');
    RelayLink.I.send({'type': 'viewer_visible', 'visible': v});
  }

  void _sampled() {
    final x = Stats.I.sample.value;
    if (x == null) return;
    for (final (l, v) in [(spark.fps, x.fps), (spark.ping, x.ping), (spark.kbps, x.kbps)]) {
      l.add(v ?? 0);
      if (l.length > 30) l.removeAt(0);
    }
    stats.value = (fps: x.fps ?? 0, rtt: x.ping ?? 0, decoder: '', kbps: x.kbps ?? 0);
  }

  void _yardsticks(Map? enc) {
    if (enc != null) Stats.I.target(((enc['bitrate_kbps'] ?? 0) as num).toInt(), ((enc['fps'] ?? 0) as num).toInt());
  }

  MediaStreamTrack? _audio;

  /// This device's own mute and volume on the computer's sound (`live::apply`).
  void applyAudio() {
    final a = _audio;
    if (a == null) return;
    a.enabled = !Settings.I.b('muted');
    Helper.setVolume(Settings.I.d('volume'), a);
  }

  /// The running session's fractional scale (input_units.js `outputScale`): a divisor, so 1 at least.
  double get outputScale => ((_config?['scale'] as num?)?.toDouble() ?? 1).clamp(.1, 10);

  /// The config last sent to the running session (Apply compares against it).
  Map<String, dynamic>? get applied => _config;

  void _closePc() {
    final p = pc;
    pc = null;
    inputDC = motionDC = fastDC = null;
    hasVideo.value = false;
    Stats.I.stop();
    Latency.I.stop();
    Presence.I.clear();
    if (_rendererReady) renderer.srcObject = null;
    p?.close();
  }

  /// "Play on this phone" with no session running (host.js `listenStart`): a peer connection
  /// carrying the computer's sound alone. With a session, its own connection carries the sound.
  bool listening = false;
  Future<void> listen() async {
    if (on.value || !RelayLink.I.up.value || pc?.connectionState == RTCPeerConnectionState.RTCPeerConnectionStateConnected) return;
    try {
      await negotiate(audioOnly: true);
    } catch (e) {
      rlog('listen failed: $e');
    }
  }

  void listenStop() {
    if (listening) _closePc();
    listening = false;
  }

  Future<void> negotiate({bool audioOnly = false}) async {
    listening = audioOnly;
    if (!_rendererReady) {
      await renderer.initialize();
      _rendererReady = true;
    }
    _closePc();
    final p = await createPeerConnection({
      'iceServers': [
        {'urls': 'stun:stun.l.google.com:19302'},
        {'urls': 'stun:stun1.l.google.com:19302'},
      ],
      'sdpSemantics': 'unified-plan',
      'candidateNetworkPolicy': 'all',
      'continualGatheringPolicy': 'gather_continually',
    });
    pc = p;
    if (!audioOnly) await p.addTransceiver(kind: RTCRtpMediaType.RTCRtpMediaTypeVideo, init: RTCRtpTransceiverInit(direction: TransceiverDirection.RecvOnly));
    if (audioOnly || Settings.I.b('stream_sound')) {
      await p.addTransceiver(kind: RTCRtpMediaType.RTCRtpMediaTypeAudio, init: RTCRtpTransceiverInit(direction: TransceiverDirection.RecvOnly));
    }
    inputDC = await p.createDataChannel(inputChannel, RTCDataChannelInit()..ordered = true);
    Latency.I.attach(inputDC!);
    motionDC = await p.createDataChannel(motionChannel, RTCDataChannelInit()
      ..ordered = true
      ..maxRetransmits = 0);
    fastDC = await p.createDataChannel(fastChannel, RTCDataChannelInit()
      ..ordered = false
      ..maxRetransmits = 0);
    // The other devices' pointers (presence.js): unordered, never retransmitted — a late
    // position is a wrong one.
    final ghosts = await p.createDataChannel('wado-presence', RTCDataChannelInit()
      ..ordered = false
      ..maxRetransmits = 0);
    ghosts.onMessage = (m) => Presence.I.message(m.text);
    p.onTrack = (ev) {
      if (ev.track.kind == 'audio') {
        _audio = ev.track;
        applyAudio();
        return;
      }
      rlog('track received — waiting for a network path');
      Stats.I.start(p);
      if (ev.streams.isNotEmpty) renderer.srcObject = ev.streams.first;
      hasVideo.value = true;
      stagebar.value = 'Streaming (relay).';
      RelayLink.I.send({'type': 'viewer_visible', 'visible': true});
    };
    p.onIceConnectionState = (s) {
      rlog('ICE state: ${s.name}');
      if (s == RTCIceConnectionState.RTCIceConnectionStateConnected || s == RTCIceConnectionState.RTCIceConnectionStateCompleted) {
        _phase(4);
      }
      if (s == RTCIceConnectionState.RTCIceConnectionStateFailed) {
        _phase(3, 'no network path to the daemon — ICE failed. Both ends are likely behind a NAT that STUN cannot traverse (a VPN on either end does this); this needs a TURN server.');
      }
    };
    p.onConnectionState = (s) => rlog('peer state: ${s.name}');
    // Gather (F4's rule): send once a reflexive candidate is in, or 700 ms after the first host
    // candidate, or 3 s at most; trickle the rest.
    final gathered = Completer<String>();
    var reflexive = false;
    Timer? settle, hostOnly;
    final cap = Timer(const Duration(seconds: 3), () {
      if (!gathered.isCompleted) gathered.complete('cap');
    });
    var sentOffer = false;
    p.onIceCandidate = (c) {
      final cand = c.candidate;
      if (cand == null || cand.isEmpty) {
        if (!gathered.isCompleted) gathered.complete('complete');
        return;
      }
      if (sentOffer) {
        RelayLink.I.send({'type': 'ice_candidate', 'candidate': jsonEncode({'candidate': cand, 'sdpMid': c.sdpMid, 'sdpMLineIndex': c.sdpMLineIndex})});
        return;
      }
      final typ = RegExp(r' typ (\w+)').firstMatch(cand)?.group(1);
      if (typ == 'srflx' || typ == 'relay' || typ == 'prflx') reflexive = true;
      if (reflexive) {
        settle?.cancel();
        settle = Timer(const Duration(milliseconds: 60), () {
          if (!gathered.isCompleted) gathered.complete('reflexive');
        });
      } else {
        hostOnly ??= Timer(const Duration(milliseconds: 700), () {
          if (!gathered.isCompleted) gathered.complete('host only');
        });
      }
    };
    p.onIceGatheringState = (s) {
      if (s == RTCIceGatheringState.RTCIceGatheringStateComplete && !gathered.isCompleted) gathered.complete('complete');
    };
    final offer = await p.createOffer();
    await p.setLocalDescription(offer);
    final why = await gathered.future;
    cap.cancel();
    settle?.cancel();
    hostOnly?.cancel();
    final local = await p.getLocalDescription();
    sentOffer = true;
    final sdp = local?.sdp ?? offer.sdp ?? '';
    rlog('sending offer — ${RegExp('a=candidate:').allMatches(sdp).length} candidates ($why)');
    _answer = Completer<String>();
    if (!RelayLink.I.send({'type': 'sdp_offer', 'sdp': jsonEncode({'type': 'offer', 'sdp': sdp})})) {
      throw Exception('the relay link went away before the offer could be sent');
    }
    final ans = await _answer!.future.timeout(const Duration(seconds: 30), onTimeout: () {
      _answer = null;
      throw Exception('the daemon did not answer the offer within 30 s');
    });
    final a = jsonDecode(ans) as Map<String, dynamic>;
    await p.setRemoteDescription(RTCSessionDescription(a['sdp'] as String, (a['type'] ?? 'answer') as String));
    rlog('remote description set — ICE checking starts now');
    setStatus('connected via relay');
  }

  /// Send an input message on the reliable input channel (input_core.js `send`).
  void sendInput(Map<String, dynamic> m) {
    final dc = inputDC;
    if (dc != null && dc.state == RTCDataChannelState.RTCDataChannelOpen) {
      dc.send(RTCDataChannelMessage(jsonEncode(m)));
    } else {
      RelayLink.I.send({'type': 'input', 'event': m});
    }
  }
}
