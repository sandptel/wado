import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/host_caps.dart';

void main() {
  test('an older daemon is Linux with everything on', () {
    final h = HostCaps.parse({'caps': []});
    expect(h.platform, 'linux');
    expect(h.mac, isFalse);
    expect(h.state('session').on, isTrue);
    expect(HostCaps.parse(null).state('wifi').on, isTrue);
  });

  test('Linux stays as it was even when the daemon marks something off', () {
    final h = HostCaps.parse({
      'platform': 'linux',
      'host_caps': {'on': [], 'off': [{'cap': 'wifi', 'why': 'no NetworkManager'}]},
    });
    expect(h.state('wifi').on, isTrue);
    expect(h.named(Cap.off), isEmpty);
  });

  test('a Mac: later with its why, off with its why, unknown on', () {
    final h = HostCaps.parse({
      'platform': 'macos',
      'host_caps': {
        'on': ['audio', 'clipboard'],
        'off': [
          {'cap': 'session', 'why': 'coming to macOS', 'later': true},
          {'cap': 'touch', 'why': 'Allow wado in System Settings › Privacy & Security › Accessibility'},
        ],
      },
    });
    expect(h.mac, isTrue);
    expect(h.state('session').cap, Cap.later);
    expect(h.state('session').why, 'coming to macOS');
    expect(h.state('touch').cap, Cap.off);
    expect(h.state('touch').why, contains('Accessibility'));
    expect(h.state('audio').on, isTrue);
    expect(h.state('teleport').on, isTrue);
    expect(h.named(Cap.later), ['session']);
    expect(h.named(Cap.off), ['touch']);
  });
}
