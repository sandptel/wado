// The quick-tile grid and every tile there is (port of ui/cc/tiles/mod.rs + pool.rs): what each
// shows, what a tap does, which page a long-press opens. A table rather than a widget per tile: a
// tile is four facts and an action.
import 'package:flutter/material.dart';
import '../../../wado/log.dart';
import 'package:share_plus/share_plus.dart';
import 'package:path_provider/path_provider.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

import '../../../wado/clip.dart';
import '../../../wado/host.dart';
import '../../../wado/session.dart';
import '../../../wado/settings.dart';
import '../../computer/quick.dart' show speaker;
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';
import '../../files/files_page.dart' show keepOnPhone;
import '../keyboard.dart';

class TileDef {
  const TileDef(this.id, this.icon, this.label, this.sub, this.act, {this.on, this.page, this.needsSession = false});
  final String id, icon, label;
  /// The subtitle when off and when on.
  final (String, String) sub;
  final bool Function()? on;
  final void Function(BuildContext) act;
  /// The page a long-press opens.
  final String? page;
  /// Does nothing without a running session.
  final bool needsSession;
}

Map<String, dynamic>? get _h => Host.I.state.value;
Map<String, dynamic>? get _audio => _h?['audio'] as Map<String, dynamic>?;
bool _onPhone() => _audio?['phone_sink'] != null && _audio?['phone_sink'] == _audio?['default_sink'];

void _flip(String k) => Settings.I.set(k, !Settings.I.b(k));

/// The default eight (pool.rs `DEFAULTS`, with Keep awake for Take mouse: Android has no
/// pointer lock to offer).
const tileDefaults = ['kbd', 'sound', 'playhere', 'pcmute', 'wifi', 'bluetooth', 'pcawake', 'fullscreen'];

/// `open` pushes a page in the control centre.
List<TileDef> tilePool(void Function(String page) open) => [
      TileDef('kbd', 'kbd', 'Keyboard', ('Off', 'On screen'), (c) => KeyboardSheet.toggle(c), on: () => KeyboardSheet.open.value, page: 'input_feel', needsSession: true),
      TileDef('sound', 'sound', 'Sound here', ('Muted', 'Playing'), (_) {
        _flip('muted');
        Session.I.applyAudio();
      }, on: () => !Settings.I.b('muted')),
      TileDef('pcmute', 'monitor', 'Computer sound', ('Muted', 'On'), (_) {
        final s = _h == null ? null : speaker(_h!);
        if (s != null) Host.I.act({'do': 'sink_mute', 'id': s['id'], 'muted': s['muted'] != true});
      }, on: () => (_h == null ? null : speaker(_h!))?['muted'] == false, page: 'sound_devices'),
      TileDef('playhere', 'phone', 'Play here', ('Computer speakers', 'Sound comes here'), (_) {
        if (_h == null) return;
        if (_onPhone()) {
          final s = speaker(_h!);
          if (s != null) Host.I.act({'do': 'all_to', 'sink': s['name']});
          Session.I.listenStop();
        } else if (_audio?['phone_sink'] != null) {
          Host.I.act({'do': 'all_to', 'sink': _audio!['phone_sink']});
          Session.I.listen();
        }
      }, on: _onPhone, page: 'sound_devices'),
      // Never a one-tap toggle: it can cut this very connection. The page asks first.
      TileDef('wifi', 'wifi', 'Wi-Fi', ('Off', 'On'), (_) => open('connection_trust'), on: () => (_h?['wifi'] as Map?)?['enabled'] == true, page: 'connection_trust'),
      TileDef('bluetooth', 'bt', 'Bluetooth', ('Off', 'On'), (_) {
        final b = _h?['bluetooth'] as Map?;
        if (b != null) Host.I.act({'do': 'bt_power', 'on': b['powered'] != true});
      }, on: () => (_h?['bluetooth'] as Map?)?['powered'] == true, page: 'sound_devices'),
      TileDef('pcawake', 'moon', 'Computer awake', ('May sleep', 'Kept awake'), (_) => Host.I.act({'do': 'keep_awake', 'on': _h?['awake'] != true}), on: () => _h?['awake'] == true),
      TileDef('fullscreen', 'max', 'Full screen', ('Fill this screen', 'The picture fills it'), (_) => _flip('cockpit'), on: () => Settings.I.b('cockpit')),
      TileDef('move', 'move', 'Move windows', ('Drag = input', 'Drag = move'), (_) => _flip('move_mode'), on: () => Settings.I.b('move_mode'), page: 'input_feel'),
      TileDef('touch', 'hand', 'Raw touch', ('Gestures', 'Multi-touch'), (_) => Settings.I.set('touch_mode', Settings.I.s('touch_mode') == 'touch' ? 'pointer' : 'touch'),
          on: () => Settings.I.s('touch_mode') == 'touch', page: 'input_feel'),
      TileDef('fpslock', 'lock', 'Lock FPS', ('Adaptive', 'Locked'), (_) => _flip('fps_lock'), on: () => Settings.I.b('fps_lock'), page: 'display_session'),
      TileDef('natscroll', 'scroll', 'Natural scroll', ('Off', 'On'), (_) => _flip('natural_scroll'), on: () => Settings.I.b('natural_scroll'), page: 'input_feel'),
      TileDef('stats', 'eye', 'Readouts', ('Hidden', 'Over the picture'), (_) => _flip('debug_master'), on: () => Settings.I.b('debug_master'), page: 'diagnostics'),
      TileDef('awake', 'sun', 'Keep awake', ('Screen may sleep', 'Screen stays on'), (_) => _flip('keep_awake'), on: () => Settings.I.b('keep_awake')),
      TileDef('clipboard', 'clip', 'Paste here', ('Phone clipboard → session', ''), (_) => Clip.I.sendPhone(), needsSession: true),
      TileDef('dnd', 'bell', 'Do not disturb', ('Notifications pop up', 'Shade only'), (_) => _flip('dnd'), on: () => Settings.I.b('dnd')),
      TileDef('screenshot', 'camera', 'Screenshot', ('Save the picture', ''), (_) => screenshot(), needsSession: true),
      TileDef('record', 'record', 'Record', ('Save a video', 'Recording · tap to stop'), (_) => Recorder.I.toggle(), on: () => Recorder.I.on.value, needsSession: true),
      TileDef('resync', 'refresh', 'Resync', ('New connection, same apps', ''), (_) => Session.I.negotiate(), needsSession: true),
      // ponytail: Take mouse stays web-only — Flutter offers no pointer capture on Android.
    ];

/// The tiles this device shows, in order: the saved list minus ids no longer in the pool.
List<String> shownTiles() {
  final ids = {for (final t in tilePool((_) {})) t.id};
  final raw = Settings.I.get<List<String>>('tiles');
  // The app's first tile list was not the web's pool: never edited, it means the defaults.
  if (raw.join(',') == 'awake,keyboard,touchpad,files,shell,resync,maximize,close') return tileDefaults;
  return raw.where(ids.contains).toList();
}

/// `.tiles`: two columns. Tap toggles; a long-press opens the tile's page.
class TileGrid extends StatelessWidget {
  const TileGrid({super.key, required this.open});
  final void Function(String page) open;
  @override
  Widget build(BuildContext context) {
    final pool = {for (final t in tilePool(open)) t.id: t};
    return ListenableBuilder(
      listenable: Listenable.merge([Settings.I, Host.I.state, Session.I.on, KeyboardSheet.open, Recorder.I.on]),
      builder: (context, _) {
        final tiles = [for (final id in shownTiles()) if (pool[id] != null) pool[id]!];
        return LayoutBuilder(builder: (context, box) {
          final w = (box.maxWidth - 8) / 2;
          return Wrap(spacing: 8, runSpacing: 8, children: [for (final t in tiles) SizedBox(width: w, child: _Tile(t: t, open: open))]);
        });
      },
    );
  }
}

class _Tile extends StatelessWidget {
  const _Tile({required this.t, required this.open});
  final TileDef t;
  final void Function(String) open;
  @override
  Widget build(BuildContext context) {
    final k = Tok.of(context);
    final on = t.on?.call() ?? false;
    final off = t.needsSession && !Session.I.on.value;
    final sub = on ? t.sub.$2 : t.sub.$1;
    return Opacity(
      opacity: off ? .45 : 1,
      child: IgnorePointer(
        ignoring: off,
        child: Press(
          onTap: () => t.act(context),
          onLongPress: t.page == null ? null : () => open(t.page!),
          label: '${t.label}, $sub',
          child: AnimatedContainer(
            duration: const Duration(milliseconds: 360),
            curve: Tokens.spring,
            constraints: const BoxConstraints(minHeight: 62),
            padding: const EdgeInsets.fromLTRB(8, 8, 12, 8),
            decoration: BoxDecoration(color: on ? k.accent : k.s2, borderRadius: BorderRadius.circular(on ? k.r3 + 8 : k.r3)),
            child: Row(children: [
              AnimatedContainer(
                duration: const Duration(milliseconds: 360),
                curve: Tokens.spring,
                width: 42,
                height: 42,
                decoration: BoxDecoration(color: on ? fade(k.onAccent, 84) : k.s3, borderRadius: BorderRadius.circular(on ? 21 : k.r2)),
                child: Center(child: WIcon(t.icon, color: on ? k.onAccent : k.bright)),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text(t.label, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .8125, w: FontWeight.w600, color: on ? k.onAccent : k.bright)),
                  if (sub.isNotEmpty) Text(sub, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .6875, color: on ? k.onAccent : k.dim)),
                ]),
              ),
            ]),
          ),
        ),
      ),
    );
  }
}

/// The picture as it is now, as a PNG, handed to the phone (the share sheet saves or sends it).
Future<void> screenshot() async {
  final v = Session.I.renderer.srcObject?.getVideoTracks().firstOrNull;
  if (v == null) return;
  final png = await v.captureFrame();
  final now = DateTime.now();
  String two(int n) => n.toString().padLeft(2, '0');
  await keepOnPhone('wado-${now.year}${two(now.month)}${two(now.day)}-${two(now.hour)}${two(now.minute)}${two(now.second)}.png', png.asUint8List());
}

/// Record (capture.js `recordToggle`): the picture — and the sound playing here — to an MP4 on
/// this phone, handed to the share sheet when it stops. Stops by itself when the picture ends.
class Recorder {
  Recorder._() {
    Session.I.hasVideo.addListener(() {
      if (!Session.I.hasVideo.value && on.value) toggle();
    });
  }
  static final I = Recorder._();
  final on = ValueNotifier(false);
  MediaRecorder? _r;
  String _path = '';

  Future<void> toggle() async {
    if (on.value) {
      on.value = false;
      final r = _r;
      _r = null;
      try {
        await r?.stop();
        await SharePlus.instance.share(ShareParams(files: [XFile(_path, mimeType: 'video/mp4')]));
      } catch (e) {
        rlog('recording did not save: $e');
      }
      return;
    }
    final v = Session.I.renderer.srcObject?.getVideoTracks().firstOrNull;
    if (v == null) return;
    final now = DateTime.now();
    String two(int n) => n.toString().padLeft(2, '0');
    _path = '${(await getTemporaryDirectory()).path}/wado-${now.year}${two(now.month)}${two(now.day)}-${two(now.hour)}${two(now.minute)}${two(now.second)}.mp4';
    final r = MediaRecorder();
    try {
      await r.start(_path, videoTrack: v, audioChannel: RecorderAudioChannel.OUTPUT);
      _r = r;
      on.value = true;
    } catch (e) {
      rlog('recording did not start: $e');
    }
  }
}
