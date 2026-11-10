// The look (theme.js + ui_size.js): scheme, accent, corners, motion, text and interface size —
// from Settings, as Tokens every widget reads through TokensScope.
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../wado/settings.dart';
import 'tokens.dart';

class Look {
  Look._() {
    Settings.I.addListener(_apply);
  }
  static final I = Look._();
  List<Scheme> schemes = const [];
  late final tokens = ValueNotifier<Tokens>(_make());

  Future<void> load() async {
    final raw = await rootBundle.loadString('assets/schemes.json');
    schemes = (jsonDecode(raw) as List)
        .map((e) => Scheme(e[0] as String, (e[1] as List).cast<String>()))
        .toList();
    _apply();
  }

  Scheme scheme(String name) =>
      schemes.firstWhere((s) => s.name == name, orElse: () => schemes.isEmpty ? const Scheme('default-dark', _dflt) : schemes.first);

  Tokens _make() {
    final s = Settings.I;
    final custom = s.s('theme_custom');
    List<Color> base;
    final parts = RegExp(r'(?<![0-9a-fA-F])([0-9a-fA-F]{6})(?![0-9a-fA-F])').allMatches(custom).map((m) => m[1]!).toList();
    if (parts.length >= 16) {
      base = Scheme('custom', parts.sublist(0, 16)).colors;
    } else {
      base = scheme(s.s('theme')).colors;
    }
    return Tokens(base, accent: s.s('accent'), radius: s.s('radius'), motion: s.s('motion'));
  }

  /// Any setting changed: new tokens only when the look did, or every write rebuilt the app.
  void _apply() {
    final t = _make();
    if (!t.same(tokens.value)) tokens.value = t;
  }

  /// Interface size (DESIGN.md "Size"): compact .9 · cozy 1 · comfy 1.1 · large 1.2.
  static double uiScale() => switch (Settings.I.s('ui_size')) {
        'compact' => .9,
        'comfy' => 1.1,
        'large' => 1.2,
        _ => 1.0,
      };

  static ThemeData themeData(Tokens t) => ThemeData(
        brightness: t.light ? Brightness.light : Brightness.dark,
        scaffoldBackgroundColor: t.base[0],
        canvasColor: t.base[0],
        fontFamily: Tokens.body,
        colorScheme: ColorScheme.fromSeed(
          seedColor: t.accent,
          brightness: t.light ? Brightness.light : Brightness.dark,
          primary: t.accent,
          onPrimary: t.onAccent,
          surface: t.base[0],
          onSurface: t.text,
          error: t.bad,
        ),
        textTheme: const TextTheme().apply(bodyColor: t.text, displayColor: t.bright),
        splashFactory: NoSplash.splashFactory,
        highlightColor: Colors.transparent,
        pageTransitionsTheme: const PageTransitionsTheme(builders: {
          TargetPlatform.android: PredictiveBackPageTransitionsBuilder(),
          TargetPlatform.linux: FadeForwardsPageTransitionsBuilder(),
        }),
      );
}

const _dflt = ['181818', '282828', '383838', '585858', 'b8b8b8', 'd8d8d8', 'e8e8e8', 'f8f8f8', 'ab4642', 'dc9656', 'f7ca88', 'a1b56c', '86c1b9', '7cafc2', 'ba8baf', 'a16946'];

/// The current tokens, for any widget: `Tok.of(context)`.
class TokensScope extends InheritedWidget {
  const TokensScope({super.key, required this.tokens, required super.child});
  final Tokens tokens;
  static Tokens of(BuildContext c) => c.dependOnInheritedWidgetOfExactType<TokensScope>()!.tokens;
  @override
  bool updateShouldNotify(TokensScope old) => old.tokens != tokens;
}

typedef Tok = TokensScope;

/// A new scheme or accent eases in, every surface and text tone with it, rather than snapping:
/// the room's light changing, not a cut. Calm and unhurried — 450 ms on (.4, 0, .2, 1); short with
/// reduced motion, at once with motion off. Corners and motion settings apply at once.
class EasedTokens extends StatefulWidget {
  const EasedTokens({super.key, required this.tokens, required this.child});
  final Tokens tokens;
  final Widget child;
  @override
  State<EasedTokens> createState() => _EasedTokensState();
}

class _EasedTokensState extends State<EasedTokens> with SingleTickerProviderStateMixin {
  late final _c = AnimationController(vsync: this)..addListener(() => setState(() {}));
  late Tokens _from = widget.tokens, _shown = widget.tokens;
  static const _curve = Cubic(.4, 0, .2, 1);

  @override
  void didUpdateWidget(EasedTokens old) {
    super.didUpdateWidget(old);
    final to = widget.tokens;
    if (identical(to, old.tokens)) return;
    if (to.sameColours(_shown) || to.still) {
      _c.stop();
      _shown = to;
      return;
    }
    _from = _shown;
    _c.duration = Duration(milliseconds: to.reduced ? 160 : 450);
    _c.forward(from: 0);
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    _shown = _c.isAnimating ? Tokens.lerp(_from, widget.tokens, _curve.transform(_c.value)) : widget.tokens;
    return TokensScope(tokens: _shown, child: widget.child);
  }
}
