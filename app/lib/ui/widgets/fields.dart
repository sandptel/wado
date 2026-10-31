// Form pieces the settings pages share (base.css `.field`, a `<select>`): a labelled input with
// its unit, and a choice that opens as a bottom sheet of 48 px rows.
import 'package:flutter/material.dart';

import '../theme/look.dart';
import '../theme/tokens.dart';
import 'basics.dart';

/// `label.field`: the label, the input, its unit. `onCommit` on submit or focus loss — each write
/// may reload the daemon's config, so not on every keystroke.
class Field extends StatefulWidget {
  const Field({super.key, required this.label, required this.value, required this.onCommit, this.unit = '', this.hint = '', this.number = true, this.disabled = false});
  final String label, value, unit, hint;
  final bool number, disabled;
  final ValueChanged<String> onCommit;
  @override
  State<Field> createState() => _FieldState();
}

class _FieldState extends State<Field> {
  late final _c = TextEditingController(text: widget.value);
  final _f = FocusNode();
  @override
  void initState() {
    super.initState();
    _f.addListener(() {
      if (!_f.hasFocus && _c.text != widget.value) widget.onCommit(_c.text.trim());
    });
  }

  @override
  void didUpdateWidget(Field o) {
    super.didUpdateWidget(o);
    if (!_f.hasFocus && o.value != widget.value) _c.text = widget.value;
  }

  @override
  void dispose() {
    _c.dispose();
    _f.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(children: [
        Expanded(child: Text(widget.label, style: body(context, color: t.text))),
        SizedBox(
          width: 120,
          child: TextField(
            controller: _c,
            focusNode: _f,
            enabled: !widget.disabled,
            keyboardType: widget.number ? TextInputType.number : TextInputType.text,
            onSubmitted: (v) => widget.onCommit(v.trim()),
            style: body(context, size: 1, color: t.bright),
            decoration: InputDecoration(
              isDense: true,
              hintText: widget.hint,
              hintStyle: body(context, size: 1, color: t.dim),
              filled: true,
              fillColor: t.s0,
              contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
              border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1), borderSide: BorderSide(color: t.line)),
              enabledBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1), borderSide: BorderSide(color: t.line)),
              focusedBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1), borderSide: BorderSide(color: t.accent)),
            ),
          ),
        ),
        if (widget.unit.isNotEmpty) ...[const SizedBox(width: 8), SizedBox(width: 34, child: Text(widget.unit, style: body(context, size: .75, color: t.dim)))],
      ]),
    );
  }
}

/// A `<select>`: the label, the current choice; a tap opens the choices as a sheet.
class Choice extends StatelessWidget {
  const Choice({super.key, this.label = '', required this.value, required this.options, required this.onPick, this.disabled = false});
  final String label, value;
  final List<(String, String)> options;
  final ValueChanged<String> onPick;
  final bool disabled;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final shown = options.where((o) => o.$1 == value).firstOrNull?.$2 ?? value;
    final box = Press(
      enabled: !disabled,
      onTap: disabled
          ? null
          : () => showModalBottomSheet(
                context: context,
                isScrollControlled: true,
                backgroundColor: t.s1,
                showDragHandle: true,
                sheetAnimationStyle: AnimationStyle(duration: t.durSheet, curve: Tokens.spring),
                builder: (ctx) => SafeArea(
                  child: ConstrainedBox(
                    constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(ctx).height * .7),
                    child: ListView(shrinkWrap: true, padding: const EdgeInsets.fromLTRB(8, 0, 8, 12), children: [
                      if (label.isNotEmpty) Sect(label),
                      for (final (v, l) in options)
                        Press(
                          scale: 1,
                          onTap: () {
                            Navigator.of(ctx).pop();
                            onPick(v);
                          },
                          child: Container(
                            constraints: const BoxConstraints(minHeight: 48),
                            padding: const EdgeInsets.symmetric(horizontal: 12),
                            child: Row(children: [
                              SizedBox(width: 22, child: v == value ? WIcon('check', size: 16, color: t.accent) : null),
                              Expanded(child: Text(l, style: body(ctx, size: .875, color: v == value ? t.bright : t.text))),
                            ]),
                          ),
                        ),
                    ]),
                  ),
                ),
              ),
      child: Container(
        constraints: const BoxConstraints(minHeight: 44),
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r1), border: Border.all(color: t.line)),
        child: Row(children: [
          Expanded(child: Text(shown, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: 1, color: t.bright))),
          WIcon('down', size: 16, color: t.dim),
        ]),
      ),
    );
    if (label.isEmpty) return box;
    return Row(children: [
      Expanded(child: Text(label, style: body(context, color: t.text))),
      SizedBox(width: 190, child: box),
    ]);
  }
}
