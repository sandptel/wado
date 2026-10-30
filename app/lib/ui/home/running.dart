// Running (port of ui/home/sessions.rs + landing.css .sessioncard): one card per session on the
// selected computer, with its shape, apps, who watches it, and Rejoin / End.
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/relay_link.dart';
import '../../wado/session.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'cards.dart';

String ago(int secs) {
  final m = secs ~/ 60;
  if (m == 0) return 'just now';
  if (m < 60) return '$m min ago';
  return '${m ~/ 60} h ${m % 60} min ago';
}

String short(String app) {
  final last = app.split('.').last;
  return last.isEmpty ? '' : last[0].toUpperCase() + last.substring(1);
}

String headline(List apps) => apps.isEmpty ? 'Empty desktop' : short('${apps.first}') + (apps.length > 1 ? ' and more' : '');

Map<String, dynamic>? best(List<Map<String, dynamic>> list) {
  if (list.isEmpty) return null;
  final l = [...list]..sort((a, b) {
      final va = a['viewer'] != null ? 1 : 0, vb = b['viewer'] != null ? 1 : 0;
      if (va != vb) return va - vb;
      return ((a['age_s'] ?? 0) as int) - ((b['age_s'] ?? 0) as int);
    });
  return l.first;
}

/// sessions.js `sessionAct`: resume or stop a session, re-dialling its daemon if it is another.
void sessionAct(String instance, String kind) {
  final l = RelayLink.I;
  void act() {
    if (kind == 'resume') {
      Session.I.start();
    } else {
      l.send({'type': 'session_stop'});
      Future.delayed(const Duration(milliseconds: 800), Host.I.sessionsGet);
    }
  }

  if (instance == l.pool.value.instance && l.up.value) {
    act();
    return;
  }
  // ponytail: a session on another daemon of the pool resumes through the seat's own routing;
  // instance-sticky redial (rememberInstance) comes with multi-daemon pools.
  act();
}

void joinBest() {
  final s = best(Host.I.sessions.value);
  if (s != null) sessionAct('${s['instance']}', 'resume');
}

class Running extends StatefulWidget {
  const Running({super.key});
  @override
  State<Running> createState() => _RunningState();
}

class _RunningState extends State<Running> {
  String? confirm;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final list = Host.I.sessions.value;
    if (list.isEmpty) return const SizedBox.shrink();
    final here = RelayLink.I.pool.value.instance;
    final comps = CoreHost.I.computers.value;
    final i = comps.indexWhere((c) => c.key == Nav.I.selected);
    final name = i < 0 ? 'this computer' : comps[i].name;
    final hue = slot(context, hues[(i < 0 ? 0 : i) % 6]);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      const Sect('Running'),
      const SizedBox(height: 8),
      for (final s in list) ...[
        _card(context, t, s, here, name, hue),
        const SizedBox(height: 8),
      ],
    ]);
  }

  Widget _card(BuildContext context, Tokens t, Map<String, dynamic> s, String here, String name, Color hue) {
    final inst = '${s['instance']}';
    final n = inst.split(':').last;
    final apps = (s['apps'] as List? ?? []);
    final viewer = s['viewer'];
    final who = viewer != null ? 'open on $viewer' : (s['detached'] == true ? 'left running' : 'nobody watching');
    final live = viewer != null;
    final w = ((s['width'] ?? 1) as num).toDouble(), h = ((s['height'] ?? 1) as num).toDouble();
    final (bw, bh) = w >= h ? (46.0, 46.0 * h / w) : (46.0 * w / h, 46.0);
    final edge = live ? t.accent : t.good;
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: t.s1,
        borderRadius: BorderRadius.circular(t.r3),
        border: Border.all(color: fade(live ? t.accent : t.good, live ? 40 : 60)),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Row(children: [
          Container(
            width: 54,
            height: 54,
            decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2)),
            alignment: Alignment.center,
            child: Container(
              width: bw,
              height: bh,
              decoration: BoxDecoration(borderRadius: BorderRadius.circular(5), border: Border.all(color: edge, width: 2), color: fade(edge, 82)),
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(headline(apps), maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright)),
              Text('${w.round()}×${h.round()} · ${s['fps'] ?? '?'} fps · started ${ago((s['age_s'] ?? 0) as int)}', style: mono(context, color: t.dim)),
              Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 6, children: [
                Row(mainAxisSize: MainAxisSize.min, children: [
                  Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: hue)),
                  const SizedBox(width: 6),
                  Text('on $name', style: body(context, size: .75, w: FontWeight.w600)),
                ]),
                Container(width: 7, height: 7, decoration: BoxDecoration(shape: BoxShape.circle, color: live ? t.accent : t.dim)),
                Text('$who · #$n', style: body(context, size: .75, color: t.dim)),
              ]),
            ]),
          ),
        ]),
        if (apps.isNotEmpty) ...[
          const SizedBox(height: 10),
          Wrap(spacing: 6, runSpacing: 6, children: [
            for (final a in apps.take(6))
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
                decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(999)),
                child: Text(short('$a'), style: body(context, size: .75)),
              ),
          ]),
        ],
        const SizedBox(height: 10),
        if (confirm == inst) ...[
          Text('End this session?', style: body(context, w: FontWeight.w600, color: t.bright)),
          const Why('Its apps are closed. Unsaved work in them is lost.'),
          const SizedBox(height: 8),
          Row(children: [
            Expanded(child: Btn('Keep it', onTap: () => setState(() => confirm = null))),
            const SizedBox(width: 8),
            Expanded(
                child: Btn('End it', danger: true, onTap: () {
              sessionAct(inst, 'stop');
              setState(() => confirm = null);
            })),
          ]),
        ] else
          Row(children: [
            Expanded(child: Btn('End', icon: 'power', onTap: () => setState(() => confirm = inst))),
            const SizedBox(width: 8),
            Expanded(child: Btn('Rejoin', icon: 'play', primary: true, onTap: () => sessionAct(inst, 'resume'))),
          ]),
      ]),
    );
  }
}
