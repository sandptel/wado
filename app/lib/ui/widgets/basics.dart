// The web client's building blocks (ui/widgets/*.rs + cc.css/base.css), as widgets: icon, disc,
// nav row, list, section head, switch row, segmented control, badges, ⓘ, buttons, card.
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../../wado/settings.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import 'icon_paths.dart';

/// 1rem in logical px: 16 × the text size (base.css `html { font-size }`).
double rem(BuildContext c, double r) {
  final t = Settings.I.d('text_scale');
  final scale = t > 0 ? t : MediaQuery.textScalerOf(c).scale(16) / 16;
  return r * 16 * scale;
}

TextStyle body(BuildContext c, {double size = .875, FontWeight w = FontWeight.w400, Color? color, double height = 1.45}) =>
    TextStyle(fontFamily: Tokens.body, fontSize: rem(c, size), fontWeight: w, color: color ?? Tok.of(c).text, height: height);

TextStyle display(BuildContext c, double size, {FontWeight w = FontWeight.w700, Color? color, double ls = 0}) =>
    TextStyle(fontFamily: Tokens.display, fontSize: size, fontWeight: w, color: color ?? Tok.of(c).bright, letterSpacing: ls, height: 1.1);

TextStyle mono(BuildContext c, {double size = .75, FontWeight w = FontWeight.w400, Color? color}) =>
    TextStyle(fontFamily: Tokens.mono, fontSize: rem(c, size), fontWeight: w, color: color ?? Tok.of(c).text);

/// `Icon { name }`: a 24×24 stroke icon (or filled, for transport controls).
class WIcon extends StatelessWidget {
  const WIcon(this.name, {super.key, this.size = 20, this.color, this.fill = false});
  final String name;
  final double size;
  final Color? color;
  final bool fill;
  @override
  Widget build(BuildContext context) {
    final c = color ?? IconTheme.of(context).color ?? Tok.of(context).text;
    final hex = '#${c.toARGB32().toRadixString(16).padLeft(8, '0').substring(2)}';
    final d = iconPaths[name] ?? '';
    final svg = fill
        ? '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="$hex" d="$d"/></svg>'
        : '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="none" stroke="$hex" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" d="$d"/></svg>';
    return SizedBox(width: size, height: size, child: SvgPicture.string(svg, width: size, height: size));
  }
}

/// A base16 slot by its two-digit name ("0D").
Color slot(BuildContext c, String hue) => Tok.of(c).base[int.parse(hue, radix: 16)];

/// `.disc`: an icon in a tinted circle.
class Disc extends StatelessWidget {
  const Disc({super.key, required this.child, this.hue, this.size = 38});
  final Widget child;
  final Color? hue;
  final double size;
  @override
  Widget build(BuildContext context) {
    final h = hue ?? Tok.of(context).accent;
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(shape: BoxShape.circle, color: fade(h, 84)),
      alignment: Alignment.center,
      child: IconTheme(data: IconThemeData(color: h), child: child),
    );
  }
}

/// A button that sinks to .96 while pressed (base.css `button:active`).
class Press extends StatefulWidget {
  const Press({super.key, required this.child, this.onTap, this.onLongPress, this.label, this.enabled = true, this.radius, this.scale = .96, this.onPressed});
  final Widget child;
  /// How far it sinks (`.96`; `1` when a parent does the giving).
  final double scale;
  /// The finger is on it / has left: for parts that answer the press themselves.
  final ValueChanged<bool>? onPressed;
  final VoidCallback? onTap;
  final VoidCallback? onLongPress;
  final String? label;
  final bool enabled;
  final BorderRadius? radius;
  @override
  State<Press> createState() => _PressState();
}

class _PressState extends State<Press> {
  bool _down = false;
  Offset _at = Offset.zero;

  void _set(bool d) {
    if (d == _down) return;
    setState(() => _down = d);
    widget.onPressed?.call(d);
  }
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final on = widget.enabled && (widget.onTap != null || widget.onLongPress != null);
    return Semantics(
      button: true,
      enabled: on,
      label: widget.label,
      // The sink follows the finger itself, at once (CSS :active), not the tap recogniser, which
      // inside a scroll view reports the press 100 ms late — a quick tap would never show it.
      child: Listener(
        onPointerDown: on ? (e) { _at = e.localPosition; _set(true); } : null,
        onPointerUp: on ? (_) => _set(false) : null,
        onPointerCancel: on ? (_) => _set(false) : null,
        onPointerMove: on && _down ? (e) { if ((e.localPosition - _at).distance > 18) _set(false); } : null,
        child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: on ? widget.onTap : null,
        onLongPress: on ? widget.onLongPress : null,
        child: AnimatedScale(
          scale: _down ? widget.scale : 1,
          duration: t.durPress,
          curve: Tokens.spring,
          child: Opacity(opacity: widget.enabled ? 1 : .4, child: widget.child),
        ),
      )),
    );
  }
}

/// `.sect`: a small uppercase section head, with an optional action on the right.
class Sect extends StatelessWidget {
  const Sect(this.title, {super.key, this.action, this.onAction, this.margin = const EdgeInsets.fromLTRB(4, 8, 4, 0)});
  final String title;
  final String? action;
  final VoidCallback? onAction;
  final EdgeInsets margin;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Padding(
      padding: margin,
      child: SizedBox(
        height: 32,
        child: Row(children: [
          Expanded(
            child: Text(title.toUpperCase(),
                style: TextStyle(fontFamily: Tokens.body, fontSize: rem(context, .6875), fontWeight: FontWeight.w600, letterSpacing: rem(context, .6875) * .1, color: t.dim)),
          ),
          if (action != null)
            Press(onTap: onAction, child: Text(action!, style: body(context, size: .8125, w: FontWeight.w600, color: t.accent))),
        ]),
      ),
    );
  }
}

/// `.list`: rows on surface-1 with hairlines between.
class ListBox extends StatelessWidget {
  const ListBox({super.key, required this.children});
  final List<Widget> children;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final rows = <Widget>[];
    for (var i = 0; i < children.length; i++) {
      if (i > 0) rows.add(Container(height: 1, color: fade(t.line, 40)));
      rows.add(children[i]);
    }
    return ClipRRect(
      borderRadius: BorderRadius.circular(t.r3),
      child: ColoredBox(color: t.s1, child: Column(children: rows)),
    );
  }
}

/// `NavRow`: disc, title, subtitle, chevron.
class NavRow extends StatelessWidget {
  const NavRow({super.key, required this.icon, required this.hue, required this.title, required this.sub, required this.onOpen});
  final String icon, hue, title, sub;
  final VoidCallback onOpen;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Press(
      onTap: onOpen,
      label: title,
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 60),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
          child: Row(children: [
            Disc(hue: slot(context, hue), child: WIcon(icon, color: slot(context, hue))),
            const SizedBox(width: 14),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text(title, style: body(context, w: FontWeight.w600, color: t.bright)),
                if (sub.isNotEmpty) Text(sub, style: body(context, size: .75, color: t.dim)),
              ]),
            ),
            WIcon('right', color: t.dim),
          ]),
        ),
      ),
    );
  }
}

/// `.card`.
class WCard extends StatelessWidget {
  const WCard({super.key, required this.children, this.alert = false, this.gap = 10, this.padding = const EdgeInsets.all(14)});
  final List<Widget> children;
  final bool alert;
  final double gap;
  final EdgeInsets padding;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final kids = <Widget>[];
    for (var i = 0; i < children.length; i++) {
      if (i > 0) kids.add(SizedBox(height: gap));
      kids.add(children[i]);
    }
    return Container(
      padding: padding,
      decoration: BoxDecoration(
        color: alert ? mix(t.bad, t.s1, 85) : t.s1,
        borderRadius: BorderRadius.circular(t.r3),
        border: alert ? Border.all(color: fade(t.bad, 60)) : null,
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: kids),
    );
  }
}

/// `.btn` (`primary`, `danger`).
class Btn extends StatelessWidget {
  const Btn(this.label, {super.key, this.onTap, this.primary = false, this.danger = false, this.icon, this.expand = true, this.busy = false});
  final String label;
  final VoidCallback? onTap;
  final bool primary, danger, expand, busy;
  final String? icon;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final bg = primary ? t.accent : (danger ? fade(t.bad, 82) : t.s2);
    final fg = primary ? t.onAccent : (danger ? t.bad : t.bright);
    return Press(
      onTap: busy ? null : onTap,
      enabled: onTap != null,
      label: label,
      child: Container(
        constraints: const BoxConstraints(minHeight: 48),
        width: expand ? double.infinity : null,
        padding: const EdgeInsets.symmetric(horizontal: 16),
        decoration: BoxDecoration(color: bg, borderRadius: BorderRadius.circular(t.r2)),
        child: Row(mainAxisSize: expand ? MainAxisSize.max : MainAxisSize.min, mainAxisAlignment: MainAxisAlignment.center, children: [
          if (busy)
            Padding(padding: const EdgeInsets.only(right: 8), child: SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2, color: fg)))
          else if (icon != null)
            Padding(padding: const EdgeInsets.only(right: 8), child: WIcon(icon!, color: fg)),
          Flexible(child: Text(label, style: body(context, w: FontWeight.w600, color: fg), overflow: TextOverflow.ellipsis)),
        ]),
      ),
    );
  }
}

/// `.iconbtn`: a 44 round icon button.
class IconBtn extends StatelessWidget {
  const IconBtn(this.icon, {super.key, required this.label, this.onTap, this.color});
  final String icon, label;
  final VoidCallback? onTap;
  final Color? color;
  @override
  Widget build(BuildContext context) => Press(
        onTap: onTap,
        label: label,
        child: SizedBox(width: 44, height: 44, child: Center(child: WIcon(icon, color: color ?? Tok.of(context).dim))),
      );
}

enum When { apply, restart, host }

/// `WhenBadge`.
class WhenBadge extends StatelessWidget {
  const WhenBadge(this.when, {super.key});
  final When when;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final (c, text) = switch (when) {
      When.apply => (t.accent, 'on apply'),
      When.restart => (t.base[0xA], 'next start'),
      When.host => (t.base[0xE], 'set by host'),
    };
    return Container(
      margin: const EdgeInsets.only(left: 2),
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3),
      decoration: BoxDecoration(color: fade(c, 86), borderRadius: BorderRadius.circular(999)),
      child: Text(text.toUpperCase(), style: TextStyle(fontFamily: Tokens.body, fontSize: rem(context, .5938), fontWeight: FontWeight.w600, letterSpacing: 0.8, color: c)),
    );
  }
}

/// `Info`: ⓘ that unfolds its text below the row.
class InfoText extends StatefulWidget {
  const InfoText({super.key, required this.text, required this.builder});
  final String text;
  final Widget Function(BuildContext, Widget button) builder;
  @override
  State<InfoText> createState() => _InfoTextState();
}

class _InfoTextState extends State<InfoText> {
  bool open = false;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final btn = Press(
      onTap: () => setState(() => open = !open),
      label: 'More about this',
      child: Container(
        width: 32,
        height: 32,
        decoration: BoxDecoration(shape: BoxShape.circle, color: open ? fade(t.accent, 88) : null),
        alignment: Alignment.center,
        child: WIcon('info', size: 17, color: open ? t.accent : t.dim),
      ),
    );
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      widget.builder(context, btn),
      AnimatedSize(
        duration: t.durSheet,
        curve: Tokens.spring,
        alignment: Alignment.topCenter,
        child: open
            ? Padding(padding: const EdgeInsets.only(top: 4, bottom: 2), child: Text(widget.text, style: body(context, size: .7812, height: 1.55)))
            : const SizedBox(width: double.infinity),
      ),
    ]);
  }
}

/// `.sw`: the switch.
class Sw extends StatelessWidget {
  const Sw({super.key, required this.on, required this.onChanged, this.label = '', this.disabled = false});
  final bool on, disabled;
  final String label;
  final ValueChanged<bool> onChanged;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final d = Duration(milliseconds: t.still ? 0 : 340);
    return Semantics(
      toggled: on,
      label: label,
      child: Press(
        onTap: disabled ? null : () => onChanged(!on),
        enabled: !disabled,
        child: AnimatedContainer(
          duration: t.dur,
          width: 52,
          height: 32,
          decoration: BoxDecoration(color: on ? t.accent : t.s3, borderRadius: BorderRadius.circular(999)),
          child: Stack(children: [
            AnimatedPositioned(
              duration: d,
              curve: t.pop,
              left: on ? 24 : 8,
              top: on ? 4 : 8,
              width: on ? 24 : 16,
              height: on ? 24 : 16,
              child: AnimatedContainer(duration: t.dur, decoration: BoxDecoration(shape: BoxShape.circle, color: on ? t.onAccent : t.dim)),
            ),
          ]),
        ),
      ),
    );
  }
}

/// `SwitchRow`.
class SwitchRow extends StatelessWidget {
  const SwitchRow({super.key, required this.title, this.sub = '', required this.on, required this.onToggle, this.disabled = false, this.when, this.info});
  final String title, sub;
  final bool on, disabled;
  final When? when;
  final String? info;
  final ValueChanged<bool> onToggle;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    Widget row(BuildContext c, Widget? infoBtn) => ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 48),
          child: Row(children: [
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                Wrap(crossAxisAlignment: WrapCrossAlignment.center, spacing: 6, children: [
                  Text(title, style: body(c, w: FontWeight.w600, color: t.bright)),
                  if (when != null) WhenBadge(when!),
                ]),
                if (sub.isNotEmpty) Text(sub, style: body(c, size: .75, color: t.dim)),
              ]),
            ),
            if (infoBtn != null) ...[const SizedBox(width: 10), infoBtn],
            const SizedBox(width: 10),
            Sw(on: on, onChanged: onToggle, label: title, disabled: disabled),
          ]),
        );
    if (info == null) return row(context, null);
    return InfoText(text: info!, builder: row);
  }
}

/// `Seg`: equal-width options with a sliding knob.
class Seg extends StatelessWidget {
  const Seg({super.key, required this.options, required this.value, required this.onPick, this.disabled = false});
  final List<(String, String)> options;
  final String value;
  final bool disabled;
  final ValueChanged<String> onPick;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final i = options.indexWhere((o) => o.$1 == value);
    final n = options.isEmpty ? 1 : options.length;
    return Container(
      padding: const EdgeInsets.all(4),
      decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2)),
      child: LayoutBuilder(builder: (context, box) {
        final w = box.maxWidth / n;
        return SizedBox(
          height: 40,
          child: Stack(children: [
            AnimatedPositioned(
              duration: Duration(milliseconds: t.still ? 0 : 420),
              curve: Tokens.spring,
              left: w * (i < 0 ? 0 : i),
              top: 0,
              bottom: 0,
              width: w,
              child: AnimatedOpacity(
                opacity: i < 0 ? 0 : 1,
                duration: t.dur,
                child: Container(decoration: BoxDecoration(color: t.accent, borderRadius: BorderRadius.circular(t.r2 - 4))),
              ),
            ),
            Row(children: [
              for (final o in options)
                Expanded(
                  child: Press(
                    onTap: disabled ? null : () => onPick(o.$1),
                    label: o.$2,
                    child: Center(
                      child: Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 4),
                        child: AnimatedDefaultTextStyle(
                          duration: t.dur,
                          style: body(context, size: .8125, w: FontWeight.w600, color: o.$1 == value ? t.onAccent : t.dim),
                          child: Text(o.$2, maxLines: 1, overflow: TextOverflow.ellipsis),
                        ),
                      ),
                    ),
                  ),
                ),
            ]),
          ]),
        );
      }),
    );
  }
}

/// `.hint`.
class Hint extends StatelessWidget {
  const Hint(this.text, {super.key});
  final String text;
  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(bottom: 10),
        child: Text(text, style: body(context, size: .75, color: Tok.of(context).dim)),
      );
}

/// `.why` (`host`, `ok`, `bad`).
class Why extends StatelessWidget {
  const Why(this.text, {super.key, this.tone = ''});
  final String text, tone;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = switch (tone) { 'host' => t.base[0xA], 'ok' => t.good, 'bad' => t.bad, _ => t.dim };
    return Text(text, style: body(context, size: .75, color: c));
  }
}
