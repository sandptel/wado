// The touchpad (port of pad_ui.js + pad_touch.js + pad_why.js + pad.css): a full-screen
// surface whose fingers go to the computer as a real multi-touch touchpad (libinput does the
// pointer, scrolling and gestures there). A ring under each finger. The keys rail beside it, the
// phone keyboard on demand, and the honest why-card when the computer says no.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../wado/pad.dart';
import '../../wado/relay_link.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'gamepad_screen.dart';

void openPad(BuildContext context, {bool keyboard = false}) {
  Navigator.of(context).push(PageRouteBuilder(
    opaque: true,
    transitionDuration: Tok.of(context).durSheet,
    pageBuilder: (_, a, _) => FadeTransition(opacity: a, child: PadScreen(keyboard: keyboard)),
  ));
}

void openGamepad(BuildContext context) {
  Navigator.of(context).push(PageRouteBuilder(
    opaque: true,
    transitionDuration: Tok.of(context).durSheet,
    pageBuilder: (_, a, _) => FadeTransition(opacity: a, child: const GamepadScreen()),
  ));
}

class PadScreen extends StatefulWidget {
  const PadScreen({super.key, this.keyboard = false});
  final bool keyboard;
  @override
  State<PadScreen> createState() => _PadScreenState();
}

class _PadScreenState extends State<PadScreen> {
  final _rings = <int, Offset>{};
  late bool _kb = widget.keyboard;
  final _mods = <String>{};
  final _field = TextEditingController(text: '​');
  final _focus = FocusNode();

  @override
  void initState() {
    super.initState();
    SystemChrome.setEnabledSystemUIMode(SystemUiMode.immersiveSticky);
    Pad.I.start();
    PadTouch.I.attach();
    if (_kb) WidgetsBinding.instance.addPostFrameCallback((_) => _focus.requestFocus());
  }

  @override
  void dispose() {
    PadTouch.I.detach();
    Pad.I.stop();
    SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
    super.dispose();
  }

  void _typed(String v) {
    final mods = _mods.toList();
    if (v.isEmpty) {
      PadKeys.key('backspace');
    } else {
      final s = v.replaceAll('​', '');
      if (s.length == 1) {
        PadKeys.char(s, mods);
      } else if (s.isNotEmpty) {
        PadKeys.text(s);
      }
      if (mods.isNotEmpty) setState(_mods.clear);
    }
    _field.value = const TextEditingValue(text: '​', selection: TextSelection.collapsed(offset: 1));
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final marks = Settings.I.b('pad_marks_off') == false;
    return Scaffold(
      backgroundColor: t.s0,
      body: SafeArea(
        child: Column(children: [
          // Top: back, link chip.
          SizedBox(
            height: 48,
            child: Row(children: [
              IconBtn('left', label: 'Back', color: t.bright, onTap: () => Navigator.of(context).maybePop()),
              Expanded(child: Text('Touchpad', style: display(context, rem(context, 1)))),
              ListenableBuilder(
                listenable: Listenable.merge([Pad.I.state, Pad.I.rtt]),
                builder: (context, _) {
                  final s = Pad.I.state.value;
                  final r = Pad.I.rtt.value;
                  final c = s == 'direct' ? t.good : (s == 'relayed' ? t.warn : t.dim);
                  return Container(
                    margin: const EdgeInsets.only(right: 8),
                    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                    decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(999), border: Border.all(color: fade(c, 60))),
                    child: Row(children: [
                      Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: c)),
                      const SizedBox(width: 6),
                      Text('${s == 'direct' ? 'Direct' : s == 'relayed' ? 'Relayed' : 'Off'}${r != null ? ' · $r ms' : ''}', style: body(context, size: .75, w: FontWeight.w600, color: t.bright)),
                    ]),
                  );
                },
              ),
              IconBtn('kbd', label: 'Keyboard', color: _kb ? t.accent : t.bright, onTap: () {
                setState(() => _kb = !_kb);
                _kb ? _focus.requestFocus() : _focus.unfocus();
              }),
            ]),
          ),
          // The surface.
          Expanded(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(8, 0, 8, 8),
              child: ValueListenableBuilder(
                valueListenable: Pad.I.error,
                builder: (context, err, _) {
                  if (err != null && err.isNotEmpty) return _WhyCard(err: err);
                  return LayoutBuilder(builder: (context, box) {
                    final w = box.maxWidth, h = box.maxHeight;
                    final dpr = MediaQuery.of(context).devicePixelRatio;
                    PadTouch.I.size((w * dpr).round(), (h * dpr).round());
                    return Listener(
                      behavior: HitTestBehavior.opaque,
                      onPointerDown: (e) {
                        PadTouch.I.down(e.pointer, e.localPosition.dx, e.localPosition.dy, w, h);
                        if (marks) setState(() => _rings[e.pointer] = e.localPosition);
                      },
                      onPointerMove: (e) {
                        PadTouch.I.move(e.pointer, e.localPosition.dx, e.localPosition.dy, w, h);
                        if (marks) setState(() => _rings[e.pointer] = e.localPosition);
                      },
                      onPointerUp: (e) {
                        PadTouch.I.up(e.pointer);
                        if (marks) setState(() => _rings.remove(e.pointer));
                      },
                      onPointerCancel: (e) {
                        PadTouch.I.up(e.pointer);
                        setState(() => _rings.remove(e.pointer));
                      },
                      child: CustomPaint(
                        painter: _Surface(t, _rings.values.toList()),
                        child: const SizedBox.expand(),
                      ),
                    );
                  });
                },
              ),
            ),
          ),
          // The keys rail.
          _KeysRail(mods: _mods, onMods: () => setState(() {})),
          if (_kb)
            SizedBox(
              height: 1,
              child: Opacity(
                opacity: 0,
                child: TextField(controller: _field, focusNode: _focus, autocorrect: true, onChanged: _typed, onSubmitted: (_) {
                  PadKeys.key('enter');
                  _focus.requestFocus();
                }),
              ),
            ),
        ]),
      ),
    );
  }
}

class _Surface extends CustomPainter {
  _Surface(this.t, this.rings);
  final Tokens t;
  final List<Offset> rings;
  @override
  void paint(Canvas canvas, Size s) {
    final r = RRect.fromRectAndRadius(Offset.zero & s, Radius.circular(t.r4));
    canvas.drawRRect(r, Paint()..color = t.s1);
    // A quiet dot texture.
    final dot = Paint()..color = fade(t.bright, 94);
    for (var y = 24.0; y < s.height; y += 28) {
      for (var x = 24.0; x < s.width; x += 28) {
        canvas.drawCircle(Offset(x, y), 1.1, dot);
      }
    }
    canvas.drawRRect(r, Paint()
      ..style = PaintingStyle.stroke
      ..color = t.line);
    for (final p in rings) {
      canvas.drawCircle(p, 30, Paint()..color = fade(t.accent, 80));
      canvas.drawCircle(p, 30, Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2
        ..color = t.accent);
    }
  }

  @override
  bool shouldRepaint(_Surface o) => true;
}

class _KeysRail extends StatelessWidget {
  const _KeysRail({required this.mods, required this.onMods});
  final Set<String> mods;
  final VoidCallback onMods;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    Widget key(String label, VoidCallback onTap, {bool on = false}) => Expanded(
          child: Padding(
            padding: const EdgeInsets.all(3),
            child: Press(
              onTap: onTap,
              label: label,
              child: Container(
                height: 44,
                alignment: Alignment.center,
                decoration: BoxDecoration(color: on ? t.accent : t.s2, borderRadius: BorderRadius.circular(t.r1)),
                child: Text(label, style: body(context, size: .8125, w: FontWeight.w600, color: on ? t.onAccent : t.bright)),
              ),
            ),
          ),
        );
    Widget mod(String name, String label) => key(label, () {
          mods.contains(name) ? mods.remove(name) : mods.add(name);
          onMods();
        }, on: mods.contains(name));
    return Padding(
      padding: const EdgeInsets.fromLTRB(5, 0, 5, 6),
      child: Column(children: [
        Row(children: [
          key('Esc', () => PadKeys.key('escape')),
          key('Tab', () => PadKeys.key('tab', mods.toList())),
          mod('ctrl', 'Ctrl'),
          mod('alt', 'Alt'),
          mod('super', 'Super'),
          mod('shift', 'Shift'),
        ]),
        Row(children: [
          key('←', () => PadKeys.key('left', mods.toList())),
          key('↑', () => PadKeys.key('up', mods.toList())),
          key('↓', () => PadKeys.key('down', mods.toList())),
          key('→', () => PadKeys.key('right', mods.toList())),
          key('Del', () => PadKeys.key('delete', mods.toList())),
          key('⏎', () => PadKeys.key('enter', mods.toList())),
        ]),
      ]),
    );
  }
}

class _WhyCard extends StatelessWidget {
  const _WhyCard({required this.err});
  final String err;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final w = padWhy(err);
    return SingleChildScrollView(
      child: WCard(children: [
        Text(w.title, style: display(context, rem(context, 1.125))),
        Text(w.body, style: body(context)),
        for (final c in w.cmds)
          Container(
            padding: const EdgeInsets.all(10),
            decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r1)),
            child: Row(children: [
              Expanded(child: SelectableText(c, style: mono(context, color: t.bright))),
              IconBtn('copy', label: 'Copy', onTap: () => Clipboard.setData(ClipboardData(text: c))),
            ]),
          ),
        if (w.alt.isNotEmpty) Why(w.alt),
        Btn('Try again', primary: true, onTap: () {
          Pad.I.stop();
          Pad.I.start();
        }),
        if (!RelayLink.I.up.value) const Why('The link to the computer is down — it reconnects by itself.', tone: 'bad'),
      ]),
    );
  }
}
