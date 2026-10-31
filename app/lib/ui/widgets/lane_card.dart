// The lane hint's card (port of ui/lane_hint.rs): at the spot the relay's cap is felt, what is
// happening in plain words, Retry direct where a direct path could change it, and a dismiss that
// holds until the words change.
import 'package:flutter/material.dart';

import '../../wado/lane.dart';
import '../../wado/link_state.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import 'basics.dart';

class LaneCard extends StatefulWidget {
  const LaneCard({super.key, required this.spot, this.onRetry});
  final String spot;
  final VoidCallback? onRetry;
  @override
  State<LaneCard> createState() => _LaneCardState();
}

class _LaneCardState extends State<LaneCard> {
  String? _dismissed;

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([Lane.I.hints, Link.I.now]),
      builder: (context, _) {
        final detail = Lane.I.hints.value[widget.spot];
        if (detail == null || detail == _dismissed) return const SizedBox.shrink();
        final w = laneWords(widget.spot, Link.I.now.value, detail);
        final c = t.base[0xA];
        return Semantics(
          liveRegion: true,
          child: Container(
            padding: const EdgeInsets.fromLTRB(14, 10, 6, 12),
            decoration: BoxDecoration(color: Color.alphaBlend(fade(c, 90), t.s1), borderRadius: BorderRadius.circular(t.r3), border: Border.all(color: fade(c, 55))),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Row(children: [
                Container(width: 8, height: 8, decoration: BoxDecoration(color: c, shape: BoxShape.circle)),
                const SizedBox(width: 8),
                Expanded(child: Text(w.title, style: body(context, size: .875, w: FontWeight.w600, color: t.bright))),
                IconBtn('x', label: 'Dismiss', color: t.dim, onTap: () => setState(() => _dismissed = detail)),
              ]),
              Padding(padding: const EdgeInsets.only(right: 8), child: Text(w.body, style: body(context, size: .8125, color: t.dim).copyWith(height: 1.45))),
              if (w.retry && widget.onRetry != null) ...[const SizedBox(height: 8), Btn('Retry direct', expand: false, onTap: widget.onRetry)],
            ]),
          ),
        );
      },
    );
  }
}
