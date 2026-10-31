// One computer's page (port of ui/computer/mod.rs + hero.rs + actions.rs + settings_list.rs +
// computer.css): hero with link / computer / sessions, the big actions, Running, Now playing,
// Quick, and "This computer". It fills in by itself as the link answers.
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/reach.dart';
import '../../wado/relay_link.dart';
import '../../wado/session.dart';
import '../../wado/settings.dart';
import '../home/cards.dart';
import '../home/home_screen.dart';
import '../home/running.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'playing.dart';
import 'clips.dart';
import '../../wado/passkey.dart';
import '../passkey/passkey_card.dart';
import 'quick.dart';
import 'actions.dart';
import 'sessions.dart';
import 'claude.dart';
import '../motion/motion.dart';

String uptime(int s) {
  final d = s ~/ 86400, h = s % 86400 ~/ 3600, m = s % 3600 ~/ 60;
  if (d == 0 && h == 0) return '$m min';
  if (d == 0) return '$h h $m min';
  return '$d d $h h';
}

class ComputerPage extends StatefulWidget {
  const ComputerPage({super.key, required this.computerKey});
  final String computerKey;
  @override
  State<ComputerPage> createState() => _ComputerPageState();
}

class _ComputerPageState extends State<ComputerPage> {
  final _scroll = ScrollController();
  bool _solid = false;

  @override
  void initState() {
    super.initState();
    Host.I.watch(true);
    Host.I.sessionsWatch(true);
    _scroll.addListener(() {
      Motion.pageScroll = _scroll.offset;
      final s = _scroll.offset > 40;
      if (s != _solid) setState(() => _solid = s);
    });
  }

  @override
  void dispose() {
    Host.I.watch(false);
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final mq = MediaQuery.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([
        CoreHost.I.computers,
        RelayLink.I.up,
        RelayLink.I.words,
        RelayLink.I.seatBusy,
        RelayLink.I.phase,
        Host.I.state,
        Host.I.config,
        Host.I.sessions,
        Session.I.on,
        Settings.I,
        Passkey.I.view,
      ]),
      builder: (context, _) {
        final comps = CoreHost.I.computers.value;
        final i = comps.indexWhere((c) => c.key == widget.computerKey);
        if (i < 0) return const SizedBox.shrink();
        final c = comps[i];
        final hue = slot(context, hues[i % 6]);
        final r = Reach.of(c);
        final off = !r.online && r.tone == 'bad';
        final host = Host.I.state.value;
        final flight = ViewClock.of(context)?.flight ?? false;
        var n = 0;
        // Each section rises in after the hero, one stagger step apart (hero_flip.js `rise`).
        Widget sx(Widget w) => Enter(delay: Enter.step(n++, base: flight ? 160 : 120), child: w);
        return Stack(children: [
          Positioned.fill(
            child: Enter(ms: 300, dy: 0, child: DecoratedBox(
              decoration: BoxDecoration(
                color: t.s0,
                gradient: RadialGradient(center: const Alignment(-.4, -1), radius: .9, colors: [fade(hue, 86), fade(hue, 100)], stops: const [0, .7]),
              ),
            )),
          ),
          ListView(
            controller: _scroll,
            padding: EdgeInsets.fromLTRB(16, 56 + mq.padding.top, 16, 32 + mq.padding.bottom),
            children: [
              Center(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 520),
                  child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                    _Hero(c: c, hue: hue, r: r),
                    if (off) ...[
                      const SizedBox(height: 12),
                      sx(WCard(children: [
                        Text('${c.name} is offline', style: body(context, w: FontWeight.w600, color: t.bright)),
                        Text('It is turned off, asleep, or its wado is not running. This page fills in by itself when it is back — nothing to retry.',
                            style: body(context, size: .8125)),
                      ])),
                    ],
                    const SizedBox(height: 12),
                    sx(ActionTiles(r: r)),
                    sx(Sessions(r: r)),
                    // Claude's cards come over the selected computer's own link.
                    if (r.selected && !off && ((Host.I.config.value?['grants'] as Map?)?['shells'] ?? true) == true)
                      sx(const Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Sect('Claude'), SizedBox(height: 8), ClaudeSection()])),
                    if (r.online && Passkey.I.view.value.state.isNotEmpty && Passkey.I.view.value.required)
                      sx(const Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Sect('Passkey'), SizedBox(height: 8), PasskeyCard()])),
                    if (!off) ...[
                      sx(Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                        const Sect('Now playing'),
                        const SizedBox(height: 8),
                        host == null ? _Empty(icon: 'music', text: 'Asking ${c.name}…') : Playing(h: host),
                      ])),
                      sx(Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                        const Sect('Quick'),
                        const SizedBox(height: 8),
                        host == null ? const _Empty(icon: 'sliders', text: 'Its controls show once it answers.') : Quick(h: host, name: c.name),
                      ])),
                    ],
                    sx(const Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Sect('This computer'), SizedBox(height: 8), _SettingsList()])),
                    if (!off) sx(const Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Sect('Clipboard'), SizedBox(height: 8), Clips()])),
                  ]),
                ),
              ),
            ],
          ),
          Positioned(
            left: 0,
            right: 0,
            top: 0,
            child: Enter(delay: const Duration(milliseconds: 200), ms: 300, dy: 0, child: AnimatedContainer(
              duration: t.dur,
              height: 56 + mq.padding.top,
              padding: EdgeInsets.fromLTRB(8, mq.padding.top, 8, 0),
              decoration: BoxDecoration(color: _solid ? t.s0 : Colors.transparent, border: _solid ? Border(bottom: BorderSide(color: t.line)) : null),
              child: Row(children: [
                IconBtn('left', label: 'Back to home', color: t.bright, onTap: Nav.I.back),
                const SizedBox(width: 4),
                Expanded(
                  child: AnimatedOpacity(
                    opacity: _solid ? 1 : 0,
                    duration: t.dur,
                    child: Text(c.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1.0625))),
                  ),
                ),
              ]),
            )),
          ),
        ]);
      },
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

class _Hero extends StatefulWidget {
  const _Hero({required this.c, required this.hue, required this.r});
  final Computer c;
  final Color hue;
  final Reach r;
  @override
  State<_Hero> createState() => _HeroState();
}

class _HeroState extends State<_Hero> {
  bool open = false;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = widget.c, r = widget.r;
    final info = Host.I.state.value?['info'] as Map<String, dynamic>?;
    var os = [c.os, c.desktop].where((x) => x.isNotEmpty).join(' · ');
    if (info != null && '${info['os'] ?? ''}'.isNotEmpty) {
      os = ['${info['os']}', '${info['desktop'] ?? ''}'].where((x) => x.isNotEmpty).join(' · ');
    }
    final l = RelayLink.I;
    final list = Host.I.sessions.value;
    final (String ltone, String lval, String lsub) = r.busy && l.seatBusy.value?.idle == true
        ? ('ok', 'Free', 'your other device has it, idle')
        : (!r.online && r.tone == 'bad')
            ? ('bad', 'None', 'nothing to reach')
            : l.up.value
                ? (Session.I.stage.value >= 4 ? ('ok', 'Direct', 'picture path up') : ('ok', 'Ready', 'relay joined'))
                : (r.busy ? ('warn', 'Waiting', 'in use elsewhere') : ('warn', 'Connecting…', 'setting up'));
    final up = (info?['uptime_s'] ?? 0) as int;
    final (String ctone, String cval, String csub) = r.online
        ? ('ok', 'Online', up > 0 ? 'up ${uptime(up)}' : 'answering')
        : r.tone == 'bad'
            ? ('bad', 'Offline', 'not seen yet')
            : ('', '…', 'asking');
    String sval, ssub;
    if (!r.online) {
      (sval, ssub) = ('—', '');
    } else if (r.busy && list.isEmpty) {
      (sval, ssub) = l.seatBusy.value?.idle == true ? ('0', 'none running') : ('—', 'running on another device');
    } else if (list.isEmpty) {
      (sval, ssub) = ('0', 'none running');
    } else {
      final first = list.expand((s) => (s['apps'] as List? ?? [])).map((a) => short('$a')).firstOrNull ?? 'empty desktop';
      (sval, ssub) = ('${list.length}', first);
    }
    Color tone(String s) => switch (s) { 'ok' => t.good, 'warn' => t.base[0xA], 'bad' => t.bad, _ => t.bright };
    Widget cell(String k, String v, String s, String tn, {VoidCallback? onTap, bool dot = false, double grow = 1}) => Expanded(
          flex: (grow * 100).round(),
          child: Press(
            onTap: onTap,
            child: Container(
              padding: const EdgeInsets.fromLTRB(10, 9, 10, 8),
              decoration: BoxDecoration(color: fade(t.s0, 35), borderRadius: BorderRadius.circular(t.r2)),
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text(k.toUpperCase(), style: body(context, size: .625, w: FontWeight.w600, color: t.dim).copyWith(letterSpacing: .8)),
                Row(children: [
                  if (dot) ...[LinkWords(tone: tn == 'ok' ? 'ok' : (tn == 'warn' ? 'warn' : 'bad'), words: ''), const SizedBox(width: 2)],
                  Flexible(child: Text(v, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1.0625), w: FontWeight.w600, color: tone(tn)))),
                ]),
                if (s.isNotEmpty) Text(s, style: body(context, size: .7188, color: t.dim)),
              ]),
            ),
          ),
        );
    final explain = l.words.value;
    final k = c.key;
    Widget later(Widget w) => Enter(delay: const Duration(milliseconds: 200), ms: 420, dy: 10, child: w);
    return Stack(children: [
      Positioned.fill(
        child: Flip(
          from: 'card:$k',
          uniform: false,
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: t.s1,
              borderRadius: BorderRadius.circular(t.r4),
              border: Border.all(color: t.line),
              gradient: LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: [mix(t.s1, widget.hue, 24), mix(t.s1, t.base[0xE], 10)], stops: const [0, .7]),
            ),
          ),
        ),
      ),
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 18, 16, 16),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            Flip(from: 'disc:$k', child: OsDisc(logo: c.logo, hue: widget.hue, size: 64)),
            const SizedBox(width: 14),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Flip(
                  from: 'name:$k',
                  child: Text(c.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1.875), ls: -rem(context, 1.875) * .025)),
                ),
                const SizedBox(height: 4),
                later(Text(os.isEmpty ? c.remoteId : os, style: mono(context, color: t.dim))),
              ]),
            ),
          ]),
          const SizedBox(height: 16),
          later(IntrinsicHeight(
            child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              cell('Link', lval, lsub, ltone, dot: true, grow: 1.25, onTap: explain.isEmpty ? null : () => setState(() => open = !open)),
              const SizedBox(width: 8),
              cell('Computer', cval, csub, ctone),
              const SizedBox(width: 8),
              cell('Sessions', sval, ssub, '', grow: .8),
            ]),
          )),
          AnimatedSize(
            duration: t.durSheet,
            curve: Tokens.spring,
            child: open && explain.isNotEmpty
                ? Padding(padding: const EdgeInsets.only(top: 10), child: Text(explain, style: body(context, size: .7812, height: 1.55)))
                : const SizedBox(width: double.infinity),
          ),
          later(_Hops()),
        ]),
      ),
    ]);
  }
}

/// The hop bar while dialling (hops.rs): Relay → Computer → Secure → Ready.
class _Hops extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final p = RelayLink.I.phase.value;
    if (p >= 3 || p == 0) return const SizedBox.shrink();
    const names = ['Relay', 'Computer', 'Secure link'];
    return Padding(
      padding: const EdgeInsets.only(top: 12),
      child: Row(children: [
        for (var i = 0; i < names.length; i++) ...[
          Container(width: 7, height: 7, decoration: BoxDecoration(shape: BoxShape.circle, color: i < p ? t.good : (i == p ? t.base[0xA] : t.s3))),
          const SizedBox(width: 6),
          Text(names[i], style: body(context, size: .75, color: i <= p ? t.text : t.dim)),
          if (i < names.length - 1) const SizedBox(width: 12),
        ],
      ]),
    );
  }
}

class _SettingsList extends StatelessWidget {
  const _SettingsList();
  @override
  Widget build(BuildContext context) {
    final s = Settings.I;
    final q = switch (s.s('quality')) { 'reactivity' => 'Reactive', 'quality' => 'Sharp', 'custom' => 'Custom', _ => 'Balanced' };
    final res = s.s('res').isEmpty ? 'Fit the console' : s.s('res').replaceAll('x', '×');
    final h = Host.I.state.value;
    String sound = "The computer's sound and Bluetooth";
    if (h != null) {
      final audio = h['audio'] as Map<String, dynamic>?;
      final sinks = (audio?['sinks'] as List? ?? []).cast<Map<String, dynamic>>();
      final def = sinks.where((x) => x['name'] == audio?['default_sink']).firstOrNull;
      final bt = h['bluetooth'] as Map<String, dynamic>?;
      sound = [def?['label'] ?? 'Speakers', if (bt != null) bt['powered'] == true ? 'Bluetooth on' : 'Bluetooth off'].join(' · ');
    }
    final wifi = (h?['wifi'] as Map<String, dynamic>?)?['connected'];
    final conn = 'Via relay${wifi != null ? ' · Wi-Fi $wifi' : ''}';
    return ListBox(children: [
      NavRow(icon: 'monitor', hue: '0D', title: 'Display & session', sub: '$res · ${s.i('fps')} fps · $q', onOpen: () => Nav.I.openPage('display_session')),
      NavRow(icon: 'phone', hue: '0C', title: 'Virtual input', sub: 'Touchpad · Keyboard · Gamepad', onOpen: () => Nav.I.openPage('input')),
      NavRow(icon: 'sound', hue: '0E', title: 'Sound & devices', sub: sound, onOpen: () => Nav.I.openPage('sound_devices')),
      NavRow(icon: 'net', hue: '09', title: 'Connection & trust', sub: conn, onOpen: () => Nav.I.openPage('connection_trust')),
    ]);
  }
}
