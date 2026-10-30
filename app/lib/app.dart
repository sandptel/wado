// The app's root: theme from the phone's look, and the one navigator (lib/ui/shell.dart).
import 'package:flutter/material.dart';

import 'ui/shell.dart';
import 'ui/theme/look.dart';

class WadoApp extends StatelessWidget {
  const WadoApp({super.key, this.failure});
  final Object? failure;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder(
      valueListenable: Look.I.tokens,
      builder: (context, t, _) => MaterialApp(
        title: 'wado',
        debugShowCheckedModeBanner: false,
        theme: Look.themeData(t),
        builder: (context, child) => TokensScope(tokens: t, child: child!),
        home: failure == null ? const Shell() : _Failed(failure!),
      ),
    );
  }
}

class _Failed extends StatelessWidget {
  const _Failed(this.e);
  final Object e;
  @override
  Widget build(BuildContext context) => Scaffold(
        body: Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Text("wado couldn't start: $e", textAlign: TextAlign.center),
          ),
        ),
      );
}
