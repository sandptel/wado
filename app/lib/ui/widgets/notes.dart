// Brief notes over everything that never move it (port of ui/toast.rs + layout.css `.toast`),
// and prompts as a bottom sheet (js/sheet.js on stage.css `.wado-menusheet`). One host, above the
// session too.
import 'dart:async';

import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../../wado/multi.dart';
import '../../wado/session.dart';
import '../share/inbound.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import 'basics.dart';

class _Toast {
  _Toast(this.key, this.icon, this.text, {this.tone = ''});
  final String key, icon, text, tone;
}

class Notes extends StatefulWidget {
  const Notes({super.key});
  @override
  State<Notes> createState() => _NotesState();
}

class _NotesState extends State<Notes> {
  final _shown = <_Toast>[];
  StreamSubscription? _sub;
  int _n = 0;
  String _swFor = '';

  @override
  void initState() {
    super.initState();
    _sub = Multi.I.notes.stream.listen((t) => _add(_Toast('n${_n++}', 'info', t)));
    Multi.I.ask.addListener(_ask);
    // Software encoding is said once at the start of every session that has it (invariant #5).
    Session.I.encoder.addListener(_encoder);
    Host.I.state.addListener(_config);
  }

  @override
  void dispose() {
    _sub?.cancel();
    Multi.I.ask.removeListener(_ask);
    Session.I.encoder.removeListener(_encoder);
    Host.I.state.removeListener(_config);
    super.dispose();
  }

  void _encoder() {
    final (mode, pipe) = Session.I.encoder.value;
    if (mode != 'software' || '$mode$pipe' == _swFor) return;
    _swFor = '$mode$pipe';
    _add(_Toast('sw${_n++}', 'alert', 'Software encoding — no working GPU encoder. Higher CPU use and latency.', tone: 'warn'));
  }

  String _cfgErr = '';
  void _config() {
    final e = (Host.I.state.value?['error'] ?? '') as String;
    if (e.isEmpty || e == _cfgErr) return;
    _cfgErr = e;
    _add(_Toast('cfg${_n++}', 'alert', 'config.kdl not applied: $e', tone: 'bad'));
  }

  void _add(_Toast t) {
    if (!mounted) return;
    setState(() => _shown.add(t));
    Timer(const Duration(seconds: 7), () {
      if (mounted) setState(() => _shown.remove(t));
    });
  }

  void _ask() {
    final a = Multi.I.ask.value;
    if (a == null) return;
    Multi.I.ask.value = null;
    final ctx = rootNav.currentContext;
    if (ctx == null) return;
    askSheet(ctx, a);
  }

  @override
  Widget build(BuildContext context) {
    final mq = MediaQuery.of(context);
    return IgnorePointer(
      child: Align(
        alignment: Alignment.topCenter,
        child: Padding(
          padding: EdgeInsets.only(top: 14 + mq.padding.top, left: 16, right: 16),
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 440),
            child: Column(mainAxisSize: MainAxisSize.min, children: [
              for (final t in _shown) Padding(key: ValueKey(t.key), padding: const EdgeInsets.only(bottom: 8), child: _ToastView(t)),
            ]),
          ),
        ),
      ),
    );
  }
}

/// `@keyframes toast`: in from 12 px up over 6% of 7 s, held to 85%, out the same way.
class _ToastView extends StatefulWidget {
  const _ToastView(this.t);
  final _Toast t;
  @override
  State<_ToastView> createState() => _ToastViewState();
}

class _ToastViewState extends State<_ToastView> with SingleTickerProviderStateMixin {
  late final _c = AnimationController(vsync: this, duration: const Duration(seconds: 7))..forward();
  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final w = widget.t;
    final iconColor = switch (w.tone) { 'warn' => t.base[0xA], 'bad' => t.bad, _ => t.bright };
    return AnimatedBuilder(
      animation: _c,
      builder: (context, child) {
        final v = _c.value;
        final double p = v < .06 ? Tokens.spring.transform(v / .06) : (v < .85 ? 1.0 : 1 - Tokens.spring.transform((v - .85) / .15));
        return Opacity(opacity: p.clamp(0, 1), child: Transform.translate(offset: Offset(0, -12 * (1 - p)), child: child));
      },
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
        decoration: BoxDecoration(
          color: t.s1,
          borderRadius: BorderRadius.circular(t.r2),
          border: Border.all(color: w.tone == 'warn' ? fade(t.base[0xA], 40) : t.line),
          boxShadow: const [BoxShadow(color: Color(0x59000000), blurRadius: 30, offset: Offset(0, 10))],
        ),
        child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          WIcon(w.icon, color: iconColor),
          const SizedBox(width: 10),
          Expanded(child: Text(w.text, style: body(context, size: .8125, color: t.bright))),
        ]),
      ),
    );
  }
}

/// js/sheet.js `ask`: a title, then a 48 px row per button; the primary one is the centred,
/// filled "cancel" row at the bottom. Rises 24 px in 160 ms.
Future<void> askSheet(BuildContext context, Ask a) {
  final t = Tok.of(context);
  return showModalBottomSheet(
    context: context,
    backgroundColor: fade(t.base[0x0], 6),
    barrierColor: Colors.black38,
    sheetAnimationStyle: AnimationStyle(duration: const Duration(milliseconds: 160), curve: Curves.easeOut),
    shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(16))),
    builder: (ctx) => SafeArea(
      top: false,
      child: Padding(
        padding: const EdgeInsets.all(8),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 10, 16, 8),
            child: Text(a.title, style: body(ctx, size: 1, w: FontWeight.w600, color: t.base[0x5], height: 1.35)),
          ),
          for (final (label, primary, run) in a.buttons)
            Padding(
              padding: EdgeInsets.only(top: primary ? 6 : 2),
              child: Material(
                color: primary ? t.base[0x1] : Colors.transparent,
                borderRadius: BorderRadius.circular(10),
                child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  highlightColor: t.base[0x2],
                  splashFactory: NoSplash.splashFactory,
                  onTap: () {
                    Navigator.of(ctx).pop();
                    run?.call();
                  },
                  child: Container(
                    constraints: const BoxConstraints(minHeight: 48),
                    padding: const EdgeInsets.symmetric(horizontal: 16),
                    alignment: primary ? Alignment.center : Alignment.centerLeft,
                    child: Text(label, style: body(ctx, size: 1, w: primary ? FontWeight.w600 : FontWeight.w400, color: t.base[0x5])),
                  ),
                ),
              ),
            ),
        ]),
      ),
    ),
  );
}
