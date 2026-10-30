// The app's frame (port of main.rs's layout + route.rs): home underneath, a computer's page or
// a settings page pushed over it, and the session over everything while it is shown. Pages are
// opaque: nothing shows through from the one behind. Back pops one level; from the session it
// always leaves to the computer's page.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../wado/nav.dart';
import '../wado/session.dart';
import 'computer/computer_page.dart';
import 'home/home_screen.dart';
import 'pages/pages.dart';
import 'session/session_view.dart';
import 'theme/look.dart';
import 'theme/tokens.dart';

class Shell extends StatelessWidget {
  const Shell({super.key});

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Listenable.merge([Nav.I.view, Session.I.shown]),
      builder: (context, _) {
        final v = Nav.I.view.value;
        final inSession = Session.I.shown.value;
        final t = Tok.of(context);
        return AnnotatedRegion<SystemUiOverlayStyle>(
          value: t.light ? SystemUiOverlayStyle.dark : SystemUiOverlayStyle.light,
          child: PopScope(
            canPop: v is HomeView && !inSession,
            onPopInvokedWithResult: (did, _) {
              if (did) return;
              if (inSession) {
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
              body: Stack(children: [
                const Positioned.fill(child: HomeScreen()),
                Positioned.fill(
                  child: AnimatedSwitcher(
                    duration: t.durSheet,
                    switchInCurve: Tokens.spring,
                    switchOutCurve: Tokens.spring,
                    transitionBuilder: (child, a) => SlideTransition(
                      position: Tween(begin: const Offset(1, 0), end: Offset.zero).animate(a),
                      child: child,
                    ),
                    child: switch (v) {
                      HomeView() => const SizedBox.shrink(key: ValueKey('home')),
                      ComputerView(:final key) => ColoredBox(key: ValueKey('c:$key'), color: t.s0, child: ComputerPage(computerKey: key)),
                      SubView(:final key, :final page) => ColoredBox(key: ValueKey('p:$key:$page'), color: t.s0, child: SettingsPage(computerKey: key, page: page)),
                    },
                  ),
                ),
                if (inSession) const Positioned.fill(child: SessionView()),
              ]),
            ),
          ),
        );
      },
    );
  }
}
