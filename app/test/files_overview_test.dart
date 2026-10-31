import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:wado/ui/files/overview.dart';
import 'package:wado/ui/theme/look.dart';
import 'package:wado/wado/files.dart';

void main() {
  testWidgets('the files overview lays out on a phone with everything in it', (tester) async {
    tester.view.physicalSize = const Size(360 * 3, 780 * 3);
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.reset);
    Files.I.info = {
      'home': '/home/me',
      'roots': ['/home/me', '/mnt/data'],
      'trash': '/home/me/.local/share/Trash/files',
      'space': [{'root': '/home/me', 'total': 500e9, 'free': 20e9}],
    };
    final quick = {
      'pins': [{'name': 'Downloads', 'path': '/home/me/Downloads', 'kind': 'xdg'}, {'name': 'A rather long project folder name', 'path': '/home/me/p', 'kind': 'pin'}],
      'recent': [for (var i = 0; i < 20; i++) {'name': 'holiday-photo-number-$i.jpg', 'path': '/home/me/Pictures/h$i.jpg', 'mtime': 1.7e9 + i}],
    };
    String? found;
    await tester.pumpWidget(MaterialApp(
      home: Material(child: TokensScope(tokens: Look.I.tokens.value, child: FilesOverview(quick: quick, onGo: (_) {}, onOpen: (_) {}, onFind: (t, {kind = '', query = ''}) => found = kind, onRecent: () {}))),
    ));
    // The categories cascade in and the storage bars fill: let them land.
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    await tester.tap(find.text('Videos'));
    expect(found, 'video');
    expect(find.text('Downloads'), findsWidgets);
    expect(find.text('Trash'), findsOneWidget);
  });
}
