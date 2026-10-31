// The deck: what the active input face puts under the thumbs (port of ui/surround/deck/* with
// js/deck_pad.js and js/deck_keys.js). A band under the chin in portrait, a column beside the
// picture in landscape. Every face drives **the session** over the input channel the picture
// already uses — never the trackpad's own link, which drives the computer's own desktop.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../../wado/host.dart';
import '../../../wado/input.dart';
import '../../../wado/pad.dart';
import '../../../wado/session.dart';
import '../../motion/motion.dart';
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';
import '../keyboard.dart' show KeyboardSheet;

/// The pointer the Touchpad face moves, normalised over the picture; null when the face is away.
/// The compositor draws no cursor, so the console draws one where this sent it (`#deck-cursor`).
final deckCursor = ValueNotifier<Offset?>(null);
/// Bumped on each click, for the cursor's tap pulse.
final deckTap = ValueNotifier<int>(0);

/// The active face, rising in on each change (`facein` / `faceinx`).
class Deck extends StatelessWidget {
  const Deck({super.key, required this.face, required this.col});
  final String face;
  final bool col;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final pad = face == 'pad';
    return Padding(
      padding: EdgeInsets.all(col ? 8 : 6),
      child: Enter(
        key: ValueKey(face),
        ms: col ? 420 : 360,
        dy: col ? 0 : 10,
        dx: col ? 10 : 0,
        fadeOnClose: false,
        child: pad
            ? Container(
                clipBehavior: Clip.antiAlias,
                decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2), border: Border.all(color: t.line)),
                child: PadFace(col: col),
              )
            : switch (face) {
                'keys' => KeysFace(col: col),
                'quick' => QuickFace(col: col),
                _ => const SizedBox.shrink(),
              },
      ),
    );
  }
}

// ── Touchpad ────────────────────────────────────────────────────────────────────────────

class _F {
  _F(this.at, this.t) : at0 = at, t0 = t;
  Offset at, at0;
  int t, t0;
}

class PadFace extends StatefulWidget {
  const PadFace({super.key, required this.col});
  final bool col;
  @override
  State<PadFace> createState() => _PadFaceState();
}

class _PadFaceState extends State<PadFace> {
  static const _tapMs = 220, _tapPx = 8.0;
  final _fingers = <int, _F>{};
  final _held = <String>{};
  bool _moved = false;
  int _max = 0;
  Offset? _scrollAt;
  final _sw = Stopwatch()..start();

  Offset get _pos => deckCursor.value ?? const Offset(.5, .5);
  Map<String, dynamic> get _pt => {'x': _pos.dx, 'y': _pos.dy};
  Input get I => Input.I;

  @override
  void initState() {
    super.initState();
    deckCursor.value ??= const Offset(.5, .5);
  }

  @override
  void dispose() {
    // A face change must not leave a button down on the computer.
    for (final b in _held) {
      I.send({'t': 'button', ..._pt, 'button': b, 'pressed': false});
    }
    deckCursor.value = null;
    super.dispose();
  }

  void _move(Offset d, int dt) {
    final c = I.content;
    if (c.isEmpty) return;
    // A slow slide is precise; a quick one crosses the picture. Gain on the finger's speed.
    final speed = d.distance / (dt < 1 ? 1 : dt);
    final gain = 1.3 + (speed * 1.1).clamp(0, 1.7);
    final p = _pos;
    deckCursor.value = Offset((p.dx + d.dx * gain / c.width).clamp(0, 1), (p.dy + d.dy * gain / c.height).clamp(0, 1));
    I.queue('pointer_motion', {'t': 'pointer_motion', ..._pt});
  }

  void _click(String b) {
    I.now({'t': 'button', ..._pt, 'button': b, 'pressed': true});
    I.send({'t': 'button', ..._pt, 'button': b, 'pressed': false});
    deckTap.value++;
  }

  void _down(PointerDownEvent e) {
    if (_fingers.isEmpty) {
      _moved = false;
      _max = 0;
    }
    _fingers[e.pointer] = _F(e.position, _sw.elapsedMilliseconds);
    if (_fingers.length > _max) _max = _fingers.length;
  }

  void _moveE(PointerMoveEvent e) {
    final f = _fingers[e.pointer];
    if (f == null) return;
    final now = _sw.elapsedMilliseconds;
    final d = e.position - f.at, dt = now - f.t;
    f.at = e.position;
    f.t = now;
    if ((e.position - f.at0).distance > _tapPx) _moved = true;
    if (!_moved) return;
    if (_fingers.length >= 2) {
      // Two fingers: scroll, once per pair — follow the first finger only.
      if (e.pointer != _fingers.keys.first) return;
      final c = I.content;
      final at = I.scrollBy(d.dx, d.dy, Offset(c.left + _pos.dx * c.width, c.top + _pos.dy * c.height));
      if (at != null) _scrollAt = at;
      return;
    }
    _move(d, dt);
  }

  void _up(PointerEvent e) {
    final f = _fingers.remove(e.pointer);
    if (f == null || _fingers.isNotEmpty) return;
    if (_scrollAt != null) {
      I.scrollStop(_scrollAt);
      _scrollAt = null;
      return;
    }
    if (!_moved && e is PointerUpEvent && _sw.elapsedMilliseconds - f.t0 < _tapMs * (_max > 1 ? 1.5 : 1)) _click(_max > 1 ? 'right' : 'left');
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    Widget zone(String b, String label) => Expanded(
          child: _Zone(
            label: label,
            onDown: () {
              _held.add(b);
              I.now({'t': 'button', ..._pt, 'button': b, 'pressed': true});
            },
            onUp: () {
              if (_held.remove(b)) I.now({'t': 'button', ..._pt, 'button': b, 'pressed': false});
            },
          ),
        );
    final line = BorderSide(color: t.line);
    return Column(children: [
      Expanded(
        child: Listener(
          behavior: HitTestBehavior.opaque,
          onPointerDown: _down,
          onPointerMove: _moveE,
          onPointerUp: _up,
          onPointerCancel: _up,
          child: Semantics(
            label: 'Touchpad: slide to move the pointer, tap to click, two fingers to scroll',
            child: Center(
              child: ExcludeSemantics(
                child: Column(mainAxisSize: MainAxisSize.min, children: [
                  Text('Touchpad', style: body(context, size: .7812, w: FontWeight.w600, color: t.text)),
                  Text('Slide to move · tap to click · two fingers to scroll', textAlign: TextAlign.center, style: body(context, size: .6875, color: t.dim)),
                ]),
              ),
            ),
          ),
        ),
      ),
      Container(
        decoration: BoxDecoration(border: Border(top: line)),
        // A narrow column stacks the zones, so each stays a full 44 px target.
        child: widget.col
            ? Column(children: [
                SizedBox(height: 44, child: Row(children: [zone('left', 'Left')])),
                Container(height: 1, color: t.line),
                SizedBox(height: 44, child: Row(children: [zone('right', 'Right')])),
              ])
            : SizedBox(height: 30, child: Row(children: [zone('left', 'Left'), Container(width: 1, color: t.line), zone('right', 'Right')])),
      ),
    ]);
  }
}

class _Zone extends StatefulWidget {
  const _Zone({required this.label, required this.onDown, required this.onUp});
  final String label;
  final VoidCallback onDown, onUp;
  @override
  State<_Zone> createState() => _ZoneState();
}

class _ZoneState extends State<_Zone> {
  bool _down = false;
  void _set(bool d) {
    if (d == _down) return;
    setState(() => _down = d);
    d ? widget.onDown() : widget.onUp();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Semantics(
      button: true,
      label: '${widget.label} click',
      child: Listener(
        behavior: HitTestBehavior.opaque,
        onPointerDown: (_) => _set(true),
        onPointerUp: (_) => _set(false),
        onPointerCancel: (_) => _set(false),
        child: AnimatedContainer(
          duration: t.dur,
          alignment: Alignment.center,
          color: _down ? t.s2 : Colors.transparent,
          child: Text(widget.label.toUpperCase(), style: body(context, size: .6875, w: FontWeight.w600, color: _down ? t.bright : t.dim).copyWith(letterSpacing: .04 * 11)),
        ),
      ),
    );
  }
}

/// The pointer the Touchpad moves, over the picture (`#deck-cursor`, its `dctap` pulse).
class DeckCursor extends StatefulWidget {
  const DeckCursor({super.key});
  @override
  State<DeckCursor> createState() => _DeckCursorState();
}

class _DeckCursorState extends State<DeckCursor> with SingleTickerProviderStateMixin {
  late final _tap = AnimationController(vsync: this, duration: const Duration(milliseconds: 320));
  @override
  void initState() {
    super.initState();
    deckTap.addListener(_pulse);
  }

  void _pulse() => _tap.forward(from: 0);

  @override
  void dispose() {
    deckTap.removeListener(_pulse);
    _tap.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ValueListenableBuilder(
      valueListenable: deckCursor,
      builder: (context, p, _) {
        final c = Input.I.content;
        final at = p == null ? null : Offset(c.left + p.dx * c.width, c.top + p.dy * c.height);
        return Stack(children: [
          if (at != null)
            Positioned(
              left: at.dx - 6,
              top: at.dy - 6,
              child: AnimatedBuilder(
                animation: _tap,
                builder: (context, child) {
                  final v = _tap.value;
                  final s = 1 - .4 * Tokens.popCurve.transform(v < .5 ? v * 2 : (1 - v) * 2).clamp(0, 1.2);
                  return Transform.scale(scale: s, child: child);
                },
                child: Container(
                  width: 12,
                  height: 12,
                  decoration: BoxDecoration(shape: BoxShape.circle, color: t.accent, boxShadow: [BoxShadow(color: t.s0, spreadRadius: 2)]),
                ),
              ),
            ),
        ]);
      },
    );
  }
}

// ── Keys ────────────────────────────────────────────────────────────────────────────────

/// Modifiers latch as the trackpad's do: tap → latched for the next key; tap again within 300 ms
/// → locked (held down on the computer until tapped, so Super + drag can move a window); tap a
/// latched or locked one → off.
class DeckKeys {
  DeckKeys._();
  static final I = DeckKeys._();
  static const mod = {'super': 125, 'ctrl': 29, 'alt': 56, 'shift': 42};
  static const key = {'esc': 1, 'tab': 15, 'left': 105, 'right': 106, 'up': 103, 'down': 108};
  final mods = ValueNotifier<Map<String, String>>({for (final m in mod.keys) m: 'off'});
  final _last = <String, int>{};
  final _sw = Stopwatch()..start();

  void _send(int code, bool pressed) => Input.I.now({'t': 'key', 'code': code, 'pressed': pressed});

  void tapMod(String name) {
    final now = _sw.elapsedMilliseconds;
    final m = {...mods.value};
    final st = m[name];
    if (st == 'latched' && now - (_last[name] ?? -1000) < 300) {
      m[name] = 'locked';
      _send(mod[name]!, true);
    } else if (st == 'off') {
      m[name] = 'latched';
    } else {
      if (st == 'locked') _send(mod[name]!, false);
      m[name] = 'off';
    }
    _last[name] = now;
    mods.value = m;
  }

  void tapKey(String name) {
    final code = key[name];
    if (code == null) return;
    final m = {...mods.value};
    final latched = [for (final e in m.entries) if (e.value == 'latched') e.key];
    for (final l in latched) {
      _send(mod[l]!, true);
    }
    _send(code, true);
    _send(code, false);
    for (final l in latched.reversed) {
      _send(mod[l]!, false);
      m[l] = 'off';
    }
    mods.value = m;
  }

  /// The face went away or the session ended: nothing stays held.
  void releaseAll() {
    final m = {...mods.value};
    for (final e in m.entries) {
      if (e.value == 'locked') _send(mod[e.key]!, false);
      m[e.key] = 'off';
    }
    mods.value = m;
  }
}

class KeysFace extends StatefulWidget {
  const KeysFace({super.key, required this.col});
  final bool col;
  @override
  State<KeysFace> createState() => _KeysFaceState();
}

class _KeysFaceState extends State<KeysFace> {
  @override
  void dispose() {
    DeckKeys.I.releaseAll();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final h = widget.col ? 44.0 : 34.0;
    return ValueListenableBuilder(
      valueListenable: DeckKeys.I.mods,
      builder: (context, mods, _) {
        Widget k(String label, VoidCallback tap, {String st = 'off', String? name}) => _Key(label: label, name: name ?? label, st: st, h: h, col: widget.col, onTap: tap);
        final keys = <Widget>[
          for (final (id, l) in const [('super', 'Super'), ('ctrl', 'Ctrl'), ('alt', 'Alt'), ('shift', 'Shift')]) k(l, () => DeckKeys.I.tapMod(id), st: mods[id]!),
          for (final (id, l, n) in const [('esc', 'Esc', 'Escape'), ('tab', 'Tab', 'Tab'), ('left', '←', 'Left arrow'), ('right', '→', 'Right arrow'), ('up', '↑', 'Up arrow'), ('down', '↓', 'Down arrow')])
            k(l, () => DeckKeys.I.tapKey(id), name: n),
        ];
        final type = Press(
          onTap: () => KeyboardSheet.toggle(context),
          scale: .94,
          label: 'Type on the computer',
          child: Container(
            height: widget.col ? 48 : 34,
            padding: const EdgeInsets.symmetric(horizontal: 12),
            decoration: BoxDecoration(color: t.accent, borderRadius: BorderRadius.circular(t.r2)),
            child: Row(mainAxisSize: widget.col ? MainAxisSize.max : MainAxisSize.min, mainAxisAlignment: MainAxisAlignment.center, children: [
              WIcon('kbd', size: 18, color: t.onAccent),
              const SizedBox(width: 6),
              Text('Type', style: body(context, w: FontWeight.w600, color: t.onAccent)),
            ]),
          ),
        );
        if (widget.col) {
          return SingleChildScrollView(
            child: Column(children: [
              for (var i = 0; i < keys.length; i += 2) ...[
                Row(children: [Expanded(child: keys[i]), const SizedBox(width: 6), Expanded(child: keys[i + 1])]),
                const SizedBox(height: 6),
              ],
              type,
            ]),
          );
        }
        return Center(child: Wrap(spacing: 6, runSpacing: 6, alignment: WrapAlignment.center, children: [...keys, type]));
      },
    );
  }
}

class _Key extends StatelessWidget {
  const _Key({required this.label, required this.name, required this.st, required this.h, required this.col, required this.onTap});
  final String label, name, st;
  final double h;
  final bool col;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final locked = st == 'locked', latched = st == 'latched';
    return Press(
      onTap: () {
        HapticFeedback.selectionClick();
        onTap();
      },
      scale: .92,
      label: name,
      child: AnimatedContainer(
        duration: t.dur,
        curve: Tokens.spring,
        height: h,
        constraints: BoxConstraints(minWidth: col ? 0 : 44),
        padding: EdgeInsets.symmetric(horizontal: col ? 0 : 12),
        decoration: BoxDecoration(
          color: locked ? t.accent : t.s2,
          borderRadius: BorderRadius.circular(locked ? 999 : t.r2),
          border: latched ? Border.all(color: t.accent, width: 2) : null,
        ),
        // `widthFactor` 1: a band's key is its label's width, not the band's.
        child: Center(widthFactor: col ? null : 1, child: Text(label, style: mono(context, size: .8125, w: FontWeight.w600, color: locked ? t.onAccent : (latched ? t.accent : t.bright)))),
      ),
    );
  }
}

// ── Quick actions ───────────────────────────────────────────────────────────────────────

class QuickFace extends StatelessWidget {
  const QuickFace({super.key, required this.col});
  final bool col;

  /// The player an action goes to: the one playing, else the first.
  static Map<String, dynamic>? player() {
    final media = (Host.I.state.value?['media'] as List?)?.cast<Map<String, dynamic>>() ?? const [];
    return media.where((p) => p['playing'] == true).firstOrNull ?? media.firstOrNull;
  }

  static void media(String op) {
    final p = player();
    if (p != null) Host.I.act({'do': 'media', 'bus': p['bus'], 'op': {'op': op}});
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([Session.I.workspace, Host.I.state, Host.I.config]),
      builder: (context, _) {
        final ws = Session.I.workspace.value < 1 ? 1 : Session.I.workspace.value;
        final now = player();
        final playing = now?['playing'] == true;
        // No grants from an older daemon: offer it and let the daemon say no.
        final granted = ((Host.I.config.value?['grants'] as Map?)?['host'] ?? true) == true;
        final acts = <Widget>[
          _Act(icon: 'left', label: 'Previous workspace', onTap: ws <= 1 ? null : () => Session.I.window({'workspace': {'n': ws - 1}})),
          _Act(icon: 'right', label: 'Next workspace', onTap: () => Session.I.window({'workspace': {'n': ws + 1}})),
          if (granted)
            _Act(icon: playing ? 'pause' : 'play', fill: true, label: 'Play / pause', onTap: now == null ? null : () => media('play_pause'))
          else
            Padding(
              padding: const EdgeInsets.all(4),
              child: Text('Playback needs the host grant. On the computer: wado allow <this device> host', style: body(context, size: .7188, color: t.dim).copyWith(height: 1.4)),
            ),
        ];
        if (!col) return Center(child: Row(children: [for (var i = 0; i < acts.length; i++) ...[if (i > 0) const SizedBox(width: 6), Expanded(child: acts[i])]]));
        return SingleChildScrollView(
          child: Column(children: [
            for (var i = 0; i < acts.length; i++) ...[if (i > 0) const SizedBox(height: 6), acts[i]],
            if (now != null && granted) _NpMini(p: now),
          ]),
        );
      },
    );
  }
}

class _Act extends StatelessWidget {
  const _Act({required this.icon, required this.label, required this.onTap, this.fill = false});
  final String icon, label;
  final bool fill;
  final VoidCallback? onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Opacity(
      opacity: onTap == null ? .4 : 1,
      child: Press(
        onTap: onTap,
        label: label,
        child: Container(
          constraints: const BoxConstraints(minHeight: 64),
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 8),
          decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r2)),
          child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
            WIcon(icon, size: 22, color: t.bright, fill: fill),
            const SizedBox(height: 4),
            Text(label, maxLines: 2, overflow: TextOverflow.ellipsis, textAlign: TextAlign.center, style: body(context, size: .75, w: FontWeight.w600, color: t.bright).copyWith(height: 1.25)),
          ]),
        ),
      ),
    );
  }
}

class _NpMini extends StatelessWidget {
  const _NpMini({required this.p});
  final Map<String, dynamic> p;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final art = p['art'] as String?;
    Widget cover = Container(width: 36, height: 36, alignment: Alignment.center, decoration: BoxDecoration(color: t.s3, borderRadius: BorderRadius.circular(8)), child: WIcon('play', size: 16, color: t.dim, fill: true));
    if (art != null && art.startsWith('data:')) {
      try {
        cover = ClipRRect(borderRadius: BorderRadius.circular(8), child: Image.memory(UriData.parse(art).contentAsBytes(), width: 36, height: 36, fit: BoxFit.cover, gaplessPlayback: true));
      } catch (_) {}
    }
    return Semantics(
      label: 'Now playing on the computer',
      child: Container(
        margin: const EdgeInsets.only(top: 10),
        padding: const EdgeInsets.all(8),
        decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r2)),
        child: Column(children: [
          Row(children: [
            cover,
            const SizedBox(width: 8),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text('${p['title'] ?? ''}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, w: FontWeight.w700, color: t.bright)),
                Text('${p['artist'] ?? ''}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .6875, color: t.dim)),
              ]),
            ),
          ]),
          const SizedBox(height: 4),
          Row(mainAxisAlignment: MainAxisAlignment.spaceAround, children: [
            IconBtn('prev-f', label: 'Previous track', color: t.bright, onTap: () => QuickFace.media('previous')),
            IconBtn('next-f', label: 'Next track', color: t.bright, onTap: () => QuickFace.media('next')),
          ]),
        ]),
      ),
    );
  }
}

// ── Gamepad ─────────────────────────────────────────────────────────────────────────────

/// The Gamepad face: a compact controller in the pad's box (the pad link's gamepad).
class GameFace extends StatefulWidget {
  const GameFace({super.key});
  @override
  State<GameFace> createState() => _GameFaceState();
}

class _GameFaceState extends State<GameFace> {
  @override
  void initState() {
    super.initState();
    if (Pad.I.state.value == 'off') Pad.I.start();
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
            width: 52,
            height: 52,
            alignment: Alignment.center,
            decoration: BoxDecoration(shape: BoxShape.circle, color: t.s2, border: Border.all(color: fade(c, 40), width: 1.5)),
            child: Text(label, style: TextStyle(fontFamily: Tokens.display, fontWeight: FontWeight.w700, fontSize: 17, color: c)),
          ),
        );
    Widget cluster(List<Widget> four) => Column(mainAxisSize: MainAxisSize.min, children: [
          four[0],
          Row(mainAxisSize: MainAxisSize.min, children: [four[1], const SizedBox(width: 52), four[2]]),
          four[3],
        ]);
    // Left groups left, right groups right (W.gpadPlace.deck).
    return Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
      cluster([b('▲', 0x220, t.bright), b('◀', 0x222, t.bright), b('▶', 0x223, t.bright), b('▼', 0x221, t.bright)]),
      cluster([b('Y', 0x134, t.base[0xA]), b('X', 0x133, t.base[0xD]), b('B', 0x131, t.base[8]), b('A', 0x130, t.base[0xB])]),
    ]);
  }
}
