// The thick slider (ui/widgets/slider.rs + cc.css .slide): a 52-high track that fills from the
// left, a thumb line, the label inside on the left and the value on the right. Drags follow the
// finger with no easing; `onChange` fires on release, `onInput` while dragging.
import 'package:flutter/material.dart';

import '../theme/look.dart';
import '../theme/tokens.dart';
import 'basics.dart';

class ThickSlider extends StatefulWidget {
  const ThickSlider({
    super.key,
    required this.label,
    required this.value,
    this.min = 0,
    this.max = 1,
    this.step = .02,
    this.icon = '',
    this.ticks = 0,
    this.thin = false,
    this.shown = '',
    this.disabled = false,
    this.onInput,
    this.onChange,
  });
  final String label, icon, shown;
  final double value, min, max, step;
  final int ticks;
  final bool thin, disabled;
  final ValueChanged<double>? onInput, onChange;
  @override
  State<ThickSlider> createState() => _ThickSliderState();
}

class _ThickSliderState extends State<ThickSlider> {
  double? _mine;
  double? _base;
  bool _grab = false;

  double get _v => (_mine != null && _base == widget.value) ? _mine! : widget.value;

  double _at(double dx, double w) {
    final f = (dx / w).clamp(0.0, 1.0);
    var v = widget.min + f * (widget.max - widget.min);
    if (widget.step > 0) v = (v / widget.step).round() * widget.step;
    return v.clamp(widget.min, widget.max);
  }

  void _set(double v) {
    setState(() {
      _mine = v;
      _base = widget.value;
    });
    widget.onInput?.call(v);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final span = (widget.max - widget.min).abs() < 1e-9 ? 1.0 : widget.max - widget.min;
    final frac = ((_v - widget.min) / span).clamp(0.0, 1.0);
    final text = widget.shown.isEmpty ? '${(frac * 100).round()}%' : widget.shown;
    final h = widget.thin ? 44.0 : 52.0;
    return Semantics(
      slider: true,
      label: widget.label,
      value: text,
      child: Opacity(
        opacity: widget.disabled ? .45 : 1,
        child: LayoutBuilder(builder: (context, box) {
          final w = box.maxWidth;
          return GestureDetector(
            behavior: HitTestBehavior.opaque,
            onHorizontalDragStart: widget.disabled ? null : (d) => setState(() => _grab = true),
            onHorizontalDragUpdate: widget.disabled ? null : (d) => _set(_at(d.localPosition.dx, w)),
            onHorizontalDragEnd: widget.disabled
                ? null
                : (_) {
                    setState(() => _grab = false);
                    widget.onChange?.call(_v);
                  },
            onTapUp: widget.disabled
                ? null
                : (d) {
                    final v = _at(d.localPosition.dx, w);
                    _set(v);
                    widget.onChange?.call(v);
                  },
            child: ClipRRect(
              borderRadius: BorderRadius.circular(t.r2),
              child: SizedBox(
                height: h,
                child: Stack(children: [
                  Positioned.fill(child: ColoredBox(color: t.s2)),
                  if (widget.ticks > 0)
                    Positioned.fill(
                      child: Padding(
                        padding: const EdgeInsets.fromLTRB(2, 0, 2, 6),
                        child: Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, crossAxisAlignment: CrossAxisAlignment.end, children: [
                          for (var i = 0; i <= widget.ticks; i++)
                            Container(width: 2, height: 6, decoration: BoxDecoration(color: fade(t.bright, 72), borderRadius: BorderRadius.circular(1))),
                        ]),
                      ),
                    ),
                  Positioned(left: 0, top: 0, bottom: 0, width: w * frac, child: ColoredBox(color: fade(t.accent, 62))),
                  Positioned(
                    left: ((w - 6) * frac).clamp(0, w - 6),
                    top: 0,
                    bottom: 0,
                    width: 6,
                    child: AnimatedScale(
                      scale: _grab ? 1.4 : 1,
                      duration: t.dur,
                      child: Container(decoration: BoxDecoration(color: t.accent, borderRadius: BorderRadius.circular(3))),
                    ),
                  ),
                  Positioned(
                    left: 16,
                    top: 0,
                    bottom: 0,
                    child: Row(children: [
                      if (widget.icon.isNotEmpty) ...[WIcon(widget.icon, size: 18, color: t.bright), const SizedBox(width: 8)],
                      Text(widget.label, style: body(context, w: FontWeight.w600, color: t.bright)),
                    ]),
                  ),
                  Positioned(
                    right: 16,
                    top: 0,
                    bottom: 0,
                    child: Center(
                      child: AnimatedScale(
                        scale: _grab ? 1.18 : 1,
                        duration: const Duration(milliseconds: 220),
                        curve: t.pop,
                        child: Text(text, style: mono(context, w: FontWeight.w600, color: _grab ? t.accent : t.bright)),
                      ),
                    ),
                  ),
                ]),
              ),
            ),
          );
        }),
      ),
    );
  }
}
