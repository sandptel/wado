// The touchpad (port of pad_ui.js, pad_orient.js and pad.css): the phone as the computer's own
// multi-touch touchpad. Held sideways (the default), the keys rail stands beside the surface — the
// link pill on top, the modifiers and keys, Type, settings — and Close sits in the surface's corner.
// Upright, or while typing, the head is on top and the keys and the text box below. The honest
// why-card sits over the surface when the computer says no; the passkey sheet comes first when the
// computer asks for one.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../wado/nav.dart';
import '../../wado/pad.dart';
import '../../wado/passkey.dart';
import '../../wado/relay_link.dart';
import '../motion/feel.dart';
import '../motion/motion.dart';
import '../pages/input_pages.dart' show TouchpadSettings;
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'gamepad_screen.dart';
import '../../wado/pad_actions.dart';
import 'pad/pad_actions_ui.dart';
import 'pad/pad_keys.dart';
import 'pad/pad_surface.dart';

void openPad(BuildContext context, {bool keyboard = false}) {
  final nav = Navigator.of(context);
  final dur = Tok.of(context).durSheet;
  Passkey.I.ask('pad', then: () => nav.push(PageRouteBuilder(
        opaque: true,
        transitionDuration: dur,
        // `tpin`: up 16 px and in.
        pageBuilder: (_, a, _) => FadeTransition(
          opacity: a,
          child: SlideTransition(position: Tween(begin: const Offset(0, .02), end: Offset.zero).animate(CurvedAnimation(parent: a, curve: Tokens.spring)), child: PadScreen(keyboard: keyboard)),
        ),
      )));
}

void openGamepad(BuildContext context) {
  Navigator.of(context).push(PageRouteBuilder(
    opaque: true,
    transitionDuration: Tok.of(context).durSheet,
    pageBuilder: (_, a, _) => FadeTransition(opacity: a, child: const GamepadScreen()),
  ));
}

class PadScreen extends StatefulWidget {
  const PadScreen({super.key, this.keyboard = false});
  final bool keyboard;
  @override
  State<PadScreen> createState() => _PadScreenState();
}

class _PadScreenState extends State<PadScreen> {
  late bool _typing = widget.keyboard || TouchpadSettings.prefs()['keyboard'] == true;
  bool _why = false;
  final _field = TextEditingController(text: '​');
  final _focus = FocusNode();

  @override
  void initState() {
    super.initState();
    active(true);
    Pad.I.start();
    PadTouch.I.attach();
    _orient();
    PadActions.I.refresh();
    if (_typing) WidgetsBinding.instance.addPostFrameCallback((_) => _focus.requestFocus());
  }

  @override
  void dispose() {
    PadMods.I.releaseAll();
    PadTouch.I.detach();
    Pad.I.stop();
    active(false);
    SystemChrome.setPreferredOrientations(const []);
    super.dispose();
  }

  bool get _landscapePref => TouchpadSettings.prefs()['landscape'] != false;

  /// Sideways unless typing or asked otherwise (`pad_orient.js`); a phone can turn for real here.
  void _orient() {
    SystemChrome.setPreferredOrientations(_landscapePref && !_typing ? const [DeviceOrientation.landscapeLeft, DeviceOrientation.landscapeRight] : const []);
  }

  void _type(bool on) {
    setState(() => _typing = on);
    _orient();
    on ? WidgetsBinding.instance.addPostFrameCallback((_) => _focus.requestFocus()) : _focus.unfocus();
  }

  void _typed(String v) {
    if (v.isEmpty) {
      PadKeys.key('backspace');
    } else {
      final s = v.replaceAll('​', '');
      if (s.length == 1) {
        PadKeys.char(s, PadMods.I.take());
      } else if (s.isNotEmpty) {
        PadKeys.text(s);
      }
    }
    _field.value = const TextEditingValue(text: '​', selection: TextSelection.collapsed(offset: 1));
  }

  void _settings() => showModalBottomSheet(
        context: context,
        isScrollControlled: true,
        backgroundColor: Tok.of(context).s0,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(Tok.of(context).r4))),
        builder: (c) => FractionallySizedBox(
          heightFactor: .85,
          child: SingleChildScrollView(padding: const EdgeInsets.all(16), child: const TouchpadSettings()),
        ),
      ).whenComplete(() {
        _orient();
        if (mounted) setState(() {});
      });

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final mq = MediaQuery.of(context);
    final landscape = !_typing && mq.size.width > mq.size.height;
    final rail = '${TouchpadSettings.prefs()['rail'] ?? 'keys'}';
    final marks = TouchpadSettings.prefs()['marks'] != false;
    final pad = EdgeInsets.fromLTRB(12 + mq.padding.left, 10 + mq.padding.top, 12 + mq.padding.right, 12 + mq.padding.bottom);
    final surface = ValueListenableBuilder(
      valueListenable: Pad.I.error,
      builder: (context, err, _) => PadSurface(
        marks: marks,
        overlay: err == null || err.isEmpty ? null : Positioned(left: 12, right: landscape ? 64 : 12, top: 12, child: _WhyCard(err: err)),
      ),
    );
    final close = IconBtn('x', label: 'Close trackpad', color: t.bright, onTap: () => Navigator.of(context).maybePop());
    final gear = IconBtn('sliders', label: 'Trackpad settings', color: t.text, onTap: _settings);
    final whyText = _why ? _Why(state: Pad.I.state.value) : null;
    return Scaffold(
      backgroundColor: t.s0,
      resizeToAvoidBottomInset: true,
      body: Padding(
        padding: pad,
        child: landscape
            ? Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                if (rail != 'none')
                  SizedBox(
                    width: 120,
                    child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                      _Pill(stacked: true, onTap: () => setState(() => _why = !_why)),
                      const SizedBox(height: 8),
                      Expanded(
                        child: ShaderMask(
                          shaderCallback: (r) => const LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [Colors.black, Colors.black, Colors.transparent], stops: [0, .93, 1]).createShader(r),
                          blendMode: BlendMode.dstIn,
                          child: SingleChildScrollView(padding: const EdgeInsets.only(bottom: 14), child: rail == 'actions' ? const PadActionList(bar: false) : const PadKeysGrid()),
                        ),
                      ),
                      const SizedBox(height: 8),
                      Enter(delay: const Duration(milliseconds: 8 * 26), ms: 420, dy: 0, dx: -14, scale: .94, fadeOnClose: false, child: _TypeBtn(onTap: () => _type(true))),
                      const SizedBox(height: 8),
                      Center(child: gear),
                    ]),
                  ),
                if (rail != 'none') const SizedBox(width: 12),
                Expanded(
                  child: Stack(children: [
                    Positioned.fill(child: surface),
                    if (rail == 'none') Positioned(left: 6, top: 6, child: _Pill(onTap: () => setState(() => _why = !_why))),
                    Positioned(
                      right: 6,
                      top: 6,
                      child: Container(decoration: BoxDecoration(color: fade(t.s2, 30), shape: BoxShape.circle), child: close),
                    ),
                    if (whyText != null) Positioned(left: 10, bottom: 10, child: _Floating(child: whyText)),
                  ]),
                ),
              ])
            : Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                SizedBox(
                  height: 44,
                  child: Row(children: [
                    _Pill(onTap: () => setState(() => _why = !_why)),
                    const Spacer(),
                    gear,
                    close,
                  ]),
                ),
                if (whyText != null) Padding(padding: const EdgeInsets.fromLTRB(4, 10, 4, 0), child: whyText),
                if (rail == 'actions') ...[const SizedBox(height: 10), const PadActionList(bar: true)],
                const SizedBox(height: 10),
                Expanded(child: surface),
                if (_typing) ...[
                  const SizedBox(height: 10),
                  const PadKeysRow(),
                  const SizedBox(height: 8),
                  TextField(
                    controller: _field,
                    focusNode: _focus,
                    autocorrect: true,
                    minLines: 1,
                    maxLines: 1,
                    textInputAction: TextInputAction.newline,
                    onChanged: _typed,
                    onSubmitted: (_) {
                      PadKeys.key('enter');
                      _focus.requestFocus();
                    },
                    style: body(context, size: 1, color: t.bright),
                    decoration: InputDecoration(
                      hintText: 'Type here — it goes to the computer',
                      hintStyle: body(context, size: 1, color: t.dim),
                      filled: true,
                      fillColor: t.s1,
                      border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r2), borderSide: BorderSide(color: t.line)),
                      enabledBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r2), borderSide: BorderSide(color: t.line)),
                    ),
                  ),
                ],
                const SizedBox(height: 10),
                Btn(_typing ? 'Hide keyboard' : 'Keyboard', icon: 'kbd', onTap: () => _type(!_typing)),
              ]),
      ),
    );
  }
}

/// The link pill: Direct, Relayed or Connecting, with the round trip; tap for what it means.
class _Pill extends StatelessWidget {
  const _Pill({required this.onTap, this.stacked = false});
  final VoidCallback onTap;
  final bool stacked;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([Pad.I.state, Pad.I.rtt, Pad.I.error]),
      builder: (context, _) {
        final s = Pad.I.state.value;
        final r = Pad.I.rtt.value;
        final tone = s == 'direct' ? t.good : (s == 'relayed' ? t.warn : t.dim);
        final words = s == 'direct' ? 'Direct' : (s == 'relayed' ? 'Relayed' : ((Pad.I.error.value ?? '').isEmpty ? 'Connecting…' : 'Not connected'));
        final rtt = r == null ? '' : '$r ms';
        final dot = Breathe(on: s == 'relayed', child: Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: tone)));
        return Press(
          onTap: onTap,
          label: words,
          child: Container(
            constraints: const BoxConstraints(minHeight: 44),
            padding: EdgeInsets.symmetric(horizontal: stacked ? 8 : 12, vertical: stacked ? 4 : 0),
            decoration: BoxDecoration(color: fade(t.s1, 12), borderRadius: BorderRadius.circular(999), border: Border.all(color: fade(tone, 60))),
            child: stacked
                ? Column(mainAxisAlignment: MainAxisAlignment.center, children: [
                    Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                      dot,
                      const SizedBox(width: 7),
                      Flexible(child: Text(words, textAlign: TextAlign.center, style: body(context, size: .8125, w: FontWeight.w600, color: t.bright))),
                    ]),
                    if (rtt.isNotEmpty) Text(rtt, style: mono(context, size: .6875, color: t.dim)),
                  ])
                : Row(mainAxisSize: MainAxisSize.min, children: [
                    dot,
                    const SizedBox(width: 7),
                    Text(words, style: body(context, size: .8125, w: FontWeight.w600, color: t.bright)),
                    if (rtt.isNotEmpty) Text(' · $rtt', style: mono(context, size: .6875, color: t.dim)),
                  ]),
          ),
        );
      },
    );
  }
}

class _Why extends StatelessWidget {
  const _Why({required this.state});
  final String state;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 360),
      child: Text(
        switch (state) {
          'direct' => 'Touches go straight to the computer over your network.',
          'relayed' => "No direct path to the computer — touches go through wado's relay, so expect some delay.",
          _ => 'Connecting to the computer…',
        },
        style: body(context, size: .7812, color: t.text).copyWith(height: 1.45),
      ),
    );
  }
}

/// A note floated over the landscape surface's corner.
class _Floating extends StatelessWidget {
  const _Floating({required this.child});
  final Widget child;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(t.r2), boxShadow: const [BoxShadow(color: Color(0xAA000000), blurRadius: 50, offset: Offset(0, 18), spreadRadius: -20)]),
      child: child,
    );
  }
}

/// `.tptype`: the accent Type button.
class _TypeBtn extends StatelessWidget {
  const _TypeBtn({required this.onTap});
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Press(
      onTap: onTap,
      scale: .94,
      label: 'Type on the computer',
      child: Container(
        height: 48,
        decoration: BoxDecoration(color: t.accent, borderRadius: BorderRadius.circular(t.r2)),
        child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
          WIcon('kbd', size: 20, color: t.onAccent),
          const SizedBox(width: 8),
          Text('Type', style: body(context, w: FontWeight.w600, color: t.onAccent)),
        ]),
      ),
    );
  }
}

class _WhyCard extends StatelessWidget {
  const _WhyCard({required this.err});
  final String err;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final w = padWhy(err);
    void retry() {
      Pad.I.stop();
      Pad.I.start();
    }

    if (w.cause == 'locked' || w.cause == 'no-passkey') {
      // The computer wants the passkey: the sheet asks, and the pad starts again once it is open.
      return WCard(children: [
        Text(w.title, style: display(context, rem(context, 1.0625))),
        Btn('Unlock', primary: true, icon: 'lock', onTap: () => Passkey.I.ask('pad', then: retry)),
      ]);
    }
    return ConstrainedBox(
      constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * .6),
      child: SingleChildScrollView(
        child: WCard(alert: true, children: [
          Text(w.title, style: body(context, size: .9375, w: FontWeight.w600, color: t.bright)),
          Text(w.body, style: body(context, size: .8125)),
          for (final c in w.cmds)
            Row(children: [
              Expanded(
                child: Container(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                  decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r2)),
                  child: SelectableText(c, style: mono(context, size: .75, color: t.bright)),
                ),
              ),
              const SizedBox(width: 8),
              Btn('Copy', expand: false, onTap: () => Clipboard.setData(ClipboardData(text: c))),
            ]),
          if (w.alt.isNotEmpty) Text(w.alt, style: body(context, size: .75, color: t.dim)),
          Btn('Try again', expand: false, onTap: retry),
          if (!RelayLink.I.up.value) const Why('The link to the computer is down — it reconnects by itself.', tone: 'bad'),
          if (Nav.I.selected.isEmpty) const SizedBox.shrink(),
        ]),
      ),
    );
  }
}
