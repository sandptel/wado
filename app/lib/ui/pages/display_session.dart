// Display & session (port of computer/pages/display_session.rs + pages/display.rs): the session
// running on this computer (and starting another), its shape, orientation, frame rate, scale and
// tuning, and what its apps get. Shape and tuning reach a running session through Apply (a
// reconfigure keeps its apps); isolation, the X server and the encoder only reach a new session,
// and say so.
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/multi.dart';
import '../../wado/nav.dart';
import '../../wado/res.dart' as res;
import '../../wado/session.dart';
import '../../wado/settings.dart';
import '../home/running.dart' show headline, ago;
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/slider.dart';
import 'host_settings.dart';

const _scaleInfo = "How large apps draw themselves, as Hyprland's monitor scale does. The stream stays at the resolution above; only the logical area apps lay out in shrinks, so text and buttons come out bigger. Fractional steps need an app that speaks wp-fractional-scale; one that does not falls back to the nearest whole number below and comes out slightly soft.";
const _lockInfo = 'Normally wado lowers the frame rate when your connection cannot keep up, so what arrives stays smooth at a slower rate. Locked, it never does: you keep the rate and congestion shows up as stutter instead. Worth it when the rate walking up and down is more distracting than the odd dropped frame — the usual case on mobile data at 90 or 120.';
const _isolateInfo = 'On, the session gets its own D-Bus bus and its apps are launched with no X11 display. That stops an app you launch here from opening a window on the computer running wado instead — browsers and file managers hand the request to the copy already running over there, and anything X11 draws on its screen. Off, apps share that computer’s desktop services: notifications and file-chooser portals work, and X11-only apps open over there rather than failing.';
const _xInfo = 'wado speaks Wayland only, so an app that can only speak X11 — Steam is the one people hit — cannot draw here at all. On, the session gets its own X server and those apps run inside it. The cost: every X app shares one screen, a single window the size of the stream that is there even when nothing uses it, with no window manager inside it.';
const _orientInfo = 'Which way round the session is. A phone at rest is held upright, so an unthinking start gave desktop applications a 360px-wide screen to lay themselves out on — hence the default. Entering fullscreen pins the phone to whichever way this makes the session, and leaving fullscreen by any route releases it.';

/// `.card` with its `.cardhead`: a title, when it takes effect, and an ⓘ that unfolds below.
class HeadCard extends StatelessWidget {
  const HeadCard({super.key, this.title, this.when, this.info, required this.children});
  final String? title;
  final When? when;
  final String? info;
  final List<Widget> children;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    Widget head(Widget? btn) => Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 6, children: [
          Text(title!, style: body(context, w: FontWeight.w600, color: t.bright)),
          if (when != null) WhenBadge(when!),
          ?btn,
        ]);
    return WCard(children: [
      if (title != null) info == null ? head(null) : InfoText(text: info!, builder: (c, b) => head(b)),
      ...children,
    ]);
  }
}

class DisplaySession extends StatefulWidget {
  const DisplaySession({super.key});
  @override
  State<DisplaySession> createState() => _DisplaySessionState();
}

class _DisplaySessionState extends State<DisplaySession> {
  bool _more = false;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Listenable.merge([Settings.I, Session.I.on, Host.I.sessions, Host.I.config]),
      builder: (context, _) {
        final t = Tok.of(context);
        final s = Settings.I;
        final name = CoreHost.I.computer(Nav.I.selected)?.name ?? 'this computer';
        final list = Host.I.sessions.value;
        final on = Session.I.on.value;
        final limits = (Host.I.config.value?['limits'] as Map?) ?? const {};
        final (sw, sh) = Session.screen();
        final hz = ui.PlatformDispatcher.instance.views.first.display.refreshRate.round();
        // The shapes worth a chip: this device's exact sizes, then the catalog's same-way ones.
        final device = Session.resOptions(sw, sh);
        final (same, turned) = res.grouped(sw, sh, device);
        final chips = [for (final v in device) (v, 'fits exactly', true), for (final (v, l) in same.take(3)) (v, res.short(l), false)];
        var now = s.s('res');
        if (now.isEmpty) now = Session.defaultRes(sw, sh);
        final inChips = chips.any((c) => c.$1 == now);
        final showMore = _more || !inChips;
        final wh = res.parseWh(now);
        final maxFps = limits['max_fps'] as int?;
        void apply() {
          if (Session.I.on.value) Session.I.reconfigure();
        }

        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          if (list.isNotEmpty) ...[
            HeadCard(children: [
              Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 6, children: [
                Text('Running session', style: body(context, w: FontWeight.w600, color: t.bright)),
                _OnBadge(name),
              ]),
              for (final x in list) _SessionHead(x),
              const Why('Shape, frame rate, scale and tuning change it in place — its apps stay open.'),
              if (!on) Btn('Start another session', icon: 'plus', onTap: Multi.I.startAnother),
            ]),
            const SizedBox(height: 12),
          ],
          HeadCard(title: 'Shape', when: When.apply, children: [
            SizedBox(
              height: 96,
              child: ListView(scrollDirection: Axis.horizontal, children: [
                for (final (v, sub, exact) in chips) _Shape(v: v, sub: sub, exact: exact, on: v == now, onTap: () => s.set('res', v)),
                _Shape(v: 'more', sub: 'every size', exact: false, on: showMore, more: true, onTap: () => setState(() => _more = !_more)),
              ]),
            ),
            if (showMore) ...[
              _Pick(
                value: now,
                groups: [('This screen', [for (final v in device) (v, v.replaceAll('x', ' × '))]), ('Same way round', same), ('Turned', turned)],
                onPick: (v) => s.set('res', v),
              ),
              if (now == 'custom')
                Row(children: [
                  Expanded(child: _Num(label: 'Width', value: s.i('custom_w'), onChanged: (v) => s.set('custom_w', v))),
                  const SizedBox(width: 8),
                  Expanded(child: _Num(label: 'Height', value: s.i('custom_h'), onChanged: (v) => s.set('custom_h', v))),
                ]),
            ],
            if (wh != null) Why(res.verdict(sw, sh, wh.$1, wh.$2)),
            if (limits['max_width'] != null && limits['max_height'] != null) Why('This computer caps sessions at ${limits['max_width']}×${limits['max_height']}.'),
          ]),
          const SizedBox(height: 12),
          HeadCard(title: 'Orientation', when: When.restart, info: _orientInfo, children: [
            Seg(options: const [('auto', 'Auto'), ('landscape', 'Landscape'), ('portrait', 'Portrait')], value: s.s('orientation'), onPick: (v) {
              s.set('orientation', v);
              // The shape follows: this screen's sizes are the other way round now.
              if (s.s('res') != 'custom') s.set('res', '');
            }),
          ]),
          const SizedBox(height: 12),
          HeadCard(title: 'Frame rate', when: When.apply, children: [
            Seg(
              options: [for (final r in const [30, 60, 90, 120]) ('$r', maxFps != null && r > maxFps ? '$r·' : '$r')],
              value: '${s.i('fps')}',
              onPick: (v) => s.set('fps', int.parse(v)),
            ),
            Why(res.fpsNote(s.i('fps'), hz)),
            if (maxFps != null && s.i('fps') > maxFps) Why('This computer caps the frame rate at $maxFps.'),
          ]),
          const SizedBox(height: 12),
          HeadCard(title: 'Scale', when: When.apply, info: _scaleInfo, children: [
            Seg(
              options: const [('1', '1×'), ('1.25', '1.25×'), ('1.5', '1.5×'), ('2', '2×'), ('3', '3×')],
              value: s.s('scale').isEmpty ? '${Session.defaultScale(ui.PlatformDispatcher.instance.views.first.devicePixelRatio)}'.replaceAll(RegExp(r'\.0$'), '') : s.s('scale'),
              onPick: (v) => s.set('scale', v),
            ),
          ]),
          const SizedBox(height: 12),
          HeadCard(title: 'Tuning', when: When.apply, children: [
            Seg(options: const [('reactivity', 'Reactive'), ('balanced', 'Balanced'), ('quality', 'Sharp'), ('custom', 'Custom')], value: s.s('quality'), onPick: (v) => s.set('quality', v)),
            if (s.s('quality') == 'custom')
              ThickSlider(
                label: 'Bitrate',
                value: s.i('bitrate').toDouble(),
                min: 500,
                max: ((limits['max_bitrate'] ?? 20000) as num).toDouble(),
                step: 500,
                shown: '${s.i('bitrate')} kbps',
                onChange: (v) => s.set('bitrate', v.round()),
              ),
            SwitchRow(title: 'Lock frame rate', sub: 'Stutter instead of slowing down', info: _lockInfo, on: s.b('fps_lock'), onToggle: (v) => s.set('fps_lock', v)),
          ]),
          const SizedBox(height: 12),
          HeadCard(children: [
            SwitchRow(
              title: 'Stream sound',
              sub: 'Off: no audio at all, so the picture is never held back to stay in sync with it — applies when you reconnect',
              when: When.restart,
              on: s.b('stream_sound'),
              onToggle: (v) => s.set('stream_sound', v),
            ),
            SwitchRow(
              title: 'Redundant input',
              sub: 'Taps and keys also sent on a fast lane — the first copy to arrive counts, so a stuck packet never stalls touch',
              on: s.b('redundant_input'),
              onToggle: (v) => s.set('redundant_input', v),
            ),
            Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 6, children: [
              Text('Keyframes', style: body(context, w: FontWeight.w600, color: t.bright)),
              const WhenBadge(When.apply),
            ]),
            Seg(options: const [('periodic', 'Every 1–2 s'), ('on_request', 'On request')], value: s.s('keyframe_mode'), onPick: (v) {
              s.set('keyframe_mode', v);
              apply();
            }),
            const Hint('A keyframe is ~8× an ordinary frame. On request sends one only when the phone asks (and every 10 s as a safety net) — fewer bursts on a jittery link.'),
            SwitchRow(
              title: 'Low-latency audio',
              sub: 'Smaller sound packets and no audio cushion — video waits for sound, so this speeds both',
              when: When.apply,
              on: s.b('low_latency_audio'),
              onToggle: (v) {
                s.set('low_latency_audio', v);
                apply();
              },
            ),
            SwitchRow(
              title: 'Auto bitrate',
              sub: "Lowers the bitrate when the link can't carry it, then climbs back",
              on: s.b('auto_bitrate'),
              onToggle: (v) => s.set('auto_bitrate', v),
            ),
          ]),
          const SizedBox(height: 12),
          HeadCard(title: 'Encoder', when: limits['encoder'] != null ? When.host : When.restart, children: [
            Seg(
              options: const [('auto', 'Auto'), ('hardware', 'GPU'), ('software', 'x264')],
              value: s.s('encoder_backend'),
              disabled: limits['encoder'] != null,
              onPick: (v) => s.set('encoder_backend', v),
            ),
          ]),
          const SizedBox(height: 12),
          HeadCard(children: [
            SwitchRow(title: 'Isolate apps', sub: 'Own D-Bus, no host X11', when: When.restart, info: _isolateInfo, on: s.b('isolate_apps'), onToggle: (v) => s.set('isolate_apps', v)),
            SwitchRow(title: 'X server', sub: 'For Steam and X11-only apps', when: When.restart, info: _xInfo, on: s.b('x_server'), onToggle: (v) => s.set('x_server', v)),
          ]),
          const SizedBox(height: 12),
          _Internals(),
          // What this computer holds for every device (display_session.rs).
          Sect('For every device on $name'),
          const SizedBox(height: 8),
          const HostNotes(),
          const HostLimits(),
          const SizedBox(height: 12),
          const HostSwipes(),
          const SizedBox(height: 12),
          const HostShortcuts(),
          const SizedBox(height: 12),
          const KeyboardRepeat(),
        ]);
      },
    );
  }
}

class _OnBadge extends StatelessWidget {
  const _OnBadge(this.name);
  final String name;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = t.base[0xE];
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3),
      decoration: BoxDecoration(color: fade(c, 86), borderRadius: BorderRadius.circular(999)),
      child: Text('ON ${name.toUpperCase()}', style: TextStyle(fontFamily: Tokens.body, fontSize: rem(context, .5938), fontWeight: FontWeight.w600, letterSpacing: .8, color: c)),
    );
  }
}

class _SessionHead extends StatelessWidget {
  const _SessionHead(this.s);
  final Map<String, dynamic> s;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final w = ((s['width'] ?? 1) as num).toDouble(), h = ((s['height'] ?? 1) as num).toDouble();
    final (bw, bh) = w >= h ? (46.0, 46.0 * h / w) : (46.0 * w / h, 46.0);
    return Row(children: [
      Container(
        width: 54,
        height: 54,
        decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2)),
        alignment: Alignment.center,
        child: Container(width: bw, height: bh, decoration: BoxDecoration(borderRadius: BorderRadius.circular(5), border: Border.all(color: t.good, width: 2), color: fade(t.good, 82))),
      ),
      const SizedBox(width: 12),
      Expanded(
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(headline((s['apps'] as List?) ?? []), maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright)),
          Text('${w.round()}×${h.round()} · ${s['fps'] ?? '?'} fps · started ${ago((s['age_s'] ?? 0) as int)}', style: mono(context, color: t.dim)),
        ]),
      ),
    ]);
  }
}

/// `.shape`: a little outline of the shape (green when it fits this screen exactly), its size
/// and what it is; the picked one ringed with the accent and filled.
class _Shape extends StatelessWidget {
  const _Shape({required this.v, required this.sub, required this.exact, required this.on, required this.onTap, this.more = false});
  final String v, sub;
  final bool exact, on, more;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final wh = res.parseWh(v) ?? (16, 9);
    final k = 34 / (wh.$1 > wh.$2 ? wh.$1 : wh.$2);
    final edge = on ? t.accent : (exact ? t.good : t.dim);
    return Padding(
      padding: const EdgeInsets.only(right: 8),
      child: Press(
        onTap: onTap,
        label: more ? 'More shapes' : v.replaceAll('x', '×'),
        child: AnimatedContainer(
          duration: t.dur,
          curve: Tokens.spring,
          constraints: const BoxConstraints(minWidth: 80),
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 10),
          decoration: BoxDecoration(
            color: t.s2,
            borderRadius: BorderRadius.circular(t.r2),
            border: Border.all(color: on ? t.accent : Colors.transparent, width: 1.5),
          ),
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            SizedBox(
              height: 34,
              child: Center(
                child: more
                    ? CustomPaint(size: const Size(34, 22), painter: _DashBox(edge))
                    : AnimatedContainer(
                        duration: t.dur,
                        width: wh.$1 * k,
                        height: wh.$2 * k,
                        decoration: BoxDecoration(
                          color: on ? fade(t.accent, 80) : null,
                          border: Border.all(color: edge, width: 1.5),
                          borderRadius: BorderRadius.circular(3),
                        ),
                      ),
              ),
            ),
            const SizedBox(height: 6),
            Text(more ? 'More' : v.replaceAll('x', '×'), style: mono(context, size: .6875, w: FontWeight.w600, color: t.bright)),
            SizedBox(width: 88, child: Text(sub, maxLines: 1, overflow: TextOverflow.ellipsis, textAlign: TextAlign.center, style: body(context, size: .625, color: t.dim))),
          ]),
        ),
      ),
    );
  }
}

class _DashBox extends CustomPainter {
  _DashBox(this.c);
  final Color c;
  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = c
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.5;
    final path = Path()..addRRect(RRect.fromRectAndRadius(Offset.zero & size, const Radius.circular(3)));
    for (final m in path.computeMetrics()) {
      for (var d = 0.0; d < m.length; d += 6) {
        canvas.drawPath(m.extractPath(d, d + 3), p);
      }
    }
  }

  @override
  bool shouldRepaint(_DashBox o) => o.c != c;
}

/// The `<select>` of every size, grouped, as a bottom sheet.
class _Pick extends StatelessWidget {
  const _Pick({required this.value, required this.groups, required this.onPick});
  final String value;
  final List<(String, List<(String, String)>)> groups;
  final ValueChanged<String> onPick;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    String labelOf(String v) {
      if (v == 'custom') return 'Custom…';
      for (final (_, l) in groups) {
        for (final (x, w) in l) {
          if (x == v) return w;
        }
      }
      return v.replaceAll('x', ' × ');
    }

    return Press(
      onTap: () => showModalBottomSheet(
        context: context,
        isScrollControlled: true,
        backgroundColor: t.s1,
        showDragHandle: true,
        sheetAnimationStyle: AnimationStyle(duration: t.durSheet, curve: Tokens.spring),
        builder: (ctx) => SafeArea(
          child: ConstrainedBox(
            constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(ctx).height * .8),
            child: ListView(shrinkWrap: true, padding: const EdgeInsets.fromLTRB(8, 0, 8, 12), children: [
              for (final (g, l) in groups) ...[
                Sect(g),
                for (final (v, w) in l)
                  _PickRow(text: w, on: v == value, onTap: () {
                    Navigator.of(ctx).pop();
                    onPick(v);
                  }),
              ],
              _PickRow(text: 'Custom…', on: value == 'custom', onTap: () {
                Navigator.of(ctx).pop();
                onPick('custom');
              }),
            ]),
          ),
        ),
      ),
      child: Container(
        constraints: const BoxConstraints(minHeight: 44),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r1), border: Border.all(color: t.line)),
        child: Row(children: [
          Expanded(child: Text(labelOf(value), style: body(context, size: 1, color: t.bright))),
          WIcon('down', size: 16, color: t.dim),
        ]),
      ),
    );
  }
}

class _PickRow extends StatelessWidget {
  const _PickRow({required this.text, required this.on, required this.onTap});
  final String text;
  final bool on;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Press(
      onTap: onTap,
      scale: 1,
      child: Container(
        constraints: const BoxConstraints(minHeight: 48),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: Row(children: [
          SizedBox(width: 22, child: on ? WIcon('check', size: 16, color: t.accent) : null),
          Expanded(child: Text(text, style: body(context, size: .875, color: on ? t.bright : t.text))),
        ]),
      ),
    );
  }
}

class _Num extends StatelessWidget {
  const _Num({required this.label, required this.value, required this.onChanged});
  final String label;
  final int value;
  final ValueChanged<int> onChanged;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return TextField(
      controller: TextEditingController(text: '$value'),
      keyboardType: TextInputType.number,
      style: body(context, size: 1, color: t.bright),
      decoration: InputDecoration(labelText: label, filled: true, fillColor: t.s0, border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1))),
      onSubmitted: (v) {
        final n = int.tryParse(v);
        if (n != null && n >= 16) onChanged(n & ~1);
      },
    );
  }
}

/// `details.card.sub` "Encoder internals": the x264 preset and the keyframe interval.
class _Internals extends StatefulWidget {
  @override
  State<_Internals> createState() => _InternalsState();
}

class _InternalsState extends State<_Internals> {
  bool _open = false;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final s = Settings.I;
    return WCard(children: [
      Press(
        onTap: () => setState(() => _open = !_open),
        scale: 1,
        child: Row(children: [
          AnimatedRotation(turns: _open ? .25 : 0, duration: t.dur, curve: Tokens.spring, child: WIcon('right', size: 16, color: t.dim)),
          const SizedBox(width: 6),
          Text('Encoder internals', style: body(context, w: FontWeight.w600, color: t.bright)),
        ]),
      ),
      AnimatedSize(
        duration: t.durSheet,
        curve: Tokens.spring,
        alignment: Alignment.topCenter,
        child: !_open
            ? const SizedBox(width: double.infinity)
            : Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                Text('x264 preset', style: body(context, size: .75, color: t.dim)),
                const SizedBox(height: 4),
                Seg(
                  options: const [('', 'Tuning'), ('ultrafast', 'ultrafast'), ('superfast', 'superfast'), ('veryfast', 'veryfast'), ('faster', 'faster')],
                  value: s.s('preset'),
                  onPick: (v) => s.set('preset', v),
                ),
                const SizedBox(height: 10),
                Text('Keyframe interval (frames)', style: body(context, size: .75, color: t.dim)),
                const SizedBox(height: 4),
                TextField(
                  controller: TextEditingController(text: s.s('keyframe')),
                  keyboardType: TextInputType.number,
                  style: body(context, size: 1, color: t.bright),
                  decoration: InputDecoration(hintText: '(from tuning)', filled: true, fillColor: t.s0, border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1))),
                  onSubmitted: (v) => s.set('keyframe', v.trim()),
                ),
              ]),
      ),
    ]);
  }
}
