// Add a computer (port of home/pages/add_computer.rs + landing/pair.rs): this device pairs with a
// code shown on the computer — a pairing link (the QR's, pasted) or the typed code with its Remote
// ID. A typed code shows four check words the computer shows too; you confirm them there.
import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:flutter_zxing/flutter_zxing.dart';

import '../../core/core_host.dart';
import '../../wado/nav.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../motion/motion.dart';

const defaultRelay = 'https://wado-relay.indiasouthcentral.cloudapp.azure.com';
const _alphabet = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';

/// landing/pair.rs `format`: look-alikes become digits, a dash after four.
String formatCode(String input) {
  final code = input
      .toUpperCase()
      .split('')
      .map((c) => switch (c) { 'I' || 'L' => '1', 'O' => '0', _ => c })
      .where(_alphabet.contains)
      .take(8)
      .join();
  return code.length <= 4 ? code : '${code.substring(0, 4)}-${code.substring(4)}';
}

class AddComputer extends StatefulWidget {
  const AddComputer({super.key});
  /// A pairing link that opened the app (ui/share/inbound.dart): paired as soon as the page is up.
  static String? pending;
  @override
  State<AddComputer> createState() => _AddComputerState();
}

class _AddComputerState extends State<AddComputer> {
  final _link = TextEditingController();
  final _id = TextEditingController();
  final _code = TextEditingController();
  final _relay = TextEditingController(text: defaultRelay);
  int? _op;
  String _step = '';
  List<String> _words = const [];
  String _why = '';
  StreamSubscription? _sub;

  @override
  void initState() {
    super.initState();
    _sub = CoreHost.I.pairSteps.listen((e) {
      if (e['op'] != _op) return;
      setState(() {
        _step = e['step'] as String;
        if (_step == 'check_words') _words = (e['words'] as List).cast<String>();
        if (_step == 'failed') _why = (e['why'] ?? '') as String;
      });
      if (_step == 'done') Nav.I.openComputer(e['key'] as String);
    });
    final link = AddComputer.pending;
    AddComputer.pending = null;
    if (link != null) {
      _link.text = link;
      _start(CoreHost.I.pair(link));
    }
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  void _start(int op) => setState(() {
        _op = op;
        _step = 'dialing';
        _words = const [];
        _why = '';
      });

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final busy = _op != null && _step != 'failed' && _step != 'done';
    final digits = _id.text.replaceAll(RegExp(r'\D'), '').length;
    final codeReady = formatCode(_code.text).length == 9;
    Widget field(String label, TextEditingController c, {String hint = '', TextInputType? kb, ValueChanged<String>? onChanged, bool monoCaps = false}) =>
        Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Text(label, style: body(context, size: .75, color: t.dim)),
          const SizedBox(height: 4),
          TextField(
            controller: c,
            keyboardType: kb,
            enabled: !busy,
            onChanged: (v) {
              onChanged?.call(v);
              setState(() {});
            },
            textCapitalization: monoCaps ? TextCapitalization.characters : TextCapitalization.none,
            autocorrect: false,
            style: monoCaps ? mono(context, size: 1, w: FontWeight.w600, color: t.bright).copyWith(letterSpacing: 1.9) : body(context, size: 1, color: t.bright),
            decoration: InputDecoration(
              hintText: hint,
              hintStyle: body(context, size: 1, color: t.dim),
              filled: true,
              fillColor: t.s0,
              isDense: true,
              contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 14),
              border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1), borderSide: BorderSide(color: t.line)),
              enabledBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1), borderSide: BorderSide(color: t.line)),
              focusedBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1), borderSide: BorderSide(color: t.accent)),
            ),
          ),
        ]);

    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (_words.isNotEmpty && busy) ...[
        _form(context, t, 'Check the computer shows the same 4 words', [
          GridView.count(
            crossAxisCount: MediaQuery.of(context).size.width >= 440 ? 4 : 2,
            shrinkWrap: true,
            physics: const NeverScrollableScrollPhysics(),
            mainAxisSpacing: 8,
            crossAxisSpacing: 8,
            childAspectRatio: 2.6,
            children: [
              // The check words appear one after another — calm, not a reveal (`sasin`).
              for (final (i, w) in _words.indexed)
                Enter(delay: Duration(milliseconds: i * 90), ms: 480, dy: 6, fadeOnClose: false, child: Container(
                  alignment: Alignment.center,
                  decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r1), border: Border.all(color: t.line)),
                  child: Text(w, style: display(context, rem(context, 1.1875))),
                )),
            ],
          ),
          _wait(context, t, 'Waiting for you to confirm on the computer. If the words differ, say no there.'),
        ]),
        const SizedBox(height: 12),
      ] else if (busy) ...[
        _form(context, t, 'Pairing…', [
          _wait(context, t, switch (_step) {
            'dialing' => 'Reaching the computer through its relay…',
            'securing' => 'Setting up the secure link…',
            'waiting' => 'Proven — waiting for the computer to let this phone in.',
            _ => 'Working…',
          }),
        ]),
        const SizedBox(height: 12),
      ],
      if (_step == 'failed') ...[
        WCard(alert: true, children: [Text(_why.isEmpty ? "That didn't work." : _why, style: body(context, color: t.bright))]),
        const SizedBox(height: 12),
      ],
      WCard(children: [
        const Why('On the computer, run `wado pair` (or wado tui → Pair). It shows a QR code, a pairing link and a code.'),
        if (Platform.isAndroid || Platform.isIOS)
          Btn('Scan the QR code', primary: true, icon: 'camera', onTap: busy
              ? null
              : () async {
                  if (Platform.isAndroid && await const MethodChannel('wado/os').invokeMethod<bool>('camera') != true) {
                    setState(() {
                      _step = 'failed';
                      _why = 'wado next needs the camera to read the QR. Allow it in the app\'s settings, or paste the link.';
                    });
                    return;
                  }
                  if (!context.mounted) return;
                  final link = await Navigator.of(context).push<String>(MaterialPageRoute(builder: (_) => const _Scan()));
                  if (link != null && link.isNotEmpty) {
                    _link.text = link;
                    _start(CoreHost.I.pair(link));
                  }
                }),
        field('Pairing link', _link, hint: 'Paste the link the computer shows', kb: TextInputType.url),
        Row(children: [
          Expanded(
            child: Btn('Paste', icon: 'paste', onTap: busy
                ? null
                : () async {
                    final d = await Clipboard.getData('text/plain');
                    if (d?.text != null) setState(() => _link.text = d!.text!.trim());
                  }),
          ),
          const SizedBox(width: 8),
          Expanded(child: Btn('Pair', primary: true, onTap: busy || _link.text.trim().isEmpty ? null : () => _start(CoreHost.I.pair(_link.text.trim())))),
        ]),
      ]),
      const SizedBox(height: 12),
      const Sect('Or type the code'),
      const SizedBox(height: 8),
      WCard(children: [
        field('Remote ID', _id, hint: '528-491-307 — shown by wado on the computer', kb: TextInputType.number),
        field('Code from the computer', _code, hint: 'XXXX-XXXX', monoCaps: true, onChanged: (v) {
          final f = formatCode(v);
          if (f != v) _code.value = TextEditingValue(text: f, selection: TextSelection.collapsed(offset: f.length));
        }),
        field('Relay', _relay, kb: TextInputType.url),
        Btn('Pair', primary: true, icon: 'link', onTap: busy || digits < 6 || !codeReady
            ? null
            : () => _start(CoreHost.I.pairTyped(_relay.text.trim(), _id.text.trim(), formatCode(_code.text)))),
      ]),
    ]);
  }

  Widget _form(BuildContext context, Tokens t, String title, List<Widget> kids) => Container(
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(
          color: t.s1,
          borderRadius: BorderRadius.circular(t.r3),
          border: Border(left: BorderSide(color: t.base[0xA], width: 3), top: BorderSide(color: t.line), right: BorderSide(color: t.line), bottom: BorderSide(color: t.line)),
        ),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            Disc(hue: t.base[0xA], child: WIcon('link', color: t.base[0xA])),
            const SizedBox(width: 10),
            Expanded(child: Text(title, style: display(context, rem(context, 1)))),
          ]),
          const SizedBox(height: 10),
          ...kids,
        ]),
      );

  Widget _wait(BuildContext context, Tokens t, String text) => Padding(
        padding: const EdgeInsets.only(top: 10),
        child: Row(children: [
          Breathe(ms: 3200, child: Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: t.base[0xA]))),
          const SizedBox(width: 10),
          Expanded(child: Text(text, style: body(context, size: .8125, color: t.dim))),
        ]),
      );
}

/// The camera, reading the computer's pairing QR (zxing-cpp; nothing leaves the phone). The QR
/// may be a terminal's half-blocks, light on dark or dark on light: try hard, both ways round.
class _Scan extends StatefulWidget {
  const _Scan();
  @override
  State<_Scan> createState() => _ScanState();
}

class _ScanState extends State<_Scan> {
  bool _done = false;
  @override
  Widget build(BuildContext context) => Scaffold(
        backgroundColor: Colors.black,
        appBar: AppBar(backgroundColor: Colors.black, foregroundColor: Colors.white, title: const Text('Scan the QR code')),
        body: ReaderWidget(
          codeFormat: Format.qrCode,
          showGallery: false,
          tryHarder: true,
          tryInverted: true,
          cropPercent: .8,
          scanDelay: const Duration(milliseconds: 250),
          onScan: (code) {
            final t = code.text;
            if (_done || !code.isValid || t == null || !(t.contains('pair=') || t.startsWith('wado:'))) return;
            _done = true;
            Navigator.of(context).pop(t);
          },
        ),
      );
}
