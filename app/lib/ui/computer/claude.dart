// Claude on the Computer page (port of ui/claude/{mod,card,picker}.rs + claude.css): a card per
// Claude Code session on that computer, Allow / Deny on the one that asks, and "New Claude
// session" in a project folder.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/core_host.dart';
import '../../wado/claude.dart';
import '../../wado/nav.dart';
import '../../wado/passkey.dart';
import 'clips.dart' show ago;
import '../motion/motion.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';

class ClaudeSection extends StatefulWidget {
  const ClaudeSection({super.key});
  @override
  State<ClaudeSection> createState() => _ClaudeSectionState();
}

class _ClaudeSectionState extends State<ClaudeSection> {
  bool _copied = false;

  @override
  void initState() {
    super.initState();
    Claude.I.shell.addListener(_shell);
  }

  @override
  void dispose() {
    Claude.I.shell.removeListener(_shell);
    super.dispose();
  }

  // A session's shell is on its way: show it.
  void _shell() {
    if (mounted) Nav.I.openPage('shells');
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final name = CoreHost.I.computer(Nav.I.selected)?.name ?? 'the computer';
    return ListenableBuilder(
      listenable: Listenable.merge([Claude.I.cards, Passkey.I.view]),
      builder: (context, _) {
        final st = Claude.I.cards.value;
        if (st == null) return _Empty('Asking $name about Claude…');
        final sessions = [for (final s in (st['sessions'] as List? ?? [])) Map<String, dynamic>.from(s as Map)];
        final open = sessions.where((s) => s['state'] != 'ended').length;
        const cmd = 'wado claude hooks install';
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          if (st['hooks'] != true && sessions.isEmpty)
            WCard(gap: 8, children: [
              Text('See Claude here', style: body(context, w: FontWeight.w600, color: t.bright)),
              Text('Status, questions and Allow / Deny for Claude Code on $name appear here once its hooks are in. On $name, run:', style: body(context, size: .8125)),
              Container(
                padding: const EdgeInsets.only(left: 12),
                decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r1)),
                child: Row(children: [
                  Expanded(child: Text(cmd, style: mono(context, size: .8125, color: t.bright))),
                  IconBtn(_copied ? 'check' : 'copy', label: _copied ? 'Copied' : 'Copy the command', color: _copied ? t.good : t.dim, onTap: () {
                    Clipboard.setData(const ClipboardData(text: cmd));
                    setState(() => _copied = true);
                    Future.delayed(const Duration(milliseconds: 1400), () {
                      if (mounted) setState(() => _copied = false);
                    });
                  }),
                ]),
              ),
              Text("It shows you the change to Claude's settings before it makes it.", style: body(context, size: .75, color: t.dim)),
            ])
          else if (open == 0 && sessions.isEmpty)
            _Empty('No Claude sessions on $name right now.'),
          for (final c in sessions) Padding(padding: const EdgeInsets.only(top: 8), child: ClaudeCard(key: ValueKey(c['id']), c: c)),
          const SizedBox(height: 10),
          Btn('New Claude session', icon: 'plus', onTap: () => Passkey.I.ask('new', then: () => _pick(context, name))),
          if (st['tmux'] != true) ...[
            const SizedBox(height: 8),
            Text("tmux isn't installed on $name, so a new session lives in a wado shell and ends with it. With tmux it keeps running, and you can attach at the desk.",
                style: body(context, size: .75, color: t.dim)),
          ],
        ]);
      },
    );
  }

  void _pick(BuildContext context, String name) {
    Claude.I.dirs.value = null;
    Claude.I.folders('');
    final t = Tok.of(context);
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      backgroundColor: t.s0,
      showDragHandle: true,
      sheetAnimationStyle: AnimationStyle(duration: t.durSheet, curve: Tokens.spring, reverseCurve: Tokens.spring),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(t.r4))),
      builder: (_) => _Picker(name: name),
    );
  }
}

class _Empty extends StatelessWidget {
  const _Empty(this.text);
  final String text;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
      child: Row(children: [WIcon('term', color: t.dim), const SizedBox(width: 10), Expanded(child: Text(text, style: body(context, color: t.dim)))]),
    );
  }
}

/// One Claude session: its project, what it is doing, the last thing it said — and when it asks,
/// the question with Deny and Allow right on the card. Allow carries a lock while locked.
class ClaudeCard extends StatelessWidget {
  const ClaudeCard({super.key, required this.c, this.compact = false});
  final Map<String, dynamic> c;
  final bool compact;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final state = '${c['state']}';
    final ask = c['ask'] as Map?;
    final reach = c['tmux'] != null || c['shell'] != null;
    final locked = !Passkey.I.view.value.unlocked;
    final perm = state == 'permission';
    final dot = switch (state) { 'working' => t.accent, 'permission' => t.base[0xA], 'done' => t.good, _ => t.dim };
    final id = '${c['id']}';
    Widget dotW = Container(width: 10, height: 10, decoration: BoxDecoration(shape: BoxShape.circle, color: dot));
    if (state == 'working') dotW = Breathe(ms: 1600, child: dotW);
    if (state == 'done') dotW = Enter(ms: 220, dy: 0, scale: 0, pop: true, fadeOnClose: false, child: dotW);
    return Opacity(
      opacity: state == 'ended' ? .62 : 1,
      child: Container(
        padding: const EdgeInsets.fromLTRB(4, 4, 4, 10),
        decoration: BoxDecoration(
          color: perm ? mix(t.base[0xA], t.s1, 88) : (compact ? t.s2 : t.s1),
          borderRadius: BorderRadius.circular(t.r3),
        ),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Press(
            onTap: reach ? () => Claude.I.open(id) : null,
            scale: 1,
            label: reach ? 'Open its shell' : 'Started outside wado — no shell to open here',
            child: Container(
              constraints: const BoxConstraints(minHeight: 52),
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
              child: Row(children: [
                Padding(padding: const EdgeInsets.only(left: 4), child: dotW),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text('${c['project']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .9375, w: FontWeight.w600, color: t.bright)),
                    Text.rich(TextSpan(children: [
                      TextSpan(text: claudeWords(state), style: TextStyle(fontWeight: FontWeight.w600, color: perm ? t.base[0xA] : (state == 'done' ? t.good : t.text))),
                      TextSpan(text: ' · ${ago((c['since'] ?? 0) as int)}'),
                    ]), style: body(context, size: .75, color: t.dim)),
                  ]),
                ),
                if (reach) SizedBox(width: 32, child: WIcon('term', color: t.dim)),
              ]),
            ),
          ),
          if ('${c['message'] ?? ''}'.isNotEmpty && ask == null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 14),
              child: Text('${c['message']}', maxLines: 2, overflow: TextOverflow.ellipsis, style: body(context, size: .8125)),
            ),
          if (ask != null)
            Container(
              key: ValueKey(ask['id']),
              margin: const EdgeInsets.symmetric(horizontal: 10),
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2)),
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                Text.rich(TextSpan(children: [const TextSpan(text: 'Wants to use '), TextSpan(text: '${ask['tool']}', style: TextStyle(color: t.bright, fontWeight: FontWeight.w700))]),
                    style: body(context, size: .75, color: t.dim)),
                const SizedBox(height: 8),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                  decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r1)),
                  child: Text('${ask['detail']}', style: mono(context, size: .75, color: t.bright)),
                ),
                const SizedBox(height: 8),
                Row(children: [
                  Expanded(flex: 10, child: Enter(ms: 220, dy: 12, fadeOnClose: false, child: Btn('Deny', icon: 'x', onTap: () => Claude.I.answer(id, '${ask['id']}', false)))),
                  const SizedBox(width: 8),
                  Expanded(
                    flex: 13,
                    child: Enter(
                      delay: const Duration(milliseconds: 40),
                      ms: 220,
                      dy: 12,
                      fadeOnClose: false,
                      child: Btn('Allow', icon: locked ? 'lock' : 'check', primary: true, onTap: () => Claude.I.answer(id, '${ask['id']}', true)),
                    ),
                  ),
                ]),
              ]),
            ),
          if (!reach && !compact && state != 'ended')
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 14),
              child: Text('Started outside wado — status only. Start the next one from wado to reach it from here.', style: body(context, size: .75, color: t.dim)),
            ),
        ]),
      ),
    );
  }
}

String _base(String p) {
  final parts = p.replaceAll(RegExp(r'/+$'), '').split('/');
  return parts.last.isEmpty ? '/' : parts.last;
}

/// "New Claude session": walk the computer's folders to the project, then start Claude there.
/// Folders Claude was used in lately are one tap away at the top.
class _Picker extends StatelessWidget {
  const _Picker({required this.name});
  final String name;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ValueListenableBuilder(
      valueListenable: Claude.I.dirs,
      builder: (context, d, _) {
        final here = '${d?['path'] ?? ''}';
        final recent = (d?['recent'] as List? ?? []).cast<String>();
        final list = (d?['dirs'] as List? ?? []).cast<String>();
        final parent = '${d?['parent'] ?? ''}';
        return SafeArea(
          top: false,
          child: ConstrainedBox(
            constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * .86),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
              child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                Text('New Claude session', style: display(context, rem(context, 1.25))),
                const SizedBox(height: 6),
                Text('Pick the project folder on $name. Claude starts there, and its shell opens here.', style: body(context, size: .8125, color: t.dim)),
                const SizedBox(height: 12),
                if (d == null)
                  _Empty("Reading $name's folders…")
                else if ('${d['error'] ?? ''}'.isNotEmpty)
                  Text('${d['error']}', style: body(context, color: t.bad))
                else ...[
                  if (recent.isNotEmpty)
                    SizedBox(
                      height: 36,
                      child: ListView(scrollDirection: Axis.horizontal, children: [
                        for (final r in recent)
                          Padding(
                            padding: const EdgeInsets.only(right: 8),
                            child: Press(
                              onTap: () => Claude.I.folders(r),
                              child: Container(
                                padding: const EdgeInsets.symmetric(horizontal: 12),
                                decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(999)),
                                child: Row(children: [WIcon('folder', size: 16, color: t.dim), const SizedBox(width: 6), Text(_base(r), style: body(context, size: .8125, color: t.bright))]),
                              ),
                            ),
                          ),
                      ]),
                    ),
                  const SizedBox(height: 8),
                  Row(children: [
                    IconBtn('up', label: 'Up one folder', onTap: parent.isEmpty ? null : () => Claude.I.folders(parent)),
                    const SizedBox(width: 6),
                    Expanded(child: Text(here, maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(context, color: t.dim))),
                  ]),
                  const SizedBox(height: 6),
                  Flexible(
                    child: Container(
                      constraints: const BoxConstraints(minHeight: 120),
                      padding: const EdgeInsets.all(4),
                      decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
                      child: list.isEmpty
                          ? Padding(padding: const EdgeInsets.all(12), child: Text('No folders in here.', style: body(context, size: .8125, color: t.dim)))
                          : ListView(shrinkWrap: true, children: [
                              for (final f in list)
                                Press(
                                  onTap: () => Claude.I.folders('${here.replaceAll(RegExp(r'/+$'), '')}/$f'),
                                  scale: 1,
                                  child: Container(
                                    constraints: const BoxConstraints(minHeight: 48),
                                    padding: const EdgeInsets.symmetric(horizontal: 12),
                                    child: Row(children: [
                                      WIcon('folder', color: t.accent),
                                      const SizedBox(width: 12),
                                      Expanded(child: Text(f, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, color: t.bright))),
                                      WIcon('right', size: 16, color: t.dim),
                                    ]),
                                  ),
                                ),
                            ]),
                    ),
                  ),
                ],
                const SizedBox(height: 12),
                Btn('Start Claude in ${_base(here)}', icon: 'play', primary: true, onTap: here.isEmpty
                    ? null
                    : () {
                        Navigator.of(context).pop();
                        Claude.I.start(here);
                      }),
                const SizedBox(height: 8),
                Btn('Cancel', onTap: () => Navigator.of(context).pop()),
              ]),
            ),
          ),
        );
      },
    );
  }
}
