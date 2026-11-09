// "Send to computer" (port of the old app's ui/share/SharePicker.kt + js/native_share.js): what is
// being sent, the computer (chosen already when there is one, or when it was picked in the share
// sheet's top row), then what to do there —
//   files: where they land (Downloads, Desktop, Documents, Pictures, pinned folders, any folder);
//   a link: open it in the computer's browser, open it with one of its apps that can, or put it on
//     its clipboard;
//   text: onto its clipboard, or saved as a .txt file.
// The share sheet's "Open on computer" and "Copy to computer" entries skip the question.
import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/files.dart';
import '../../wado/host.dart';
import '../../wado/host_caps.dart';
import '../../wado/nav.dart';
import '../../wado/online.dart';
import '../../wado/relay_link.dart';
import '../motion/feel.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/data_image.dart';
import 'folder_pick.dart';
import 'inbound.dart';

class SharePayload {
  SharePayload(this.files, this.links, this.text, {this.via = '', this.computer = ''});
  final List<Map<String, dynamic>> files;
  final List<String> links;
  final String text;
  /// Which share-sheet entry: ShareSend (ask), ShareOpen (open the link), ShareCopy (clipboard).
  final String via;
  /// A computer picked in the share sheet's top row: its key.
  final String computer;

  static final _link = RegExp(r"""https?://[^\s<>"']+""", caseSensitive: false);

  factory SharePayload.of(Map<String, dynamic> m) {
    final text = ((m['text'] ?? '') as String).trim();
    final links = _link.allMatches(text).map((x) => x[0]!.replaceAll(RegExp(r'[.,);!?]+$'), '')).toSet().toList();
    final files = [for (final f in (m['files'] as List? ?? const [])) Map<String, dynamic>.from(f as Map)];
    return SharePayload(files, links, text, via: '${m['via'] ?? ''}', computer: '${m['computer'] ?? ''}');
  }

  bool get images => files.isNotEmpty && files.every((f) => '${f['mime']}'.startsWith('image/'));

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

/// pick (a computer) · connecting · choose · sending · done · failed
class _SendSheetState extends State<SendSheet> {
  Computer? _to;
  String _phase = 'pick';
  String _why = '';
  String _done = '';
  /// Where files (or a text file) go; the home folder, once known.
  String? _dir;
  String _home = '';
  List<(String, String)> _places = [];
  double _progress = 0;
  /// The apps that open the link, the default's exec, and their icons; whether to show them.
  List<Map<String, dynamic>> _urlApps = [];
  String? _default;
  final _icons = <String, String>{};
  bool _openWith = false;
  late final List<void Function(Map<String, dynamic>)> _handlers = [_onUrlApps, _onIcons, _onOpened, _onError];

  @override
  void initState() {
    super.initState();
    final list = CoreHost.I.computers.value;
    Online.I.watch([for (final c in list) (c.relayUrl, c.remoteId)]);
    final picked = list.where((c) => c.key == widget.p.computer).firstOrNull ?? (list.length == 1 ? list.first : null);
    final l = RelayLink.I;
    l.on('url_apps_list', _handlers[0]);
    l.on('apps_icons_list', _handlers[1]);
    l.on('url_opened', _handlers[2]);
    l.on('host_error', _handlers[3]);
    if (picked != null) WidgetsBinding.instance.addPostFrameCallback((_) => _connect(picked));
  }

  @override
  void dispose() {
    final l = RelayLink.I;
    l.off('url_apps_list', _handlers[0]);
    l.off('apps_icons_list', _handlers[1]);
    l.off('url_opened', _handlers[2]);
    l.off('host_error', _handlers[3]);
    super.dispose();
  }

  SharePayload get p => widget.p;
  bool get _mac => HostCaps.now.has('desk_apps');

  void _onUrlApps(Map<String, dynamic> m) {
    if (p.links.isEmpty || m['url'] != p.links.first || !mounted) return;
    setState(() {
      _urlApps = (m['apps'] as List? ?? const []).cast<Map<String, dynamic>>();
      _default = m['default'] as String?;
    });
    RelayLink.I.send({'type': 'apps_icons', 'execs': [for (final a in _urlApps) a['exec']]});
  }

  void _onIcons(Map<String, dynamic> m) {
    if (!mounted) return;
    setState(() => (m['icons'] as Map? ?? const {}).forEach((k, v) => _icons['$k'] = '$v'));
  }

  void _onOpened(Map<String, dynamic> m) {
    if (_phase == 'sending' && mounted) _finish('Opened on ${_to!.name}.');
  }

  void _onError(Map<String, dynamic> m) {
    if (_phase == 'sending' && mounted) _fail('${m['message'] ?? 'that did not work'}');
  }

  void _finish(String words) {
    feel('confirm');
    setState(() {
      _phase = 'done';
      _done = words;
    });
  }

  void _fail(String why) {
    feel('reject');
    setState(() {
      _phase = 'failed';
      _why = why;
    });
  }

  /// Reach `c`, learn what it is, then do at once what the entry asked — or ask.
  Future<void> _connect(Computer c) async {
    setState(() {
      _to = c;
      _phase = 'connecting';
    });
    try {
      Nav.I.openComputer(c.key);
      final t0 = DateTime.now();
      while (!RelayLink.I.up.value || Host.I.config.value == null) {
        if (DateTime.now().difference(t0).inSeconds > 25) throw 'the computer did not answer';
        // An older daemon may never send its config: the link alone will do after a while.
        if (RelayLink.I.up.value && DateTime.now().difference(t0).inSeconds > 4) break;
        await Future.delayed(const Duration(milliseconds: 200));
      }
      if (p.files.isNotEmpty || p.links.isEmpty) {
        final info = await Files.I.connect();
        _home = '${info['home'] ?? ''}'.replaceAll(RegExp(r'/$'), '');
        final q = await Files.I.req('quick').catchError((_) => <String, dynamic>{});
        _places = [
          ('Downloads', '$_home/Downloads'),
          ('Desktop', '$_home/Desktop'),
          ('Documents', '$_home/Documents'),
          if (p.images) ('Pictures', '$_home/Pictures'),
          for (final x in (q['pins'] as List? ?? const []))
            if (x is Map && (x['kind'] ?? 'pin') == 'pin') ('${x['name'] ?? '${x['path']}'.split('/').last}', '${x['path']}'),
        ];
        _dir = _places.first.$2;
      }
      if (p.links.isNotEmpty && _mac) RelayLink.I.send({'type': 'url_apps', 'url': p.links.first});
      if (!mounted) return;
      switch (p.via) {
        case 'ShareOpen' when p.links.isNotEmpty:
          return _open(null);
        case 'ShareCopy':
          return _copy();
      }
      setState(() => _phase = 'choose');
    } catch (e) {
      _fail('$e');
    }
  }

  Future<void> _saveFiles() async {
    setState(() {
      _phase = 'sending';
      _progress = 0;
    });
    try {
      for (var i = 0; i < p.files.length; i++) {
        await _sendOne(p.files[i], (x) {
          if (mounted) setState(() => _progress = (i + x) / p.files.length);
        });
      }
      _finish('In ${_dirName(_dir!)} on ${_to!.name}.');
    } catch (e) {
      _fail('$e'.replaceFirst('Exception: ', ''));
    }
  }

  /// One shared file, read from the phone a megabyte at a time as it goes up — a long video never
  /// sits in the app's memory whole.
  Future<void> _sendOne(Map<String, dynamic> f, void Function(double) progress) async {
    final name = '${f['name']}';
    final o = await os.invokeMethod<Map>('readOpen', {'uri': f['uri']});
    if (o == null) throw 'could not read $name';
    final id = o['id'] as int;
    var size = (o['size'] as num? ?? -1).toInt();
    if (size < 0) size = (f['size'] as num? ?? -1).toInt();
    try {
      if (size < 0) {
        // Neither the sharing app nor the file says how big it is: read whole (rare, and small).
        final bytes = await os.invokeMethod<Uint8List>('read', {'uri': f['uri']});
        if (bytes == null) throw 'could not read $name';
        await upload(_dir!, name, bytes, progress: progress);
        return;
      }
      Stream<List<int>> pieces() async* {
        while (true) {
          final b = await os.invokeMethod<Uint8List>('readNext', {'id': id, 'max': 1 << 20});
          if (b == null || b.isEmpty) break;
          yield b;
        }
      }

      await uploadStream(_dir!, name, size, pieces, progress: progress);
    } finally {
      await os.invokeMethod('readClose', {'id': id});
    }
  }

  Future<void> _saveText() async {
    setState(() => _phase = 'sending');
    try {
      final first = p.text.split('\n').first.replaceAll(RegExp(r'[^\w\- ]'), '').trim();
      final name = '${first.isEmpty ? 'Note' : first.substring(0, first.length.clamp(0, 40))}.txt';
      await upload(_dir!, name, Uint8List.fromList(utf8.encode(p.text)));
      _finish('Saved as $name in ${_dirName(_dir!)} on ${_to!.name}.');
    } catch (e) {
      _fail('$e'.replaceFirst('Exception: ', ''));
    }
  }

  /// Open the link(s): with `exec`, or the computer's default app. A Mac opens them on its
  /// desktop; another computer in its session.
  void _open(String? exec) {
    setState(() => _phase = 'sending');
    for (final url in p.links) {
      final sent = _mac
          ? RelayLink.I.send({'type': 'open_url', 'url': url, 'exec': ?exec})
          : RelayLink.I.send({'type': 'session_launch', 'command': "xdg-open '${url.replaceAll("'", r"'\''")}'"});
      if (!sent) return _fail('not connected');
    }
    // A session's launch has no answer of its own.
    if (!_mac) _finish('Opened on ${_to!.name}.');
  }

  void _copy() {
    final what = p.links.isNotEmpty && p.text.trim() == p.links.join(' ') ? p.links.join('\n') : p.text;
    if (!RelayLink.I.send({'type': 'clip_push', 'mime': 'text/plain', 'data': what, 'sensitive': false})) return _fail('not connected');
    _finish("On ${_to!.name}'s clipboard — paste it there.");
  }

  String _dirName(String d) => d == _home ? 'Home' : _places.where((x) => x.$2 == d).map((x) => x.$1).firstOrNull ?? d.split('/').last;

  String _nameOf(String? exec) => _urlApps.where((a) => a['exec'] == exec).map((a) => '${a['name']}').firstOrNull ?? 'its browser';

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final list = CoreHost.I.computers.value;
    return SafeArea(
      top: false,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
        child: AnimatedSize(
          duration: t.dur,
          curve: Tokens.spring,
          alignment: Alignment.topCenter,
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Text(_to == null ? 'Send ${p.describe()} to…' : 'Send ${p.describe()} to ${_to!.name}', style: display(context, rem(context, 1.25))),
            const SizedBox(height: 12),
            ...switch (_phase) {
              'pick' => [
                  if (list.isEmpty) Text('Pair a computer first — then share to it from any app.', style: body(context, color: t.dim)),
                  ValueListenableBuilder(
                    valueListenable: Online.I.map,
                    builder: (context, seen, _) => ListBox(children: [
                      for (final c in list)
                        NavRow(icon: 'monitor', hue: '0D', title: c.name, sub: switch (seen['${c.relayUrl}|${c.remoteId}']) { null => 'checking…', -1 => "can't reach its relay", 0 => 'offline', _ => 'online' }, onOpen: () => _connect(c)),
                    ]),
                  ),
                ],
              'connecting' => [_busyRow(context, 'Reaching ${_to!.name}…')],
              'choose' => _choices(context),
              'sending' => [
                  _busyRow(context, p.files.isNotEmpty ? 'Sending to ${_dirName(_dir ?? '')}…' : 'On its way…'),
                  if (p.files.isNotEmpty) ...[const SizedBox(height: 10), LinearProgressIndicator(value: _progress, color: t.accent, backgroundColor: t.s2)],
                ],
              _ => [
                  Row(children: [
                    WIcon(_phase == 'done' ? 'check' : 'alert', color: _phase == 'done' ? t.good : t.bad),
                    const SizedBox(width: 10),
                    Expanded(child: Text(_phase == 'done' ? _done : "It didn't reach ${_to!.name}: $_why", style: body(context, color: t.bright))),
                  ]),
                ],
            },
            const SizedBox(height: 14),
            if (_phase != 'sending' && _phase != 'connecting') Btn(_phase == 'done' ? 'Done' : 'Cancel', onTap: () => Navigator.of(context).pop()),
          ]),
        ),
      ),
    );
  }

  Widget _busyRow(BuildContext context, String s) {
    final t = Tok.of(context);
    return Row(children: [
      SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2, color: t.accent)),
      const SizedBox(width: 10),
      Expanded(child: Text(s, style: body(context, color: t.dim))),
    ]);
  }

  /// Where files go: the usual folders and the pinned ones as chips, and any other.
  Widget _where(BuildContext context) => Wrap(spacing: 8, runSpacing: 8, children: [
        for (final (name, path) in _places) ChoiceChip(label: Text(name), selected: _dir == path, onSelected: (_) => setState(() => _dir = path)),
        if (_dir != null && !_places.any((x) => x.$2 == _dir)) ChoiceChip(label: Text(_dirName(_dir!)), selected: true, onSelected: (_) {}),
        ActionChip(
          avatar: const Icon(Icons.folder_open, size: 18),
          label: const Text('Other folder…'),
          onPressed: () async {
            final d = await pickFolder(context, _dir ?? _home);
            if (d != null) setState(() => _dir = d);
          },
        ),
      ]);

  List<Widget> _choices(BuildContext context) {
    final t = Tok.of(context);
    Widget label(String s) => Padding(padding: const EdgeInsets.fromLTRB(4, 4, 4, 8), child: Text(s, style: body(context, size: .75, w: FontWeight.w600, color: t.dim)));
    if (p.files.isNotEmpty) {
      return [
        label('SAVE TO'),
        _where(context),
        const SizedBox(height: 14),
        Btn('Send to ${_dirName(_dir!)}', primary: true, icon: 'share', onTap: _saveFiles),
      ];
    }
    if (p.links.isNotEmpty) {
      final link = p.links.first;
      return [
        Text(p.links.length == 1 ? link : '$link and ${p.links.length - 1} more', maxLines: 2, overflow: TextOverflow.ellipsis, style: mono(context, color: t.dim)),
        const SizedBox(height: 12),
        Btn(_mac ? 'Open in ${_nameOf(_default)}' : 'Open on ${_to!.name}', primary: true, icon: 'link', onTap: () => _open(_default)),
        const SizedBox(height: 8),
        if (_mac && _urlApps.length > 1) ...[
          Btn(_openWith ? 'Hide the other apps' : 'Open with…', icon: 'apps', onTap: () => setState(() => _openWith = !_openWith)),
          const SizedBox(height: 8),
          if (_openWith)
            ListBox(children: [
              for (final a in _urlApps)
                ListTile(
                  leading: DataPicture(_icons['${a['exec']}'], size: 32, fallback: WIcon('apps', color: t.bright)),
                  title: Text('${a['name']}'),
                  subtitle: a['exec'] == _default ? const Text('Opens links by default') : (a['running'] == true ? const Text('Open now') : null),
                  onTap: () => _open('${a['exec']}'),
                ),
            ]),
          const SizedBox(height: 8),
        ],
        Btn("Copy to ${_to!.name}'s clipboard", icon: 'copy', onTap: _copy),
      ];
    }
    return [
      Text(p.text, maxLines: 4, overflow: TextOverflow.ellipsis, style: body(context, color: t.dim)),
      const SizedBox(height: 12),
      Btn("Copy to ${_to!.name}'s clipboard", primary: true, icon: 'copy', onTap: _copy),
      const SizedBox(height: 14),
      label('OR SAVE AS A TEXT FILE IN'),
      _where(context),
      const SizedBox(height: 10),
      Btn('Save as a .txt file', icon: 'text', onTap: _saveText),
    ];
  }
}
