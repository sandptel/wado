// Now playing (port of ui/computer/playing.rs + computer.css .np): art, app, title, artist, a
// running progress bar you can scrub, and the transport. Several players: a row each to pick.
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/data_image.dart';
import '../motion/motion.dart';
import 'quick.dart';

String clock(int ms) {
  final s = ms ~/ 1000;
  return '${s ~/ 60}:${(s % 60).toString().padLeft(2, '0')}';
}

Uint8List? dataUrl(String? u) {
  if (u == null || !u.contains('base64,')) return null;
  return dataImage(u)?.bytes;
}

class Playing extends StatefulWidget {
  const Playing({super.key, required this.h});
  final Map<String, dynamic> h;
  @override
  State<Playing> createState() => _PlayingState();
}

class _PlayingState extends State<Playing> with SingleTickerProviderStateMixin {
  String? pick;
  late final _tick = AnimationController(vsync: this, duration: const Duration(days: 1))..forward();
  DateTime _at = DateTime.now();
  int _lastPos = -1;

  @override
  void dispose() {
    _tick.dispose();
    super.dispose();
  }

  void media(String bus, Map<String, dynamic> op) => Host.I.act({'do': 'media', 'bus': bus, 'op': op});

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final g = (Host.I.config.value?['grants'] as Map<String, dynamic>?);
    if (g != null && g['host'] == false) {
      return _empty(context, 'Needs the host permission — see Quick below.');
    }
    final players = (widget.h['media'] as List? ?? []).cast<Map<String, dynamic>>();
    if (players.isEmpty) return _empty(context, 'Nothing is playing.');
    final p = players.where((x) => x['bus'] == pick).firstOrNull ?? players.where((x) => x['playing'] == true).firstOrNull ?? players.first;
    final pos = (p['position_ms'] ?? 0) as int, len = (p['length_ms'] ?? 0) as int;
    if (pos != _lastPos) {
      _lastPos = pos;
      _at = DateTime.now();
    }
    final playing = p['playing'] == true;
    final art = dataUrl(p['art'] as String?) ?? dataUrl(p['icon'] as String?);
    final s = speaker(widget.h);
    final bus = '${p['bus']}';
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (players.length > 1) ...[
        ListBox(children: [
          for (final o in players)
            Press(
              onTap: () => setState(() => pick = '${o['bus']}'),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
                child: Row(children: [
                  Expanded(
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Text('${o['app']}', style: body(context, w: FontWeight.w600, color: o['bus'] == p['bus'] ? t.accent : t.bright)),
                      Text('${o['title']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, color: t.dim)),
                    ]),
                  ),
                  IconBtn(o['playing'] == true ? 'pause' : 'play', label: o['playing'] == true ? 'Pause' : 'Play', onTap: () => media('${o['bus']}', {'op': 'play_pause'})),
                ]),
              ),
            ),
        ]),
        const SizedBox(height: 8),
      ],
      Container(
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(t.r4),
          gradient: LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: [mix(t.accent, t.s1, 70), mix(t.base[0xE], t.s1, 78)]),
        ),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            Container(
              width: 92,
              height: 92,
              clipBehavior: Clip.antiAlias,
              decoration: BoxDecoration(
                color: t.s2,
                borderRadius: BorderRadius.circular(t.r2),
                boxShadow: const [BoxShadow(color: Color(0x73000000), blurRadius: 24, offset: Offset(0, 8), spreadRadius: -6)],
              ),
              child: art != null
                  ? ArtSwap(child: Image(image: dataImage(p['art'] as String?) ?? dataImage(p['icon'] as String?) ?? MemoryImage(art), key: ValueKey(art.length), fit: BoxFit.cover, gaplessPlayback: true))
                  : Center(child: WIcon('music', size: 32, color: t.dim)),
            ),
            const SizedBox(width: 14),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text('${p['app']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .7188, w: FontWeight.w600)),
                // A new track's words swap in from 10 px below (`swapin`).
                Enter(
                  key: ValueKey('t${p['title']}'),
                  ms: 320,
                  dy: 10,
                  fadeOnClose: false,
                  child: Text('${p['title']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1.3125), ls: -.3).copyWith(height: 1.15)),
                ),
                if ('${p['artist'] ?? ''}'.isNotEmpty)
                  Enter(
                    key: ValueKey('a${p['artist']}'),
                    ms: 320,
                    dy: 10,
                    fadeOnClose: false,
                    child: Text('${p['artist']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .8125)),
                  ),
              ]),
            ),
          ]),
          if (len > 0) ...[
            const SizedBox(height: 12),
            AnimatedBuilder(
              animation: _tick,
              builder: (context, _) {
                final now = playing ? pos + DateTime.now().difference(_at).inMilliseconds : pos;
                final f = (now / len).clamp(0.0, 1.0);
                return Column(children: [
                  LayoutBuilder(builder: (context, box) {
                    return GestureDetector(
                      onTapUp: p['can_seek'] == true ? (d) => media(bus, {'op': 'seek_to', 'ms': (d.localPosition.dx / box.maxWidth * len).round()}) : null,
                      child: SizedBox(
                        height: 22,
                        child: Center(
                          child: ClipRRect(
                            borderRadius: BorderRadius.circular(2),
                            child: SizedBox(
                              height: 4,
                              child: Stack(children: [
                                Positioned.fill(child: ColoredBox(color: fade(t.bright, 76))),
                                FractionallySizedBox(widthFactor: f, child: ColoredBox(color: t.bright)),
                              ]),
                            ),
                          ),
                        ),
                      ),
                    );
                  }),
                  Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
                    Text(clock(now.clamp(0, len)), style: mono(context, size: .6562)),
                    Text(clock(len), style: mono(context, size: .6562)),
                  ]),
                ]);
              },
            ),
          ],
          const SizedBox(height: 12),
          Row(children: [
            Flexible(
              child: Container(
                constraints: const BoxConstraints(minHeight: 40),
                padding: const EdgeInsets.symmetric(horizontal: 12),
                decoration: BoxDecoration(color: fade(t.bright, 86), borderRadius: BorderRadius.circular(999)),
                child: Row(mainAxisSize: MainAxisSize.min, children: [
                  WIcon('sound', size: 18, color: t.bright),
                  const SizedBox(width: 6),
                  Flexible(child: Text('${s?['label'] ?? 'Speakers'}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .7812, w: FontWeight.w600, color: t.bright))),
                ]),
              ),
            ),
            const Spacer(),
            IconBtn('prev-f', label: 'Previous', color: t.bright, onTap: p['can_prev'] == true ? () => media(bus, {'op': 'previous'}) : null),
            Press(
              onTap: () => media(bus, {'op': 'play_pause'}),
              label: playing ? 'Pause' : 'Play',
              child: Container(
                width: 52,
                height: 52,
                decoration: BoxDecoration(shape: BoxShape.circle, color: t.bright),
                alignment: Alignment.center,
                // The glyph swaps with a pop: the new one comes in from .4 (`glyphin`).
                child: Enter(key: ValueKey(playing), ms: 260, dy: 0, scale: .4, pop: true, fadeOnClose: false, child: WIcon(playing ? 'pause' : 'play', fill: true, color: t.s0)),
              ),
            ),
            IconBtn('next-f', label: 'Next', color: t.bright, onTap: p['can_next'] == true ? () => media(bus, {'op': 'next'}) : null),
          ]),
        ]),
      ),
    ]);
  }

  Widget _empty(BuildContext context, String text) {
    final t = Tok.of(context);
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
      child: Row(children: [WIcon('music', color: t.dim), const SizedBox(width: 12), Expanded(child: Text(text, style: body(context, size: .8125, color: t.dim)))]),
    );
  }
}
