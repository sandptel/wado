// The Computer page's Clipboard section (port of ui/computer/clips.rs + ui/clipboard/{mod,
// switches,filter,tile,words}.rs + clips.css): this phone's two sharing switches, then the
// computer's clipboard history — search, kind chips, and an entry per copy with Copy to this
// phone, Send back, Pin and Delete.
//
// Motion: entries rise in on the spring with a stagger; neighbours glide (FLIP) when the list
// changes; a copy just made arrives with an accent ring that fades; a tap's tick pops in, a refused
// one shakes once; a deleted entry folds away while it shrinks and fades.

import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/clip.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../motion/motion.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/data_image.dart';

String ago(int seen) {
  if (seen == 0) return '';
  final s = DateTime.now().millisecondsSinceEpoch ~/ 1000 - seen;
  if (s < 90) return 'just now';
  if (s < 3600) return '${s ~/ 60} min ago';
  if (s < 86400) return '${s ~/ 3600} h ago';
  return '${s ~/ 86400} d ago';
}

/// Where it was copied, in words (words.rs `origin`).
String origin(String from, String computer) => switch (from) {
      '' => '',
      'desktop' => computer,
      'session' => "$computer's wado session",
      _ => from,
    };

String meta(Map<String, dynamic> e, String computer) {
  final a = ago(((e['at_ms'] ?? 0) as int) ~/ 1000);
  final f = origin((e['from'] ?? '') as String, computer);
  return [f, a].where((x) => x.isNotEmpty).join(' · ');
}

class Clips extends StatefulWidget {
  const Clips({super.key});
  @override
  State<Clips> createState() => _ClipsState();
}

class _ClipsState extends State<Clips> {
  final _q = TextEditingController();
  String? _kind;

  @override
  void initState() {
    super.initState();
    Clip.I.list();
  }

  @override
  void dispose() {
    _q.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = Clip.I;
    return ListenableBuilder(
      listenable: Listenable.merge([c.entries, c.available, c.error, c.loaded, c.images, c.changed, Host.I.config]),
      builder: (context, _) {
        final key = Nav.I.selected;
        final name = CoreHost.I.computer(key)?.name ?? 'the computer';
        final g = Host.I.config.value?['grants'] as Map<String, dynamic>?;
        if (g != null && g['clipboard'] == false) {
          return WCard(children: [
            Text('The clipboard needs permission', style: body(context, w: FontWeight.w600, color: t.bright)),
            Why('On $name: wado allow "${g['name'] ?? 'this phone'}" clipboard — or wado tui → Devices.'),
          ]);
        }
        final all = c.entries.value;
        final q = _q.text.toLowerCase();
        final shown = [
          for (final e in all)
            if ((_kind == null || e['kind'] == _kind) && (q.isEmpty || '${e['preview']}'.toLowerCase().contains(q))) e
        ];
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          // This phone's two switches for it (switches.rs).
          Container(
            padding: const EdgeInsets.fromLTRB(14, 6, 14, 10),
            decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
            child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              SwitchRow(
                title: 'Copy from $name automatically',
                sub: 'What you copy there lands on this phone, ready to paste.',
                on: c.pref(key, 'pull'),
                onToggle: (v) => c.setPref(key, 'pull', v),
              ),
              SwitchRow(
                title: "Send this phone's copies automatically",
                sub: 'When you come back to wado, what you copied here goes to $name.',
                on: c.pref(key, 'push'),
                onToggle: (v) => c.setPref(key, 'push', v),
              ),
              const SizedBox(height: 4),
              Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Padding(padding: const EdgeInsets.only(top: 1), child: WIcon('lock', size: 15, color: t.dim)),
                const SizedBox(width: 8),
                Expanded(
                  child: Text('Passwords never travel: a copy a password manager marks as secret stays where it was made, and is kept in no history.',
                      style: body(context, size: .75, color: t.dim, height: 1.45)),
                ),
              ]),
            ]),
          ),
          const SizedBox(height: 10),
          if (!c.loaded.value)
            _Note(icon: null, text: 'Asking $name…')
          else ...[
            if (c.available.value && all.isNotEmpty) ...[
              _Search(controller: _q, onChanged: () => setState(() {})),
              const SizedBox(height: 8),
              Row(children: [
                for (final (k, icon, label) in const [(null, 'clip', 'All'), ('text', 'text', 'Text'), ('link', 'link', 'Links'), ('image', 'image', 'Pictures')]) ...[
                  if (k != null) const SizedBox(width: 6),
                  Expanded(
                    child: _KindChip(
                      icon: icon,
                      label: label,
                      n: all.where((e) => k == null || e['kind'] == k).length,
                      on: _kind == k,
                      onTap: () => setState(() => _kind = k),
                    ),
                  ),
                ],
              ]),
              const SizedBox(height: 10),
            ],
            if (c.available.value) ...[
              for (var i = 0; i < shown.length; i++)
                Glide(key: ValueKey('g${shown[i]['id']}'), child: _Tile(key: ValueKey(shown[i]['id']), e: shown[i], computer: name, i: i, img: c.images.value['${shown[i]['id']}'])),
              if (shown.isEmpty) _Note(icon: 'clip', text: all.isEmpty ? 'Nothing copied on $name yet.' : 'Nothing matches.', quiet: true),
            ] else
              _Note(icon: 'info', text: c.error.value.isEmpty ? 'Its clipboard shows here once it answers.' : c.error.value),
          ],
          const SizedBox(height: 10),
          Btn("Send this phone's clipboard", icon: 'paste', onTap: c.sendPhone),
        ]);
      },
    );
  }
}

class _Note extends StatelessWidget {
  const _Note({required this.icon, required this.text, this.quiet = false});
  final String? icon;
  final String text;
  final bool quiet;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
      child: Row(children: [
        if (icon == null)
          SizedBox(width: 14, height: 14, child: CircularProgressIndicator(strokeWidth: 2, color: t.accent, backgroundColor: t.s3))
        else
          WIcon(icon!, size: 18, color: t.dim),
        const SizedBox(width: 10),
        Expanded(child: Text(text, style: body(context, size: .8125, color: quiet ? t.dim : t.text, height: 1.45))),
      ]),
    );
  }
}

/// `.clipsearch`: the accent ring on focus.
class _Search extends StatefulWidget {
  const _Search({required this.controller, required this.onChanged});
  final TextEditingController controller;
  final VoidCallback onChanged;
  @override
  State<_Search> createState() => _SearchState();
}

class _SearchState extends State<_Search> {
  final _f = FocusNode();
  @override
  void initState() {
    super.initState();
    _f.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _f.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final on = _f.hasFocus;
    return AnimatedContainer(
      duration: t.dur,
      curve: Tokens.spring,
      constraints: const BoxConstraints(minHeight: 44),
      padding: const EdgeInsets.symmetric(horizontal: 12),
      decoration: BoxDecoration(
        color: t.s0,
        borderRadius: BorderRadius.circular(t.r1),
        border: Border.all(color: on ? t.accent : t.line),
        boxShadow: on ? [BoxShadow(color: fade(t.accent, 72), spreadRadius: 3)] : null,
      ),
      child: Row(children: [
        WIcon('search', size: 17, color: t.dim),
        const SizedBox(width: 8),
        Expanded(
          child: TextField(
            controller: widget.controller,
            focusNode: _f,
            onChanged: (_) => widget.onChanged(),
            style: body(context, size: 1, color: t.bright),
            decoration: InputDecoration(isCollapsed: true, border: InputBorder.none, hintText: 'Search the clipboard', hintStyle: body(context, size: 1, color: t.dim)),
          ),
        ),
      ]),
    );
  }
}

class _KindChip extends StatelessWidget {
  const _KindChip({required this.icon, required this.label, required this.n, required this.on, required this.onTap});
  final String icon, label;
  final int n;
  final bool on;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final fg = on ? t.onAccent : t.dim;
    final narrow = MediaQuery.sizeOf(context).width <= 380;
    return Press(
      onTap: onTap,
      label: '$label, $n',
      child: AnimatedContainer(
        duration: t.dur,
        curve: Tokens.spring,
        height: 44,
        padding: const EdgeInsets.symmetric(horizontal: 6),
        decoration: BoxDecoration(color: on ? t.accent : t.s2, borderRadius: BorderRadius.circular(999)),
        child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
          WIcon(icon, size: 15, color: fg),
          if (!narrow) ...[
            const SizedBox(width: 5),
            Flexible(child: Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, w: FontWeight.w600, color: fg))),
          ],
          const SizedBox(width: 5),
          Text('$n', style: mono(context, size: .6875, w: FontWeight.w600, color: fade(fg, 20))),
        ]),
      ),
    );
  }
}

/// One entry (tile.rs): what it is and where it came from, its text or picture, and its actions.
class _Tile extends StatefulWidget {
  const _Tile({super.key, required this.e, required this.computer, required this.i, required this.img});
  final Map<String, dynamic> e;
  final String computer;
  final int i;
  final String? img;
  @override
  State<_Tile> createState() => _TileState();
}

class _TileState extends State<_Tile> with SingleTickerProviderStateMixin {
  bool _leaving = false;
  late final bool _fresh;
  late final _ring = AnimationController(vsync: this, duration: const Duration(milliseconds: 1800));

  @override
  void initState() {
    super.initState();
    final at = (widget.e['at_ms'] ?? 0) as int;
    // Copied in the last few seconds: it arrives with a mark.
    _fresh = at > 0 && DateTime.now().millisecondsSinceEpoch - at < 6000;
    if (_fresh) _ring.forward();
    if (widget.e['kind'] == 'image') Clip.I.thumb('${widget.e['id']}');
  }

  @override
  void dispose() {
    _ring.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final e = widget.e;
    final id = '${e['id']}';
    final kind = '${e['kind']}';
    final pinned = e['pinned'] == true;
    final hue = switch (kind) { 'link' => t.base[0xE], 'image' => t.base[0xA], _ => t.base[0xC] };
    final m = meta(e, widget.computer);
    final preview = '${e['preview']}';
    final card = Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: t.s1,
        borderRadius: BorderRadius.circular(t.r3),
        border: Border.all(color: pinned ? fade(t.accent, 30) : Colors.transparent, width: 1.5),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Enter(
            ms: _fresh ? 420 : 1,
            dy: 0,
            scale: _fresh ? .6 : 1,
            pop: true,
            fadeOnClose: false,
            child: Container(
              width: 38,
              height: 38,
              decoration: BoxDecoration(shape: BoxShape.circle, color: fade(hue, 80)),
              alignment: Alignment.center,
              child: WIcon(kind == 'image' ? 'image' : (kind == 'link' ? 'link' : 'text'), size: 18, color: mix(hue, t.bright, 30)),
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: kind == 'image'
                ? Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                    ClipRRect(
                      borderRadius: BorderRadius.circular(t.r2),
                      child: Container(
                        height: 120,
                        color: t.s2,
                        child: widget.img == null
                            ? null
                            : Enter(ms: 480, dy: 0, scale: 1.04, fadeOnClose: false, child: Image(image: dataImage(widget.img!)!, fit: BoxFit.cover, width: double.infinity, gaplessPlayback: true)),
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(preview, style: mono(context, size: .7188, color: t.dim)),
                  ])
                : Text(preview,
                    maxLines: kind == 'link' ? 2 : 3,
                    overflow: TextOverflow.ellipsis,
                    style: mono(context, size: .8125, color: kind == 'link' ? mix(t.base[0xE], t.bright, 50) : t.text).copyWith(height: 1.45)),
          ),
        ]),
        if (pinned || m.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(left: 50),
            child: Wrap(spacing: 8, crossAxisAlignment: WrapCrossAlignment.center, children: [
              if (pinned)
                Row(mainAxisSize: MainAxisSize.min, children: [
                  WIcon('pin', size: 13, color: t.accent),
                  const SizedBox(width: 3),
                  Text('Pinned', style: body(context, size: .75, w: FontWeight.w600, color: t.accent)),
                ]),
              if (m.isNotEmpty) Text(m, style: body(context, size: .75, color: t.dim)),
            ]),
          ),
        const SizedBox(height: 8),
        Row(mainAxisAlignment: MainAxisAlignment.end, children: [
          _Act(icon: 'copy', done: true, go: true, label: 'Copy to this phone', run: () => Clip.I.copy(id, kind)),
          const SizedBox(width: 6),
          _Act(icon: 'laptop', done: true, label: "Put on ${widget.computer}'s clipboard", run: () => Clip.I.sendBack(id)),
          const SizedBox(width: 6),
          _Act(icon: 'pin', on: pinned, label: pinned ? 'Unpin' : 'Pin', run: () async {
            Clip.I.pin(id, !pinned);
            return true;
          }),
          const SizedBox(width: 6),
          _Act(icon: 'trash', dim: true, label: 'Delete', run: () async {
            setState(() => _leaving = true);
            Future.delayed(const Duration(milliseconds: 260), () => Clip.I.delete(id));
            return true;
          }),
        ]),
      ]),
    );
    // Just copied: an accent ring that fades out.
    final ringed = !_fresh
        ? card
        : Stack(children: [
            card,
            Positioned.fill(
              child: IgnorePointer(
                child: AnimatedBuilder(
                  animation: _ring,
                  builder: (context, _) {
                    final v = _ring.value;
                    final o = v < .12 ? v / .12 : 1 - Tokens.spring.transform((v - .12) / .88);
                    return Opacity(
                      opacity: o.clamp(0, 1),
                      child: DecoratedBox(decoration: BoxDecoration(borderRadius: BorderRadius.circular(t.r3), border: Border.all(color: t.accent, width: 2))),
                    );
                  },
                ),
              ),
            ),
          ]);
    // A deleted entry folds its row to nothing while it shrinks and fades (`.leaving`).
    return Enter(
      delay: Duration(milliseconds: widget.i.clamp(0, 12) * 40),
      ms: 480,
      dy: 10,
      scale: .97,
      fadeOnClose: false,
      child: AnimatedSize(
        duration: t.dur,
        curve: Tokens.spring,
        alignment: Alignment.topCenter,
        child: _leaving
            ? const SizedBox(width: double.infinity, height: 0)
            : Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: AnimatedScale(
                  scale: _leaving ? .96 : 1,
                  duration: t.dur,
                  child: ringed,
                ),
              ),
      ),
    );
  }
}

/// `.clipact`: 44 px round; a copy or send swaps its glyph for a popped tick, a refusal shakes.
class _Act extends StatefulWidget {
  const _Act({required this.icon, required this.label, required this.run, this.done = false, this.go = false, this.on = false, this.dim = false});
  final String icon, label;
  final Future<bool> Function() run;
  final bool done, go, on, dim;
  @override
  State<_Act> createState() => _ActState();
}

class _ActState extends State<_Act> with SingleTickerProviderStateMixin {
  String _state = ''; // ok · no
  late final _shake = AnimationController(vsync: this, duration: const Duration(milliseconds: 320));

  @override
  void dispose() {
    _shake.dispose();
    super.dispose();
  }

  Future<void> _tap() async {
    final ok = await widget.run();
    if (!mounted || !widget.done) return;
    setState(() => _state = ok ? 'ok' : 'no');
    if (!ok) _shake.forward(from: 0);
    final s = _state;
    Future.delayed(const Duration(milliseconds: 1600), () {
      if (mounted && _state == s) setState(() => _state = '');
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final ok = _state == 'ok';
    final bg = ok
        ? fade(t.good, 75)
        : widget.on
            ? t.accent
            : widget.go
                ? fade(t.accent, 78)
                : t.s2;
    final fg = ok
        ? t.good
        : _state == 'no'
            ? t.bad
            : widget.on
                ? t.onAccent
                : widget.go
                    ? mix(t.accent, t.bright, 35)
                    : (widget.dim ? t.dim : t.bright);
    return Press(
      onTap: _tap,
      label: widget.label,
      scale: .9,
      child: AnimatedBuilder(
        animation: _shake,
        builder: (context, child) {
          final v = _shake.value;
          final dx = !_shake.isAnimating ? 0.0 : (v < .2 ? -4 * v / .2 : v < .45 ? -4 + 8 * (v - .2) / .25 : v < .7 ? 4 - 6 * (v - .45) / .25 : -2 + 2 * (v - .7) / .3);
          return Transform.translate(offset: Offset(dx, 0), child: child);
        },
        child: AnimatedContainer(
          duration: t.dur,
          curve: Tokens.spring,
          width: 44,
          height: 44,
          decoration: BoxDecoration(shape: BoxShape.circle, color: bg),
          child: Stack(alignment: Alignment.center, children: [
            AnimatedOpacity(
              opacity: ok ? 0 : 1,
              duration: t.dur,
              child: AnimatedScale(scale: ok ? .4 : 1, duration: t.dur, curve: t.pop, child: WIcon(widget.icon, size: 18, color: fg)),
            ),
            AnimatedOpacity(
              opacity: ok ? 1 : 0,
              duration: t.dur,
              child: AnimatedScale(scale: ok ? 1 : .4, duration: t.dur, curve: t.pop, child: WIcon('check', size: 18, color: fg)),
            ),
          ]),
        ),
      ),
    );
  }
}
