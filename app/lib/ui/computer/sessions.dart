// The Computer page's Sessions section (port of ui/computer/sessions.rs + session_row.rs +
// linkmeter.rs + sessions.css): every session this computer runs, with who is in each — and the
// way into each: Rejoin, Join (everyone controls it at once), Take over (it becomes yours, at your
// size; the others are told), End (for everyone). Then Start new, and the link test that says
// whether any of it would work from here.
//
// Motion, heavy and deliberate: rows rise in a long stagger on the spring; each row's device
// chips pop in after it lands, one by one; the meter sweeps while it measures (a clock: linear),
// then its fill settles slowly and the capacity pips light in sequence.
import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../../wado/multi.dart';
import '../../wado/reach.dart';
import '../../wado/relay_link.dart';
import '../../wado/session.dart';
import '../home/running.dart';
import '../motion/motion.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';

/// multi.rs `offer`: what this device may do with a session.
({bool rejoin, bool join, bool takeover, bool end, String whyNot}) offer(Map<String, dynamic> s, String here, bool inIt, bool rooms, (bool, bool)? granted) {
  final (mayJoin, mayTake) = granted ?? (true, true);
  final parts = (s['participants'] as List? ?? []);
  final max = (s['max_participants'] ?? 0) as int;
  final full = max > 0 && parts.length >= max;
  final mine = s['instance'] == here;
  final nobody = parts.isEmpty && s['viewer'] == null;
  final joinable = max > 0;
  if (inIt || nobody || (mine && !joinable)) return (rejoin: true, join: false, takeover: false, end: true, whyNot: '');
  return (
    rejoin: false,
    join: rooms && joinable && mayJoin && !full,
    takeover: mayTake,
    end: true,
    whyNot: !joinable
        ? "its computer's wado is older than joining — take it over instead"
        : !rooms
            ? 'the relay is older than joining — take it over instead'
            : !mayJoin
                ? "this device may not join others' sessions — on the computer: wado allow <device> join"
                : full
                    ? 'it is full — the most devices it carries are in'
                    : '',
  );
}

/// A device's colour (its participant hue, 0..8) as a base16 slot, as its dot over the picture.
Color hueOf(BuildContext context, int h) => Tok.of(context).base[const [8, 9, 0xA, 0xB, 0xC, 0xD, 0xE, 0xF][h % 8]];

/// "Pixel and Laptop", "Pixel, Laptop and 2 more".
String names(List<String> l) => switch (l.length) {
      0 => 'nobody',
      1 => l[0],
      2 => '${l[0]} and ${l[1]}',
      _ => '${l[0]}, ${l[1]} and ${l.length - 2} more',
    };

class Sessions extends StatefulWidget {
  const Sessions({super.key, required this.r});
  final Reach r;
  @override
  State<Sessions> createState() => _SessionsState();
}

class _SessionsState extends State<Sessions> {
  @override
  void initState() {
    super.initState();
    // Asked while this page is up and no session is on screen — the list is looked at here.
    Host.I.sessionsWatch(true);
  }

  @override
  void dispose() {
    Host.I.sessionsWatch(false);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final m = Multi.I;
    return ListenableBuilder(
      listenable: Listenable.merge([Host.I.sessions, m.list, m.you, m.busy, m.testing, m.verdict, RelayLink.I.caps]),
      builder: (context, _) {
        final list = Host.I.sessions.value;
        final here = RelayLink.I.pool.value.instance;
        final g = Host.I.config.value?['grants'] as Map<String, dynamic>?;
        final granted = g == null ? null : (g['join'] == true, g['takeover'] == true);
        final inSession = m.list.value.isNotEmpty;
        final starting = m.busy.value.$2 == 'start';
        final host = m.verdict.value?['host'] as Map<String, dynamic>?;
        final maxS = (host?['max_sessions'] ?? 0) as int;
        final free = maxS > 0 ? (maxS - ((host?['sessions'] ?? 0) as int)).clamp(0, maxS) : null;
        final full = free == 0;
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Sect('Sessions', trailing: list.isEmpty ? null : _Count(list.length)),
          const SizedBox(height: 8),
          if (!widget.r.online)
            _Empty(icon: 'monitor', text: 'Sessions show once it answers.')
          else ...[
            if (list.isEmpty)
              Enter(
                delay: const Duration(milliseconds: 60),
                ms: 520,
                dy: 22,
                scale: .985,
                child: Container(
                  padding: const EdgeInsets.all(14),
                  decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
                  child: Row(children: [
                    for (final (h, d) in [(26.0, 0), (18.0, 300), (22.0, 600)])
                      Padding(
                        padding: const EdgeInsets.only(right: 4),
                        child: Breathe(delay: d, child: Container(width: 10, height: h, decoration: BoxDecoration(color: t.s3, borderRadius: BorderRadius.circular(4)))),
                      ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text("Nothing running. A session starts at this screen's size; others can join it once it runs.", style: body(context, size: .8125, color: t.dim)),
                    ),
                  ]),
                ),
              ),
            for (var i = 0; i < list.length; i++) ...[
              () {
                final s = list[i];
                final you = s['instance'] == here && inSession ? m.you.value : 0;
                final inIt = you != 0 && (s['participants'] as List? ?? []).any((p) => (p as Map)['id'] == you);
                final busy = m.busy.value.$1 == s['instance'] ? m.busy.value.$2 : '';
                return SessionRow(key: ValueKey(s['instance']), s: s, o: offer(s, here, inIt, m.rooms, granted), i: i, you: you, busy: busy);
              }(),
              const SizedBox(height: 8),
            ],
            LinkMeter(intent: list.isEmpty ? 'start' : 'join'),
            const SizedBox(height: 8),
            Press(
              onTap: starting || full ? null : m.start,
              enabled: !(starting || full),
              label: 'Start a new session',
              child: Container(
                constraints: const BoxConstraints(minHeight: 48),
                padding: const EdgeInsets.symmetric(horizontal: 16),
                decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r2)),
                child: Row(children: [
                  WIcon('plus', color: t.bright),
                  const SizedBox(width: 8),
                  Text(starting ? 'Finding a place for it…' : (full ? 'Every session it runs is in use' : 'Start a new session'), style: body(context, w: FontWeight.w600, color: t.bright)),
                  const Spacer(),
                  if (free != null && !full && !starting) Text('$free of $maxS free', style: mono(context, size: .6875, color: fade(t.bright, 40))),
                ]),
              ),
            ),
            if (!m.rooms && list.isNotEmpty) ...[
              const SizedBox(height: 8),
              _Why('This relay is older than joining: one device per session, and Take over moves it to you.'),
            ],
          ],
        ]);
      },
    );
  }
}

class _Count extends StatelessWidget {
  const _Count(this.n);
  final int n;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 1),
      decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(999)),
      child: Text('$n', style: mono(context, size: .6875, color: t.bright)),
    );
  }
}

class _Empty extends StatelessWidget {
  const _Empty({required this.icon, required this.text});
  final String icon, text;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
      child: Row(children: [WIcon(icon, color: t.dim), const SizedBox(width: 10), Expanded(child: Text(text, style: body(context, color: t.dim)))]),
    );
  }
}

/// `.swhy`: an info glyph and a dim line.
class _Why extends StatelessWidget {
  const _Why(this.text);
  final String text;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Padding(padding: const EdgeInsets.only(top: 1), child: WIcon('info', size: 16, color: t.dim)),
      const SizedBox(width: 8),
      Expanded(child: Text(text, style: body(context, size: .75, color: t.dim, height: 1.45))),
    ]);
  }
}

/// One running session: its shape, its apps, who is in it (a chip per device, the lead marked),
/// and what this device may do with it — each saying what it will do to the others first.
class SessionRow extends StatefulWidget {
  const SessionRow({super.key, required this.s, required this.o, required this.i, required this.you, required this.busy});
  final Map<String, dynamic> s;
  final ({bool rejoin, bool join, bool takeover, bool end, String whyNot}) o;
  final int i, you;
  final String busy;
  @override
  State<SessionRow> createState() => _SessionRowState();
}

class _SessionRowState extends State<SessionRow> {
  String? _confirm;

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = widget.s;
    final inst = '${s['instance']}';
    final apps = (s['apps'] as List? ?? []);
    final parts = [for (final p in (s['participants'] as List? ?? [])) Map<String, dynamic>.from(p as Map)];
    final others = [for (final p in parts) if (p['id'] != widget.you) '${p['name']}'];
    final count = parts.length;
    final max = (s['max_participants'] ?? 0) as int;
    final state = widget.you != 0
        ? "you're in"
        : count == 0 && s['detached'] == true
            ? 'left running'
            : count == 0
                ? 'nobody in it'
                : max > 0
                    ? '$count of $max in'
                    : '$count in';
    final live = count > 0 || s['viewer'] != null;
    final doing = switch (widget.busy) { 'join' => 'Joining — testing the link…', 'takeover' => 'Taking over…', 'end' => 'Ending…', _ => '' };
    final w = ((s['width'] ?? 1) as num).toDouble(), h = ((s['height'] ?? 1) as num).toDouble();
    final (bw, bh) = w >= h ? (46.0, 46.0 * h / w) : (46.0 * w / h, 46.0);
    final edge = live ? t.good : t.dim;
    final i = widget.i;
    return Enter(
      delay: Duration(milliseconds: i * 90 + 60),
      ms: 520,
      dy: 22,
      scale: .985,
      child: Container(
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(
          color: t.s1,
          borderRadius: BorderRadius.circular(t.r3),
          border: Border.all(color: live ? fade(t.good, 55) : t.line),
        ),
        child: Stack(children: [
          // Someone is in it: a slow wash that says it is alive (`swash`).
          if (live)
            Positioned.fill(
              child: Breathe(
                ms: 3200,
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    gradient: RadialGradient(center: Alignment.topLeft, radius: 1.1, colors: [fade(t.good, 90), fade(t.good, 100)], stops: const [0, .7]),
                  ),
                ),
              ),
            ),
          Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Wrap(spacing: 12, runSpacing: 8, crossAxisAlignment: WrapCrossAlignment.center, children: [
              Container(
                width: 54,
                height: 54,
                decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2)),
                alignment: Alignment.center,
                child: Container(width: bw, height: bh, decoration: BoxDecoration(borderRadius: BorderRadius.circular(5), border: Border.all(color: edge, width: 2), color: fade(edge, live ? 82 : 86))),
              ),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 220),
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                  Text(headline(apps), maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .9375, w: FontWeight.w600, color: t.bright)),
                  const SizedBox(height: 2),
                  Text('${w.round()}×${h.round()} · ${s['fps'] ?? '?'} fps · #${inst.split(':').last}', style: mono(context, size: .6875, color: t.dim)),
                ]),
              ),
              _Pill(state, on: live),
            ]),
            if (parts.isNotEmpty || s['viewer'] != null) ...[
              const SizedBox(height: 12),
              Wrap(spacing: 6, runSpacing: 6, children: [
                if (parts.isEmpty)
                  _Chip(name: '${s['viewer']}', hue: t.accent, lead: true, you: false, delay: i * 90 + 260)
                else
                  for (var k = 0; k < parts.length; k++)
                    _Chip(
                      name: '${parts[k]['name']}',
                      hue: hueOf(context, (parts[k]['hue'] ?? 0) as int),
                      lead: parts[k]['lead'] == true,
                      you: widget.you != 0 && parts[k]['id'] == widget.you,
                      delay: i * 90 + k * 70 + 260,
                    ),
              ]),
            ],
            if (apps.isNotEmpty) ...[
              const SizedBox(height: 12),
              Wrap(spacing: 6, runSpacing: 6, children: [
                for (final a in apps.take(5))
                  Container(
                    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                    decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(999)),
                    child: Text(short('$a'), style: body(context, size: .75)),
                  ),
              ]),
            ],
            if (widget.o.whyNot.isNotEmpty && !widget.o.rejoin) ...[const SizedBox(height: 12), _Why('Join: ${widget.o.whyNot}')],
            const SizedBox(height: 12),
            if (doing.isNotEmpty)
              Enter(
                ms: 480,
                dy: 22,
                child: Container(
                  height: 48,
                  padding: const EdgeInsets.symmetric(horizontal: 14),
                  decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r2)),
                  child: Row(children: [
                    Breathe(ms: 1100, child: Container(width: 10, height: 10, decoration: BoxDecoration(shape: BoxShape.circle, color: t.accent))),
                    const SizedBox(width: 10),
                    Text(doing, style: body(context, w: FontWeight.w600, color: t.bright)),
                  ]),
                ),
              )
            else if (_confirm != null)
              Enter(
                key: ValueKey(_confirm),
                ms: 480,
                dy: 22,
                child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                  Text(_confirm == 'end' ? 'End it for everyone?' : 'Take this session over?', style: body(context, w: FontWeight.w600, color: t.bright)),
                  const SizedBox(height: 4),
                  Text(
                    others.isEmpty
                        ? (_confirm == 'end' ? 'Its apps are closed. Unsaved work in them is lost.' : "It is rebuilt at this screen's size.")
                        : _confirm == 'end'
                            ? '${names(others)} will be disconnected, and its apps closed. Unsaved work in them is lost.'
                            : "${names(others)} will be disconnected — they can rejoin. It is rebuilt at this screen's size.",
                    style: body(context, size: .8125, color: t.dim),
                  ),
                  const SizedBox(height: 10),
                  Row(children: [
                    Expanded(child: Btn('Keep it', onTap: () => setState(() => _confirm = null))),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Btn(_confirm == 'end' ? 'End it' : 'Take over', danger: _confirm == 'end', primary: _confirm != 'end', onTap: () {
                        final kind = _confirm!;
                        setState(() => _confirm = null);
                        Multi.I.act(inst, kind == 'end' ? 'end' : 'takeover', Session.config());
                      }),
                    ),
                  ]),
                ]),
              )
            else
              Wrap(spacing: 8, runSpacing: 8, children: [
                if (widget.o.end) Btn('End', icon: 'power', danger: true, expand: false, onTap: () => setState(() => _confirm = 'end')),
                if (widget.o.takeover) Btn('Take over', icon: 'hand', expand: false, onTap: () => setState(() => _confirm = 'takeover')),
                if (widget.o.join) Btn('Join', icon: 'plus', primary: true, expand: false, onTap: () => Multi.I.act(inst, 'join', Session.config())),
                if (widget.o.rejoin) Btn('Rejoin', icon: 'play', primary: true, expand: false, onTap: () => sessionAct(inst, 'resume')),
              ]),
          ]),
        ]),
      ),
    );
  }
}

/// `.spill`: a tone dot that breathes while someone is in it.
class _Pill extends StatelessWidget {
  const _Pill(this.text, {required this.on});
  final String text;
  final bool on;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = on ? t.good : t.dim;
    return Container(
      height: 26,
      padding: const EdgeInsets.symmetric(horizontal: 10),
      decoration: BoxDecoration(color: fade(c, on ? 85 : 88), borderRadius: BorderRadius.circular(999)),
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        Breathe(ms: 2000, on: on, child: Container(width: 7, height: 7, decoration: BoxDecoration(shape: BoxShape.circle, color: c))),
        const SizedBox(width: 6),
        Text(text, style: body(context, size: .7188, w: FontWeight.w600, color: c)),
      ]),
    );
  }
}

/// `.devchip`: a device in the session — its hue dot, its name; the lead ringed and marked.
/// Pops in from .4 (`chipin`, on the pop curve) after its row lands.
class _Chip extends StatelessWidget {
  const _Chip({required this.name, required this.hue, required this.lead, required this.you, required this.delay});
  final String name;
  final Color hue;
  final bool lead, you;
  final int delay;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Enter(
      delay: Duration(milliseconds: delay),
      ms: 460,
      dy: 0,
      dx: -6,
      scale: .4,
      pop: true,
      child: Container(
        height: 32,
        padding: const EdgeInsets.fromLTRB(10, 0, 12, 0),
        decoration: BoxDecoration(
          color: mix(hue, t.s1, 84),
          borderRadius: BorderRadius.circular(999),
          border: lead ? Border.all(color: fade(hue, 30), width: 1.5) : null,
        ),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          Container(
            width: 10,
            height: 10,
            decoration: BoxDecoration(shape: BoxShape.circle, color: hue, boxShadow: [BoxShadow(color: fade(hue, 75), spreadRadius: 3)]),
          ),
          const SizedBox(width: 7),
          Flexible(child: Text(name, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .7812, w: FontWeight.w600, color: t.bright))),
          if (you) ...[const SizedBox(width: 7), Text('you', style: body(context, size: .6875, w: FontWeight.w500, color: t.dim))],
          if (lead) ...[
            const SizedBox(width: 7),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              decoration: BoxDecoration(color: hue, borderRadius: BorderRadius.circular(999)),
              child: Text('LEAD', style: body(context, size: .5938, w: FontWeight.w600, color: t.onAccent).copyWith(letterSpacing: .08 * 9.5)),
            ),
          ],
        ]),
      ),
    );
  }
}

/// The link speed test, shown: a meter that sweeps while the burst is pulled, then fills to what
/// this link carried and settles into a verdict, with the computer's reasons and how many devices
/// one session here carries, as pips lighting one by one.
class LinkMeter extends StatelessWidget {
  const LinkMeter({super.key, required this.intent});
  final String intent;

  static String _mbps(int k) => k >= 10000 ? (k / 1000).toStringAsFixed(0) : (k / 1000).toStringAsFixed(1);

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final m = Multi.I;
    final v = m.verdict.value;
    final testing = m.testing.value;
    final (Color tone, String word) = testing
        ? (t.accent, 'Measuring…')
        : switch (v?['level']) {
            'ok' => (t.good, 'Good to go'),
            'warn' => (t.base[0xA], 'Works, worse'),
            'refuse' => (t.bad, 'Would not work'),
            _ => (t.dim, 'Not tested'),
          };
    final fill = v == null || testing ? 0.0 : (((v['down_kbps'] ?? 0) as num) / 50000).clamp(.04, 1.0).toDouble();
    final reasons = (v?['reasons'] as List? ?? []).cast<String>();
    final maxP = (v?['max_participants'] ?? 0) as int;
    final sugg = (v?['suggested_max'] ?? 0) as int;
    final sk = (v?['session_kbps'] ?? 0) as int;
    return Enter(
      delay: const Duration(milliseconds: 220),
      ms: 520,
      dy: 22,
      scale: .985,
      child: Container(
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            Text('LINK TEST', style: body(context, size: .625, w: FontWeight.w600, color: t.dim).copyWith(letterSpacing: .8)),
            const SizedBox(width: 10),
            AnimatedContainer(
              duration: t.dur,
              curve: Tokens.spring,
              height: 26,
              padding: const EdgeInsets.symmetric(horizontal: 10),
              decoration: BoxDecoration(color: fade(tone, 86), borderRadius: BorderRadius.circular(999)),
              child: Row(mainAxisSize: MainAxisSize.min, children: [
                Breathe(ms: 900, on: testing, child: Container(width: 7, height: 7, decoration: BoxDecoration(shape: BoxShape.circle, color: tone))),
                const SizedBox(width: 6),
                Text(word, style: body(context, size: .75, w: FontWeight.w600, color: tone)),
              ]),
            ),
            const Spacer(),
            Press(
              onTap: testing ? null : () => m.linkTest(intent),
              enabled: !testing,
              label: 'Test this link again',
              child: Container(
                height: 36,
                padding: const EdgeInsets.symmetric(horizontal: 12),
                decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(999)),
                child: Row(mainAxisSize: MainAxisSize.min, children: [
                  WIcon('pulse', size: 16, color: t.bright),
                  const SizedBox(width: 6),
                  Text(v != null ? 'Again' : 'Test', style: body(context, size: .7812, w: FontWeight.w600, color: t.bright)),
                ]),
              ),
            ),
          ]),
          const SizedBox(height: 10),
          _Track(fill: fill, tone: tone, sweep: testing),
          if (v != null && !testing) ...[
            const SizedBox(height: 10),
            Enter(
              delay: const Duration(milliseconds: 120),
              ms: 480,
              dy: 22,
              child: Wrap(spacing: 16, runSpacing: 6, children: [
                _Num(_mbps((v['down_kbps'] ?? 0) as int), ' Mbps down'),
                _Num('${v['rtt_ms'] ?? 0}', ' ms round trip'),
                if (sk > 0) _Num(_mbps(sk), ' Mbps stream'),
              ]),
            ),
            const SizedBox(height: 10),
            Row(children: [
              for (var i = 0; i < (maxP > sugg ? maxP : sugg).clamp(0, 8); i++)
                Padding(
                  padding: const EdgeInsets.only(right: 4),
                  child: i < maxP
                      ? Enter(
                          delay: Duration(milliseconds: i * 80 + 420),
                          ms: 380,
                          dy: 0,
                          scale: 0,
                          pop: true,
                          child: Container(width: 14, height: 14, decoration: BoxDecoration(shape: BoxShape.circle, color: tone)),
                        )
                      : Container(width: 14, height: 14, decoration: BoxDecoration(shape: BoxShape.circle, color: t.s3)),
                ),
              const SizedBox(width: 6),
              Expanded(child: Text('One session here carries $maxP device${maxP == 1 ? '' : 's'}', style: body(context, size: .75, color: t.dim))),
            ]),
            for (var i = 0; i < reasons.length; i++)
              Enter(
                delay: Duration(milliseconds: i * 70 + 300),
                ms: 480,
                dy: 22,
                child: Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Padding(padding: const EdgeInsets.only(top: 6, left: 2, right: 8), child: Container(width: 6, height: 6, decoration: BoxDecoration(shape: BoxShape.circle, color: tone))),
                    Expanded(child: Text(reasons[i], style: body(context, size: .7812, height: 1.45))),
                  ]),
                ),
              ),
          ],
        ]),
      ),
    );
  }
}

class _Num extends StatelessWidget {
  const _Num(this.n, this.unit);
  final String n, unit;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Text.rich(TextSpan(children: [
      TextSpan(text: n, style: display(context, rem(context, 1.0625), w: FontWeight.w600, color: t.bright).copyWith(fontFeatures: const [FontFeature.tabularFigures()])),
      TextSpan(text: unit, style: body(context, size: .7188, color: t.dim)),
    ]));
  }
}

/// `.lmtrack`: 8 px; the fill settles on the spring over 900 ms; while measuring, a band a third
/// as wide sweeps across, linear (a clock).
class _Track extends StatefulWidget {
  const _Track({required this.fill, required this.tone, required this.sweep});
  final double fill;
  final Color tone;
  final bool sweep;
  @override
  State<_Track> createState() => _TrackState();
}

class _TrackState extends State<_Track> with SingleTickerProviderStateMixin {
  late final _c = AnimationController(vsync: this, duration: const Duration(milliseconds: 1100));
  @override
  void didUpdateWidget(_Track o) {
    super.didUpdateWidget(o);
    _sync();
  }

  @override
  void initState() {
    super.initState();
    _sync();
  }

  void _sync() {
    if (widget.sweep && !_c.isAnimating) _c.repeat();
    if (!widget.sweep) _c.stop();
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return RepaintBoundary(child: ClipRRect(
      borderRadius: BorderRadius.circular(999),
      child: Container(
        height: 8,
        color: t.s3,
        child: LayoutBuilder(builder: (context, box) {
          if (widget.sweep) {
            return AnimatedBuilder(
              animation: _c,
              builder: (context, _) => Transform.translate(
                offset: Offset(box.maxWidth * .32 * (-1 + 4.2 * _c.value), 0),
                child: Align(alignment: Alignment.centerLeft, child: Container(width: box.maxWidth * .32, decoration: BoxDecoration(color: widget.tone, borderRadius: BorderRadius.circular(999)))),
              ),
            );
          }
          return TweenAnimationBuilder<double>(
            tween: Tween(end: widget.fill),
            duration: const Duration(milliseconds: 900),
            curve: Tokens.spring,
            builder: (context, f, _) => Align(
              alignment: Alignment.centerLeft,
              child: AnimatedContainer(
                duration: t.dur,
                width: box.maxWidth * f,
                decoration: BoxDecoration(color: widget.tone, borderRadius: BorderRadius.circular(999)),
              ),
            ),
          );
        }),
      ),
    ));
  }
}
