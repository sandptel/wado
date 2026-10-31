// Files (port of js/files_ui.js + files_panels.js): the computer's files for this phone — its
// places and recent files at the top, folders to browse, a file to view; upload from the phone,
// download to it, new folder, rename, move to the computer's trash.
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

import '../../wado/files.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';
import 'viewer.dart';

String fmtSize(num b) {
  if (b < 1024) return '$b B';
  const u = ['KB', 'MB', 'GB', 'TB'];
  var v = b / 1024;
  var i = 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return '${v.toStringAsFixed(v < 10 ? 1 : 0)} ${u[i]}';
}

String fmtWhen(num secs) {
  if (secs <= 0) return '';
  final d = DateTime.fromMillisecondsSinceEpoch((secs * 1000).round());
  final now = DateTime.now();
  if (now.difference(d).inDays < 1 && now.day == d.day) return '${d.hour.toString().padLeft(2, '0')}:${d.minute.toString().padLeft(2, '0')}';
  return '${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';
}

String iconFor(Map<String, dynamic> e) {
  if (e['dir'] == true) return 'folder';
  final n = '${e['name']}'.toLowerCase();
  final ext = n.contains('.') ? n.split('.').last : '';
  if (const ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'avif', 'heic'].contains(ext)) return 'image';
  if (const ['mp3', 'flac', 'ogg', 'wav', 'm4a', 'opus'].contains(ext)) return 'music';
  if (const ['mp4', 'mkv', 'webm', 'mov', 'avi'].contains(ext)) return 'camera';
  return 'text';
}

/// Save bytes on the phone and offer them to its apps.
Future<void> keepOnPhone(String name, List<int> bytes) async {
  final dir = Platform.isAndroid ? (await getExternalStorageDirectory() ?? await getApplicationDocumentsDirectory()) : (await getDownloadsDirectory() ?? await getApplicationDocumentsDirectory());
  final f = File('${dir.path}/$name');
  await f.writeAsBytes(bytes);
  if (Platform.isAndroid || Platform.isIOS) {
    await SharePlus.instance.share(ShareParams(files: [XFile(f.path)]));
  }
}

class FilesPage extends StatefulWidget {
  const FilesPage({super.key});
  @override
  State<FilesPage> createState() => _FilesPageState();
}

class _FilesPageState extends State<FilesPage> {
  String? _path;
  List<Map<String, dynamic>> _entries = [];
  Map<String, dynamic>? _quick;
  String _err = '';
  bool _busy = true;
  final _jobs = <String, double>{};

  @override
  void initState() {
    super.initState();
    _start();
  }

  Future<void> _start() async {
    try {
      final info = await Files.I.connect();
      _quick = await Files.I.req('quick').catchError((_) => <String, dynamic>{});
      await _go((info['home'] ?? '/') as String);
    } catch (e) {
      setState(() {
        _err = '$e'.replaceFirst('Exception: ', '');
        _busy = false;
      });
    }
  }

  Future<void> _go(String path) async {
    setState(() {
      _busy = true;
      _err = '';
    });
    try {
      final r = await Files.I.req('list', {'path': path});
      final list = (r['entries'] as List).cast<Map<String, dynamic>>()
        ..sort((a, b) {
          if (a['dir'] != b['dir']) return a['dir'] == true ? -1 : 1;
          return '${a['name']}'.toLowerCase().compareTo('${b['name']}'.toLowerCase());
        });
      setState(() {
        _path = (r['path'] ?? path) as String;
        _entries = list.where((e) => !'${e['name']}'.startsWith('.')).toList();
        _busy = false;
      });
    } catch (e) {
      setState(() {
        _err = '$e'.replaceFirst('Exception: ', '');
        _busy = false;
      });
    }
  }

  String _join(String name) => '${_path!.replaceAll(RegExp(r'/$'), '')}/$name';

  Future<void> _upload() async {
    final pick = await FilePicker.pickFiles();
    if (pick.isEmpty || _path == null) return;
    for (final f in pick) {
      setState(() => _jobs['↑ ${f.name}'] = 0);
      try {
        await uploadStream(_path!, f.name, await f.xFile.length(), () => f.xFile.openRead(), progress: (p) => setState(() => _jobs['↑ ${f.name}'] = p));
      } catch (e) {
        if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Couldn't send ${f.name}: $e")));
      }
      setState(() => _jobs.remove('↑ ${f.name}'));
    }
    _go(_path!);
  }

  Future<void> _download(Map<String, dynamic> e) async {
    final name = '${e['name']}';
    setState(() => _jobs['↓ $name'] = 0);
    try {
      final bytes = await Files.I.get(_join(name), progress: (p) => setState(() => _jobs['↓ $name'] = p));
      await keepOnPhone(name, bytes);
    } catch (err) {
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Couldn't download $name: $err")));
    }
    setState(() => _jobs.remove('↓ $name'));
  }

  Future<String?> _ask(String title, String initial) async {
    final c = TextEditingController(text: initial);
    return showDialog<String>(
      context: context,
      builder: (d) => AlertDialog(
        title: Text(title),
        content: TextField(controller: c, autofocus: true),
        actions: [TextButton(onPressed: () => Navigator.pop(d), child: const Text('Cancel')), TextButton(onPressed: () => Navigator.pop(d, c.text.trim()), child: const Text('OK'))],
      ),
    );
  }

  void _menu(Map<String, dynamic> e) {
    final t = Tok.of(context);
    showModalBottomSheet(
      context: context,
      backgroundColor: t.s1,
      builder: (s) => SafeArea(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          ListTile(title: Text('${e['name']}', style: body(context, w: FontWeight.w600, color: t.bright))),
          if (e['dir'] != true) ListTile(leading: WIcon('down', color: t.bright), title: const Text('Download to this phone'), onTap: () {
            Navigator.pop(s);
            _download(e);
          }),
          ListTile(leading: WIcon('edit', color: t.bright), title: const Text('Rename'), onTap: () async {
            Navigator.pop(s);
            final to = await _ask('Rename', '${e['name']}');
            if (to == null || to.isEmpty) return;
            await Files.I.req('rename', {'path': _join('${e['name']}'), 'to': to}).catchError((_) => <String, dynamic>{});
            _go(_path!);
          }),
          ListTile(leading: WIcon('trash', color: t.bad), title: Text('Move to the computer’s trash', style: TextStyle(color: t.bad)), onTap: () async {
            Navigator.pop(s);
            await Files.I.req('trash', {'paths': [_join('${e['name']}')]}).catchError((_) => <String, dynamic>{});
            _go(_path!);
          }),
        ]),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final info = Files.I.info;
    final home = (info?['home'] ?? '') as String;
    final up = _path != null && _path != '/' && _path != home ? _path!.substring(0, _path!.lastIndexOf('/')).ifEmpty('/') : null;
    return Column(children: [
      // Path and actions.
      Container(
        padding: const EdgeInsets.fromLTRB(8, 6, 8, 6),
        decoration: BoxDecoration(color: t.s0, border: Border(bottom: BorderSide(color: t.line))),
        child: Row(children: [
          IconBtn('up', label: 'Up a folder', color: up == null ? t.dim : t.bright, onTap: up == null ? null : () => _go(up)),
          Expanded(child: Text(_path ?? '…', maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(context, color: t.bright))),
          IconBtn('plus', label: 'New folder', color: t.bright, onTap: _path == null ? null : () async {
            final n = await _ask('New folder', '');
            if (n == null || n.isEmpty) return;
            await Files.I.req('mkdir', {'path': _join(n)}).catchError((_) => <String, dynamic>{});
            _go(_path!);
          }),
          IconBtn('share', label: 'Send from this phone', color: t.accent, onTap: _path == null ? null : _upload),
        ]),
      ),
      ValueListenableBuilder(
        valueListenable: Files.I.laneNote,
        builder: (context, n, _) => n.isEmpty ? const SizedBox.shrink() : Padding(padding: const EdgeInsets.fromLTRB(16, 8, 16, 0), child: Why(n, tone: 'host')),
      ),
      for (final j in _jobs.entries)
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 6, 16, 0),
          child: Row(children: [
            Expanded(child: Text(j.key, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75))),
            SizedBox(width: 100, child: LinearProgressIndicator(value: j.value, color: t.accent, backgroundColor: t.s2)),
          ]),
        ),
      Expanded(
        child: _err.isNotEmpty
            ? Padding(padding: const EdgeInsets.all(16), child: WCard(alert: true, children: [Text(_err, style: body(context, color: t.bright)), Btn('Try again', onTap: _start)]))
            : _busy && _entries.isEmpty
                ? const Center(child: CircularProgressIndicator(strokeWidth: 2))
                : RefreshIndicator(
                    onRefresh: () => _go(_path ?? home),
                    child: ListView(padding: EdgeInsets.only(bottom: 24 + MediaQuery.of(context).padding.bottom), children: [
                      if (_path == home && _quick != null) ..._places(context),
                      for (final e in _entries)
                        Press(
                          onTap: () => e['dir'] == true ? _go(_join('${e['name']}')) : openViewer(context, _join('${e['name']}'), e),
                          onLongPress: () => _menu(e),
                          label: '${e['name']}',
                          child: Padding(
                            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                            child: Row(children: [
                              Disc(hue: e['dir'] == true ? t.base[0xA] : t.base[0xD], child: WIcon(iconFor(e), color: e['dir'] == true ? t.base[0xA] : t.base[0xD])),
                              const SizedBox(width: 12),
                              Expanded(
                                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                                  Text('${e['name']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright)),
                                  Text([if (e['dir'] != true) fmtSize((e['size'] ?? 0) as num), fmtWhen((e['mtime'] ?? 0) as num)].where((x) => x.isNotEmpty).join(' · '),
                                      style: body(context, size: .75, color: t.dim)),
                                ]),
                              ),
                              IconBtn('dots', label: 'More', onTap: () => _menu(e)),
                            ]),
                          ),
                        ),
                      if (_entries.isEmpty && !_busy) Padding(padding: const EdgeInsets.all(24), child: Center(child: Text('Empty folder', style: body(context, color: t.dim)))),
                    ]),
                  ),
      ),
    ]);
  }

  List<Widget> _places(BuildContext context) {
    final t = Tok.of(context);
    final pins = (_quick?['pins'] as List? ?? []).cast<Map<String, dynamic>>();
    final recent = (_quick?['recent'] as List? ?? []).cast<Map<String, dynamic>>();
    return [
      if (pins.isNotEmpty) ...[
        const Padding(padding: EdgeInsets.symmetric(horizontal: 12), child: Sect('Places')),
        SizedBox(
          height: 52,
          child: ListView(scrollDirection: Axis.horizontal, padding: const EdgeInsets.symmetric(horizontal: 12), children: [
            for (final p in pins)
              Padding(
                padding: const EdgeInsets.all(4),
                child: Press(
                  onTap: () => _go('${p['path']}'),
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(999)),
                    child: Row(children: [WIcon('folder', size: 16, color: t.base[0xA]), const SizedBox(width: 6), Text('${p['name']}', style: body(context, w: FontWeight.w600, color: t.bright))]),
                  ),
                ),
              ),
          ]),
        ),
      ],
      if (recent.isNotEmpty) ...[
        const Padding(padding: EdgeInsets.symmetric(horizontal: 12), child: Sect('Recent')),
        for (final r in recent.take(6))
          Press(
            onTap: () => openViewer(context, '${r['path']}', r),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              child: Row(children: [
                WIcon(iconFor(r), color: t.dim),
                const SizedBox(width: 12),
                Expanded(child: Text('${r['name']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, color: t.bright))),
                Text(fmtWhen((r['mtime'] ?? 0) as num), style: body(context, size: .75, color: t.dim)),
              ]),
            ),
          ),
        const Padding(padding: EdgeInsets.symmetric(horizontal: 12), child: Sect('Home')),
      ],
    ];
  }
}

extension on String {
  String ifEmpty(String v) => isEmpty ? v : this;
}
