// The one bottom sheet of the new surfaces: s1, an r4 top, a grab handle, rising on the spring in
// --dur-sheet. And the explain-locked sheet built on it — what, why in plain words, the fix (a
// button when this phone can do it), then the technical name under Details. No control is greyed
// out without one of these answering its tap.
import 'package:flutter/material.dart';

import '../theme/look.dart';
import '../theme/tokens.dart';
import 'basics.dart';

Future<T?> openSheet<T>(BuildContext context, WidgetBuilder body) {
  final t = Tok.of(context);
  return showModalBottomSheet<T>(
    context: context,
    isScrollControlled: true,
    backgroundColor: t.s1,
    barrierColor: Colors.black54,
    sheetAnimationStyle: AnimationStyle(duration: t.durSheet, curve: Tokens.spring, reverseDuration: t.dur),
    shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(t.r4))),
    builder: (ctx) => SafeArea(
      top: false,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.of(ctx).size.height * .88),
        child: SingleChildScrollView(
          padding: const EdgeInsets.fromLTRB(16, 10, 16, 20),
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Center(child: Container(width: 36, height: 4, decoration: BoxDecoration(color: t.s3, borderRadius: BorderRadius.circular(4)))),
            const SizedBox(height: 14),
            body(ctx),
          ]),
        ),
      ),
    ),
  );
}

/// The sheet's head: a title, and a dim line under it.
class SheetHead extends StatelessWidget {
  const SheetHead(this.title, {super.key, this.sub, this.lead});
  final String title;
  final String? sub;
  final Widget? lead;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Row(children: [
        if (lead != null) ...[lead!, const SizedBox(width: 12)],
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(title, style: display(context, 19, ls: -.2, color: t.bright)),
            if (sub != null) Padding(padding: const EdgeInsets.only(top: 3), child: Text(sub!, style: body(context, size: .75, color: t.dim))),
          ]),
        ),
      ]),
    );
  }
}

/// Something that cannot work right now, and what would make it work.
class Locked {
  const Locked(this.title, this.why, {this.fix, this.onFix, this.details});
  final String title, why;
  /// The fix's words; with [onFix] it is a button, without it a step to take elsewhere.
  final String? fix;
  final VoidCallback? onFix;
  /// The real names (a setting, a grant, a command), one tap away.
  final String? details;

  Future<void> show(BuildContext context) => openSheet(context, (ctx) => _LockedBody(this));
}

class _LockedBody extends StatefulWidget {
  const _LockedBody(this.l);
  final Locked l;
  @override
  State<_LockedBody> createState() => _LockedBodyState();
}

class _LockedBodyState extends State<_LockedBody> {
  bool _details = false;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final l = widget.l;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      SheetHead(l.title, lead: Disc(hue: t.base[0xA], size: 40, child: WIcon('lock', color: t.base[0xA]))),
      Text(l.why, style: body(context, color: t.text)),
      const SizedBox(height: 14),
      if (l.fix != null && l.onFix != null)
        Btn(l.fix!, primary: true, onTap: () {
          Navigator.of(context).pop();
          l.onFix!();
        })
      else if (l.fix != null)
        Container(
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r2)),
          child: Text(l.fix!, style: body(context, color: t.bright)),
        ),
      if (l.details != null) ...[
        const SizedBox(height: 8),
        Press(
          onTap: () => setState(() => _details = !_details),
          label: 'Details',
          child: SizedBox(
            height: 44,
            child: Row(children: [
              Text('Details', style: body(context, size: .8125, w: FontWeight.w600, color: t.dim)),
              AnimatedRotation(turns: _details ? .25 : 0, duration: t.dur, curve: Tokens.spring, child: WIcon('right', size: 16, color: t.dim)),
            ]),
          ),
        ),
        AnimatedSize(
          duration: t.dur,
          curve: Tokens.spring,
          alignment: Alignment.topCenter,
          child: _details
              ? Container(
                  width: double.infinity,
                  padding: const EdgeInsets.all(10),
                  decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r1)),
                  child: SelectableText(l.details!, style: mono(context, color: t.dim)),
                )
              : const SizedBox(width: double.infinity),
        ),
      ],
    ]);
  }
}
