// Files (port of js/files_ui.js + files_panels.js): the computer's files for this phone — an
// overview to start from (overview.dart), folders to browse by breadcrumbs, searches and
// categories, a file to view. A list or a grid with thumbnails, sorted as "Sort & view" says
// (files_view.dart). Long-press to select several; cut or copy, then paste in another folder
// (the bar waits while you browse); duplicate, rename, pin, properties, download (a folder as a
// .zip), the computer's Trash and restoring from it. A move, copy, duplicate or rename can be
// undone from its message (files_ops.dart).
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

import '../../wado/files.dart';
import '../../wado/files_ops.dart';
import '../../wado/files_view.dart';
import '../../wado/lane.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/lane_card.dart';
import 'files_bars.dart';
import 'files_items.dart';
import 'files_sheets.dart';
import 'overview.dart';
import 'viewer.dart';
import '../motion/feel.dart';

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

Future<Directory> _phoneDir() async =>
    Platform.isAndroid ? (await getExternalStorageDirectory() ?? await getApplicationDocumentsDirectory()) : (await getDownloadsDirectory() ?? await getApplicationDocumentsDirectory());

/// Save bytes on the phone and offer them to its apps.
Future<void> keepOnPhone(String name, List<int> bytes) async {
  final f = File('${(await _phoneDir()).path}/$name');
  await f.writeAsBytes(bytes);
  await _offer(f);
}

/// Bring `path` from the computer onto the phone as it arrives (any size), then offer it to the
/// phone's apps. The file it landed in.
Future<File> downloadToPhone(String path, String name, {bool zip = false, void Function(double)? progress, bool offer = true}) async {
  final f = await getToFile(path, File('${(await _phoneDir()).path}/$name'), zip: zip, progress: progress);
  if (offer) await _offer(f);
  return f;
}

Future<void> _offer(File f) async {
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
  /// The filter box is open over the folder; what it narrows to.
  bool _filtering = false;
  final _filter = TextEditingController();

  @override
  void initState() {
    super.initState();
    Settings.I.addListener(_changed);
    filesClip.addListener(_changed);
    _start();
  }

  @override
  void dispose() {
    Settings.I.removeListener(_changed);
    filesClip.removeListener(_changed);
    _filter.dispose();
    super.dispose();
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  Map<String, dynamic>? get _info => Files.I.info;
  String get _trash => '${_info?['trash'] ?? ''}';
  bool get _inTrash => _trash.isNotEmpty && _path != null && (_path == _trash || _path!.startsWith('$_trash/'));
  bool get _rw => '${_info?['access'] ?? 'rw'}' == 'rw';

  Future<void> _start() async {
    try {
      await Files.I.connect();
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
    final same = path == _path && _view == 'dir';
    setState(() {
      _view = 'dir';
      if (!same) {
        _sel.clear();
        _filtering = false;
        _filter.clear();
      }
      _busy = true;
      _err = '';
    });
    try {
      final r = await Files.I.req('list', {'path': path});
      setState(() {
        _path = (r['path'] ?? path) as String;
        _entries = (r['entries'] as List).cast<Map<String, dynamic>>();
        _busy = false;
      });
    } catch (e) {
      setState(() {
        _err = '$e'.replaceFirst('Exception: ', '');
        _busy = false;
      });
    }
  }

  /// A search or a category (`find`), searched from the top of what this phone may see — or,
  /// from a folder's filter box, from that folder down.
  Future<void> _find(String title, {String kind = '', String query = '', String under = ''}) async {
    setState(() {
      _view = 'results';
      _title = title;
      _found = [];
      _sel.clear();
      _busy = true;
      _err = '';
    });
    try {
      final r = await Files.I.req('find', {'path': under, 'query': query, 'kind': kind});
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

  void _home() {
    setState(() {
      _view = 'home';
      _sel.clear();
      _err = '';
    });
    Files.I.req('quick').then((q) => mounted ? setState(() => _quick = q) : null).catchError((_) => null);
  }

  String _pathOf(Map<String, dynamic> e) => (e['path'] as String?) ?? _join('${e['name']}');

  void _toggle(Map<String, dynamic> e) => setState(() {
        final p = _pathOf(e);
        _sel.contains(p) ? _sel.remove(p) : _sel.add(p);
      });

  void _refresh() => _view == 'dir' && _path != null ? _go(_path!) : (_view == 'home' ? _start() : null);

  String _join(String name) => '${_path!.replaceAll(RegExp(r'/$'), '')}/$name';

  void _say(String msg, {Undo? undo, bool bad = false}) {
    if (!mounted) return;
    feel(bad ? 'reject' : 'confirm');
    ScaffoldMessenger.of(context).hideCurrentSnackBar();
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
      content: Text(msg),
      duration: Duration(seconds: undo == null ? 3 : 6),
      action: undo == null
          ? null
          : SnackBarAction(
              label: 'Undo',
              onPressed: () async {
                try {
                  await undo.run();
                  _say('Undone');
                } catch (e) {
                  _say("Couldn't undo: ${'$e'.replaceFirst('Exception: ', '')}", bad: true);
                }
                _refresh();
              },
            ),
    ));
  }

  /// Run something that changes files: its message (with an Undo when it has one), a fresh view.
  Future<void> _do(Future<Undo?> Function() run, {String? done}) async {
    try {
      final u = await run();
      _say(u?.label ?? done ?? 'Done', undo: u);
    } catch (e) {
      _say('$e'.replaceFirst('Exception: ', ''), bad: true);
    }
    _refresh();
  }

  // ── what can be done ───────────────────────────────────────────────────────

  void _clip(String op, List<String> paths) {
    filesClip.value = FilesClip(op, paths);
    setState(_sel.clear);
    feel('tick');
    if (_view != 'dir') _go(_path ?? '${_info?['home'] ?? '/'}');
  }

  Future<void> _paste() async {
    final c = filesClip.value;
    if (c == null || _path == null) return;
    var clash = 'fail';
    while (true) {
      try {
        final u = await transfer(c.op, c.paths, _path!, clash: clash);
        filesClip.value = null;
        _say(u.label, undo: u);
        break;
      } catch (e) {
        if ('$e'.endsWith('exists') && clash == 'fail' && mounted) {
          final pick = await askClash(context);
          if (pick == null) return;
          clash = pick;
          continue;
        }
        _say('$e'.replaceFirst('Exception: ', ''), bad: true);
        break;
      }
    }
    _go(_path!);
  }

  Future<void> _rename(String path) async {
    final to = await askText(context, 'Rename', baseOf(path), ok: 'Rename');
    if (to == null || to.isEmpty || to == baseOf(path)) return;
    _do(() => rename(path, to));
  }

  Future<void> _newFolder() async {
    final n = await askText(context, 'New folder', '', ok: 'Create');
    if (n == null || n.isEmpty || _path == null) return;
    await _do(() async {
      await Files.I.req('mkdir', {'path': _join(n)});
      return null;
    }, done: 'Created $n');
  }

  void _trashOut(List<String> paths) => _do(() async {
        await Files.I.req('trash', {'paths': paths});
        setState(_sel.clear);
        return null;
      }, done: paths.length == 1 ? 'Moved ${baseOf(paths.first)} to the Trash — restore it from there' : 'Moved ${paths.length} items to the Trash');

  void _restore(List<String> paths) => _do(() async {
        for (final p in paths) {
          await Files.I.req('restore', {'path': p});
        }
        setState(_sel.clear);
        return null;
      }, done: paths.length == 1 ? 'Restored ${baseOf(paths.first)}' : 'Restored ${paths.length} items');

  bool _pinned(String p) => ((_quick?['pins'] as List?) ?? const []).any((x) => x is Map && x['path'] == p && (x['kind'] ?? 'pin') == 'pin');

  void _pin(String p, bool on) => _do(() async {
        await Files.I.req(on ? 'pin' : 'unpin', {'path': p});
        _quick = await Files.I.req('quick').catchError((_) => _quick ?? <String, dynamic>{});
        return null;
      }, done: on ? 'Pinned to the overview' : 'Unpinned');

  void _copyPath(String p) {
    Clipboard.setData(ClipboardData(text: p));
    _say('Path copied');
  }

  Future<void> _upload() async {
    final pick = await FilePicker.pickFiles();
    if (pick.isEmpty || _path == null) return;
    for (final f in pick) {
      final x = f.xFile;
      setState(() => _jobs['↑ ${f.name}'] = 0);
      Lane.I.transfer(1, lane: Files.I.lane);
      try {
        await uploadStream(_path!, f.name, await x.length(), () => x.openRead(), progress: (p) {
          Lane.I.moved();
          setState(() => _jobs['↑ ${f.name}'] = p);
        });
      } catch (e) {
        if (mounted) _say("Couldn't send ${f.name}: $e", bad: true);
      }
      Lane.I.transfer(-1, lane: Files.I.lane);
      setState(() => _jobs.remove('↑ ${f.name}'));
    }
    _go(_path!);
  }

  Future<void> _download(Map<String, dynamic> e) async {
    final dir = e['dir'] == true;
    final name = dir ? '${e['name']}.zip' : '${e['name']}', path = _pathOf(e);
    setState(() => _jobs['↓ $name'] = 0);
    Lane.I.transfer(1, lane: Files.I.lane);
    try {
      await downloadToPhone(path, name, zip: dir, progress: (p) {
        Lane.I.moved();
        setState(() => _jobs['↓ $name'] = p);
      });
    } catch (err) {
      if (mounted) _say("Couldn't download $name: $err", bad: true);
    }
    Lane.I.transfer(-1, lane: Files.I.lane);
    setState(() => _jobs.remove('↓ $name'));
  }

  // ── sheets ─────────────────────────────────────────────────────────────────

  void _itemSheet(Map<String, dynamic> e) {
    final p = _pathOf(e), dir = e['dir'] == true;
    final trashed = _trash.isNotEmpty && p.startsWith('$_trash/');
    showActions(context, '${e['name']}', [
      if (dir) FileAction('folder', 'Open', () => _go(p)) else FileAction('eye', 'Open here', () => openViewer(context, p, e)),
      if (_view == 'results') FileAction('folder', 'Show in folder', () => _go(parentOf(p))),
      FileAction('down', dir ? 'Download as .zip' : 'Download to this phone', () => _download(e)),
      if (_rw && !trashed) ...[
        FileAction('move', 'Cut', () => _clip('move', [p])),
        FileAction('copy', 'Copy', () => _clip('copy', [p])),
        FileAction('layers', 'Duplicate', () => _do(() => duplicate([p]))),
        FileAction('edit', 'Rename', () => _rename(p)),
      ],
      if (dir && !trashed) FileAction('pin', _pinned(p) ? 'Unpin from the overview' : 'Pin to the overview', () => _pin(p, !_pinned(p))),
      FileAction('link', 'Copy path', () => _copyPath(p)),
      FileAction('info', 'Properties', () => showProperties(context, e, p)),
      if (trashed && _rw) FileAction('restore', 'Restore', () => _restore([p])),
      if (!trashed && _rw) FileAction('trash', 'Move to the computer’s Trash', () => _trashOut([p]), danger: true),
    ]);
  }

  void _folderSheet() {
    final p = _path;
    final shown = _shown();
    showActions(context, p == null ? 'Files' : (p == _info?['home'] ? 'Home' : baseOf(p)), [
      FileAction('sliders', 'Sort & view', () => showViewSheet(context)),
      if (_view == 'dir' && shown.isNotEmpty) FileAction('check', 'Select all', () => setState(() => _sel.addAll(shown.map(_pathOf)))),
      if (_view == 'dir' && p != null) ...[
        if (_rw && !_inTrash) FileAction('plus', 'New folder', _newFolder),
        if (_rw && !_inTrash) FileAction('share', 'Send files from this phone', _upload),
        if (!_inTrash) FileAction('pin', _pinned(p) ? 'Unpin this folder' : 'Pin this folder', () => _pin(p, !_pinned(p))),
        FileAction('down', 'Download this folder (.zip)', () => _download({'name': baseOf(p), 'dir': true, 'path': p})),
        FileAction('link', 'Copy path', () => _copyPath(p)),
      ],
      if (_trash.isNotEmpty && !_inTrash) FileAction('trash', 'Trash', () => _go(_trash)),
      FileAction('refresh', 'Refresh', _refresh),
    ]);
  }

  void _selectionMore(List<Map<String, dynamic>> picked) {
    final paths = [..._sel];
    final one = picked.length == 1 ? picked.first : null;
    final shown = _shown();
    showActions(context, '${paths.length} selected', [
      FileAction('check', 'Select all', () => setState(() => _sel.addAll(shown.map(_pathOf)))),
      FileAction('refresh', 'Invert selection', () => setState(() {
            final all = shown.map(_pathOf).toSet();
            final now = all.difference(_sel);
            _sel
              ..clear()
              ..addAll(now);
          })),
      FileAction('down', 'Download to this phone', () async {
        setState(_sel.clear);
        for (final e in picked) {
          await _download(e);
        }
      }),
      if (_rw && !_inTrash) FileAction('layers', 'Duplicate', () {
        setState(_sel.clear);
        _do(() => duplicate(paths));
      }),
      if (one != null && _rw && !_inTrash) FileAction('edit', 'Rename', () {
        setState(_sel.clear);
        _rename(_pathOf(one));
      }),
      if (one != null) FileAction('info', 'Properties', () => showProperties(context, one, _pathOf(one))),
    ]);
  }

  List<Map<String, dynamic>> _shown() => _view == 'dir' ? arrange(_entries, FilesView.saved, filter: _filtering ? _filter.text : '') : _found;

  // ── the page ───────────────────────────────────────────────────────────────

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final info = _info;
    final home = (info?['home'] ?? '') as String;
    final roots = ((info?['roots'] as List?) ?? const []).map((e) => '$e').toSet();
    final up = _path != null && !roots.contains(_path) && _path != '/' ? parentOf(_path!) : null;
    final dir = _view == 'dir';
    final v = FilesView.saved;
    final list = _shown();
    final picked = [for (final e in list) if (_sel.contains(_pathOf(e))) e];
    final clip = filesClip.value;
    return PopScope(
      // Back: out of a selection, then the filter, then up to the overview.
      canPop: _view == 'home' && _sel.isEmpty,
      onPopInvokedWithResult: (did, _) {
        if (did) return;
        if (_sel.isNotEmpty) return setState(_sel.clear);
        if (_filtering) {
          return setState(() {
            _filtering = false;
            _filter.clear();
          });
        }
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
                    if (_rw && !_inTrash) ...[
                      IconBtn('move', label: 'Cut', color: t.bright, onTap: () => _clip('move', [..._sel])),
                      IconBtn('copy', label: 'Copy', color: t.bright, onTap: () => _clip('copy', [..._sel])),
                      IconBtn('trash', label: 'Move to the computer’s Trash', color: t.bad, onTap: () => _trashOut([..._sel])),
                    ],
                    if (_inTrash && _rw) IconBtn('restore', label: 'Restore', color: t.bright, onTap: () => _restore([..._sel])),
                    IconBtn('dots', label: 'More', color: t.bright, onTap: () => _selectionMore(picked)),
                  ])
                : Row(key: const ValueKey('title'), children: [
                    if (_view == 'home')
                      Padding(padding: const EdgeInsets.fromLTRB(8, 0, 4, 0), child: WIcon('folder', color: t.base[0xA]))
                    else
                      IconBtn('back', label: dir && up != null && _path != home ? 'Up a folder' : 'Back to the overview', color: t.bright, onTap: () => dir && up != null && _path != home ? _go(up) : _home()),
                    Expanded(
                      child: dir && _path != null
                          ? (_filtering
                              ? TextField(
                                  controller: _filter,
                                  autofocus: true,
                                  decoration: InputDecoration(isDense: true, border: InputBorder.none, hintText: 'Filter ${_path == home ? 'Home' : baseOf(_path!)} — Enter searches inside'),
                                  onChanged: (_) => setState(() {}),
                                  onSubmitted: (q) => q.trim().isEmpty ? null : _find('“${q.trim()}” in ${baseOf(_path!)}', query: q.trim(), under: _path!),
                                )
                              : Crumbs(path: _path!, home: home, roots: roots, onGo: _go))
                          : Text(_view == 'home' ? 'Files' : _title, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright)),
                    ),
                    if (dir)
                      IconBtn(_filtering ? 'x' : 'search', label: _filtering ? 'Close the filter' : 'Filter this folder', color: t.bright, onTap: () => setState(() {
                            _filtering = !_filtering;
                            _filter.clear();
                          })),
                    if (dir && _rw && !_inTrash) IconBtn('plus', label: 'New', color: t.accent, onTap: () => showActions(context, 'New', [
                          FileAction('folder', 'New folder', _newFolder),
                          FileAction('share', 'Send files from this phone', _upload),
                        ])),
                    if (_view != 'home') IconBtn('sliders', label: 'Sort & view', color: t.bright, onTap: () => showViewSheet(context)),
                    IconBtn('dots', label: 'More', color: t.bright, onTap: _folderSheet),
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
        if (_inTrash)
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
            child: Text('The computer’s Trash. Long-press to pick, then Restore puts things back where they were.', style: body(context, size: .75, color: t.dim)),
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
                      : RefreshIndicator(onRefresh: () async => _refresh(), child: _items(context, list, v, where: !dir)),
        ),
        if (clip != null)
          PasteBar(clip: clip, canPaste: dir && _rw && !_inTrash, onPaste: _paste, onNewFolder: _newFolder),
      ]),
    );
  }

  Widget _items(BuildContext context, List<Map<String, dynamic>> list, FilesView v, {bool where = false}) {
    final t = Tok.of(context);
    final pad = EdgeInsets.only(bottom: 24 + (filesClip.value == null ? MediaQuery.of(context).padding.bottom : 0));
    if (list.isEmpty && !_busy) {
      return ListView(padding: pad, children: [
        Padding(
          padding: const EdgeInsets.all(24),
          child: Center(child: Text(_view == 'dir' ? (_filtering && _filter.text.isNotEmpty ? 'Nothing here by that name — Enter searches inside.' : 'Empty folder') : 'Nothing found — try another word.', style: body(context, color: t.dim))),
        ),
      ]);
    }
    void tap(Map<String, dynamic> e) {
      final p = _pathOf(e);
      if (_sel.isNotEmpty) return _toggle(e);
      e['dir'] == true ? _go(p) : openViewer(context, p, e);
    }

    void long(Map<String, dynamic> e) {
      feel('tick');
      _toggle(e);
    }

    if (v.grid) {
      return GridView.builder(
        padding: pad.add(const EdgeInsets.fromLTRB(12, 12, 12, 0)),
        gridDelegate: SliverGridDelegateWithMaxCrossAxisExtent(maxCrossAxisExtent: v.tileWidth, mainAxisSpacing: 8, crossAxisSpacing: 8, childAspectRatio: v.tile == 0 ? .82 : .9),
        itemCount: list.length,
        itemBuilder: (context, i) {
          final e = list[i], p = _pathOf(e);
          return FileTile(e: e, path: p, picked: _sel.contains(p), onTap: () => tap(e), onLong: () => long(e), width: v.tileWidth);
        },
      );
    }
    return ListView.builder(
      padding: pad,
      itemCount: list.length,
      itemBuilder: (context, i) {
        final e = list[i], p = _pathOf(e);
        return FileRow(
          e: e,
          path: p,
          picked: _sel.contains(p),
          where: where && p.contains('/') ? parentOf(p) : '',
          onTap: () => tap(e),
          onLong: () => long(e),
          onMore: _sel.isEmpty ? () => _itemSheet(e) : null,
        );
      },
    );
  }
}
