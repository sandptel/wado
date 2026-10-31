// The session's controls, each one node in both arrangements (port of ui/link_badge.rs,
// surround/top_band.rs, surround/window_strip.rs, surround/faces.rs, workspaces.rs and dock.rs):
// the link badge, the computer's name (or the software-encoding chip in its place), the people
// chip, the workspace bar, the window strip, the input faces, and the dock's and rail's buttons.
import 'package:flutter/material.dart' hide Flow;

import '../../../core/core_host.dart';
import '../../../wado/console_layout.dart';
import '../../../wado/link_state.dart';
import '../../../wado/multi.dart';
import '../../../wado/nav.dart';
import '../../../wado/session.dart';
import '../../home/cards.dart' show hues;
import '../../motion/motion.dart';
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';

/// `.barbtn`: a round 44 (or 40 on the rail) button.
class BarBtn extends StatelessWidget {
  const BarBtn({super.key, required this.icon, required this.label, required this.onTap, this.size = 44, this.minW, this.on = false, this.led, this.child});
  final String icon, label;
  final VoidCallback? onTap;
  final double size;
  final double? minW;
  final bool on;
  /// The ⋯ LED: ok · warn · bad · sw (software encoding — breathing, never off).
  final String? led;
  final Widget? child;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final ledC = switch (led) { 'warn' => t.base[0xA], 'bad' => t.bad, 'sw' => t.warn, _ => t.good };
    return Press(
      onTap: onTap,
      label: label,
      child: Container(
        constraints: BoxConstraints(minWidth: minW ?? size, minHeight: size),
        decoration: BoxDecoration(color: on ? t.accent : null, borderRadius: BorderRadius.circular(999)),
        child: Stack(alignment: Alignment.center, children: [
          child ?? WIcon(icon, color: on ? t.onAccent : t.text),
          if (led != null)
            Positioned(
              top: 9,
              right: ((minW ?? size) - 28) / 2,
              child: Breathe(
                on: led == 'sw',
                child: Container(
                  width: 6,
                  height: 6,
                  decoration: BoxDecoration(shape: BoxShape.circle, color: ledC, boxShadow: [BoxShadow(color: t.s1, spreadRadius: 2)]),
                ),
              ),
            ),
        ]),
      ),
    );
  }
}

/// The link badge: which path this device is on, always on screen. During a healthy direct
/// session in the cockpit it folds to its dot, so a game keeps its corner; tap for what it means.
class LinkBadge extends StatefulWidget {
  const LinkBadge({super.key, required this.quietable, required this.waiting});
  /// The cockpit: a healthy direct link folds to the dot.
  final bool quietable;
  /// No picture: the stage says all this itself, so no card over it.
  final bool waiting;
  @override
  State<LinkBadge> createState() => _LinkBadgeState();
}

class _LinkBadgeState extends State<LinkBadge> {
  bool _open = false;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ValueListenableBuilder(
      valueListenable: Link.I.now,
      builder: (context, l, _) {
        final tone = switch (l.tone) { 'warn' => t.base[0x9], 'bad' => t.bad, _ => t.good };
        final quiet = widget.quietable && l.state == 'direct' && !_open;
        return Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
          Press(
            onTap: () => setState(() => _open = !_open),
            label: l.label,
            child: AnimatedContainer(
              duration: t.dur,
              curve: Tokens.spring,
              height: 32,
              width: quiet ? 32 : null,
              padding: quiet ? EdgeInsets.zero : const EdgeInsets.fromLTRB(10, 0, 12, 0),
              decoration: BoxDecoration(
                color: quiet ? Colors.transparent : fade(t.s1, 12),
                borderRadius: BorderRadius.circular(999),
                border: Border.all(color: quiet ? Colors.transparent : fade(tone, 60)),
              ),
              child: Row(mainAxisSize: MainAxisSize.min, mainAxisAlignment: MainAxisAlignment.center, children: [
                Breathe(on: l.tone == 'warn', child: Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: tone))),
                if (!quiet) ...[const SizedBox(width: 7), Text(l.label, style: body(context, size: .8125, w: FontWeight.w600, color: t.bright).copyWith(height: 1))],
              ]),
            ),
          ),
          if (_open && !widget.waiting)
            Enter(
              ms: 360,
              dy: -6,
              fadeOnClose: false,
              child: Container(
                margin: const EdgeInsets.only(top: 6),
                width: 320,
                padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
                decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r2), border: Border.all(color: t.line)),
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                  Text(l.label, style: body(context, size: .8125, w: FontWeight.w700, color: tone)),
                  const SizedBox(height: 8),
                  Text(l.detail, style: body(context, size: .8125)),
                  if (l.state == 'relayed') ...[const SizedBox(height: 8), Btn('Retry direct', expand: false, onTap: () => Session.I.negotiate())],
                ]),
              ),
            ),
        ]);
      },
    );
  }
}

/// `.cname`: the computer's name with its hue dot — or "Software encoding" in its place
/// (invariant 5: the status is never dropped for lack of room).
class NameChip extends StatelessWidget {
  const NameChip({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ValueListenableBuilder(
      valueListenable: Session.I.encoder,
      builder: (context, enc, _) {
        if (enc.$1 == 'software') {
          return Container(
            height: 28,
            padding: const EdgeInsets.symmetric(horizontal: 10),
            decoration: BoxDecoration(color: fade(t.warn, 85), borderRadius: BorderRadius.circular(999)),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              Breathe(child: Container(width: 7, height: 7, decoration: BoxDecoration(shape: BoxShape.circle, color: t.warn))),
              const SizedBox(width: 6),
              Text('Software encoding', style: body(context, size: .75, w: FontWeight.w600, color: t.warn)),
            ]),
          );
        }
        final comps = CoreHost.I.computers.value;
        final i = comps.indexWhere((c) => c.key == Nav.I.selected);
        return Row(mainAxisSize: MainAxisSize.min, children: [
          Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: slot(context, hues[(i < 0 ? 0 : i) % 6]))),
          const SizedBox(width: 8),
          Flexible(
            child: Text(i < 0 ? '' : comps[i].name, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, .9375), ls: -.15)),
          ),
        ]);
      },
    );
  }
}

/// `#wado-here`: who else is in this session, while someone is.
class PeopleChip extends StatelessWidget {
  const PeopleChip({super.key, this.shadow = true});
  final bool shadow;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ValueListenableBuilder(
      valueListenable: Multi.I.list,
      builder: (context, list, _) {
        final others = [for (final p in list) if (p['id'] != Multi.I.you.value) '${p['name']}'];
        return AnimatedSlide(
          offset: Offset(0, others.isEmpty ? -.45 : 0),
          duration: t.durSheet,
          curve: Tokens.spring,
          child: AnimatedOpacity(
            opacity: others.isEmpty ? 0 : 1,
            duration: t.dur,
            child: Container(
              height: 32,
              constraints: const BoxConstraints(maxWidth: 260),
              padding: const EdgeInsets.fromLTRB(10, 0, 12, 0),
              decoration: BoxDecoration(
                color: fade(t.s1, 12),
                borderRadius: BorderRadius.circular(999),
                boxShadow: shadow ? const [BoxShadow(color: Color(0x8C000000), blurRadius: 24, offset: Offset(0, 6), spreadRadius: -10)] : null,
              ),
              child: Row(mainAxisSize: MainAxisSize.min, children: [
                Breathe(ms: 2000, child: Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: t.good))),
                const SizedBox(width: 7),
                Text('${list.length} here', style: display(context, rem(context, .8125)).copyWith(fontFeatures: const [FontFeature.tabularFigures()])),
                const SizedBox(width: 6),
                Flexible(child: Text(others.join(', '), maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, w: FontWeight.w500))),
              ]),
            ),
          ),
        );
      },
    );
  }
}

String _short(String app) {
  final last = app.split('.').last;
  return last.isEmpty ? '' : last[0].toUpperCase() + last.substring(1);
}

/// The window's app: its entry's name by app id, else its id, capitalised.
String appName(Map<String, dynamic> w) {
  final id = '${w['app_id'] ?? ''}'.toLowerCase();
  for (final a in Session.I.apps.value) {
    if ((a['app_ids'] as List? ?? []).any((x) => '$x'.toLowerCase() == id)) return '${a['name']}';
  }
  return _short('${w['app_id'] ?? ''}');
}

String? iconOf(Map<String, dynamic> w) {
  final id = '${w['app_id'] ?? ''}'.toLowerCase();
  for (final a in Session.I.apps.value) {
    if ((a['app_ids'] as List? ?? []).any((x) => '$x'.toLowerCase() == id)) return a['icon'] as String?;
  }
  return null;
}

String letter(Map<String, dynamic> w) {
  final src = '${w['app_id'] ?? ''}'.isEmpty ? '${w['title'] ?? ''}' : '${w['app_id']}';
  final base = src.split('.').last;
  return base.isEmpty ? '?' : base[0].toUpperCase();
}

/// An app's icon (a data URI or a URL from the daemon), or its letter tile.
class AppGlyph extends StatelessWidget {
  const AppGlyph({super.key, required this.w, this.size = 26});
  final Map<String, dynamic> w;
  final double size;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final src = iconOf(w);
    if (src != null && src.startsWith('data:')) {
      try {
        return Image.memory(UriData.parse(src).contentAsBytes(), width: size, height: size, gaplessPlayback: true);
      } catch (_) {}
    }
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(color: t.s3, borderRadius: BorderRadius.circular(8)),
      child: Text(letter(w), style: body(context, size: .8125, w: FontWeight.w700, color: t.bright)),
    );
  }
}

/// The workspace bar: a pill per workspace in use — its number, and on the one showing, its
/// apps' icons. Tap a number to switch, an icon to focus, hold an icon for its sheet. `+` opens an
/// empty workspace. `vertical`: the console's left rail.
class WsBar extends StatelessWidget {
  const WsBar({super.key, required this.vertical, required this.onWindowMenu, this.big = false});
  final bool vertical, big;
  final ValueChanged<int> onWindowMenu;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Session.I;
    return ListenableBuilder(
      listenable: Listenable.merge([s.windows, s.workspace, s.apps]),
      builder: (context, _) {
        final wins = s.windows.value;
        final active = s.workspace.value < 1 ? 1 : s.workspace.value;
        final used = {for (final w in wins) ((w['workspace'] ?? 1) as int).clamp(1, 99), active}.toList()..sort();
        final next = used.last + 1;
        final b = big ? 44.0 : 34.0;
        Widget num(int n, bool on) => Press(
              onTap: () => s.window({'workspace': {'n': n}}),
              label: 'Workspace $n',
              child: Container(
                constraints: BoxConstraints(minWidth: b, minHeight: b),
                decoration: BoxDecoration(color: on ? t.accent : t.s2, borderRadius: BorderRadius.circular(999)),
                child: Center(widthFactor: 1, heightFactor: 1, child: Text('$n', style: mono(context, size: .8125, w: FontWeight.w700, color: on ? t.onAccent : t.dim))),
              ),
            );
        Widget icon(Map<String, dynamic> w) {
          final focused = w['focused'] == true;
          return GestureDetector(
            onLongPress: () {
              buzzTap();
              onWindowMenu((w['id'] ?? 0) as int);
            },
            child: Press(
              onTap: () => s.window({'focus': {'id': w['id']}}),
              label: '${w['title'] ?? w['app_id'] ?? ''}',
              scale: .88,
              child: Enter(
                ms: 260,
                dy: 0,
                scale: .4,
                pop: true,
                fadeOnClose: false,
                child: SizedBox(
                  width: b,
                  height: b,
                  child: Stack(alignment: Alignment.center, children: [
                    AppGlyph(w: w),
                    if (focused)
                      Positioned(
                        bottom: vertical ? null : 0,
                        left: vertical ? -2 : null,
                        child: Container(width: vertical ? 3 : 12, height: vertical ? 12 : 3, decoration: BoxDecoration(color: t.accent, borderRadius: BorderRadius.circular(2))),
                      ),
                  ]),
                ),
              ),
            ),
          );
        }

        final pills = <Widget>[
          for (final n in used)
            () {
              final here = wins.where((w) => ((w['workspace'] ?? 1) as int).clamp(1, 99) == n).toList();
              final on = n == active;
              final kids = <Widget>[
                num(n, on),
                if (on)
                  for (final w in here.take(5)) icon(w)
                else if (here.isNotEmpty)
                  Padding(
                    padding: vertical ? const EdgeInsets.fromLTRB(0, 2, 0, 6) : const EdgeInsets.fromLTRB(2, 0, 6, 0),
                    child: Flex(direction: vertical ? Axis.vertical : Axis.horizontal, mainAxisSize: MainAxisSize.min, children: [
                      for (var i = 0; i < here.length.clamp(0, 3); i++)
                        Padding(padding: const EdgeInsets.all(1.5), child: Container(width: 5, height: 5, decoration: BoxDecoration(shape: BoxShape.circle, color: t.dim))),
                    ]),
                  ),
              ];
              return AnimatedContainer(
                duration: t.durSheet,
                curve: Tokens.spring,
                padding: on ? (vertical ? const EdgeInsets.fromLTRB(2, 2, 2, 6) : const EdgeInsets.fromLTRB(2, 2, 6, 2)) : const EdgeInsets.all(2),
                decoration: BoxDecoration(color: on ? fade(t.accent, 78) : null, borderRadius: BorderRadius.circular(999)),
                child: Flex(direction: vertical ? Axis.vertical : Axis.horizontal, mainAxisSize: MainAxisSize.min, children: [
                  for (var i = 0; i < kids.length; i++) ...[if (i > 0) const SizedBox(width: 2, height: 2), kids[i]],
                ]),
              );
            }(),
          Press(
            onTap: () => s.window({'workspace': {'n': next}}),
            label: 'New workspace',
            child: Container(
              width: b,
              height: b,
              alignment: Alignment.center,
              child: Dashed2(color: t.line, child: SizedBox(width: b, height: b, child: Center(child: WIcon('plus', size: 15, color: t.dim)))),
            ),
          ),
        ];
        return SingleChildScrollView(
          scrollDirection: vertical ? Axis.vertical : Axis.horizontal,
          child: Flex(direction: vertical ? Axis.vertical : Axis.horizontal, mainAxisSize: MainAxisSize.min, children: [
            for (var i = 0; i < pills.length; i++) ...[if (i > 0) SizedBox(width: vertical ? 0 : 4, height: vertical ? 6 : 0), pills[i]],
          ]),
        );
      },
    );
  }
}

void buzzTap() {}

/// A dashed circle (`.wsnum.add`).
class Dashed2 extends StatelessWidget {
  const Dashed2({super.key, required this.color, required this.child});
  final Color color;
  final Widget child;
  @override
  Widget build(BuildContext context) => CustomPaint(painter: _Ring(color), child: child);
}

class _Ring extends CustomPainter {
  _Ring(this.c);
  final Color c;
  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = c
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.5;
    final r = size.shortestSide / 2 - 1;
    const n = 14;
    for (var i = 0; i < n; i++) {
      canvas.drawArc(Rect.fromCircle(center: size.center(Offset.zero), radius: r), i * 2 * 3.14159 / n, 3.14159 / n, false, p);
    }
  }

  @override
  bool shouldRepaint(_Ring o) => o.c != c;
}

/// The window strip, the bezel's chin: the focused window (its title opens the window sheet),
/// Minimise, Maximise or restore, and Close (10 px apart: not a slip of the thumb).
class WindowStrip extends StatelessWidget {
  const WindowStrip({super.key, required this.onWindowMenu});
  final ValueChanged<int> onWindowMenu;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Session.I;
    return ListenableBuilder(
      listenable: Listenable.merge([s.windows, s.apps, s.hasVideo]),
      builder: (context, _) {
        if (!s.hasVideo.value) return const SizedBox.shrink();
        final w = s.windows.value.where((w) => w['focused'] == true).firstOrNull;
        if (w == null) {
          return Align(alignment: Alignment.centerLeft, child: Padding(padding: const EdgeInsets.only(left: 12), child: Text('No window open', style: body(context, size: .8125, color: t.dim))));
        }
        final app = appName(w);
        final title = '${w['title'] ?? ''}'.isEmpty ? app : '${w['title']}';
        Widget win(String icon, String label, Object action, {bool close = false}) => _WinBtn(icon: icon, label: '$label $app', close: close, onTap: () => s.window(action));
        return Row(children: [
          Expanded(
            child: Press(
              onTap: () => onWindowMenu((w['id'] ?? 0) as int),
              scale: .98,
              label: 'Window: $app, $title. Open its window sheet.',
              child: Container(
                height: 44,
                padding: const EdgeInsets.fromLTRB(6, 0, 10, 0),
                child: Row(children: [
                  SizedBox(width: 28, height: 28, child: Center(child: AppGlyph(w: w))),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisAlignment: MainAxisAlignment.center, children: [
                      Text(title, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .8438, w: FontWeight.w600, color: t.bright)),
                      Text(app, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .7188, color: t.dim)),
                    ]),
                  ),
                ]),
              ),
            ),
          ),
          const SizedBox(width: 6),
          win('winmin', 'Minimise', 'minimize'),
          const SizedBox(width: 6),
          win('winmax', 'Maximise or restore', 'maximize'),
          const SizedBox(width: 10),
          win('x', 'Close', 'close', close: true),
        ]);
      },
    );
  }
}

class _WinBtn extends StatefulWidget {
  const _WinBtn({required this.icon, required this.label, required this.onTap, this.close = false});
  final String icon, label;
  final VoidCallback onTap;
  final bool close;
  @override
  State<_WinBtn> createState() => _WinBtnState();
}

class _WinBtnState extends State<_WinBtn> {
  bool _down = false;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final bad = widget.close && _down;
    return Press(
      onTap: widget.onTap,
      label: widget.label,
      onPressed: (d) => setState(() => _down = d),
      child: AnimatedContainer(
        duration: t.dur,
        width: 44,
        height: 44,
        alignment: Alignment.center,
        decoration: BoxDecoration(color: bad ? fade(t.bad, 78) : t.s2, borderRadius: BorderRadius.circular(t.r2)),
        child: WIcon(widget.icon, color: bad ? t.bad : t.bright),
      ),
    );
  }
}

/// The input faces (faces.rs): the active one accent and a full pill (the Morphing Toggle Rule).
/// A desktop has no Touchpad.
class FacesSwitch extends StatelessWidget {
  const FacesSwitch({super.key, required this.active, required this.flow, required this.touch, required this.onPick});
  final Face active;
  final Flow flow;
  final bool touch;
  final ValueChanged<Face> onPick;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final faces = [for (final f in Face.values) if (touch || f != Face.pad) f];
    Widget b(Face f) {
      final on = f == active;
      return Press(
        onTap: () => onPick(f),
        label: f.label,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 360),
          curve: Tokens.spring,
          width: 44,
          height: 44,
          alignment: Alignment.center,
          decoration: BoxDecoration(color: on ? t.accent : null, borderRadius: BorderRadius.circular(on ? 999 : t.r2)),
          child: Bump(on: on, child: WIcon(f.icon, color: on ? t.onAccent : t.dim)),
        ),
      );
    }

    return switch (flow) {
      Flow.col => Column(mainAxisSize: MainAxisSize.min, children: [for (var i = 0; i < faces.length; i++) ...[if (i > 0) const SizedBox(height: 6), b(faces[i])]]),
      Flow.row => Row(mainAxisSize: MainAxisSize.min, children: [for (var i = 0; i < faces.length; i++) ...[if (i > 0) const SizedBox(width: 2), b(faces[i])]]),
      Flow.grid => Wrap(spacing: 12, runSpacing: 6, alignment: WrapAlignment.center, children: [for (final f in faces) b(f)]),
    };
  }
}
