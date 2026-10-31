// Files (port of js/files_ui.js + files_panels.js): the computer's files for this phone — an
// overview to start from (overview.dart), folders to browse, searches and categories, a file to
// view; long-press to select several; upload from the phone, download to it, new folder, rename,
// move to the computer's trash.
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

import '../../wado/files.dart';
import '../../wado/lane.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/lane_card.dart';
import 'overview.dart';
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
  /// home (the overview) · dir (a folder) · results (a search, a category, or all recent files)
  String _view = 'home';
  String _title = '';
  List<Map<String, dynamic>> _found = [];
  /// Long-pressed into a selection: paths.
  final _sel = <String>{};

  @override
  void initState() {
    super.initState();
    _start();
  }

  Future<void> _start() async {
    try {
      final info = await Files.I.connect();
      _quick = await Files.I.req('quick').catchError((_) => <String, dynamic>{});
      if (!mounted) return;
      setState(() {
        _view = 'home';
        _busy = false;
      });
    } catch (e) {
      setState(() {
        _err = '$e'.replaceFirst('Exception: ', '');
        _busy = false;
      });
    }
  }

  Future<void> _go(String path) async {
    setState(() {
      _view = 'dir';
      _sel.clear();
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

  /// A search or a category (`find`), searched from the top of what this phone may see.
  Future<void> _find(String title, {String kind = '', String query = ''}) async {
    setState(() {
      _view = 'results';
      _title = title;
      _found = [];
      _sel.clear();
      _busy = true;
      _err = '';
    });
    try {
      final r = await Files.I.req('find', {'path': '', 'query': query, 'kind': kind});
      if (!mounted || _title != title) return;
      setState(() {
        _found = ((r['entries'] as List?) ?? const []).cast<Map<String, dynamic>>();
        _busy = false;
      });
    } catch (e) {
      if (mounted) {
        setState(() {
          _err = '$e'.replaceFirst('Exception: ', '');
          _busy = false;
        });
      }
    }
  }

  void _recentAll() => setState(() {
        _view = 'results';
        _title = 'Recent';
        _sel.clear();
        _found = ((_quick?['recent'] as List?) ?? const []).cast<Map<String, dynamic>>().toList()..sort((a, b) => ((b['mtime'] ?? 0) as num).compareTo((a['mtime'] ?? 0) as num));
      });

  void _home() => setState(() {
        _view = 'home';
        _sel.clear();
        _err = '';
      });

  String _pathOf(Map<String, dynamic> e) => (e['path'] as String?) ?? _join('${e['name']}');

  void _toggle(Map<String, dynamic> e) => setState(() {
        final p = _pathOf(e);
        _sel.contains(p) ? _sel.remove(p) : _sel.add(p);
      });

  void _refresh() => _view == 'dir' && _path != null ? _go(_path!) : _start();

  String _join(String name) => '${_path!.replaceAll(RegExp(r'/$'), '')}/$name';

  Future<void> _upload() async {
    final pick = await FilePicker.pickFiles();
    if (pick.isEmpty || _path == null) return;
    for (final f in pick) {
      setState(() => _jobs['↑ ${f.name}'] = 0);
      Lane.I.transfer(1, lane: Files.I.lane);
      try {
        await uploadStream(_path!, f.name, await f.xFile.length(), () => f.xFile.openRead(), progress: (p) {
          Lane.I.moved();
          setState(() => _jobs['↑ ${f.name}'] = p);
        });
      } catch (e) {
        if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Couldn't send ${f.name}: $e")));
      }
      Lane.I.transfer(-1, lane: Files.I.lane);
      setState(() => _jobs.remove('↑ ${f.name}'));
    }
    _go(_path!);
  }

  Future<void> _download(Map<String, dynamic> e) async {
    final name = '${e['name']}', path = _pathOf(e);
    setState(() => _jobs['↓ $name'] = 0);
    Lane.I.transfer(1, lane: Files.I.lane);
    try {
      final bytes = await Files.I.get(path, progress: (p) {
          Lane.I.moved();
          setState(() => _jobs['↓ $name'] = p);
        });
      await keepOnPhone(name, bytes);
    } catch (err) {
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text("Couldn't download $name: $err")));
    }
    Lane.I.transfer(-1, lane: Files.I.lane);
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
            await Files.I.req('rename', {'path': _pathOf(e), 'to': to}).catchError((_) => <String, dynamic>{});
            _refresh();
          }),
          ListTile(leading: WIcon('trash', color: t.bad), title: Text('Move to the computer’s trash', style: TextStyle(color: t.bad)), onTap: () async {
            Navigator.pop(s);
            await Files.I.req('trash', {'paths': [_pathOf(e)]}).catchError((_) => <String, dynamic>{});
            _refresh();
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
    final roots = ((info?['roots'] as List?) ?? const []).map((e) => '$e').toSet();
    final up = _path != null && !roots.contains(_path) && _path != '/' ? _path!.substring(0, _path!.lastIndexOf('/')).ifEmpty('/') : null;
    final dir = _view == 'dir';
    final list = dir ? _entries : _found;
    final picked = [for (final e in list) if (_sel.contains(_pathOf(e))) e];
    return PopScope(
      // Back: out of a selection, then up to the overview.
      canPop: _view == 'home' && _sel.isEmpty,
      onPopInvokedWithResult: (did, _) {
        if (did) return;
        if (_sel.isNotEmpty) return setState(_sel.clear);
        if (dir && up != null && _path != home) {
          _go(up);
          return;
        }
        _home();
      },
      child: Column(children: [
        Container(
          padding: const EdgeInsets.fromLTRB(8, 6, 8, 6),
          decoration: BoxDecoration(color: t.s0, border: Border(bottom: BorderSide(color: t.line))),
          // The selection's bar takes over the title bar: it rises 6 px into place while the
          // title sinks away, so the swap reads as one surface changing role.
          child: AnimatedSwitcher(
            duration: t.dur,
            switchInCurve: Tokens.spring,
            switchOutCurve: Curves.easeIn,
            transitionBuilder: (child, a) => FadeTransition(
              opacity: a,
              child: SlideTransition(position: Tween(begin: Offset(0, child.key == const ValueKey('sel') ? .25 : -.25), end: Offset.zero).animate(a), child: child),
            ),
            child: _sel.isNotEmpty
              // The selection's bar: how many, and what can be done to all of them.
              ? Row(key: const ValueKey('sel'), children: [
                  IconBtn('x', label: 'Clear the selection', color: t.bright, onTap: () => setState(_sel.clear)),
                  Expanded(child: Text('${_sel.length} selected', style: body(context, w: FontWeight.w600, color: t.bright))),
                  IconBtn('down', label: 'Download to this phone', color: t.bright, onTap: () async {
                    final files = [for (final e in picked) if (e['dir'] != true) e];
                    setState(_sel.clear);
                    for (final e in files) {
                      await _download(e);
                    }
                  }),
                  IconBtn('trash', label: 'Move to the computer’s trash', color: t.bad, onTap: () async {
                    final paths = [..._sel];
                    setState(_sel.clear);
                    await Files.I.req('trash', {'paths': paths}).catchError((_) => <String, dynamic>{});
                    _refresh();
                  }),
                ])
              : Row(key: const ValueKey('title'), children: [
                  if (_view == 'home')
                    Padding(padding: const EdgeInsets.fromLTRB(8, 0, 4, 0), child: WIcon('folder', color: t.base[0xA]))
                  else
                    IconBtn('back', label: dir && up != null && _path != home ? 'Up a folder' : 'Back to the overview', color: t.bright, onTap: () => dir && up != null && _path != home ? _go(up) : _home()),
                  Expanded(
                    child: Text(
                      switch (_view) { 'home' => 'Files', 'dir' => _path ?? '…', _ => _title },
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: dir ? mono(context, color: t.bright) : body(context, w: FontWeight.w600, color: t.bright),
                    ),
                  ),
                  if (dir) ...[
                    IconBtn('plus', label: 'New folder', color: t.bright, onTap: _path == null ? null : () async {
                      final n = await _ask('New folder', '');
                      if (n == null || n.isEmpty) return;
                      await Files.I.req('mkdir', {'path': _join(n)}).catchError((_) => <String, dynamic>{});
                      _go(_path!);
                    }),
                    IconBtn('share', label: 'Send from this phone', color: t.accent, onTap: _path == null ? null : _upload),
                  ],
                ]),
          ),
        ),
        LaneCard(
          spot: 'files',
          margin: const EdgeInsets.fromLTRB(16, 8, 16, 0),
          onRetry: () {
            Files.I.close();
            _start();
          },
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
              ? Padding(padding: const EdgeInsets.all(16), child: WCard(alert: true, children: [Text(_err, style: body(context, color: t.bright)), Btn('Try again', onTap: _refresh)]))
              : _view == 'home'
                  ? (info == null
                      ? Center(child: Column(mainAxisSize: MainAxisSize.min, children: [const CircularProgressIndicator(strokeWidth: 2), const SizedBox(height: 12), Text('Opening a secure channel to the computer…', style: body(context, color: t.dim))]))
                      : FilesOverview(quick: _quick ?? const {}, onGo: _go, onOpen: (e) => openViewer(context, _pathOf(e), e), onFind: _find, onRecent: _recentAll))
                  : _busy && list.isEmpty
                      ? Center(child: Column(mainAxisSize: MainAxisSize.min, children: [const CircularProgressIndicator(strokeWidth: 2), if (!dir) ...[const SizedBox(height: 12), Text('Searching the computer…', style: body(context, color: t.dim))]]))
                      : RefreshIndicator(
                          onRefresh: () async => _refresh(),
                          child: ListView(padding: EdgeInsets.only(bottom: 24 + MediaQuery.of(context).padding.bottom), children: [
                            for (final e in list) _tile(context, e, where: !dir),
                            if (list.isEmpty && !_busy)
                              Padding(padding: const EdgeInsets.all(24), child: Center(child: Text(dir ? 'Empty folder' : 'Nothing found — try another word.', style: body(context, color: t.dim)))),
                          ]),
                        ),
        ),
      ]),
    );
  }

  /// A row: tap opens (or, while selecting, picks), long-press starts a selection, ⋯ for actions.
  Widget _tile(BuildContext context, Map<String, dynamic> e, {bool where = false}) {
    final t = Tok.of(context);
    final path = _pathOf(e);
    final on = _sel.contains(path);
    final isDir = e['dir'] == true;
    final hue = isDir ? t.base[0xA] : t.base[0xD];
    final parent = where && path.contains('/') ? path.substring(0, path.lastIndexOf('/')) : '';
    return Semantics(
      selected: on,
      child: Press(
      onTap: () => _sel.isNotEmpty ? _toggle(e) : (isDir ? _go(path) : openViewer(context, path, e)),
      onLongPress: () {
        HapticFeedback.selectionClick();
        _toggle(e);
      },
      label: '${e['name']}${on ? ', selected' : ''}',
      child: AnimatedContainer(
        duration: t.durPress,
        curve: Tokens.spring,
        color: on ? fade(t.accent, 88) : fade(t.accent, 100),
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
        child: Row(children: [
          // Picked: the kind's icon gives way to a check that pops in (and back without the pop).
          AnimatedSwitcher(
            duration: t.dur,
            switchInCurve: t.pop,
            switchOutCurve: Curves.easeIn,
            transitionBuilder: (child, a) => ScaleTransition(scale: Tween(begin: .6, end: 1.0).animate(a), child: FadeTransition(opacity: a, child: child)),
            child: Disc(key: ValueKey(on), hue: on ? t.accent : hue, child: WIcon(on ? 'check' : iconFor(e), color: on ? t.accent : hue)),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('${e['name']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright)),
              Text([if (!isDir) fmtSize((e['size'] ?? 0) as num), fmtWhen((e['mtime'] ?? 0) as num), parent].where((x) => x.isNotEmpty).join(' · '),
                  maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, color: t.dim)),
            ]),
          ),
          if (_sel.isEmpty) IconBtn('dots', label: 'More', onTap: () => _menu(e)),
        ]),
      ),
      ),
    );
  }
}

extension on String {
  String ifEmpty(String v) => isEmpty ? v : this;
}
