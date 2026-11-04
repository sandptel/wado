import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/settings.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  test('a keys() signal fires for its own settings only', () {
    var n = 0;
    final l = Settings.I.keys(const ['theme'])..addListener(() => n++);
    Settings.I.set('volume', 0.3);
    Settings.I.set('volume', 0.4);
    expect(n, 0, reason: 'a slider elsewhere must not rebuild what listens to the theme');
    Settings.I.set('theme', 'nord');
    expect(n, 1);
    (l as ChangeNotifier).dispose();
  });
}
