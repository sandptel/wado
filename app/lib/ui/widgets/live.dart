// A ListenableBuilder that sleeps while its page is hidden. The shell keeps covered pages mounted
// (home under a computer's page, both under the session) with TickerMode off, and Offstage still
// builds and lays out: every host poll, link word and setting rebuilt pages nobody could see, on
// the main thread the video's frames also wait on. Hidden, it ignores notifications; shown
// again, it rebuilds once.
import 'package:flutter/widgets.dart';

class Live extends StatefulWidget {
  const Live({super.key, required this.listenable, required this.builder, this.child});
  final Listenable listenable;
  final TransitionBuilder builder;
  final Widget? child;
  @override
  State<Live> createState() => _LiveState();
}

class _LiveState extends State<Live> {
  bool _shown = true;

  @override
  void initState() {
    super.initState();
    widget.listenable.addListener(_changed);
  }

  @override
  void didUpdateWidget(Live o) {
    super.didUpdateWidget(o);
    if (o.listenable != widget.listenable) {
      o.listenable.removeListener(_changed);
      widget.listenable.addListener(_changed);
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // Depending on TickerMode: shown again, this rebuilds — the one rebuild a stale page owes.
    _shown = TickerMode.valuesOf(context).enabled;
  }

  @override
  void dispose() {
    widget.listenable.removeListener(_changed);
    super.dispose();
  }

  void _changed() {
    if (_shown) setState(() {});
  }

  @override
  Widget build(BuildContext context) => widget.builder(context, widget.child);
}
