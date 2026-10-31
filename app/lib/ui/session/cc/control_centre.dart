// The control centre (port of ui/cc/mod.rs + cc.css): one sheet for everything that is not the
// picture. It floats over the stage and never takes space from it — a bottom sheet on a phone, a
// card on the right on a wide screen. The root is the hero, the tiles, sound, files and shells,
// and the settings list; every other page is pushed over it, and the root stays mounted beneath
// so coming back is instant and keeps its scroll.
import 'package:flutter/material.dart';

import '../../../core/core_host.dart';
import '../../../wado/host.dart';
import '../../../wado/nav.dart';
import '../../../wado/session.dart';
import '../../../wado/shells.dart';
import '../../computer/playing.dart';
import '../../home/cards.dart' show hues;
import '../../motion/motion.dart';
import '../../pages/page_registry.dart';
import '../../pages/pages.dart' show ApplyBar;
import '../../theme/look.dart';
import '../../theme/tokens.dart';
import '../../widgets/basics.dart';
import '../console/stage_state.dart' show leaveToComputer;
import 'hero.dart';
import 'sound_card.dart';
import 'tiles.dart';

final ccOpen = ValueNotifier<bool>(false);

void openControlCentre(BuildContext context) => ccOpen.value = true;

/// The two list pages of the root's settings list, pushed like any other.
const _thisComputer = 'cc:this_computer', _onThisPhone = 'cc:on_this_phone';
const _aboutComputer = {_thisComputer, 'display_session', 'input', 'sound_devices', 'connection_trust', 'files', 'shells'};

class ControlCentre extends StatefulWidget {
  const ControlCentre({super.key});
  @override
  State<ControlCentre> createState() => _ControlCentreState();
}

class _ControlCentreState extends State<ControlCentre> with TickerProviderStateMixin {
  late final _sheet = AnimationController(vsync: this, duration: const Duration(milliseconds: 480));
  late final _push = AnimationController(vsync: this, duration: const Duration(milliseconds: 480));
  final _pages = <String>[];
  int _opened = 0;

  @override
  void initState() {
    super.initState();
    ccOpen.addListener(_sync);
  }

  @override
  void dispose() {
    ccOpen.removeListener(_sync);
    _sheet.dispose();
    _push.dispose();
    super.dispose();
  }

  void _sync() {
    if (ccOpen.value) {
      Host.I.watch(true);
      Shells.I.request();
      _opened++;
      _sheet.animateTo(1, curve: Tokens.spring);
    } else {
      Host.I.watch(false);
      _sheet.animateBack(0, curve: Tokens.spring).then((_) {
        if (mounted && !ccOpen.value) setState(_pages.clear);
      });
    }
    setState(() {});
  }

  void _open(String page) {
    setState(() => _pages.add(page));
    _push.forward(from: 0);
  }

  void _back() {
    if (_pages.isEmpty) return;
    _push.reverse().then((_) {
      if (!mounted) return;
      setState(_pages.removeLast);
      if (_pages.isNotEmpty) _push.value = 1;
    });
  }

  void _close() => ccOpen.value = false;

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final mq = MediaQuery.of(context);
    final wide = mq.size.width >= 720;
    return AnimatedBuilder(
      animation: _sheet,
      builder: (context, _) {
        final v = _sheet.value;
        if (v == 0 && !ccOpen.value) return const SizedBox.shrink();
        final h = (mq.size.height * .88).clamp(0.0, 860.0);
        final sheet = Container(
          decoration: BoxDecoration(
            color: t.s0,
            borderRadius: wide ? BorderRadius.circular(t.r4) : BorderRadius.vertical(top: Radius.circular(t.r4)),
            border: Border.all(color: t.line),
            boxShadow: const [BoxShadow(color: Color(0x59000000), blurRadius: 60, offset: Offset(0, -20))],
          ),
          clipBehavior: Clip.antiAlias,
          child: Column(children: [
            if (!wide)
              Press(
                onTap: _close,
                label: 'Close the control centre',
                scale: 1,
                child: SizedBox(
                  height: 22,
                  child: Center(child: Container(width: 38, height: 5, decoration: BoxDecoration(color: t.s3, borderRadius: BorderRadius.circular(3)))),
                ),
              ),
            Expanded(child: _pagesStack(context)),
            const ApplyBar(),
          ]),
        );
        return Stack(children: [
          // The stage behind only dims.
          Positioned.fill(
            child: IgnorePointer(
              ignoring: !ccOpen.value,
              child: GestureDetector(
                onTap: _close,
                child: ColoredBox(color: Color.fromRGBO(0, 0, 0, wide ? 0 : .45 * v)),
              ),
            ),
          ),
          if (wide)
            Positioned(top: 14, right: 14, bottom: 14, width: 400, child: FractionalTranslation(translation: Offset((1 - v) * 1.08, 0), child: sheet))
          else
            Positioned(left: 0, right: 0, bottom: 0, height: h, child: FractionalTranslation(translation: Offset(0, (1 - v) * 1.02), child: sheet)),
        ]);
      },
    );
  }

  Widget _pagesStack(BuildContext context) {
    final t = Tok.of(context);
    return AnimatedBuilder(
      animation: _push,
      builder: (context, _) {
        final p = Tokens.spring.transform(_push.value);
        final top = _pages.isEmpty ? null : _pages.last;
        return LayoutBuilder(builder: (context, box) {
          return Stack(children: [
            // Under a pushed page: drifts and fades, then is hidden.
            Offstage(
              offstage: top != null && _push.isCompleted,
              child: IgnorePointer(
                ignoring: top != null,
                child: Opacity(
                  opacity: top == null ? 1 : (1 - p).clamp(0, 1),
                  child: Transform.translate(offset: Offset(top == null ? 0 : -.24 * box.maxWidth * p, 0), child: _root(context)),
                ),
              ),
            ),
            if (top != null)
              Transform.translate(
                offset: Offset((1 - p) * box.maxWidth, 0),
                child: ColoredBox(color: t.s0, child: _pushed(context, top)),
              ),
          ]);
        });
      },
    );
  }

  Widget _root(BuildContext context) {
    final t = Tok.of(context);
    final s = Session.I;
    var n = 0;
    // The root's blocks rise in order each time the sheet opens (`rise`, 40 ms apart).
    Widget rise(Widget w) {
      final i = n++;
      return Enter(key: ValueKey('r$_opened.$i'), delay: Duration(milliseconds: const [0, 40, 80, 110, 140][i.clamp(0, 4)]), ms: 500, dy: 14, fadeOnClose: false, child: w);
    }

    Widget sect(String title, String action, VoidCallback? onTap) => Padding(
          padding: const EdgeInsets.fromLTRB(4, 4, 4, 0),
          child: Row(children: [
            Expanded(child: Text(title.toUpperCase(), style: body(context, size: .6875, w: FontWeight.w600, color: t.dim).copyWith(letterSpacing: 1.1))),
            Press(
              onTap: onTap,
              child: SizedBox(height: 32, child: Center(child: Text(action, style: body(context, size: .8125, w: FontWeight.w600, color: onTap == null ? t.dim : t.accent)))),
            ),
          ]),
        );
    final name = CoreHost.I.computer(Nav.I.selected)?.name ?? 'Computer';
    final comps = CoreHost.I.computers.value;
    final i = comps.indexWhere((c) => c.key == Nav.I.selected);
    return ListenableBuilder(
      listenable: Listenable.merge([Host.I.state, Shells.I.shells, s.on]),
      builder: (context, _) {
        final h = Host.I.state.value;
        final shells = Shells.I.shells.value;
        final blocks = <Widget>[
          const CcHero(),
          if (h != null && (h['media'] as List? ?? const []).isNotEmpty) Playing(h: h),
          TileGrid(open: _open),
          SoundCard(open: _open),
          sect('Files', 'Open', () => _open('files')),
          sect('Shells', shells.isEmpty ? 'Open' : 'Show', () => _open('shells')),
          if (shells.isNotEmpty)
            SizedBox(
              height: 64,
              child: ListView(scrollDirection: Axis.horizontal, children: [
                for (final sh in shells)
                  Padding(
                    padding: const EdgeInsets.only(right: 8),
                    child: Press(
                      onTap: () {
                        Shells.I.active.value = (sh['id'] ?? 0) as int;
                        _open('shells');
                      },
                      child: Container(
                        padding: const EdgeInsets.fromLTRB(8, 8, 14, 8),
                        decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
                        child: Opacity(
                          opacity: sh['alive'] == false ? .55 : 1,
                          child: Row(children: [
                            Disc(hue: t.base[sh['host'] != null ? 0xE : 0xB], child: WIcon(sh['host'] != null ? 'server' : 'term', color: t.base[sh['host'] != null ? 0xE : 0xB])),
                            const SizedBox(width: 10),
                            Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisAlignment: MainAxisAlignment.center, children: [
                              Text('${sh['title'] ?? 'Shell'}', style: body(context, size: .8125, w: FontWeight.w600, color: t.bright)),
                              Text('${sh['host'] != null ? 'ssh' : 'this computer'}${sh['alive'] == false ? ' · exited' : ''}', style: body(context, size: .6875, color: t.dim)),
                            ]),
                          ]),
                        ),
                      ),
                    ),
                  ),
              ]),
            ),
          sect('Settings', 'Edit tiles', () => _open('tiles')),
          ListBox(children: [
            NavRow(icon: 'monitor', hue: hues[(i < 0 ? 0 : i) % 6], title: 'This computer', sub: '$name · display, trackpad, sound, connection', onOpen: () => _open(_thisComputer)),
            NavRow(icon: 'phone', hue: '0A', title: 'On this phone', sub: 'Appearance, input feel, gamepad, tiles, diagnostics', onOpen: () => _open(_onThisPhone)),
          ]),
          Row(children: [
            Expanded(child: Btn('Resync', icon: 'refresh', onTap: s.on.value ? s.negotiate : null)),
            const SizedBox(width: 8),
            Expanded(
              child: Btn('Leave running', icon: 'home', onTap: s.on.value
                  ? () {
                      _close();
                      leaveToComputer();
                    }
                  : null),
            ),
          ]),
          Btn('End session', icon: 'power', danger: true, onTap: s.on.value
              ? () {
                  _close();
                  s.stop();
                  final k = Nav.I.selected;
                  if (k.isNotEmpty) Nav.I.view.value = ComputerView(k);
                }
              : null),
        ];
        final mq = MediaQuery.of(context);
        return ListView(
          padding: EdgeInsets.fromLTRB(16, 6, 16, 24 + mq.padding.bottom),
          children: [for (var j = 0; j < blocks.length; j++) ...[if (j > 0) const SizedBox(height: 12), rise(blocks[j])]],
        );
      },
    );
  }

  Widget _pushed(BuildContext context, String page) {
    final t = Tok.of(context);
    final comps = CoreHost.I.computers.value;
    final i = comps.indexWhere((c) => c.key == Nav.I.selected);
    final name = i < 0 ? '' : comps[i].name;
    final title = switch (page) { _thisComputer => 'This computer', _onThisPhone => 'On this phone', _ => pageFor(page).title };
    final Widget content = switch (page) {
      _thisComputer => ListBox(children: [
          NavRow(icon: 'monitor', hue: '0D', title: 'Display & session', sub: 'Resolution, frame rate, quality', onOpen: () => _open('display_session')),
          NavRow(icon: 'phone', hue: '0C', title: 'Virtual input', sub: 'Touchpad · Keyboard · Gamepad', onOpen: () => _open('input')),
          NavRow(icon: 'sound', hue: '0E', title: 'Sound & devices', sub: 'Outputs, apps, Bluetooth', onOpen: () => _open('sound_devices')),
          NavRow(icon: 'net', hue: '09', title: 'Connection & trust', sub: 'Link, Wi-Fi, devices', onOpen: () => _open('connection_trust')),
        ]),
      _onThisPhone => ListBox(children: [
          NavRow(icon: 'palette', hue: '0A', title: 'Appearance', sub: 'Colours, shape, text and interface size', onOpen: () => _open('appearance')),
          NavRow(icon: 'hand', hue: '0C', title: 'Input feel', sub: 'Gestures, scrolling, touch', onOpen: () => _open('input_feel')),
          NavRow(icon: 'apps', hue: '0E', title: 'Quick tiles', sub: 'What this sheet shows', onOpen: () => _open('tiles')),
          NavRow(icon: 'pulse', hue: '08', title: 'Diagnostics', sub: 'Readouts, health, server log', onOpen: () => _open('diagnostics')),
        ]),
      _ => pageFor(page).build(context),
    };
    final full = pageFor(page).fullBleed;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(8, 0, 16, 2),
        child: Row(children: [
          IconBtn('left', label: 'Back', color: t.bright, onTap: _back),
          const SizedBox(width: 4),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
              if (_aboutComputer.contains(page) && name.isNotEmpty)
                Row(children: [
                  Container(width: 7, height: 7, decoration: BoxDecoration(shape: BoxShape.circle, color: slot(context, hues[(i < 0 ? 0 : i) % 6]))),
                  const SizedBox(width: 6),
                  Text(name.toUpperCase(), style: body(context, size: .6562, w: FontWeight.w600, color: t.dim).copyWith(letterSpacing: 1)),
                ]),
              Text(title, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1.25), ls: -.2)),
            ]),
          ),
        ]),
      ),
      Expanded(child: full ? content : SingleChildScrollView(padding: const EdgeInsets.fromLTRB(16, 6, 16, 24), child: content)),
    ]);
  }
}
