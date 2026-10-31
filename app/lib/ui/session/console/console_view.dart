// The session (port of ui/surround/mod.rs, dock.rs, js/stage_fit.js and js/stage_morph.js): two
// arrangements of the same nodes. The **console** insets the picture at its true aspect with its
// controls around it (console_layout.dart); the **cockpit** is the picture alone, the dock along
// the bottom and the quick rail on a side. Controls present in both are one node that flies between
// its two places; the plates behind them never move, they fade (spec §2.5).
//
// Scale, never resize: the video keeps the stage's size and one transform takes its content onto
// the console's picture slot, so the session's output never changes size.
import 'dart:async';

import 'package:flutter/material.dart' hide Flow, Form;
import 'package:flutter/services.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

import '../../../wado/console_layout.dart';
import '../../../wado/host.dart';
import '../../../wado/input.dart';
import '../../../wado/pad.dart';
import '../../../wado/session.dart';
import '../../../wado/settings.dart';
import '../../motion/feel.dart';
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';
import '../control_centre.dart';
import '../keyboard.dart' show KeyboardSheet;
import '../picture_input.dart';
import 'deck.dart';
import 'parts.dart';
import 'stage_state.dart';

/// `x` of the way from `lo` to `hi`, clamped: one segment of the morph.
double _ph(double x, double lo, double hi) => ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
double _l(double a, double b, double t) => a + (b - a) * t;

class ConsoleView extends StatefulWidget {
  const ConsoleView({super.key});
  @override
  State<ConsoleView> createState() => _ConsoleViewState();
}

class _ConsoleViewState extends State<ConsoleView> with SingleTickerProviderStateMixin {
  /// 1 = console, 0 = cockpit.
  late final _arr = AnimationController(vsync: this, value: Settings.I.b('cockpit') ? 0 : 1);
  bool _cockpit = Settings.I.b('cockpit');
  bool _idle = false;
  Timer? _idleT;

  @override
  void initState() {
    super.initState();
    active(true);
    Host.I.watch(true);
    Settings.I.addListener(_settings);
    Session.I.renderer.addListener(_video);
    _wake();
    _orient();
  }

  @override
  void dispose() {
    active(false);
    Host.I.watch(false);
    Settings.I.removeListener(_settings);
    Session.I.renderer.removeListener(_video);
    _idleT?.cancel();
    DeckKeys.I.releaseAll();
    if (Pad.I.state.value != 'off') Pad.I.stop();
    SystemChrome.setPreferredOrientations(const []);
    _arr.dispose();
    super.dispose();
  }

  void _video() {
    _orient();
    setState(() {});
  }

  void _settings() {
    final c = Settings.I.b('cockpit');
    if (c != _cockpit) {
      _cockpit = c;
      final t = Tok.of(context);
      // 620 ms into the cockpit, 520 back (stage_morph.js).
      _arr.animateTo(c ? 0 : 1, duration: c ? t.durStage : t.durStageOut, curve: Tokens.spring);
      _orient();
      _wake();
    }
    setState(() {});
  }

  /// The cockpit holds the phone the way the picture is; the console lets it turn.
  void _orient() {
    final r = Session.I.renderer;
    if (!_cockpit || r.videoWidth <= 0) {
      SystemChrome.setPreferredOrientations(const []);
      return;
    }
    SystemChrome.setPreferredOrientations(r.videoWidth >= r.videoHeight
        ? const [DeviceOrientation.landscapeLeft, DeviceOrientation.landscapeRight]
        : const [DeviceOrientation.portraitUp]);
  }

  /// The cockpit's dock fades after 2 s untouched (js/bar.js); a press near the bottom wakes it.
  void _wake() {
    _idleT?.cancel();
    if (_idle) setState(() => _idle = false);
    if (_cockpit) _idleT = Timer(const Duration(seconds: 2), () => mounted ? setState(() => _idle = true) : null);
  }

  void _toggle() => Settings.I.set('cockpit', !_cockpit);

  void _gesture(String action) {
    switch (action) {
      case 'app-drawer' || 'control-centre':
        openControlCentre(context);
      case 'keyboard':
        KeyboardSheet.toggle(context);
      case 'back':
        back();
      case 'workspace-next' || 'workspace-prev':
        final ws = Session.I.workspace.value < 1 ? 1 : Session.I.workspace.value;
        final n = action == 'workspace-next' ? ws + 1 : ws - 1;
        if (n >= 1) Session.I.window({'workspace': {'n': n}});
      default:
        Session.I.window(action.replaceAll('-', '_'));
    }
  }

  /// `W.back`: close the keyboard first, then the session's own Back.
  void back() {
    if (KeyboardSheet.open.value) return KeyboardSheet.toggle(context);
    Session.I.window('back');
  }

  void _rotate(String to) {
    Settings.I.set('orientation', to);
    Session.I.reconfigure();
  }

  void _face(Face f) {
    final id = switch (f) { Face.pad => 'touchpad', Face.keys => 'keys', Face.quick => 'quick', Face.gamepad => 'gamepad' };
    Settings.I.set('console_face', id);
  }

  void _windowMenu(int id) {
    final w = Session.I.windows.value.where((w) => w['id'] == id).firstOrNull;
    if (w != null) _windowSheet(context, w);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final mq = MediaQuery.of(context);
    final size = mq.size;
    final vw = size.width, vh = size.height;
    final sa = mq.viewPadding;
    final r = Session.I.renderer;
    final outW = r.videoWidth.toDouble(), outH = r.videoHeight.toDouble();
    final form = Form.of(size, touch: true);
    final pref = switch (Settings.I.s('console_face')) { 'touchpad' => 'pad', final s => s };
    final face = Face.resolve(pref, form);
    final L = console(vw: vw, vh: vh, inset: sa, outW: outW, outH: outH, form: form, face: face);
    final stage = Offset.zero & size;
    final c0 = fit(stage, outW > 0 && outH > 0 ? outW / outH : vw / vh);
    final railSide = Settings.I.s('rail');
    final wsBar = Settings.I.b('ws_bar');
    final turnTo = (Session.I.applied?['width'] ?? outW) >= (Session.I.applied?['height'] ?? outH) ? 'portrait' : 'landscape';

    return ColoredBox(
      color: Tokens.stage,
      child: AnimatedBuilder(
        animation: _arr,
        builder: (context, _) {
          final a = _arr.value;
          final toConsole = !_cockpit;
          // The plates and the console-only controls: in from 30% of the run, out by 38%.
          final ground = toConsole ? _ph(a, 0, .55) : _ph(a, .45, 1);
          final plates = toConsole ? _ph(a, .3, 1) : _ph(a, .45, 1);
          final only = toConsole ? _ph(a, .3, .9) : _ph(a, .62, 1);
          // The cockpit's pill plates arrive from 45% of the run and leave by 30%.
          final pills = toConsole ? 1 - _ph(a, 0, .3) : 1 - _ph(a, 0, .55);
          final content = Rect.lerp(c0, L.pic, a)!;
          Input.I
            ..content = content
            ..video = Size(outW, outH);
          final k = _l(1, L.k, a);
          final dockO = _cockpit && _idle ? 0.0 : 1.0;

          Rect slot(String k) => L[k]!.r;
          Widget plate(String k, double radius, Color c) {
            final s = slot(k);
            return Positioned.fromRect(rect: s, child: IgnorePointer(child: Opacity(opacity: plates, child: DecoratedBox(decoration: BoxDecoration(color: c, borderRadius: BorderRadius.circular(radius))))));
          }

          Widget only_(String k, Widget child) {
            final s = L[k]!;
            return Positioned.fromRect(
              rect: s.r,
              child: IgnorePointer(ignoring: a < .5 || !s.shown, child: Opacity(opacity: s.shown ? only : 0, child: child)),
            );
          }

          // A control that is in both arrangements: placed by its own size at each end.
          Widget fly(Offset Function(Size) cockpit, Offset Function(Size) cons, Widget child, {double opacity = 1}) => Positioned.fill(
                child: CustomSingleChildLayout(
                  delegate: _Fly(cockpit, cons, a),
                  child: Opacity(opacity: opacity, child: child),
                ),
              );
          Offset slotAt(String k, Size s, {bool centreX = false, bool centreY = true}) {
            final sl = L[k]!;
            final x = sl.right ? vw - sl.r.left - s.width : (centreX ? sl.r.left + (sl.r.width - s.width) / 2 : sl.r.left);
            final y = centreY && sl.r.height > 0 ? sl.r.top + (sl.r.height - s.height) / 2 : sl.r.top;
            return Offset(x, y);
          }

          Widget pill(Widget child, {double pad = 6}) => Stack(children: [
                Positioned.fill(
                  child: Opacity(
                    opacity: pills,
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        color: fade(t.s1, 12),
                        borderRadius: BorderRadius.circular(999),
                        border: Border.all(color: fade(t.line, 30)),
                        boxShadow: const [BoxShadow(color: Color(0x8C000000), blurRadius: 24, offset: Offset(0, 6), spreadRadius: -10)],
                      ),
                    ),
                  ),
                ),
                Padding(padding: EdgeInsets.all(pad * (1 - a)), child: child),
              ]);

          // The dock's bottom line in the cockpit; a narrow portrait phone puts workspaces above it.
          final narrow = vw <= 430;
          final dockBottom = vh - 12 - sa.bottom;
          final type = Session.I.textWanted.value && !KeyboardSheet.open.value;

          // The quick rail's buttons, cockpit places: a column on the screen's edge.
          final railN = 4;
          const rb = 40.0;
          final railH = railN * rb + (railN - 1) * 4 + 12;
          final railTop = (vh - railH) / 2 + 6;
          final railX = railSide == 'left' ? 8 + sa.left + 6 : vw - 8 - sa.right - 6 - rb;
          final railSlide = _cockpit && _idle ? (railSide == 'left' ? -.6 : .6) * (rb + 12) : 0.0;
          final railO = railSide == 'off' ? 0.0 : (_cockpit && _idle ? 0.0 : 1.0);
          Offset railAt(int i, Size s) => Offset(railX + railSlide + (rb - s.width) / 2, railTop + i * (rb + 4) + (rb - s.height) / 2);
          final fullSlot = L['full']!;
          final fullW = fullSlot.r.width > 0 ? fullSlot.r.width : fullW_;
          final railOpacity = _l(railO, 1, a);

          return Listener(
            behavior: HitTestBehavior.translucent,
            onPointerDown: (e) {
              if (_cockpit && e.position.dy > vh * .8) _wake();
            },
            child: Stack(children: [
              // ── the ground and the plates, under the picture ──
              Positioned.fill(
                child: IgnorePointer(
                  child: Opacity(
                    opacity: ground,
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        color: t.s0,
                        gradient: RadialGradient(center: const Alignment(0, -1), radius: 1.1, colors: [fade(t.accent, 88), fade(t.accent, 100)], stops: const [0, .7]),
                      ),
                    ),
                  ),
                ),
              ),
              plate('bezel', t.r4, t.s1),
              if (L['deck']!.shown) plate('deck', t.r3, t.s1),
              plate('pic', 16, Colors.black),
              // ── the picture: one transform ──
              Positioned.fill(
                child: IgnorePointer(
                  child: ClipPath(
                    clipper: _Round(content, 16 * a),
                    child: Transform(
                      transform: Matrix4.identity()
                        ..translateByDouble(L.tx * a, L.ty * a, 0, 1)
                        ..scaleByDouble(k, k, 1, 1),
                      child: RTCVideoView(r, objectFit: RTCVideoViewObjectFit.RTCVideoViewObjectFitContain),
                    ),
                  ),
                ),
              ),
              // The picture's input: only over the picture's own content.
              Positioned.fromRect(rect: content, child: PictureInput(onGesture: _gesture, child: const SizedBox.expand())),
              if (face == Face.pad) Positioned.fill(child: IgnorePointer(child: Opacity(opacity: only, child: const DeckCursor()))),
              // ── console only: the strip, the faces, the deck ──
              if (face != Face.gamepad) only_('strip', WindowStrip(onWindowMenu: _windowMenu)),
              only_(
                'faces',
                Align(alignment: Alignment.topCenter, child: FacesSwitch(active: face, flow: L.faces, touch: form.touch, onPick: _face)),
              ),
              if (face == Face.gamepad)
                only_('pad', const GameFace())
              else
                only_('deck', Deck(face: switch (face) { Face.pad => 'pad', Face.keys => 'keys', _ => 'quick' }, col: L.deckCol)),
              // ── the top band: the link badge flies, the name only fades ──
              fly(
                (s) => Offset(10 + sa.left, 10 + sa.top),
                (s) => slotAt('link', s, centreY: false),
                Row(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
                  LinkBadge(quietable: a < .5, waiting: stageWaiting()),
                  const SizedBox(width: 12),
                  IgnorePointer(
                    ignoring: a < .5,
                    child: Opacity(
                      opacity: only,
                      child: ConstrainedBox(
                        constraints: BoxConstraints(maxWidth: (slot('band').width - 150 - 140).clamp(60, 600)),
                        child: SizedBox(height: 32, child: const Align(alignment: Alignment.centerLeft, child: NameChip())),
                      ),
                    ),
                  ),
                ]),
              ),
              fly(
                (s) => Offset(vw - 10 - sa.right - s.width, 10 + sa.top),
                (s) => slotAt('people', s, centreY: false),
                ConstrainedBox(constraints: BoxConstraints(maxWidth: L.form == Form.port || L.form == Form.land ? 132 : 260), child: PeopleChip(shadow: a < .5)),
              ),
              // ── the dock's groups ──
              if (wsBar)
                fly(
                  (s) => Offset(10 + sa.left, dockBottom - s.height - (narrow ? 64 : 0)),
                  (s) => slotAt('ws', s, centreX: true, centreY: L.ws == Flow.row),
                  ConstrainedBox(
                    constraints: BoxConstraints(
                      maxWidth: _cockpit ? vw * (narrow ? 1 : .4) - 20 : slot('ws').width,
                      maxHeight: _cockpit ? 60 : slot('ws').height,
                    ),
                    child: pill(WsBar(vertical: !_cockpit && L.ws == Flow.col, big: !_cockpit, onWindowMenu: _windowMenu)),
                  ),
                  opacity: _l(dockO, 1, a),
                ),
              fly(
                (s) => Offset((vw - s.width) / 2, dockBottom - s.height),
                (s) => L.nav == Flow.grid ? Offset(slot('nav').left + (slot('nav').width - s.width) / 2, slot('nav').top) : slotAt('nav', s, centreX: true),
                pill(_Nav(grid: !_cockpit && L.nav == Flow.grid, onApps: () => openControlCentre(context), onBack: back)),
                opacity: _l(dockO, 1, a),
              ),
              fly(
                (s) => Offset(vw - 10 - sa.right - s.width, dockBottom - s.height),
                (s) => slotAt('more', s),
                pill(type
                    ? BarBtn(icon: 'kbd', label: 'Keyboard', minW: 54, onTap: () => KeyboardSheet.toggle(context))
                    : ValueListenableBuilder(
                        valueListenable: Session.I.encoder,
                        builder: (context, enc, _) => BarBtn(
                          icon: 'dots',
                          label: 'Control centre',
                          minW: 54,
                          led: enc.$1 == 'software' ? 'sw' : 'ok',
                          onTap: () => openControlCentre(context),
                        ),
                      )),
                opacity: _l(dockO, 1, a),
              ),
              // ── the quick rail: its plate fades; its buttons go to their own slots ──
              if (railSide != 'off')
                Positioned(
                  left: railX - 6 + railSlide,
                  top: railTop - 6,
                  width: rb + 12,
                  height: railH,
                  child: IgnorePointer(child: Opacity(opacity: pills * railO, child: DecoratedBox(decoration: BoxDecoration(color: fade(t.s1, 18), borderRadius: BorderRadius.circular(999), border: Border.all(color: fade(t.line, 30)))))),
                ),
              fly(
                (s) => railAt(0, s),
                (s) => slotAt('faces', s, centreY: false),
                IgnorePointer(ignoring: a > .5, child: _RailFace(face: face, touch: form.touch, onPick: _face)),
                opacity: _l(railOpacity, 0, a),
              ),
              fly(
                (s) => railAt(1, s),
                (s) => slotAt('rotate', s),
                IgnorePointer(ignoring: a > .5 && !L['rotate']!.shown, child: BarBtn(icon: 'rotate', label: 'Turn to $turnTo', size: _l(rb, 44, a), onTap: () => _rotate(turnTo))),
                opacity: _l(railOpacity, L['rotate']!.shown ? 1 : 0, a),
              ),
              fly(
                (s) => railAt(2, s),
                (s) => slotAt('full', s),
                _FullBtn(cockpit: _cockpit, a: a, w: _l(rb, fullW, a), h: _l(rb, 44, a), onTap: _toggle),
                opacity: railOpacity,
              ),
              fly(
                (s) => railAt(3, s),
                (s) => slotAt('kbd', s),
                IgnorePointer(
                  ignoring: a > .5 && !L['kbd']!.shown,
                  child: ValueListenableBuilder(
                    valueListenable: KeyboardSheet.open,
                    builder: (context, on, _) => BarBtn(icon: 'kbd', label: 'Keyboard', on: on, size: _l(rb, 44, a), onTap: () => KeyboardSheet.toggle(context)),
                  ),
                ),
                opacity: _l(railOpacity, L['kbd']!.shown ? 1 : 0, a),
              ),
              // ── no picture yet: in the picture's place ──
              Positioned.fromRect(rect: Rect.lerp(stage, L.pic, a)!, child: const Padding(padding: EdgeInsets.all(10), child: StageState())),
              const Positioned(left: 0, right: 0, bottom: 0, child: KeyboardSheet()),
            ]),
          );
        },
      ),
    );
  }
}

const fullW_ = fullW;

class _Fly extends SingleChildLayoutDelegate {
  _Fly(this.a0, this.a1, this.t);
  final Offset Function(Size) a0, a1;
  final double t;
  @override
  BoxConstraints getConstraintsForChild(BoxConstraints c) => c.loosen();
  @override
  Offset getPositionForChild(Size size, Size child) => Offset.lerp(a0(child), a1(child), t)!;
  @override
  bool shouldRelayout(_Fly o) => true;
}

class _Round extends CustomClipper<Path> {
  _Round(this.r, this.radius);
  final Rect r;
  final double radius;
  @override
  Path getClip(Size size) => Path()..addRRect(RRect.fromRectAndRadius(r, Radius.circular(radius)));
  @override
  bool shouldReclip(_Round o) => o.r != r || o.radius != radius;
}

/// Apps · ○ Home · ◁ Back.
class _Nav extends StatelessWidget {
  const _Nav({required this.grid, required this.onApps, required this.onBack});
  final bool grid;
  final VoidCallback onApps, onBack;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final apps = BarBtn(icon: 'apps', label: 'Apps', minW: grid ? 48 : 54, onTap: onApps);
    final home = BarBtn(
      icon: '',
      label: 'Home — an empty workspace',
      minW: grid ? 48 : 62,
      onTap: () => Session.I.window('home'),
      child: Container(width: 20, height: 20, decoration: BoxDecoration(shape: BoxShape.circle, border: Border.all(color: t.text, width: 2.2))),
    );
    final back = BarBtn(icon: 'back', label: 'Back', minW: grid ? 48 : 54, onTap: onBack);
    if (grid) {
      return SizedBox(
        width: 104,
        child: Wrap(spacing: 8, runSpacing: 6, children: [apps, home, back]),
      );
    }
    return Row(mainAxisSize: MainAxisSize.min, children: [apps, const SizedBox(width: 4), home, const SizedBox(width: 4), back]);
  }
}

/// Full screen: the console's labelled accent pill, the cockpit's "Leave full screen".
class _FullBtn extends StatelessWidget {
  const _FullBtn({required this.cockpit, required this.a, required this.w, required this.h, required this.onTap});
  final bool cockpit;
  final double a, w, h;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = Color.lerp(t.text, t.accent, a)!;
    return Press(
      onTap: onTap,
      label: cockpit ? 'Leave full screen' : 'Full screen',
      child: Container(
        width: w,
        height: h,
        decoration: BoxDecoration(color: fade(t.accent, 100 - 18 * a), borderRadius: BorderRadius.circular(999)),
        child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
          WIcon(cockpit ? 'min' : 'max', color: c),
          if (a > .4) ...[
            const SizedBox(width: 8),
            Flexible(child: Opacity(opacity: _ph(a, .4, 1), child: Text('Full screen', maxLines: 1, overflow: TextOverflow.clip, style: body(context, size: .8125, w: FontWeight.w600, color: t.accent)))),
          ],
        ]),
      ),
    );
  }
}

/// The cockpit rail's face button: tap cycles, a long press lists them (faces.rs `rail_button`).
class _RailFace extends StatelessWidget {
  const _RailFace({required this.face, required this.touch, required this.onPick});
  final Face face;
  final bool touch;
  final ValueChanged<Face> onPick;
  @override
  Widget build(BuildContext context) {
    final faces = [for (final f in Face.values) if (touch || f != Face.pad) f];
    return Press(
      onTap: () => onPick(faces[(faces.indexOf(face) + 1) % faces.length]),
      onLongPress: () => showModalBottomSheet(
        context: context,
        backgroundColor: Colors.transparent,
        builder: (c) {
          final t = Tok.of(c);
          return SafeArea(
            child: Container(
              margin: const EdgeInsets.all(10),
              padding: const EdgeInsets.all(6),
              decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                for (final f in faces)
                  Press(
                    onTap: () {
                      Navigator.pop(c);
                      onPick(f);
                    },
                    child: Container(
                      height: 44,
                      padding: const EdgeInsets.symmetric(horizontal: 10),
                      decoration: BoxDecoration(color: f == face ? fade(t.accent, 80) : null, borderRadius: BorderRadius.circular(t.r2)),
                      child: Row(children: [
                        WIcon(f.icon, color: f == face ? t.accent : t.bright),
                        const SizedBox(width: 10),
                        Text(f.label, style: body(context, size: .8125, w: FontWeight.w600, color: f == face ? t.accent : t.bright)),
                      ]),
                    ),
                  ),
              ]),
            ),
          );
        },
      ),
      label: 'Input: ${face.label}',
      child: SizedBox(width: 40, height: 40, child: Center(child: WIcon(face.icon, color: Tok.of(context).text))),
    );
  }
}

/// The window sheet (workspaces.rs `sheet`): a long press on a window's icon or its title.
void _windowSheet(BuildContext context, Map<String, dynamic> w) {
  final s = Session.I;
  final id = w['id'];
  final here = ((w['workspace'] ?? 1) as int).clamp(1, 99);
  final used = {for (final x in s.windows.value) ((x['workspace'] ?? 1) as int).clamp(1, 99), s.workspace.value};
  final next = used.reduce((a, b) => a > b ? a : b) + 1;
  final title = '${w['title'] ?? ''}'.isEmpty ? appName(w) : '${w['title']}';
  showModalBottomSheet(
    context: context,
    backgroundColor: Colors.transparent,
    barrierColor: const Color(0x59000000),
    builder: (c) {
      final t = Tok.of(c);
      void go(Object a) {
        Navigator.pop(c);
        s.window(a);
      }

      Widget chip(String icon, String label, VoidCallback onTap, {bool bad = false}) => Press(
            onTap: onTap,
            label: label,
            child: Container(
              height: 40,
              padding: const EdgeInsets.symmetric(horizontal: 12),
              decoration: BoxDecoration(borderRadius: BorderRadius.circular(999), border: Border.all(color: bad ? fade(t.bad, 50) : t.line)),
              child: Row(mainAxisSize: MainAxisSize.min, children: [
                WIcon(icon, size: 16, color: bad ? t.bad : t.text),
                const SizedBox(width: 6),
                Text(label, style: body(c, size: .8125, w: FontWeight.w600, color: bad ? t.bad : t.text)),
              ]),
            ),
          );
      return SafeArea(
        child: Center(
          heightFactor: 1,
          child: Container(
            constraints: const BoxConstraints(maxWidth: 420),
            margin: const EdgeInsets.all(10),
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
            decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3), border: Border.all(color: t.line)),
            child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
              Center(child: Container(width: 36, height: 4, margin: const EdgeInsets.only(bottom: 10), decoration: BoxDecoration(color: t.s3, borderRadius: BorderRadius.circular(2)))),
              Text(title, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(c, w: FontWeight.w700, color: t.bright)),
              const SizedBox(height: 10),
              Text('MOVE TO WORKSPACE', style: body(c, size: .6875, w: FontWeight.w600, color: t.dim).copyWith(letterSpacing: .8)),
              const SizedBox(height: 8),
              Wrap(spacing: 8, runSpacing: 8, children: [
                for (final n in [...(used.toList()..sort()), next])
                  Press(
                    onTap: n == here ? null : () => go({'move_to_workspace': {'id': id, 'n': n, 'follow': false}}),
                    label: n == next ? 'A new workspace' : 'Workspace $n',
                    child: Container(
                      width: 44,
                      height: 44,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(color: n == here ? t.accent : t.s2, borderRadius: BorderRadius.circular(999)),
                      child: n == next ? WIcon('plus', size: 15, color: t.dim) : Text('$n', style: mono(c, size: .8125, w: FontWeight.w700, color: n == here ? t.onAccent : t.dim)),
                    ),
                  ),
              ]),
              const SizedBox(height: 12),
              Wrap(spacing: 8, runSpacing: 8, children: [
                chip('right', 'Take it to a new workspace', () => go({'move_to_workspace': {'id': id, 'n': next, 'follow': true}})),
                chip('max', 'Maximise', () {
                  s.window({'focus': {'id': id}});
                  go('maximize');
                }),
                chip('x', 'Close', () {
                  s.window({'focus': {'id': id}});
                  go('close');
                }, bad: true),
              ]),
            ]),
          ),
        ),
      );
    },
  );
}
