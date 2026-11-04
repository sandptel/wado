// Devices (privileged, 2026-11-04): every device this computer trusts, like a "where you're signed
// in" page — what each may do, how long it has, who let it in — and Sign out on the ones this phone
// let in. "Add a device" asks the computer for an ordinary pairing code, capped to this phone's own
// grants and seven days, after a passkey check; this phone then answers the new device's check
// words. Only the computer can turn any of this on.
import 'dart:async';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_zxing/flutter_zxing.dart';

import '../../core/core_host.dart';
import '../../wado/devices.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/passkey.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';
import '../widgets/sheet.dart';

String _computer() => CoreHost.I.computer(Nav.I.selected)?.name ?? 'this computer';

String _left(int? until) {
  if (until == null) return 'until removed';
  final s = until - DateTime.now().millisecondsSinceEpoch ~/ 1000;
  if (s <= 0) return 'ending';
  if (s < 3600) return 'ends in ${(s / 60).ceil()} min';
  if (s < 86400) return 'ends in ${(s / 3600).round()} h';
  return 'ends in ${(s / 86400).round()} d';
}

String _words(String grants) {
  if (grants.isEmpty) return 'the desktop only';
  const say = {
    'files-ro': 'files (read)',
    'files-rw': 'files',
    'host': 'sound & sleep',
    'input': 'trackpad',
    'takeover': 'take over',
    'devices': 'manages devices',
  };
  return grants.split(',').map((g) => say[g] ?? g).join(', ');
}

class DevicesPage extends StatefulWidget {
  const DevicesPage({super.key});
  @override
  State<DevicesPage> createState() => _DevicesPageState();
}

class _DevicesPageState extends State<DevicesPage> {
  @override
  void initState() {
    super.initState();
    Devices.I.watch(true);
  }

  @override
  void dispose() {
    Devices.I.watch(false);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([Devices.I.list, Devices.I.error, Devices.I.asks]),
      builder: (context, _) {
        final list = Devices.I.list.value;
        final err = Devices.I.error.value;
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Why('Every device ${_computer()} lets in. This phone can sign out the ones it let in; the rest are signed out on the computer.'),
          const SizedBox(height: 10),
          if (Devices.I.asks.value.isNotEmpty) ...[for (final a in Devices.I.asks.value) _AskCard(a: a), const SizedBox(height: 10)],
          if (err.isNotEmpty) ...[WCard(alert: true, children: [Text(err, style: body(context, color: t.bright))]), const SizedBox(height: 10)],
          ListBox(children: [
            if (list.isEmpty) Padding(padding: const EdgeInsets.all(14), child: Text('Asking ${_computer()}…', style: body(context, color: t.dim))),
            for (final d in list) _Row(d: d),
          ]),
          const SizedBox(height: 12),
          Btn('Add a device', primary: true, icon: 'plus', onTap: () => openSheet(context, (_) => const _Add())),
          if (list.any((d) => d.mine)) ...[
            const SizedBox(height: 8),
            Btn('Sign out everything this phone let in', danger: true, onTap: () {
              for (final d in list.where((d) => d.mine)) {
                Devices.I.signOut(d.key);
              }
            }),
          ],
        ]);
      },
    );
  }
}

class _Row extends StatelessWidget {
  const _Row({required this.d});
  final DeviceRow d;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final by = d.mine ? ' · let in by this phone' : '';
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      child: Row(children: [
        Disc(hue: d.until != null ? t.base[0xA] : t.accent, size: 36, child: WIcon(d.until != null ? 'pulse' : 'phone', size: 18, color: d.until != null ? t.base[0xA] : t.accent)),
        const SizedBox(width: 12),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(children: [
              Flexible(child: Text(d.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright))),
              if (d.me) Padding(padding: const EdgeInsets.only(left: 6), child: Text('THIS PHONE', style: body(context, size: .5625, w: FontWeight.w700, color: t.good).copyWith(letterSpacing: .6))),
            ]),
            Text('${_words(d.grants)} · ${_left(d.until)}$by', style: body(context, size: .6875, color: t.dim)),
          ]),
        ),
        if (d.mine)
          Press(
            onTap: () => Devices.I.signOut(d.key),
            label: 'Sign out ${d.name}',
            child: SizedBox(height: 44, child: Center(child: Text('Sign out', style: body(context, size: .75, w: FontWeight.w700, color: t.bad)))),
          ),
      ]),
    );
  }
}

/// A device typed one of this phone's codes: are its four words the ones it shows?
class _AskCard extends StatelessWidget {
  const _AskCard({required this.a});
  final PairAsk a;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return WCard(children: [
      Text('${a.device} typed your code', style: body(context, w: FontWeight.w600, color: t.bright)),
      const Why('Let it in only if it shows these same four words.'),
      const SizedBox(height: 6),
      Wrap(spacing: 6, runSpacing: 6, children: [
        for (final w in a.words.split(' '))
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
            decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(999)),
            child: Text(w, style: mono(context, size: .8125, w: FontWeight.w600, color: t.bright)),
          ),
      ]),
      const SizedBox(height: 10),
      Row(children: [
        Expanded(child: Btn('They differ', danger: true, onTap: () => Devices.I.answer(a.typed, false))),
        const SizedBox(width: 8),
        Expanded(child: Btn('Same — let it in', primary: true, onTap: () => Devices.I.answer(a.typed, true))),
      ]),
      Text('${a.left} s left · silence counts as no', style: body(context, size: .6875, color: t.dim)),
    ]);
  }
}

/// Add a device: what it may do (never more than this phone), how long, then the code.
class _Add extends StatefulWidget {
  const _Add();
  @override
  State<_Add> createState() => _AddState();
}

class _AddState extends State<_Add> {
  final _on = <String>{'clipboard'};
  int _for = 86400;

  @override
  void initState() {
    super.initState();
    Devices.I.minted.value = null;
  }

  Map<String, dynamic> get _mine => (Host.I.config.value?['grants'] as Map?)?.cast<String, dynamic>() ?? {};
  bool _has(String token) => switch (token) {
        'files-ro' => _mine['files'] == 'ro' || _mine['files'] == 'rw',
        'files-rw' => _mine['files'] == 'rw',
        _ => _mine[token] == true,
      };

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([Devices.I.minted, Devices.I.busy, Devices.I.error, Devices.I.asks]),
      builder: (context, _) {
        final m = Devices.I.minted.value;
        if (m != null) return _Code(m: m);
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          SheetHead('Let a device into ${_computer()}', sub: '${_computer()} makes the code; it works once, for 10 minutes'),
          Text('IT CAN', style: body(context, size: .6875, w: FontWeight.w600, color: t.dim).copyWith(letterSpacing: 1.1)),
          for (final (tok, label) in const [
            ('clipboard', 'Clipboard'),
            ('files-ro', 'Files, read only'),
            ('join', 'Join a session in use'),
            ('input', 'Trackpad and typing'),
            ('host', 'Sound, Bluetooth, sleep'),
            ('shells', 'Shells'),
          ])
            _has(tok)
                ? SwitchRow(title: label, on: _on.contains(tok), onToggle: (v) => setState(() => v ? _on.add(tok) : _on.remove(tok)))
                : SwitchRow(title: label, sub: 'This phone doesn\'t have it, so it can\'t give it', on: false, disabled: true, onToggle: (_) {}),
          SwitchRow(title: 'Manage devices', sub: 'Only ${_computer()} can give this', on: false, disabled: true, onToggle: (_) {}),
          const SizedBox(height: 10),
          Text('FOR', style: body(context, size: .6875, w: FontWeight.w600, color: t.dim).copyWith(letterSpacing: 1.1)),
          const SizedBox(height: 6),
          Seg(options: const [('3600', '1 hour'), ('86400', '1 day'), ('604800', '7 days')], value: '$_for', onPick: (v) => setState(() => _for = int.parse(v))),
          const Hint('Up to 7 days from a phone. Longer only on the computer.'),
          if (Devices.I.error.value.isNotEmpty) ...[
            WCard(alert: true, children: [Text(Devices.I.error.value, style: body(context, color: t.bright))]),
            const SizedBox(height: 8),
          ],
          Btn(
            'Confirm with passkey · make the code',
            primary: true,
            icon: 'lock',
            busy: Devices.I.busy.value,
            onTap: () => Passkey.I.ask('allow', then: () => Devices.I.mint([for (final g in _on) if (_has(g)) g], _for)),
          ),
        ]);
      },
    );
  }
}

class _Code extends StatefulWidget {
  const _Code({required this.m});
  final Minted m;
  @override
  State<_Code> createState() => _CodeState();
}

class _CodeState extends State<_Code> {
  ui.Image? _qr;
  Timer? _tick;
  @override
  void initState() {
    super.initState();
    _draw();
    _tick = Timer.periodic(const Duration(seconds: 1), (_) => mounted ? setState(() {}) : null);
  }

  /// The link as a QR, encoded once off the frame's build and kept as an image.
  Future<void> _draw() async {
    final e = zx.encodeBarcode(contents: widget.m.link, params: EncodeParams(format: Format.qrCode, width: 240, height: 240, margin: 8));
    final d = e.data;
    if (!e.isValid || d == null) return;
    final w = e.width ?? 240, h = e.height ?? 240;
    final rgba = Uint8List(w * h * 4);
    for (var i = 0; i < w * h; i++) {
      final v = d[i];
      rgba[i * 4] = v;
      rgba[i * 4 + 1] = v;
      rgba[i * 4 + 2] = v;
      rgba[i * 4 + 3] = 255;
    }
    ui.decodeImageFromPixels(rgba, w, h, ui.PixelFormat.rgba8888, (img) => mounted ? setState(() => _qr = img) : null);
  }

  @override
  void dispose() {
    _tick?.cancel();
    _qr?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final left = widget.m.until - DateTime.now().millisecondsSinceEpoch ~/ 1000;
    final asks = Devices.I.asks.value;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      SheetHead('Code for ${_computer()}', sub: 'On the new device, open wado and scan this, or type the code'),
      Center(
        child: Container(
          width: 220,
          height: 220,
          padding: const EdgeInsets.all(10),
          // The QR keeps its own black on white whatever the scheme: a reader needs that.
          decoration: BoxDecoration(color: const Color(0xFFFFFFFF), borderRadius: BorderRadius.circular(t.r3)),
          child: _qr == null ? const SizedBox.shrink() : RawImage(image: _qr, filterQuality: FilterQuality.none),
        ),
      ),
      const SizedBox(height: 12),
      Center(
        child: Press(
          onTap: () => Clipboard.setData(ClipboardData(text: widget.m.typed)),
          label: 'Copy the code',
          child: Text(widget.m.typed, style: mono(context, size: 1.25, w: FontWeight.w700, color: t.bright)),
        ),
      ),
      const SizedBox(height: 6),
      Text(
        left > 0 ? 'Works once · ${left ~/ 60}:${(left % 60).toString().padLeft(2, '0')} left' : 'This code ran out — make a new one',
        textAlign: TextAlign.center,
        style: body(context, size: .75, color: t.dim),
      ),
      if (asks.isNotEmpty) ...[const SizedBox(height: 12), for (final a in asks) _AskCard(a: a)],
      const SizedBox(height: 12),
      Btn('Done', onTap: () => Navigator.of(context).pop()),
    ]);
  }
}
