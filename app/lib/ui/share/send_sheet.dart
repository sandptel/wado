// "Send to computer" (port of the old app's ui/share/SharePicker.kt + js/native_share.js): a
// sheet naming what is being sent, the computers with their live status, then plainly what
// happened — files land in the computer's Downloads, links open in its session, text goes to
// its clipboard.
import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/files.dart';
import '../../wado/nav.dart';
import '../../wado/online.dart';
import '../../wado/relay_link.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'inbound.dart';

class SharePayload {
  SharePayload(this.files, this.links, this.text);
  final List<Map<String, dynamic>> files;
  final List<String> links;
  final String text;

  static final _link = RegExp(r'''https?://[^\s<>"']+''', caseSensitive: false);

  factory SharePayload.of(Map<String, dynamic> m) {
    final text = ((m['text'] ?? '') as String).trim();
    final links = _link.allMatches(text).map((x) => x[0]!.replaceAll(RegExp(r'[.,);!?]+$'), '')).toSet().toList();
    final files = [for (final f in (m['files'] as List? ?? const [])) Map<String, dynamic>.from(f as Map)];
    return SharePayload(files, links, text);
  }

  /// "2 photos", "a link", "some text".
  String describe() {
    final parts = <String>[];
    if (files.isNotEmpty) {
      bool all(String p) => files.every((f) => '${f['mime']}'.startsWith(p));
      final noun = all('image/') ? 'photo' : (all('video/') ? 'video' : 'file');
      parts.add(files.length == 1 ? 'a $noun' : '${files.length} ${noun}s');
    }
    if (links.isNotEmpty) parts.add(links.length == 1 ? 'a link' : '${links.length} links');
    if (parts.isEmpty && text.isNotEmpty) parts.add('some text');
    return parts.join(' and ');
  }
}

class SendSheet extends StatefulWidget {
  const SendSheet(this.p, {super.key});
  final SharePayload p;

  static void show(SharePayload p) {
    final ctx = rootNav.currentContext;
    if (ctx == null || (p.files.isEmpty && p.text.isEmpty)) return;
    final t = Tok.of(ctx);
    showModalBottomSheet(
      context: ctx,
      backgroundColor: t.s1,
      barrierColor: Colors.black54,
      isScrollControlled: true,
      showDragHandle: true,
      sheetAnimationStyle: AnimationStyle(duration: t.durSheet, curve: Tokens.spring, reverseCurve: Tokens.spring),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(t.r4))),
      builder: (_) => SendSheet(p),
    );
  }

  @override
  State<SendSheet> createState() => _SendSheetState();
}

class _SendSheetState extends State<SendSheet> {
  Computer? _to;
  bool _sending = false;
  String? _why;
  bool _done = false;

  @override
  void initState() {
    super.initState();
    Online.I.watch([for (final c in CoreHost.I.computers.value) (c.relayUrl, c.remoteId)]);
  }

  Future<void> _send(Computer c) async {
    setState(() {
      _to = c;
      _sending = true;
    });
    final p = widget.p;
    try {
      Nav.I.openComputer(c.key);
      final t0 = DateTime.now();
      while (!RelayLink.I.up.value) {
        if (DateTime.now().difference(t0).inSeconds > 25) throw 'the computer did not answer';
        await Future.delayed(const Duration(milliseconds: 200));
      }
      if (p.files.isNotEmpty) {
        final info = await Files.I.connect();
        final dir = '${'${info['home'] ?? '~'}'.replaceAll(RegExp(r'/$'), '')}/Downloads';
        for (final f in p.files) {
          final bytes = await os.invokeMethod<Uint8List>('read', {'uri': f['uri']});
          if (bytes == null) throw 'could not read ${f['name']}';
          await upload(dir, '${f['name']}', bytes);
        }
      }
      for (final url in p.links) {
        final q = "'${url.replaceAll("'", r"'\''")}'";
        if (!RelayLink.I.send({'type': 'session_launch', 'command': 'xdg-open $q'})) throw 'not connected';
      }
      if (p.links.isEmpty && p.files.isEmpty && p.text.isNotEmpty) {
        if (!RelayLink.I.send({'type': 'clip_push', 'mime': 'text/plain', 'data': p.text, 'sensitive': false})) throw 'not connected';
      }
      setState(() => _done = true);
    } catch (e) {
      setState(() => _why = '$e');
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  String _doneWords(Computer c) => [
        if (widget.p.files.isNotEmpty) 'In Downloads on ${c.name}.',
        if (widget.p.links.isNotEmpty) 'Opened on ${c.name}.',
        if (widget.p.files.isEmpty && widget.p.links.isEmpty) "On ${c.name}'s clipboard.",
      ].join(' ');

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final list = CoreHost.I.computers.value;
    return SafeArea(
      top: false,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Text('Send ${widget.p.describe()} to…', style: display(context, rem(context, 1.25))),
          const SizedBox(height: 12),
          if (_to == null) ...[
            if (list.isEmpty) Text('Pair a computer first — then share to it from any app.', style: body(context, color: t.dim)),
            ValueListenableBuilder(
              valueListenable: Online.I.map,
              builder: (context, seen, _) => ListBox(children: [
                for (final c in list)
                  NavRow(icon: 'monitor', hue: '0D', title: c.name, sub: switch (seen['${c.relayUrl}|${c.remoteId}']) { null => 'checking…', -1 => "can't reach its relay", 0 => 'offline', _ => 'online' }, onOpen: () => _send(c)),
              ]),
            ),
          ] else if (_sending)
            Row(children: [
              SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2, color: t.accent)),
              const SizedBox(width: 10),
              Text('Sending to ${_to!.name}…', style: body(context, color: t.dim)),
            ])
          else
            Row(children: [
              WIcon(_done ? 'check' : 'alert', color: _done ? t.good : t.bad),
              const SizedBox(width: 10),
              Expanded(child: Text(_done ? _doneWords(_to!) : "It didn't reach ${_to!.name}: $_why", style: body(context, color: t.bright))),
            ]),
          const SizedBox(height: 14),
          if (!_sending) Btn(_done ? 'Done' : 'Cancel', onTap: () => Navigator.of(context).pop()),
        ]),
      ),
    );
  }
}
