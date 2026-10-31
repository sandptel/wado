// A settings page pushed over home or a computer's page (port of ui/subpage.rs +
// cc/apply_bar.rs): the head — an eyebrow naming the computer, so a setting is never ambiguous
// about which computer it changes — the page, and the Apply bar while settings differ from the
// running session.
import 'dart:convert';

import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/nav.dart';
import '../../wado/session.dart';
import '../../wado/settings.dart';
import '../home/cards.dart' show hues;
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'page_registry.dart';

const _underInput = {'trackpad', 'keyboard', 'gamepad'};

class SettingsPage extends StatelessWidget {
  const SettingsPage({super.key, required this.computerKey, required this.page});
  final String? computerKey;
  final String page;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final mq = MediaQuery.of(context);
    final p = pageFor(page);
    final comps = CoreHost.I.computers.value;
    final i = computerKey == null ? -1 : comps.indexWhere((c) => c.key == computerKey);
    final name = i < 0 ? '' : comps[i].name;
    final hue = slot(context, hues[(i < 0 ? 0 : i) % 6]);
    final eyebrow = _underInput.contains(page) ? '$name · Virtual input' : name;
    final back = _underInput.contains(page) ? 'Back to Virtual input' : (computerKey != null ? 'Back to $name' : 'Back to home');
    final head = Padding(
      padding: EdgeInsets.fromLTRB(8, 6 + mq.padding.top, 16, 6),
      child: Row(children: [
        IconBtn('left', label: back, color: t.bright, onTap: Nav.I.back),
        const SizedBox(width: 6),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
            if (computerKey != null && name.isNotEmpty)
              Row(children: [
                Container(width: 7, height: 7, decoration: BoxDecoration(shape: BoxShape.circle, color: hue)),
                const SizedBox(width: 6),
                Flexible(
                  child: Text(eyebrow.toUpperCase(),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(fontFamily: Tokens.body, fontSize: rem(context, .6562), fontWeight: FontWeight.w600, letterSpacing: rem(context, .6562) * .1, color: t.dim)),
                ),
              ]),
            Text(p.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1.25), ls: -.2).copyWith(height: 1.15)),
          ]),
        ),
      ]),
    );
    return ColoredBox(
      color: t.s0,
      child: Stack(children: [
        Column(children: [
          if (p.fullBleed) head,
          Expanded(
            child: p.fullBleed
                ? p.build(context)
                : ListView(
                    padding: EdgeInsets.fromLTRB(16, 0, 16, 96 + mq.padding.bottom),
                    children: [
                      Center(
                        child: ConstrainedBox(
                          constraints: const BoxConstraints(maxWidth: 560),
                          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [head, const SizedBox(height: 6), p.build(context)]),
                        ),
                      ),
                    ],
                  ),
          ),
        ]),
        const Positioned(left: 0, right: 0, bottom: 0, child: ApplyBar()),
      ]),
    );
  }
}

/// How many top-level fields differ between the config last sent and the one these settings
/// would build — so it cannot disagree with what Apply will actually send.
int pendingChanges() {
  final a = Session.I.applied;
  if (a == null) return 0;
  final b = Session.config();
  return b.keys.where((k) => jsonEncode(a[k]) != jsonEncode(b[k])).length;
}

/// The pending-changes bar: only while a session runs and the settings differ from it.
class ApplyBar extends StatelessWidget {
  const ApplyBar({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final mq = MediaQuery.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([Settings.I, Session.I.on]),
      builder: (context, _) {
        final n = Session.I.on.value ? pendingChanges() : 0;
        return AnimatedSlide(
          offset: Offset(0, n == 0 ? 1.2 : 0),
          duration: t.durSheet,
          curve: Tokens.spring,
          child: n == 0
              ? const SizedBox(height: 0)
              : Container(
                  padding: EdgeInsets.fromLTRB(16, 10, 16, 10 + mq.padding.bottom),
                  decoration: BoxDecoration(color: t.s1, border: Border(top: BorderSide(color: t.line))),
                  child: Row(children: [
                    Text(n == 1 ? '1 change' : '$n changes', style: body(context, w: FontWeight.w600, color: t.bright)),
                    const SizedBox(width: 10),
                    Expanded(child: Text('Apps keep running', style: body(context, size: .75, color: t.dim))),
                    Btn('Apply', primary: true, expand: false, onTap: Session.I.reconfigure),
                  ]),
                ),
        );
      },
    );
  }
}
