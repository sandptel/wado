import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/lane.dart';
import 'package:wado/wado/link_state.dart';

void main() {
  test('the lane says it is the network, names the cause, and offers retry only where it helps', () {
    const relayed = LinkNow(state: 'relayed', nat: 'symmetric');
    const direct = LinkNow(state: 'direct');
    expect(laneWords('files', relayed, '').body, contains('strict NAT'));
    expect(laneWords('files', direct, 'stalled').retry, isFalse);
    expect(laneWords('files', relayed, 'stalled').retry, isTrue);
    expect(laneWords('files', relayed, 'too big for the lane').body, 'too big for the lane');
    expect(laneWords('shell', direct, '').retry, isFalse);
    expect(laneWords('shell', relayed, '').title, contains('limited lane'));
  });
}
