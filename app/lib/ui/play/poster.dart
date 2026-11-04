// A game's poster: the app's own icon, large, on a wash of one base16 accent slot mixed into the
// ground — so a scheme swap recolours the whole shelf and no colour is hard-coded.
import 'package:flutter/material.dart';

import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/data_image.dart';

/// The accent slot (base08–0F) an app's poster washes in.
// ponytail: its name hashed onto the eight slots, not the icon's dominant colour — decoding the
// icon's pixels on the frame is the jitter this avoids; sample off-isolate if a hue reads wrong.
int posterHue(String name) => 8 + name.codeUnits.fold(0, (a, c) => (a * 31 + c) & 0xffff) % 8;

/// The wash behind a poster or a launch: the slot over the ground, brighter at the top.
BoxDecoration wash(Tokens t, int hue, double radius) {
  final h = t.base[hue];
  return BoxDecoration(
    borderRadius: BorderRadius.circular(radius),
    gradient: LinearGradient(
      begin: Alignment.topCenter,
      end: Alignment.bottomCenter,
      colors: [mix(h, t.base[0], 52), mix(h, t.base[0], 86)],
    ),
  );
}

/// The app's icon in a well of its own hue, or its first letter.
class PosterGlyph extends StatelessWidget {
  const PosterGlyph({super.key, required this.name, this.icon, required this.size});
  final String name;
  final String? icon;
  final double size;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final h = t.base[posterHue(name)];
    final letter = Text(name.isEmpty ? '?' : name.characters.first.toUpperCase(), style: display(context, size * .42, color: t.base[0]));
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(color: mix(h, t.base[0], 12), borderRadius: BorderRadius.circular(size * .3)),
      child: SizedBox.square(dimension: size * .62, child: DataPicture(icon, size: size * .62, fallback: Center(child: letter))),
    );
  }
}

class Poster extends StatelessWidget {
  const Poster({super.key, required this.name, this.icon, this.sub = '', this.running = false, this.width = 96, this.onTap, this.onLongPress, this.posterKey});
  final String name, sub;
  final String? icon;
  final bool running;
  final double width;
  final VoidCallback? onTap, onLongPress;
  /// Where the launch grows from.
  final GlobalKey? posterKey;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Press(
      onTap: onTap,
      onLongPress: onLongPress,
      label: running ? '$name, running' : name,
      child: SizedBox(
        width: width,
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          AspectRatio(
            aspectRatio: 3 / 4,
            child: Container(
              key: posterKey,
              decoration: wash(t, posterHue(name), t.r3),
              alignment: Alignment.center,
              child: Stack(clipBehavior: Clip.none, children: [
                Center(child: PosterGlyph(name: name, icon: icon, size: width * .5)),
                if (running)
                  Positioned(
                    top: 8,
                    left: 8,
                    child: Container(width: 8, height: 8, decoration: BoxDecoration(color: t.good, shape: BoxShape.circle)),
                  ),
              ]),
            ),
          ),
          const SizedBox(height: 6),
          Text(name, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, w: FontWeight.w600, color: t.bright)),
          if (sub.isNotEmpty) Text(sub, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .6875, color: t.dim)),
        ]),
      ),
    );
  }
}
