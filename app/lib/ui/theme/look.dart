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
    final parts = custom.split(RegExp(r'[,\s]+')).where((x) => x.length == 6).toList();
    if (parts.length == 16) {
      base = Scheme('custom', parts).colors;
    } else {
      base = scheme(s.s('theme')).colors;
    }
    return Tokens(base, accent: s.s('accent'), radius: s.s('radius'), motion: s.s('motion'));
  }

  void _apply() => tokens.value = _make();

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
