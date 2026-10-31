// The interface size (port of size.css + js/ui_size.js `--ui-scale`): `zoom` on the app's own
// pages and sheets — spacing, radii, control heights and icons grow or shrink together, and the
// text with them (Android's "display size"). Never on the stage: the picture's size is the
// session's, and the console, touchpad and gamepad lay themselves out against the glass.
import 'package:flutter/material.dart';

import '../../wado/settings.dart';

const interfaces = {'compact': .9, 'cozy': 1.0, 'comfy': 1.1, 'large': 1.2};

double get uiScale => interfaces[Settings.I.s('ui_size')] ?? 1.0;

class Zoom extends StatelessWidget {
  const Zoom({super.key, required this.child});
  final Widget child;
  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: Settings.I,
        child: child,
        builder: (context, child) {
          final z = uiScale;
          if (z == 1) return child!;
          final mq = MediaQuery.of(context);
          // Laid out on a canvas 1/z the size, then drawn z times larger: what fills the screen
          // still fills it, and the safe areas stay where they are on the glass.
          return LayoutBuilder(
            builder: (context, box) => ClipRect(
              child: OverflowBox(
                alignment: Alignment.topLeft,
                minWidth: box.maxWidth / z,
                maxWidth: box.maxWidth / z,
                minHeight: box.maxHeight / z,
                maxHeight: box.maxHeight / z,
                child: Transform.scale(
                  scale: z,
                  alignment: Alignment.topLeft,
                  child: MediaQuery(
                    data: mq.copyWith(
                      size: mq.size / z,
                      padding: mq.padding / z,
                      viewPadding: mq.viewPadding / z,
                      viewInsets: mq.viewInsets / z,
                    ),
                    child: child!,
                  ),
                ),
              ),
            ),
          );
        },
      );
}
