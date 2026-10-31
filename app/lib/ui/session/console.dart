// The Pocket Console (port of ui/surround/*: layout.rs, top_band.rs, window_strip.rs, faces.rs,
// deck/*): the picture inset at its true aspect, the controls around it — the computer's name,
// the focused window's strip, four input faces (Touchpad, Keys, Quick actions, Gamepad) and the
// deck the active face puts under the thumbs. The cockpit is the picture alone, full screen; the
// two morph into each other on one spring.
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/pad.dart';
import '../../wado/session.dart';
import '../../wado/settings.dart';
import '../computer/quick.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/slider.dart';
import 'control_centre.dart';
import 'input_layer.dart';
import 'session_view.dart';

/// Evdev codes for the Keys face (linux/input-event-codes.h).
const _ev = {
  'esc': 1, 'tab': 15, 'ctrl': 29, 'alt': 56, 'super': 125, 'shift': 42, 'enter': 28, 'bksp': 14, 'del': 111, //
  'up': 103, 'down': 108, 'left': 105, 'right': 106, 'home': 102, 'end': 107, 'pgup': 104, 'pgdn': 109,
  'f1': 59, 'f2': 60, 'f3': 61, 'f4': 62, 'f5': 63, 'f6': 64, 'f7': 65, 'f8': 66, 'f9': 67, 'f10': 68, 'f11': 87, 'f12': 88,
};

class Console extends StatefulWidget {
  const Console({super.key});
  @override
  State<Console> createState() => _ConsoleState();
}

class _ConsoleState extends State<Console> {
  bool get _cockpit => Settings.I.b('cockpit');
  String get _face => Settings.I.s('console_face').isEmpty ? 'keys' : Settings.I.s('console_face');

  @override
  void initState() {
    super.initState();
    Host.I.watch(true);
  }

  @override
  void dispose() {
    Host.I.watch(false);
    if (Pad.I.state.value != 'off') {
      PadTouch.I.detach();
      Pad.I.stop();
    }
    super.dispose();
  }

  void _setFace(String f) {
    if (f == 'touchpad' && Pad.I.state.value == 'off') {
      Pad.I.start();
      PadTouch.I.attach();
    } else if (f != 'touchpad' && f != 'gamepad' && Pad.I.state.value != 'off') {
      PadTouch.I.detach();
      Pad.I.stop();
    }
    if (f == 'gamepad' && Pad.I.state.value == 'off') Pad.I.start();
    Settings.I.set('console_face', f);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([Settings.I, Session.I.windows, Session.I.encoder]),
      builder: (context, _) => LayoutBuilder(builder: (context, box) {
        final mq = MediaQuery.of(context);
        final w = box.maxWidth, h = box.maxHeight;
        final r = Session.I.renderer;
        final aspect = (r.videoWidth > 0 && r.videoHeight > 0) ? r.videoWidth / r.videoHeight : 16 / 9;
        // Console geometry (layout.rs, portrait): the band, the picture at its aspect, the strip,
        // then the faces and the deck take what is left.
        final top = mq.padding.top + 44;
        final picW = w - 16;
        final picH = math.min(picW / aspect, h * .5);
        final picRect = _cockpit ? Rect.fromLTWH(0, 0, w, h) : Rect.fromLTWH(8, top, picW, picH);
        final below = top + picH + 8;
        final dur = _cockpit ? t.durStage : t.durStageOut;
        final c = CoreHost.I.computer(Nav.I.selected);
        final soft = Session.I.encoder.value.$1 == 'software';
        final win = Session.I.windows.value.where((x) => x['focused'] == true).firstOrNull;
        return Stack(children: [
          // The top band: the computer's name, or the software-encoding chip in its place.
          AnimatedOpacity(
            opacity: _cockpit ? 0 : 1,
            duration: t.dur,
            child: Padding(
              padding: EdgeInsets.fromLTRB(16, mq.padding.top + 10, 16, 0),
              child: Row(children: [
                Expanded(
                  child: soft
                      ? Container(
                          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                          decoration: BoxDecoration(color: fade(t.warn, 80), borderRadius: BorderRadius.circular(999)),
                          child: Text('Software encoding — higher latency', style: body(context, size: .75, w: FontWeight.w600, color: t.warn)),
                        )
                      : Text(c?.name ?? '', maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1), color: Colors.white)),
                ),
                IconBtn('max', label: 'Full screen', color: Colors.white70, onTap: () => Settings.I.set('cockpit', true)),
              ]),
            ),
          ),
          // The picture: one rect that springs between the two arrangements.
          AnimatedPositioned.fromRect(
            rect: picRect,
            duration: dur,
            curve: Tokens.spring,
            child: ClipRRect(
              borderRadius: BorderRadius.circular(_cockpit ? 0 : t.r2),
              child: ColoredBox(
                color: Tokens.stage,
                child: InputLayer(child: RTCVideoView(r, objectFit: RTCVideoViewObjectFit.RTCVideoViewObjectFitContain)),
              ),
            ),
          ),
          if (!_cockpit) ...[
            // The window strip: the focused window, Minimise, Maximise, Close.
            Positioned(
              left: 8,
              right: 8,
              top: below,
              height: 40,
              child: Row(children: [
                const SizedBox(width: 8),
                Expanded(
                  child: Text(win == null ? 'No window' : '${'${win['title']}'.isEmpty ? win['app_id'] : win['title']}', maxLines: 1, overflow: TextOverflow.ellipsis,
                      style: body(context, size: .8125, color: Colors.white70)),
                ),
                IconBtn('min', label: 'Minimise', color: Colors.white70, onTap: () => Session.I.window('minimize')),
                IconBtn('winmax', label: 'Maximise', color: Colors.white70, onTap: () => Session.I.window('maximize')),
                IconBtn('x', label: 'Close', color: Colors.white70, onTap: () => Session.I.window('close')),
              ]),
            ),
            // The faces.
            Positioned(
              left: 12,
              right: 12,
              top: below + 44,
              height: 44,
              child: Row(children: [
                for (final (f, icon, label) in const [('touchpad', 'trackpad', 'Touchpad'), ('keys', 'kbd', 'Keys'), ('quick', 'sliders', 'Quick'), ('gamepad', 'pad', 'Gamepad')])
                  Expanded(
                    child: Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 3),
                      child: Press(
                        onTap: () => _setFace(f),
                        label: label,
                        child: AnimatedContainer(
                          duration: t.dur,
                          decoration: BoxDecoration(color: _face == f ? t.accent : t.s1, borderRadius: BorderRadius.circular(999)),
                          child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                            WIcon(icon, size: 16, color: _face == f ? t.onAccent : t.bright),
                            const SizedBox(width: 6),
                            Flexible(child: Text(label, overflow: TextOverflow.ellipsis, style: body(context, size: .75, w: FontWeight.w600, color: _face == f ? t.onAccent : t.bright))),
                          ]),
                        ),
                      ),
                    ),
                  ),
              ]),
            ),
            // The deck.
            Positioned(
              left: 8,
              right: 8,
              top: below + 96,
              bottom: mq.padding.bottom + 64,
              child: switch (_face) {
                'touchpad' => const _TouchDeck(),
                'quick' => const _QuickDeck(),
                'gamepad' => const _GameDeck(),
                _ => const _KeysDeck(),
              },
            ),
          ],
          // The dock rides in both arrangements; in the cockpit, a way back to the console.
          Positioned(
            right: 8,
            bottom: 8,
            child: Row(children: [
              if (_cockpit)
                Padding(
                  padding: const EdgeInsets.only(right: 6),
                  child: SafeArea(child: IconBtn('min', label: 'Back to the console', color: Colors.white70, onTap: () => Settings.I.set('cockpit', false))),
                ),
              const Dock(),
            ]),
          ),
        ]);
      }),
    );
  }
}

/// The Touchpad face: the phone as the computer's touchpad (the pad link), under the thumbs.
class _TouchDeck extends StatefulWidget {
  const _TouchDeck();
  @override
  State<_TouchDeck> createState() => _TouchDeckState();
}

class _TouchDeckState extends State<_TouchDeck> {
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ValueListenableBuilder(
      valueListenable: Pad.I.error,
      builder: (context, err, _) {
        if (err != null && err.isNotEmpty) {
          return Center(child: Padding(padding: const EdgeInsets.all(12), child: Text(padWhy(err).title, textAlign: TextAlign.center, style: body(context, color: t.warn))));
        }
        return LayoutBuilder(builder: (context, box) {
          final w = box.maxWidth, h = box.maxHeight;
          final dpr = MediaQuery.of(context).devicePixelRatio;
          PadTouch.I.size((w * dpr).round(), (h * dpr).round());
          return Listener(
            behavior: HitTestBehavior.opaque,
            onPointerDown: (e) => PadTouch.I.down(e.pointer, e.localPosition.dx, e.localPosition.dy, w, h),
            onPointerMove: (e) => PadTouch.I.move(e.pointer, e.localPosition.dx, e.localPosition.dy, w, h),
            onPointerUp: (e) => PadTouch.I.up(e.pointer),
            onPointerCancel: (e) => PadTouch.I.up(e.pointer),
            child: Container(
              decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3), border: Border.all(color: t.line)),
              alignment: Alignment.bottomCenter,
              padding: const EdgeInsets.all(10),
              child: Text('Slide to move · tap to click · two fingers scroll', style: body(context, size: .6875, color: t.dim)),
            ),
          );
        });
      },
    );
  }
}

/// The Keys face: the keys a phone keyboard lacks, with sticky modifiers, and the keyboard.
class _KeysDeck extends StatefulWidget {
  const _KeysDeck();
  @override
  State<_KeysDeck> createState() => _KeysDeckState();
}

class _KeysDeckState extends State<_KeysDeck> {
  final _mods = <String>{};
  bool _fn = false;

  void _key(String k) {
    final s = Session.I;
    if (const ['ctrl', 'alt', 'super', 'shift'].contains(k)) {
      setState(() => _mods.contains(k) ? _mods.remove(k) : _mods.add(k));
      return;
    }
    for (final m in _mods) {
      s.sendInput({'t': 'key', 'code': _ev[m], 'pressed': true});
    }
    s.sendInput({'t': 'key', 'code': _ev[k], 'pressed': true});
    s.sendInput({'t': 'key', 'code': _ev[k], 'pressed': false});
    for (final m in _mods) {
      s.sendInput({'t': 'key', 'code': _ev[m], 'pressed': false});
    }
    if (_mods.isNotEmpty) setState(_mods.clear);
    HapticFeedback.selectionClick();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    Widget k(String id, String label, {int flex = 1}) => Expanded(
          flex: flex,
          child: Padding(
            padding: const EdgeInsets.all(3),
            child: Press(
              onTap: () => _key(id),
              label: label,
              child: Container(
                alignment: Alignment.center,
                decoration: BoxDecoration(color: _mods.contains(id) ? t.accent : t.s2, borderRadius: BorderRadius.circular(t.r1)),
                child: Text(label, style: body(context, size: .8125, w: FontWeight.w600, color: _mods.contains(id) ? t.onAccent : t.bright)),
              ),
            ),
          ),
        );
    final rows = _fn
        ? [
            [k('f1', 'F1'), k('f2', 'F2'), k('f3', 'F3'), k('f4', 'F4')],
            [k('f5', 'F5'), k('f6', 'F6'), k('f7', 'F7'), k('f8', 'F8')],
            [k('f9', 'F9'), k('f10', 'F10'), k('f11', 'F11'), k('f12', 'F12')],
          ]
        : [
            [k('esc', 'Esc'), k('tab', 'Tab'), k('home', 'Home'), k('up', '↑'), k('end', 'End'), k('bksp', '⌫')],
            [k('ctrl', 'Ctrl'), k('alt', 'Alt'), k('left', '←'), k('down', '↓'), k('right', '→'), k('del', 'Del')],
            [k('super', 'Super'), k('shift', 'Shift'), k('pgup', 'PgUp'), k('pgdn', 'PgDn'), k('enter', 'Enter', flex: 2)],
          ];
    return Column(children: [
      for (final r in rows) Expanded(child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: r)),
      SizedBox(
        height: 48,
        child: Row(children: [
          Expanded(child: Btn(_fn ? 'Keys' : 'F1–F12', onTap: () => setState(() => _fn = !_fn))),
          const SizedBox(width: 8),
          Expanded(child: Btn('Keyboard', icon: 'kbd', primary: true, onTap: () => KeyboardSheet.toggle(context))),
        ]),
      ),
    ]);
  }
}

/// The Quick actions face: volume, keep awake, workspaces, the control centre.
class _QuickDeck extends StatelessWidget {
  const _QuickDeck();
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([Host.I.state, Session.I.workspace]),
      builder: (context, _) {
        final h = Host.I.state.value;
        final sp = h == null ? null : speaker(h);
        return ListView(children: [
          if (sp != null)
            Row(children: [
              IconBtn(sp['muted'] == true ? 'mute' : 'sound', label: 'Mute', color: t.bright, onTap: () => Host.I.act({'do': 'sink_mute', 'id': sp['id'], 'muted': sp['muted'] != true})),
              Expanded(child: ThickSlider(label: 'Volume', thin: true, value: ((sp['volume'] ?? 0) as num).toDouble().clamp(0, 1), onChange: (v) => Host.I.act({'do': 'sink_volume', 'id': sp['id'], 'volume': v}))),
            ]),
          const SizedBox(height: 8),
          Row(children: [
            for (var n = 1; n <= 5; n++)
              Expanded(
                child: Padding(
                  padding: const EdgeInsets.all(3),
                  child: Press(
                    onTap: () => Session.I.window({'workspace': {'n': n}}),
                    child: Container(
                      height: 44,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(color: Session.I.workspace.value == n ? t.accent : t.s2, borderRadius: BorderRadius.circular(t.r1)),
                      child: Text('$n', style: mono(context, size: .875, w: FontWeight.w600, color: Session.I.workspace.value == n ? t.onAccent : t.bright)),
                    ),
                  ),
                ),
              ),
          ]),
          const SizedBox(height: 8),
          Row(children: [
            Expanded(child: QTile(icon: 'eye', title: 'Keep awake', on: h?['awake'] == true, sub: h?['awake'] == true ? "Won't sleep" : 'Sleeps as usual', onFlip: () => Host.I.act({'do': 'keep_awake', 'on': h?['awake'] != true}))),
            const SizedBox(width: 8),
            Expanded(child: QTile(icon: 'apps', title: 'Apps & more', on: false, sub: 'The control centre', onFlip: () => openControlCentre(context))),
          ]),
        ]);
      },
    );
  }
}

/// The Gamepad face: a compact controller under the thumbs (the pad link's gamepad).
class _GameDeck extends StatefulWidget {
  const _GameDeck();
  @override
  State<_GameDeck> createState() => _GameDeckState();
}

class _GameDeckState extends State<_GameDeck> {
  @override
  void initState() {
    super.initState();
    Pad.I.send({'k': 'game_on'});
  }

  @override
  void dispose() {
    Pad.I.send({'k': 'game_off'});
    super.dispose();
  }

  void _btn(int code, bool down) {
    Pad.I.send({'k': 'btn', 'code': code, 'down': down});
    if (down) HapticFeedback.lightImpact();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    Widget b(String label, int code, Color c) => Listener(
          onPointerDown: (_) => _btn(code, true),
          onPointerUp: (_) => _btn(code, false),
          onPointerCancel: (_) => _btn(code, false),
          child: Container(
            width: 56,
            height: 56,
            alignment: Alignment.center,
            decoration: BoxDecoration(shape: BoxShape.circle, color: t.s2, border: Border.all(color: fade(c, 40), width: 1.5)),
            child: Text(label, style: TextStyle(fontFamily: Tokens.display, fontWeight: FontWeight.w700, fontSize: 18, color: c)),
          ),
        );
    return Row(mainAxisAlignment: MainAxisAlignment.spaceEvenly, children: [
      Column(mainAxisAlignment: MainAxisAlignment.center, children: [
        b('▲', 0x220, t.bright),
        Row(mainAxisSize: MainAxisSize.min, children: [b('◀', 0x222, t.bright), const SizedBox(width: 56), b('▶', 0x223, t.bright)]),
        b('▼', 0x221, t.bright),
      ]),
      Column(mainAxisAlignment: MainAxisAlignment.center, children: [
        b('Y', 0x134, t.base[0xA]),
        Row(mainAxisSize: MainAxisSize.min, children: [b('X', 0x133, t.base[0xD]), const SizedBox(width: 56), b('B', 0x131, t.base[8])]),
        b('A', 0x130, t.base[0xB]),
      ]),
    ]);
  }
}
