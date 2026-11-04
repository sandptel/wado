// Just-in-time hints: each shown once, the first time it is relevant, then never again unless the
// person brings them back (Appearance → Show tips again). And the ? button that opens the overlay.
import 'dart:async';

import 'package:flutter/material.dart';

import '../../wado/pad.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/sheet.dart';
import 'cheat_sheet.dart';

bool seen(String id) => Settings.I.get<List<String>>('tips_seen').contains(id);
void markSeen(String id) {
  if (!seen(id)) Settings.I.set('tips_seen', [...Settings.I.get<List<String>>('tips_seen'), id]);
}

/// A 32 px "?" that lays the overlay over this surface.
class HelpBtn extends StatelessWidget {
  const HelpBtn({super.key, required this.surface});
  final String surface;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Press(
      onTap: () {
        markSeen('help');
        showCheatSheet(context, surface);
      },
      label: 'What can I do here?',
      child: Container(
        width: 32,
        height: 32,
        decoration: BoxDecoration(color: fade(t.s1, 20), shape: BoxShape.circle, border: Border.all(color: fade(t.line, 30))),
        child: Center(child: Text('?', style: display(context, 15, color: t.accent))),
      ),
    );
  }
}

/// A once-only bubble: shown while [id] is unseen, gone on "Got it" or after 10 s.
class Tip extends StatefulWidget {
  const Tip({super.key, required this.id, required this.title, required this.words});
  final String id, title, words;
  @override
  State<Tip> createState() => _TipState();
}

class _TipState extends State<Tip> {
  late bool _on = !seen(widget.id);
  Timer? _t;
  @override
  void initState() {
    super.initState();
    if (_on) _t = Timer(const Duration(seconds: 10), _done);
  }

  void _done() {
    markSeen(widget.id);
    if (mounted) setState(() => _on = false);
  }

  @override
  void dispose() {
    _t?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return AnimatedSwitcher(
      duration: t.dur,
      switchInCurve: Tokens.spring,
      transitionBuilder: (c, a) => FadeTransition(opacity: a, child: ScaleTransition(scale: Tween(begin: .94, end: 1.0).animate(a), alignment: Alignment.topLeft, child: c)),
      child: !_on
          ? const SizedBox.shrink()
          : Container(
              key: const ValueKey(1),
              constraints: const BoxConstraints(maxWidth: 250),
              padding: const EdgeInsets.fromLTRB(12, 10, 12, 4),
              decoration: BoxDecoration(
                color: t.accent,
                borderRadius: BorderRadius.circular(t.r2),
                boxShadow: const [BoxShadow(color: Color(0xAA000000), blurRadius: 50, offset: Offset(0, 18), spreadRadius: -20)],
              ),
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                Text(widget.title, style: body(context, size: .8125, w: FontWeight.w700, color: t.onAccent)),
                Text(widget.words, style: body(context, size: .78, color: t.onAccent, height: 1.35)),
                Press(
                  onTap: _done,
                  label: 'Got it',
                  child: SizedBox(height: 36, child: Align(alignment: Alignment.centerLeft, child: Text('Got it', style: body(context, size: .75, w: FontWeight.w700, color: t.onAccent)))),
                ),
              ]),
            ),
    );
  }
}

/// The pad link refused: say so on the pad itself, with the why one tap away — never silence.
class PadRefused extends StatelessWidget {
  const PadRefused({super.key});
  @override
  Widget build(BuildContext context) => ValueListenableBuilder<String?>(
        valueListenable: Pad.I.error,
        builder: (context, err, _) {
          if (err == null || err.isEmpty) return const SizedBox.shrink();
          final t = Tok.of(context);
          final w = padWhy(err);
          final title = w.title.replaceAll('trackpad', 'gamepad');
          return Center(
            child: Press(
              onTap: () => Locked(
                title,
                w.body.replaceAll('trackpad', 'gamepad'),
                fix: w.cmds.isEmpty ? null : '${w.cmds.join('\n')}${w.alt.isEmpty ? '' : '\n\n${w.alt}'}',
                details: err,
              ).show(context),
              label: '$title. Why?',
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(999), border: Border.all(color: fade(t.base[0xA], 40))),
                child: Row(mainAxisSize: MainAxisSize.min, children: [
                  WIcon('lock', size: 16, color: t.base[0xA]),
                  const SizedBox(width: 8),
                  Flexible(child: Text('$title · Why?', style: body(context, size: .8125, w: FontWeight.w600, color: t.bright))),
                ]),
              ),
            ),
          );
        },
      );
}
