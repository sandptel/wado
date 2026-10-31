// The four reasons to open a computer (port of ui/computer/actions.rs + input/tile.rs +
// input/open.rs, computer.css `.act`, vi.css `.act.vi`): its desktop, this phone as its input,
// its files, a shell. Big tiles, Desktop first and loudest. Each says what it will do *now*, and a
// door this phone may not use says so instead of failing after the tap.
import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/reach.dart';
import '../../wado/relay_link.dart';
import '../../wado/session.dart';
import '../../wado/shells.dart';
import '../home/running.dart';
import '../input/pad_screen.dart';
import '../motion/motion.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';

enum Door { touchpad, keyboard, gamepad }

Map<String, dynamic>? get _grants => Host.I.config.value?['grants'] as Map<String, dynamic>?;
bool _may(String g) => _grants == null || _grants![g] == true;

/// Why a door is shut, in plain words; null when it opens (input/open.rs `shut`).
String? shut(Reach r, Door d) {
  if (!r.online) return 'Offline';
  if (!_may('input')) return 'Not allowed for this phone';
  final caps = (Host.I.config.value?['caps'] as List?)?.cast<String>();
  if (d == Door.gamepad && caps != null && !caps.contains('pad-gamepad')) return 'Update wado on the computer';
  return null;
}

void openDoor(BuildContext context, Door d) => switch (d) {
      Door.touchpad => openPad(context),
      Door.keyboard => openPad(context, keyboard: true),
      Door.gamepad => openGamepad(context),
    };

/// The Desktop door: take over a computer in use elsewhere, rejoin what runs, or start anew.
void desktop(Reach r) {
  final running = Host.I.sessions.value.isNotEmpty;
  final idle = RelayLink.I.seatBusy.value?.idle == true;
  if (r.busy && idle) {
    // The other device holds the seat with nothing running: one tap moves it here and starts.
    Session.I.start();
  } else if (r.busy) {
    RelayLink.I.takeover();
  } else if (running) {
    joinBest();
  } else {
    Session.I.start();
  }
}

class ActionTiles extends StatelessWidget {
  const ActionTiles({super.key, required this.r});
  final Reach r;

  @override
  Widget build(BuildContext context) {
    final list = Host.I.sessions.value;
    final running = list.isNotEmpty;
    final off = !r.online;
    final cfg = Host.I.config.value;
    final filesOk = _grants == null || _grants!['files'] != 'none';
    final shellOk = _may('shells') && (cfg == null || cfg['shells'] != false);
    final shellsOpen = Shells.I.shells.value.where((s) => s['alive'] != false).length;
    final seat = RelayLink.I.seatBusy.value;
    final idleElsewhere = r.busy && seat?.idle == true;
    final String deskSub;
    if (idleElsewhere) {
      deskSub = 'Use it here — ${seat!.holder.isEmpty ? 'your other device' : seat.holder} is connected, nothing running';
    } else if (r.busy) {
      deskSub = 'In use on another device';
    } else if (running) {
      final apps = list.expand((s) => (s['apps'] as List? ?? [])).map((a) => short('$a')).toList();
      deskSub = switch (apps.length) { 0 => 'Rejoin · empty desktop', 1 => 'Rejoin · ${apps[0]}', _ => 'Rejoin · ${apps[0]} and ${apps.length - 1} more' };
    } else {
      deskSub = "Start at this screen's size";
    }
    final tiles = <Widget>[
      _Act(
        icon: 'play',
        fill: true,
        title: 'Desktop',
        sub: deskSub,
        primary: true,
        badge: running && !r.busy ? 'running' : (r.busy && !idleElsewhere ? 'take over' : null),
        onTap: off && !r.busy ? null : () => desktop(r),
      ),
      _ViTile(r: r),
      _Act(icon: 'folder', title: 'Files', sub: filesOk ? 'Browse, send, download' : 'Not allowed for this phone', onTap: off || !filesOk ? null : () => Nav.I.openPage('files')),
      _Act(
        icon: 'term',
        title: 'Shell',
        sub: shellOk ? 'A terminal, no video' : 'Not allowed for this phone',
        badge: shellsOpen > 0 ? '$shellsOpen open' : null,
        onTap: off || !shellOk ? null : () => Nav.I.openPage('shells'),
      ),
    ];
    // `repeat(auto-fit, minmax(max(8.5rem, 50% - 4px), 1fr))`: two to a row, rows as tall as
    // their tallest tile; one to a row when the column is too narrow for two.
    return LayoutBuilder(builder: (context, box) {
      if (box.maxWidth < 2 * 136 + 8) return Column(children: [for (final x in tiles) Padding(padding: const EdgeInsets.only(bottom: 8), child: x)]);
      Widget row(Widget a, Widget b) => IntrinsicHeight(
            child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Expanded(child: a), const SizedBox(width: 8), Expanded(child: b)]),
          );
      return Column(children: [row(tiles[0], tiles[1]), const SizedBox(height: 8), row(tiles[2], tiles[3])]);
    });
  }
}

/// `.act`: icon square, name, what it will do; the badge pops in a beat after the page.
class _Act extends StatefulWidget {
  const _Act({required this.icon, required this.title, required this.sub, required this.onTap, this.primary = false, this.badge, this.fill = false});
  final String icon, title, sub;
  final VoidCallback? onTap;
  final bool primary, fill;
  final String? badge;
  @override
  State<_Act> createState() => _ActState();
}

class _ActState extends State<_Act> {
  bool _down = false;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final p = widget.primary;
    final fg = p ? t.onAccent : t.bright;
    return Press(
      onTap: widget.onTap,
      label: widget.title,
      onPressed: (d) => setState(() => _down = d),
      child: Container(
        constraints: const BoxConstraints(minHeight: 104),
        padding: const EdgeInsets.fromLTRB(14, 12, 14, 14),
        clipBehavior: Clip.antiAlias,
        decoration: BoxDecoration(
          color: p ? t.accent : t.s2,
          borderRadius: BorderRadius.circular(t.r3),
          boxShadow: p && widget.onTap != null ? [BoxShadow(color: fade(t.accent, 72), blurRadius: 30, offset: const Offset(0, 10))] : null,
        ),
        child: Stack(children: [
          Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
            // The icon square gives a little more than the tile (`.act:active .aic`).
            AnimatedSlide(
              offset: Offset(0, _down ? 2 / 42 : 0),
              duration: _down ? t.durPress : const Duration(milliseconds: 220),
              curve: _down ? Tokens.spring : t.pop,
              child: Aic(icon: widget.icon, primary: p, fill: widget.fill),
            ),
            const SizedBox(height: 14),
            Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(widget.title, style: display(context, rem(context, 1.0625), color: fg, ls: -.17).copyWith(height: 1.1)),
              const SizedBox(height: 2),
              Text(widget.sub, style: body(context, size: .75, color: p ? fade(t.onAccent, 20) : t.dim)),
            ]),
          ]),
          if (widget.badge != null) Positioned(top: 0, right: 0, child: Badge(widget.badge!, primary: p)),
        ]),
      ),
    );
  }
}

/// `.aic`: the 42 px icon square (a circle on the primary tile).
class Aic extends StatelessWidget {
  const Aic({super.key, required this.icon, this.primary = false, this.fill = false});
  final String icon;
  final bool primary, fill;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Container(
      width: 42,
      height: 42,
      decoration: BoxDecoration(
        color: primary ? fade(t.onAccent, 84) : t.s3,
        borderRadius: BorderRadius.circular(primary ? 21 : t.r2),
      ),
      alignment: Alignment.center,
      child: WIcon(icon, color: primary ? t.onAccent : t.bright, fill: fill),
    );
  }
}

/// `.act .badge`: a mono pill that pops in 260 ms after the page (`popin`).
class Badge extends StatelessWidget {
  const Badge(this.text, {super.key, this.primary = false});
  final String text;
  final bool primary;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Enter(
      delay: const Duration(milliseconds: 260),
      ms: 360,
      dy: 0,
      scale: 0,
      pop: true,
      fadeOnClose: false,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(color: primary ? fade(t.onAccent, 82) : t.s3, borderRadius: BorderRadius.circular(999)),
        child: Text(text, style: mono(context, size: .6875, w: FontWeight.w600, color: primary ? t.onAccent : t.bright)),
      ),
    );
  }
}

/// The Virtual input tile (input/tile.rs): a group, not a button. Its heading opens the hub;
/// three 44 px doors open Touchpad, Keyboard or Gamepad in one tap.
class _ViTile extends StatelessWidget {
  const _ViTile({required this.r});
  final Reach r;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    const doors = [(Door.touchpad, 'Touchpad', 'trackpad'), (Door.keyboard, 'Keyboard', 'kbd'), (Door.gamepad, 'Gamepad', 'pad')];
    final allShut = doors.every((d) => shut(r, d.$1) != null);
    final why = shut(r, Door.touchpad);
    final sub = why != null && allShut ? why : 'This phone as its…';
    return Container(
      constraints: const BoxConstraints(minHeight: 104),
      clipBehavior: Clip.antiAlias,
      decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r3)),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Expanded(
          child: _ViHead(sub: sub),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(8, 0, 8, 8),
          child: Row(children: [
            for (var i = 0; i < doors.length; i++) ...[
              if (i > 0) const SizedBox(width: 6),
              Expanded(child: _ViDoor(door: doors[i].$1, name: doors[i].$2, icon: doors[i].$3, why: shut(r, doors[i].$1))),
            ],
          ]),
        ),
      ]),
    );
  }
}

/// `.vihead`: tints instead of sinking (`.act.vi .vihead:active { background: var(--s3) }`).
class _ViHead extends StatefulWidget {
  const _ViHead({required this.sub});
  final String sub;
  @override
  State<_ViHead> createState() => _ViHeadState();
}

class _ViHeadState extends State<_ViHead> {
  bool _down = false;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Press(
      onTap: () => Nav.I.openPage('input'),
      label: 'Virtual input',
      scale: 1,
      onPressed: (d) => setState(() => _down = d),
      child: AnimatedContainer(
        duration: t.dur,
        color: _down ? t.s3 : Colors.transparent,
        padding: const EdgeInsets.fromLTRB(14, 12, 14, 8),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
          const Aic(icon: 'phone'),
          const SizedBox(height: 14),
          Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text('Virtual input', style: display(context, rem(context, 1.0625), ls: -.17).copyWith(height: 1.1)),
            const SizedBox(height: 2),
            Text(widget.sub, style: body(context, size: .75, color: t.dim)),
          ]),
        ]),
      ),
    );
  }
}

class _ViDoor extends StatelessWidget {
  const _ViDoor({required this.door, required this.name, required this.icon, required this.why});
  final Door door;
  final String name, icon;
  final String? why;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final label = why == null ? name : '$name — $why';
    return Tooltip(
      message: label,
      child: Press(
        onTap: why == null ? () => openDoor(context, door) : null,
        enabled: why == null,
        label: label,
        child: Container(
          height: 44,
          decoration: BoxDecoration(color: t.s3, borderRadius: BorderRadius.circular(t.r2)),
          alignment: Alignment.center,
          child: WIcon(icon, color: t.bright),
        ),
      ),
    );
  }
}
