// The control centre (port of ui/cc/*, ui/drawer/*, ui/workspaces.rs): over the picture, the
// session's hero (computer, link, picture stats), quick tiles, the app drawer, windows and
// workspaces, and Leave running / End session.
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/relay_link.dart';
import '../../wado/session.dart';
import '../../wado/settings.dart';
import '../computer/quick.dart';
import '../files/files_page.dart';
import '../input/pad_screen.dart';
import '../shell/shell_page.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';
import '../widgets/slider.dart';
import '../pages/display_session.dart';
import 'input_layer.dart';
import 'session_view.dart';

void openControlCentre(BuildContext context) {
  final t = Tok.of(context);
  Host.I.watch(true);
  Session.I.watchStats(true);
  showModalBottomSheet(
    context: context,
    isScrollControlled: true,
    backgroundColor: t.s0,
    barrierColor: Colors.black54,
    shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(t.r4))),
    builder: (_) => const FractionallySizedBox(heightFactor: .88, child: ControlCentre()),
  ).whenComplete(() {
    Host.I.watch(false);
    Session.I.watchStats(false);
  });
}

void _push(BuildContext context, String title, Widget page) {
  Navigator.of(context).push(MaterialPageRoute(
    builder: (c) => Scaffold(
      backgroundColor: Tok.of(c).s0,
      appBar: AppBar(backgroundColor: Tok.of(c).s0, foregroundColor: Tok.of(c).bright, title: Text(title)),
      body: page,
    ),
  ));
}

class ControlCentre extends StatefulWidget {
  const ControlCentre({super.key});
  @override
  State<ControlCentre> createState() => _ControlCentreState();
}

class _ControlCentreState extends State<ControlCentre> {
  final _run = TextEditingController();
  String _q = '';

  @override
  void initState() {
    super.initState();
    // apps_request is answered with apps_list (relay.js).
    if (Session.I.apps.value.isEmpty) RelayLink.I.send({'type': 'apps_request'});
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Session.I;
    return ListenableBuilder(
      listenable: Listenable.merge([s.stats, s.encoder, s.apps, s.windows, s.workspace, Host.I.state, Settings.I]),
      builder: (context, _) {
        final c = CoreHost.I.computer(Nav.I.selected);
        final st = s.stats.value;
        final h = Host.I.state.value;
        final sp = h == null ? null : speaker(h);
        final apps = s.apps.value.where((a) => _q.isEmpty || '${a['name']}'.toLowerCase().contains(_q.toLowerCase())).toList();
        final recent = Settings.I.get<List<String>>('recent');
        return ListView(padding: const EdgeInsets.fromLTRB(16, 10, 16, 24), children: [
          Center(child: Container(width: 40, height: 4, decoration: BoxDecoration(color: t.line, borderRadius: BorderRadius.circular(2)))),
          const SizedBox(height: 12),
          // Hero.
          WCard(children: [
            Text(c?.name ?? 'Computer', style: display(context, rem(context, 1.375))),
            Text(
              [
                if (st != null && st.fps > 0) '${st.fps.round()} fps',
                if (st != null && st.rtt > 0) '${st.rtt.round()} ms',
                if (s.encoder.value.$1.isNotEmpty) 'encoder ${s.encoder.value.$1}',
                if (st != null && st.decoder.isNotEmpty) 'decode ${st.decoder}',
              ].join(' · '),
              style: mono(context, color: t.dim),
            ),
            if (s.encoder.value.$1 == 'software') const Why('Software encoding — no working GPU encoder. Higher CPU use and latency.', tone: 'host'),
          ]),
          const Sect('Quick'),
          const SizedBox(height: 8),
          if (sp != null) ...[
            Row(children: [
              IconBtn(sp['muted'] == true ? 'mute' : 'sound', label: 'Mute', color: sp['muted'] == true ? t.bad : t.bright,
                  onTap: () => Host.I.act({'do': 'sink_mute', 'id': sp['id'], 'muted': sp['muted'] != true})),
              Expanded(child: ThickSlider(label: 'Volume', value: ((sp['volume'] ?? 0) as num).toDouble().clamp(0, 1), onChange: (v) => Host.I.act({'do': 'sink_volume', 'id': sp['id'], 'volume': v}))),
            ]),
            const SizedBox(height: 8),
          ],
          GridView.count(
            crossAxisCount: 4,
            shrinkWrap: true,
            physics: const NeverScrollableScrollPhysics(),
            mainAxisSpacing: 8,
            crossAxisSpacing: 8,
            childAspectRatio: .95,
            children: [
              for (final id in Settings.I.get<List<String>>('tiles'))
                if (tileSpecs[id] != null) _Tile(icon: tileSpecs[id]!.$1, label: tileSpecs[id]!.$2, on: id == 'awake' && h?['awake'] == true, onTap: () => _tap(context, id, h)),
            ],
          ),
          const Sect('Workspaces'),
          const SizedBox(height: 8),
          Row(children: [
            for (var n = 1; n <= 6; n++)
              Expanded(
                child: Padding(
                  padding: const EdgeInsets.all(3),
                  child: Press(
                    onTap: () => s.window({'workspace': {'n': n}}),
                    child: Container(
                      height: 40,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(color: s.workspace.value == n ? t.accent : t.s2, borderRadius: BorderRadius.circular(t.r1)),
                      child: Text('$n', style: mono(context, size: .875, w: FontWeight.w600, color: s.workspace.value == n ? t.onAccent : t.bright)),
                    ),
                  ),
                ),
              ),
          ]),
          if (s.windows.value.isNotEmpty) ...[
            const Sect('Windows'),
            const SizedBox(height: 8),
            ListBox(children: [
              for (final w in s.windows.value)
                Press(
                  onTap: () {
                    s.window({'focus': {'id': w['id']}});
                    Navigator.pop(context);
                  },
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                    child: Row(children: [
                      Expanded(
                        child: Text('${w['title'].toString().isEmpty ? w['app_id'] : w['title']}', maxLines: 1, overflow: TextOverflow.ellipsis,
                            style: body(context, w: w['focused'] == true ? FontWeight.w600 : FontWeight.w400, color: w['focused'] == true ? t.accent : t.bright)),
                      ),
                      Text('${w['workspace']}', style: mono(context, color: t.dim)),
                    ]),
                  ),
                ),
            ]),
          ],
          const Sect('Apps'),
          const SizedBox(height: 8),
          Row(children: [
            Expanded(
              child: TextField(
                controller: _run,
                onChanged: (v) => setState(() => _q = v),
                onSubmitted: (v) {
                  s.launch(v);
                  Navigator.pop(context);
                },
                style: body(context, size: 1, color: t.bright),
                decoration: InputDecoration(
                  hintText: 'Find an app, or type a command',
                  filled: true,
                  fillColor: t.s1,
                  isDense: true,
                  border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r2), borderSide: BorderSide.none),
                ),
              ),
            ),
            const SizedBox(width: 8),
            IconBtn('play', label: 'Run what is in the box', color: t.accent, onTap: () {
              s.launch(_run.text);
              Navigator.pop(context);
            }),
          ]),
          if (recent.isNotEmpty && _q.isEmpty) ...[
            const SizedBox(height: 8),
            Wrap(spacing: 6, runSpacing: 6, children: [
              for (final r in recent.take(6))
                Press(
                  onTap: () {
                    s.launch(r);
                    Navigator.pop(context);
                  },
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                    decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(999)),
                    child: Text(r, style: mono(context, color: t.bright)),
                  ),
                ),
            ]),
          ],
          const SizedBox(height: 8),
          if (s.apps.value.isEmpty) const Why('No application list from the server — ▶ still runs whatever you type.'),
          for (final a in apps.take(40))
            Press(
              onTap: () {
                s.launch('${a['exec']}');
                Navigator.pop(context);
              },
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 10, horizontal: 4),
                child: Row(children: [
                  WIcon('apps', color: t.dim),
                  const SizedBox(width: 12),
                  Expanded(child: Text('${a['name']}', style: body(context, color: t.bright))),
                ]),
              ),
            ),
          const SizedBox(height: 16),
          Row(children: [
            Expanded(
              child: Btn('Leave running', icon: 'home', onTap: () {
                Navigator.pop(context);
                leaveToComputer();
              }),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Btn('End session', danger: true, icon: 'power', onTap: () {
                Navigator.pop(context);
                Session.I.stop();
                final k = Nav.I.selected;
                if (k.isNotEmpty) Nav.I.view.value = ComputerView(k);
              }),
            ),
          ]),
        ]);
      },
    );
  }
}

/// Every tile the control centre can show (ui/cc/tiles/pool.rs), by id: icon, label.
const tileSpecs = <String, (String, String)>{
  'awake': ('eye', 'Awake'),
  'keyboard': ('kbd', 'Keyboard'),
  'touchpad': ('trackpad', 'Touchpad'),
  'files': ('folder', 'Files'),
  'shell': ('term', 'Shell'),
  'resync': ('refresh', 'Resync'),
  'maximize': ('max', 'Maximize'),
  'close': ('x', 'Close app'),
  'sound': ('sound', 'Mute'),
  'settings': ('sliders', 'Settings'),
};

void _tap(BuildContext context, String id, Map<String, dynamic>? h) {
  final s = Session.I;
  switch (id) {
    case 'awake':
      Host.I.act({'do': 'keep_awake', 'on': h?['awake'] != true});
    case 'keyboard':
      Navigator.pop(context);
      KeyboardSheet.toggle(context);
    case 'touchpad':
      openPad(context);
    case 'files':
      _push(context, 'Files', const FilesPage());
    case 'shell':
      _push(context, 'Shell', const ShellPage());
    case 'resync':
      s.negotiate();
    case 'maximize':
      s.window('maximize');
    case 'close':
      s.window('close');
    case 'sound':
      final sp = h == null ? null : speaker(h);
      if (sp != null) Host.I.act({'do': 'sink_mute', 'id': sp['id'], 'muted': sp['muted'] != true});
    case 'settings':
      _push(context, 'Display & session', const SingleChildScrollView(padding: EdgeInsets.all(16), child: DisplaySession()));
  }
}

class _Tile extends StatelessWidget {
  const _Tile({required this.icon, required this.label, required this.onTap, this.on = false});
  final String icon, label;
  final bool on;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Press(
      onTap: onTap,
      label: label,
      child: AnimatedContainer(
        duration: t.dur,
        decoration: BoxDecoration(color: on ? t.accent : t.s1, borderRadius: BorderRadius.circular(on ? t.r3 + 6 : t.r3)),
        child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
          WIcon(icon, color: on ? t.onAccent : t.bright),
          const SizedBox(height: 6),
          Text(label, textAlign: TextAlign.center, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .6875, w: FontWeight.w600, color: on ? t.onAccent : t.text)),
        ]),
      ),
    );
  }
}
