// The app's frame (port of main.rs's layout + route.rs): home underneath, a computer's page or
// a settings page pushed over it, and the session over everything while it is shown. Pages are
// opaque: nothing shows through from the one behind. Back pops one level; from the session it
// always leaves to the computer's page.
import 'package:flutter/material.dart' hide View;
import 'package:flutter/services.dart';

import '../wado/nav.dart';
import '../wado/session.dart';
import 'computer/computer_page.dart';
import 'home/home_screen.dart';
import 'pages/pages.dart';
import 'passkey/passkey_sheet.dart';
import 'session/console/console_view.dart';
import 'theme/look.dart';
import 'theme/tokens.dart';
import 'motion/feel.dart';
import 'motion/motion.dart';
import 'widgets/notes.dart';
import 'widgets/clip_dock.dart';
import 'widgets/zoom.dart';

class Shell extends StatefulWidget {
  const Shell({super.key});
  @override
  State<Shell> createState() => _ShellState();
}

/// Three layers, each with its own clock (js/hero_flip.js, computer.css):
/// - home, which scales to .94 and fades while a computer's page covers it;
/// - the computer's page: its card grows into the hero (or, with no card on screen, it fades),
///   and its sections rise in after; closing runs the same spring at about 80%;
/// - a settings page, pushed in from the right while what it covers drifts 24% left and fades.
class _ShellState extends State<Shell> with TickerProviderStateMixin {
  late final _comp = AnimationController(vsync: this, duration: const Duration(milliseconds: 520), reverseDuration: const Duration(milliseconds: 420));
  late final _sub = AnimationController(vsync: this, duration: const Duration(milliseconds: 480), reverseDuration: const Duration(milliseconds: 340));
  String? _key; // the computer page shown, or leaving
  SubView? _page; // the settings page shown, or leaving
  bool _flight = false;
  View _was = Nav.I.view.value;

  @override
  void initState() {
    super.initState();
    final v = Nav.I.view.value;
    _key = v.computer;
    if (_key != null) _comp.value = 1;
    if (v is SubView) {
      _page = v;
      _sub.value = 1;
    }
    Nav.I.view.addListener(_go);
  }

  @override
  void dispose() {
    Nav.I.view.removeListener(_go);
    _comp.dispose();
    _sub.dispose();
    super.dispose();
  }

  void _go() {
    final v = Nav.I.view.value, was = _was;
    _was = v;
    final t = Tok.of(context);
    final k = v.computer;
    _comp.duration = t.reduced ? const Duration(milliseconds: 160) : const Duration(milliseconds: 520);
    _comp.reverseDuration = t.reduced ? const Duration(milliseconds: 120) : const Duration(milliseconds: 420);
    _sub.duration = t.reduced ? const Duration(milliseconds: 120) : const Duration(milliseconds: 480);
    _sub.reverseDuration = t.reduced ? const Duration(milliseconds: 120) : const Duration(milliseconds: 340);

    // The computer's page.
    if (k != null && (k != _key || _comp.status == AnimationStatus.reverse)) {
      final fromHome = was is HomeView;
      _flight = !t.reduced && fromHome && Motion.capture(k, MediaQuery.sizeOf(context));
      _key = k;
      Motion.pageScroll = 0;
      if (_flight) buzz(6);
      // Rendered first, then moved (hero_flip.js `waitFor`): the page's first build — its
      // heaviest frame — must not eat the start of the flight.
      _comp.value = v is SubView ? 1 : 0;
      _comp.stop();
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => WidgetsBinding.instance.endOfFrame.then((_) {
          if (!mounted || _key != k || _comp.status == AnimationStatus.reverse) return;
          if (_flight) Motion.flying.value = k;
          _comp.forward().whenComplete(() {
            if (Motion.flying.value == k && _comp.value == 1) Motion.flying.value = null;
          });
        }),
      );
    } else if (k == null && _key != null) {
      final key = _key!;
      // Scrolled away, or the card is gone: the hero is no longer near it — fade instead.
      _flight = !t.reduced && Motion.pageScroll <= 40 && Motion.capture(key, MediaQuery.sizeOf(context));
      if (_flight) Motion.flying.value = key;
      if (v is SubView) {
        // A page pushed over home from a computer's page: the page goes at once.
        _comp.value = 0;
        _key = null;
      } else {
        _comp.reverse().whenComplete(() {
          if (Nav.I.view.value.computer == null && mounted) setState(() => _key = null);
          if (Motion.flying.value == key) Motion.flying.value = null;
        });
      }
    }

    // A settings page.
    if (v is SubView) {
      final fresh = _page == null || _sub.status == AnimationStatus.reverse || _sub.value == 0;
      _page = v;
      if (fresh) _sub.forward(from: 0);
    } else if (_page != null) {
      _sub.reverse().whenComplete(() {
        if (Nav.I.view.value is! SubView && mounted) setState(() => _page = null);
      });
    }
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Session.I.shown,
      builder: (context, _) {
        final v = Nav.I.view.value;
        final inSession = Session.I.shown.value;
        final t = Tok.of(context);
        final w = MediaQuery.sizeOf(context).width;
        final comp = CurvedAnimation(parent: _comp, curve: Tokens.spring, reverseCurve: Tokens.spring.flipped);
        final sub = CurvedAnimation(parent: _sub, curve: Tokens.spring, reverseCurve: Tokens.spring.flipped);
        // What a pushed page covers: the computer's page if there is one, else home.
        final underIsComp = _key != null && _page != null && _page!.key != null;
        Widget under(Widget child, {required bool isUnder}) => AnimatedBuilder(
          animation: sub,
          child: child,
          builder: (context, child) {
            final p = isUnder && _page != null ? sub.value : 0.0;
            // One shape in every state — a changed wrapper would rebuild the page under it.
            final hidden = p >= 1 && _sub.isCompleted;
            return Offstage(
              offstage: hidden,
              child: TickerMode(
                enabled: !hidden,
                child: Opacity(
                  opacity: (1 - Curves.easeOut.transform((p * (t.reduced ? 1 : 480 / 220)).clamp(0, 1))).clamp(0, 1),
                  child: Transform.translate(offset: Offset(t.reduced ? 0 : -.24 * w * p, 0), child: child),
                ),
              ),
            );
          },
        );
        return AnnotatedRegion<SystemUiOverlayStyle>(
          value: t.light ? SystemUiOverlayStyle.dark : SystemUiOverlayStyle.light,
          child: PopScope(
            canPop: v is HomeView && !inSession,
            onPopInvokedWithResult: (did, _) {
              if (did) return;
              if (inSession) {
                if (closeSheets()) return;
                Session.I.leave();
                final k = Nav.I.selected;
                if (k.isNotEmpty) Nav.I.view.value = ComputerView(k);
                return;
              }
              Nav.I.back();
            },
            child: Scaffold(
              backgroundColor: t.s0,
              resizeToAvoidBottomInset: false,
              body: Stack(
                children: [
                  // Home: covered by a computer's page (scale .94, fade), or under a settings page.
                  Positioned.fill(
                    child: under(
                      isUnder: !underIsComp,
                      AnimatedBuilder(
                        animation: _comp,
                        child: const Zoom(child: HomeScreen()),
                        builder: (context, child) {
                          final p = _key == null ? 0.0 : const Interval(0, 360 / 520, curve: Tokens.spring).transform(_comp.value);
                          // Under a computer's page, or under a session: off stage, not ticking, not read aloud.
                          final hidden = (_key != null && _comp.isCompleted) || inSession;
                          return Offstage(
                            offstage: hidden,
                            child: TickerMode(
                              enabled: !hidden,
                              child: Opacity(
                                opacity: 1 - p,
                                child: Transform.scale(scale: t.reduced ? 1 : 1 - .06 * p, alignment: Alignment.center, child: child),
                              ),
                            ),
                          );
                        },
                      ),
                    ),
                  ),
                  if (_key != null)
                    Positioned.fill(
                      child: _Behind(
                        inSession,
                        under(
                          isUnder: underIsComp,
                          ViewClock(
                            a: _comp,
                            flight: _flight,
                            child: AnimatedBuilder(
                              animation: comp,
                              child: Zoom(
                                child: ComputerPage(key: ValueKey('c:$_key'), computerKey: _key!),
                              ),
                              builder: (context, child) {
                                // The flight carries the hero; the rest of the page fades with it. With no
                                // card to fly from, the whole page crossfades (360 ms in, 260 ms out).
                                final o = _flight ? 1.0 : comp.value;
                                return IgnorePointer(
                                  ignoring: _comp.status == AnimationStatus.reverse,
                                  child: Opacity(opacity: o.clamp(0, 1), child: child),
                                );
                              },
                            ),
                          ),
                        ),
                      ),
                    ),
                  if (_page != null)
                    Positioned.fill(
                      child: _Behind(
                        inSession,
                        AnimatedBuilder(
                          animation: sub,
                          child: ColoredBox(
                            color: t.s0,
                            child: AnimatedSwitcher(
                              duration: t.dur,
                              child: Zoom(
                                key: ValueKey('p:${_page!.key}:${_page!.page}'),
                                child: SettingsPage(computerKey: _page!.key, page: _page!.page),
                              ),
                            ),
                          ),
                          builder: (context, child) => IgnorePointer(
                            ignoring: _sub.status == AnimationStatus.reverse,
                            child: t.reduced ? Opacity(opacity: sub.value.clamp(0, 1), child: child) : Transform.translate(offset: Offset((1 - sub.value) * w, 0), child: child),
                          ),
                        ),
                      ),
                    ),
                  if (inSession) const Positioned.fill(child: ConsoleView()),
                  const Positioned.fill(child: Notes()),
                const Positioned.fill(child: PasskeySheet()),
                  const Positioned.fill(child: ClipDock()),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

/// A page under the session: kept (its state and scroll), but off stage and not ticking. One
/// shape whatever the flag, so the page under it never rebuilds.
class _Behind extends StatelessWidget {
  const _Behind(this.hidden, this.child);
  final bool hidden;
  final Widget child;
  @override
  Widget build(BuildContext context) => Offstage(
    offstage: hidden,
    child: TickerMode(enabled: !hidden, child: child),
  );
}
