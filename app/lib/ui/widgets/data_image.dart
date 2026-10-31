// Pictures that arrive as `data:` URIs — app icons, cover art, clipboard images. Decoded once per
// URI and kept: the same `MemoryImage` every rebuild means Flutter's image cache hits, instead of
// re-parsing base64 and decoding the picture again on each frame that rebuilds around it.
import 'dart:collection';
import 'dart:convert';

import 'package:flutter/foundation.dart' show Uint8List;
import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

final _cache = LinkedHashMap<String, MemoryImage?>();
const _keep = 600;

/// The picture in a `data:…;base64,` URI (or bare base64 after a comma), or null.
MemoryImage? dataImage(String? uri) {
  if (uri == null || uri.isEmpty) return null;
  final hit = _cache.remove(uri);
  if (hit != null || _cache.containsKey(uri)) {
    _cache[uri] = hit;
    return hit;
  }
  MemoryImage? img;
  try {
    final i = uri.indexOf(',');
    img = MemoryImage(base64Decode(i < 0 ? uri : uri.substring(i + 1)));
  } catch (_) {}
  _cache[uri] = img;
  if (_cache.length > _keep) _cache.remove(_cache.keys.first);
  return img;
}

final _svgs = LinkedHashMap<String, Uint8List?>();

/// An app icon or picture from a `data:` URI, raster or SVG (most desktop icon themes are SVG).
/// `fallback` when it is neither.
class DataPicture extends StatelessWidget {
  const DataPicture(this.uri, {super.key, this.size, this.fit = BoxFit.contain, this.fallback = const SizedBox.shrink()});
  final String? uri;
  final double? size;
  final BoxFit fit;
  final Widget fallback;

  @override
  Widget build(BuildContext context) {
    final u = uri;
    if (u == null || !u.startsWith('data:')) return fallback;
    if (u.startsWith('data:image/svg')) {
      var b = _svgs.remove(u);
      if (b == null && !_svgs.containsKey(u)) {
        try {
          final i = u.indexOf(',');
          final head = u.substring(0, i);
          b = head.contains('base64') ? base64Decode(u.substring(i + 1)) : Uint8List.fromList(utf8.encode(Uri.decodeComponent(u.substring(i + 1))));
        } catch (_) {}
      }
      _svgs[u] = b;
      if (_svgs.length > _keep) _svgs.remove(_svgs.keys.first);
      if (b == null) return fallback;
      return SvgPicture.memory(b, width: size, height: size, fit: fit, placeholderBuilder: (_) => fallback);
    }
    final img = dataImage(u);
    if (img == null) return fallback;
    return Image(image: img, width: size, height: size, fit: fit, gaplessPlayback: true, errorBuilder: (_, _, _) => fallback);
  }
}
