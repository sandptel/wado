// What the hand feels on the touchpad: the moment the computer will read something from the
// fingers — a click, a right click, a drag taking hold, a swipe taking over, the point a Space or
// Mission Control commits — the phone answers with its own haptic, each kind its own pattern and
// strength. Read here from the fingers with the computer's own thresholds (the Mac's interpreter,
// `crates/macos/host/src/input/pad.rs`; libinput's are near enough), so the feel is immediate and
// never waits on the network.
import 'dart:math' as math;
import 'dart:ui' show Offset, Size;

/// Each feel as Android's composition primitives: (primitive, scale 0..1, ms before it).
const padFeels = <String, List<(String, double, int)>>{
  'click': [('click', .75, 0)],
  'right': [('tick', 1, 0), ('click', .55, 35)],
  'grab': [('thud', .9, 0)],
  'drop': [('tick', .5, 0)],
  'scroll': [('low_tick', .35, 0)],
  'zoom': [('tick', .3, 0)],
  'swipe': [('quick_rise', .55, 0)],
  'pinch': [('slow_rise', .6, 0)],
  'detent': [('click', 1, 0)],
  'undo': [('low_tick', .6, 0)],
};

/// The phone's nearest predefined effect (`feel` kinds) where composition is missing.
const padFallback = <String, String>{
  'click': 'tap',
  'right': 'reject',
  'grab': 'heavy',
  'detent': 'tap',
};

/// The computer's thresholds, in pad px and ms.
const tapMs = 180, tapSlop = 8.0, dragWindowMs = 250, gestureSlop = 10.0;
const dockShare = .5, zoomStep = 1.12;

class PadHaptics {
  PadHaptics(this.out);
  final void Function(String feel) out;

  final _at = <int, Offset>{};
  Size _size = const Size(400, 300);
  String _g = 'idle';
  int _began = 0, _most = 0;
  double _moved = 0;
  int? _lastTap;
  Offset _from = Offset.zero;
  double _spread0 = 0, _zoomRef = 0;
  /// The Dock-style gesture under way: its axis (`h`, `v`, `pinch`), and which half-way line
  /// it has passed last.
  String _axis = '';
  int _zone = 0;

  void down(int id, Offset p, int now, Size size) {
    _size = size;
    final first = _at.isEmpty;
    _at[id] = p;
    final n = _at.length;
    if (first) {
      _began = now;
      _moved = 0;
      _most = 1;
      _g = _lastTap != null && now - _lastTap! < dragWindowMs ? 'armed' : 'point';
      return;
    }
    _most = math.max(_most, n);
    // A finger joining early changes what the touch is; once under way, it stays.
    if (_moved >= tapSlop) return;
    if (n == 2 && _g == 'point') {
      _g = 'two';
    } else if (n >= 3 && (_g == 'point' || _g == 'two' || _g == 'many')) {
      _g = 'many';
    } else {
      return;
    }
    _from = _centroid();
    _spread0 = _spread();
  }

  void move(int id, Offset p, int now) {
    final was = _at[id];
    if (was == null) return;
    _at[id] = p;
    _moved += (p - was).distance / _at.length;
    switch (_g) {
      case 'armed' when _moved > 3:
        out('grab');
        _g = 'drag';
      case 'two' when _at.length == 2:
        final travel = (_centroid() - _from).distance, change = (_spread() - _spread0).abs();
        if (change >= tapSlop && change > travel) {
          _g = 'zoom';
          _zoomRef = _spread();
          out('zoom');
        } else if (travel >= tapSlop) {
          _g = 'scroll';
          out('scroll');
        }
      case 'zoom':
        final r = _spread() / math.max(_zoomRef, 1);
        if (r >= zoomStep || r <= 1 / zoomStep) {
          _zoomRef = _spread();
          out('zoom');
        }
      case 'many':
        final d = _centroid() - _from, grew = _spread() - _spread0;
        final travel = d.distance;
        if (math.max(travel, grew.abs()) < gestureSlop) return;
        _axis = _at.length >= 4 && grew.abs() > travel ? 'pinch' : (d.dx.abs() >= d.dy.abs() ? 'h' : 'v');
        _g = 'dock';
        _zone = 0;
        out(_axis == 'pinch' ? 'pinch' : 'swipe');
        _dock();
      case 'dock':
        _dock();
    }
  }

  /// Every half-way line crossed outward is a detent (the Space or view that will commit);
  /// crossed back, a softer undo.
  void _dock() {
    final d = _centroid() - _from;
    final progress = switch (_axis) {
      'h' => d.dx / (_size.width * dockShare),
      'v' => d.dy / (_size.height * dockShare),
      _ => (_spread() - _spread0) / (_size.width * dockShare * .5),
    };
    final zone = (progress.abs() + .5).floor();
    if (zone > _zone) out('detent');
    if (zone < _zone) out('undo');
    _zone = zone;
  }

  void up(int id, int now) {
    if (_at.remove(id) == null) return;
    if (_at.isNotEmpty) {
      // Fewer fingers: a one-finger drag or point goes on, and a two-finger tap whose fingers
      // lift one after the other is still a tap; anything else is over.
      if (!['point', 'armed', 'drag', 'two'].contains(_g)) _g = 'done';
      return;
    }
    final quick = now - _began < tapMs && _moved < tapSlop;
    switch (_g) {
      case 'drag':
        out('drop');
        _lastTap = null;
      case 'point' || 'armed' when quick && _most == 1:
        out('click');
        // A second tap is the double click: it does not arm a drag.
        _lastTap = _g == 'armed' ? null : now;
      case 'two' when quick && _most == 2:
        out('right');
        _lastTap = null;
      default:
        _lastTap = null;
    }
    _g = 'idle';
  }

  Offset _centroid() => _at.values.reduce((a, b) => a + b) / _at.length.toDouble();

  double _spread() {
    final c = _centroid();
    return _at.values.map((p) => (p - c).distance).reduce((a, b) => a + b) / _at.length;
  }
}
