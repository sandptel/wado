// The first-run tour: four cards right after the first pairing, then out of the way. Skippable;
// shown once (tips_seen `tour`). Each card's picture is drawn from the app's own pieces.
import 'package:flutter/material.dart';

import '../play/poster.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../../wado/ux.dart';
import 'tips.dart';

void tourOnce(BuildContext context, String computer) {
  if (seen('tour')) return;
  markSeen('tour');
  Navigator.of(context, rootNavigator: true).push(PageRouteBuilder<void>(
    opaque: true,
    transitionDuration: Tok.of(context).durSheet,
    pageBuilder: (_, _, _) => _Tour(computer: computer),
    transitionsBuilder: (_, a, _, child) => FadeTransition(opacity: CurvedAnimation(parent: a, curve: Tokens.spring), child: child),
  ));
}

class _Tour extends StatefulWidget {
  const _Tour({required this.computer});
  final String computer;
  @override
  State<_Tour> createState() => _TourState();
}

class _TourState extends State<_Tour> {
  final _pc = PageController();
  int _i = 0;

  @override
  void dispose() {
    _pc.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = widget.computer;
    // The shared cards (`wado_ux::tour`), worded for the form in hand.
    final size = MediaQuery.sizeOf(context);
    final raw = ux('tour', {'computer': c, 'form': ux('form', {'w': size.width, 'h': size.height, 'touch': true})});
    final cards = [
      for (final m in (raw as List? ?? const []))
        if (m is Map)
          ((m['hue'] as num? ?? 13).toInt(), '${m['title']}', '${m['words']}', m['art'] == 'shelf' ? _Shelf() : _Big('${m['art']}') as Widget),
    ];
    final last = _i == cards.length - 1;
    return Scaffold(
      backgroundColor: t.s0,
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(22, 24, 22, 18),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Expanded(
              child: PageView(
                controller: _pc,
                onPageChanged: (i) => setState(() => _i = i),
                children: [
                  for (final (hue, title, words, art) in cards)
                    Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                      Expanded(child: Container(decoration: wash(t, hue, t.r4), alignment: Alignment.center, child: art)),
                      const SizedBox(height: 22),
                      Text(title, style: display(context, 26, ls: -.8, color: t.bright)),
                      const SizedBox(height: 8),
                      Text(words, style: body(context, color: t.dim)),
                    ]),
                ],
              ),
            ),
            const SizedBox(height: 18),
            Row(mainAxisAlignment: MainAxisAlignment.center, children: [
              for (var i = 0; i < cards.length; i++)
                AnimatedContainer(
                  duration: t.dur,
                  curve: Tokens.spring,
                  width: i == _i ? 22 : 7,
                  height: 7,
                  margin: const EdgeInsets.symmetric(horizontal: 3),
                  decoration: BoxDecoration(color: i == _i ? t.accent : t.s3, borderRadius: BorderRadius.circular(4)),
                ),
            ]),
            const SizedBox(height: 16),
            Row(children: [
              Expanded(child: Btn('Skip', onTap: () => Navigator.of(context).pop())),
              const SizedBox(width: 8),
              Expanded(
                flex: 2,
                child: Btn(last ? 'Start' : 'Next', primary: true, onTap: () {
                  if (last) return Navigator.of(context).pop();
                  _pc.nextPage(duration: t.durSheet, curve: Tokens.spring);
                }),
              ),
            ]),
          ]),
        ),
      ),
    );
  }
}

class _Shelf extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Transform.rotate(
      angle: -.14,
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        // Shapes, not titles: this computer's real games aren't known yet.
        for (final (hue, icon) in const [(0xD, 'pad'), (0xB, 'apps'), (0x9, 'play')])
          Container(
            width: 70,
            height: 94,
            margin: const EdgeInsets.symmetric(horizontal: 5),
            decoration: wash(t, hue, 16),
            alignment: Alignment.center,
            child: WIcon(icon, size: 30, color: t.bright),
          ),
      ]),
    );
  }
}

class _Big extends StatelessWidget {
  const _Big(this.icon);
  final String icon;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Container(
      width: 108,
      height: 108,
      decoration: BoxDecoration(color: fade(t.base[0], 30), shape: BoxShape.circle),
      alignment: Alignment.center,
      child: icon == '?' ? Text('?', style: display(context, 54, color: t.accent)) : WIcon(icon, size: 52, color: t.bright),
    );
  }
}
