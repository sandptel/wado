// The thick slider is felt: a drag across a 10-step slider ticks at its marks, not per pixel.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:wado/ui/theme/look.dart';
import 'package:wado/ui/widgets/slider.dart';

void main() {
  testWidgets('a drag across ticks at each step and once at the end', (tester) async {
    var felt = 0, moved = 0;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (c) async {
      if (c.method == 'HapticFeedback.vibrate') felt++;
      return null;
    });
    await tester.pumpWidget(MaterialApp(
      home: Scaffold(body: TokensScope(tokens: Look.I.tokens.value, child: Center(child: SizedBox(width: 400, child: ThickSlider(label: 'Brightness', value: 0, ticks: 10, step: .01, onInput: (_) => moved++))))),
    ));
    final g = await tester.startGesture(tester.getTopLeft(find.byType(ThickSlider)) + const Offset(1, 20));
    for (var x = 0; x < 420; x += 4) {
      await g.moveBy(const Offset(4, 0));
      await tester.pump();
    }
    await g.up();
    // The grab, nine marks between the ends and the end itself — a hundred moves, not a hundred buzzes.
    expect(moved, greaterThan(50));
    expect(felt, inInclusiveRange(10, 13));
  });
}
