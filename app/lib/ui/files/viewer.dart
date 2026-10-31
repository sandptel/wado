// The viewer (port of js/viewer/*): one sheet per file, by kind. Pictures zoom and pan on Stage
// Black; text and source read in mono; markdown formatted (or its source); JSON pretty; CSV as a
// table; archives list what is inside (the computer reads them). PDF, video and sound open in
// this phone's own app for them — as the web viewer does on a phone. Anything else gets the honest
// card: what it is, how big, and Download.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

import '../../wado/files.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'files_page.dart';
import 'kinds.dart';
import 'markdown.dart';

const _os = MethodChannel('wado/os');

/// Kinds shown here, fetched whole; their limit.
const _shown = {'image', 'text', 'markdown', 'json', 'csv'};
const _imageMax = 40 << 20;
/// Kinds handed to this phone's own app.
const _handed = {'pdf', 'video', 'audio'};

void openViewer(BuildContext context, String path, Map<String, dynamic> e) {
  Navigator.of(context).push(MaterialPageRoute(builder: (_) => _Viewer(path: path, e: e)));
}

/// Fetch a file into the cache and open it in the phone's app for its kind. False: no app.
Future<bool> openOnPhone(String path, String name, {void Function(double)? progress}) async {
  final bytes = await Files.I.get(path, progress: progress);
  final dir = Directory('${(await getTemporaryDirectory()).path}/open')..createSync(recursive: true);
  final f = File('${dir.path}/$name');
  await f.writeAsBytes(bytes);
  return await _os.invokeMethod<bool>('view', {'path': f.path, 'mime': mimeOf(name)}) ?? false;
}

class _Viewer extends StatefulWidget {
  const _Viewer({required this.path, required this.e});
  final String path;
  final Map<String, dynamic> e;
  @override
  State<_Viewer> createState() => _ViewerState();
}

class _ViewerState extends State<_Viewer> {
  Uint8List? _bytes;
  Map<String, dynamic>? _entries;
  String _err = '';
  double _p = 0;
  bool _source = false, _opening = false;
  late final String name = widget.path.split('/').last;
  late final String kind = kindOf(name);
  late final int size = ((widget.e['size'] ?? 0) as num).toInt();
  bool get _fetch => _shown.contains(kind) && size <= (kind == 'image' ? _imageMax : 8 << 20);

  @override
  void initState() {
    super.initState();
    if (_fetch) {
      Files.I.get(widget.path, progress: (p) => mounted ? setState(() => _p = p) : null).then((b) => mounted ? setState(() => _bytes = b) : null).catchError((Object e) {
        if (mounted) setState(() => _err = '$e');
      });
    }
    if (kind == 'archive') {
      Files.I.req('entries', {'path': widget.path}).timeout(const Duration(seconds: 8)).then((r) => mounted ? setState(() => _entries = r) : null).catchError((Object e) {
        if (!mounted) return;
        setState(() => _err = '$e'.contains('Timeout')
            ? "The computer's wado is older and cannot list archives yet. Download it, or open it on the computer."
            : 'Its contents could not be listed: $e.');
      });
    }
  }

  Future<void> _hand() async {
    setState(() {
      _opening = true;
      _p = 0;
    });
    try {
      final ok = await openOnPhone(widget.path, name, progress: (p) => mounted ? setState(() => _p = p) : null);
      if (!ok && mounted) setState(() => _err = 'No app on this phone opens ${typeOf(name)}s. Download it instead.');
    } catch (e) {
      if (mounted) setState(() => _err = '$e');
    }
    if (mounted) setState(() => _opening = false);
  }

  String get _text {
    final b = _bytes!;
    return utf8.decode(b.length > textMax ? b.sublist(0, textMax) : b, allowMalformed: true);
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final dark = kind == 'image';
    final fg = dark ? Colors.white : t.bright;
    return Scaffold(
      backgroundColor: dark ? Tokens.stage : t.s0,
      appBar: AppBar(
        backgroundColor: dark ? Tokens.stage : t.s0,
        foregroundColor: fg,
        title: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(name, maxLines: 1, overflow: TextOverflow.ellipsis, style: display(context, rem(context, 1), color: fg)),
          Text('${typeOf(name)} · ${fmtSize(size)}', style: body(context, size: .75, color: dark ? Colors.white60 : t.dim)),
        ]),
        actions: [
          if (kind == 'markdown' && _bytes != null)
            IconBtn('term', label: _source ? 'Formatted' : 'Source', color: _source ? t.accent : fg, onTap: () => setState(() => _source = !_source)),
          IconBtn('down', label: 'Download to this phone', color: fg, onTap: () async => keepOnPhone(name, _bytes ?? await Files.I.get(widget.path))),
        ],
      ),
      body: _body(context),
    );
  }

  Widget _body(BuildContext context) {
    final t = Tok.of(context);
    if (_err.isNotEmpty) return _card(context, why: _err, bad: true);
    if (_handed.contains(kind)) {
      final what = switch (kind) { 'pdf' => 'PDF app', 'video' => 'video player', _ => 'music player' };
      return _card(
        context,
        icon: switch (kind) { 'pdf' => 'text', 'video' => 'play', _ => 'music' },
        title: 'Opens in your $what',
        why: 'It comes to this phone first, then your $what shows it.',
        act: Btn(_opening ? 'Bringing it here… ${(_p * 100).round()}%' : 'Open the ${kind == 'pdf' ? 'PDF' : typeOf(name)}', primary: true, busy: _opening, onTap: _opening ? null : _hand),
      );
    }
    if (kind == 'archive') {
      final r = _entries;
      if (r == null) return const Center(child: CircularProgressIndicator(strokeWidth: 2.5));
      final list = (r['entries'] as List? ?? const []).cast<Map>();
      final files = list.where((x) => x['dir'] != true).length;
      return ListView(padding: const EdgeInsets.fromLTRB(12, 8, 12, 24), children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(8, 4, 8, 8),
          child: Text('$files${r['more'] == true ? '+' : ''} files${list.length - files > 0 ? ' · ${list.length - files} folders' : ''}', style: body(context, size: .8125, color: t.dim)),
        ),
        for (final x in list)
          () {
            final n = '${x['name']}'.replaceFirst(RegExp(r'/$'), '');
            final depth = n.split('/').length - 1;
            final dir = x['dir'] == true;
            return SizedBox(
              height: 44,
              child: Padding(
                padding: EdgeInsets.only(left: 8 + depth.clamp(0, 6) * 18.0, right: 8),
                child: Row(children: [
                  WIcon(dir ? 'folder' : 'text', size: 18, color: dir ? t.accent : t.dim),
                  const SizedBox(width: 10),
                  Expanded(child: Text(n.split('/').last, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, color: t.bright))),
                  if (!dir) Text(fmtSize(((x['size'] ?? 0) as num).toInt()), style: body(context, size: .75, color: t.dim)),
                ]),
              ),
            );
          }(),
        if (r['more'] == true) Padding(padding: const EdgeInsets.all(8), child: Text('Showing the first ${list.length} of ${r['total']} entries.', style: body(context, size: .8125, color: t.dim))),
      ]);
    }
    if (!_fetch) return _card(context, why: "This kind can't be shown here. Download it, or open it on the computer.");
    if (_bytes == null) {
      return Center(child: SizedBox(width: 160, child: LinearProgressIndicator(value: _p > 0 ? _p : null, color: t.accent, backgroundColor: t.s2)));
    }
    final cut = _bytes!.length > textMax && kind != 'image';
    final note = cut ? Padding(padding: const EdgeInsets.fromLTRB(16, 12, 16, 0), child: Text('Showing the first ${fmtSize(textMax)} — download it for the rest.', style: body(context, size: .8125, color: t.dim))) : null;
    Widget code(String s) => SingleChildScrollView(
          padding: const EdgeInsets.all(12),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [?note, SelectableText(s, style: mono(context, size: .8125, color: t.text))]),
        );
    switch (kind) {
      case 'image':
        return InteractiveViewer(maxScale: 8, child: Center(child: Image.memory(_bytes!, errorBuilder: (_, _, _) => _card(context, why: "This picture's format can't be drawn on this phone."))));
      case 'markdown':
        return _source ? code(_text) : SingleChildScrollView(child: Column(children: [?note, MarkdownView(_text)]));
      case 'json':
        try {
          return code(const JsonEncoder.withIndent('  ').convert(jsonDecode(_text)));
        } catch (_) {
          return code(_text);
        }
      case 'csv':
        return _Table(text: _text, tab: extOf(name) == 'tsv');
      default:
        return code(_text);
    }
  }

  Widget _card(BuildContext context, {String icon = 'text', String? title, required String why, Widget? act, bool bad = false}) {
    final t = Tok.of(context);
    return ListView(padding: const EdgeInsets.all(16), children: [
      WCard(children: [
        Row(children: [
          Disc(child: WIcon(icon, color: t.accent)),
          const SizedBox(width: 12),
          Expanded(child: Text(title ?? name, style: display(context, rem(context, 1.0625)))),
        ]),
        Text(why, style: body(context, color: bad ? t.bad : t.text)),
        _facts(context),
        ?act,
        if (act == null) Btn('Download to this phone', primary: !bad, icon: 'down', onTap: () async => keepOnPhone(name, await Files.I.get(widget.path))),
      ]),
    ]);
  }

  Widget _facts(BuildContext context) {
    final t = Tok.of(context);
    Widget row(String k, String v) => Padding(
          padding: const EdgeInsets.symmetric(vertical: 4),
          child: Row(children: [SizedBox(width: 90, child: Text(k, style: body(context, size: .8125, color: t.dim))), Expanded(child: Text(v, style: body(context, size: .8125, color: t.bright)))]),
        );
    return Column(children: [
      row('Kind', typeOf(name)),
      row('Size', fmtSize(size)),
      if ((widget.e['mtime'] ?? 0) != 0) row('Changed', fmtWhen((widget.e['mtime'] ?? 0) as num)),
      row('Where', widget.path.substring(0, widget.path.length - name.length)),
    ]);
  }
}

/// CSV or TSV as a table (csv.js): the first row is the head, quoted fields keep their commas.
class _Table extends StatelessWidget {
  const _Table({required this.text, required this.tab});
  final String text;
  final bool tab;

  List<List<String>> _rows() {
    final sep = tab ? '\t' : ',';
    final rows = <List<String>>[];
    var row = <String>[], f = StringBuffer();
    var q = false;
    for (var i = 0; i < text.length && rows.length < 1000; i++) {
      final ch = text[i];
      if (q) {
        if (ch == '"' && i + 1 < text.length && text[i + 1] == '"') {
          f.write('"');
          i++;
        } else if (ch == '"') {
          q = false;
        } else {
          f.write(ch);
        }
      } else if (ch == '"') {
        q = true;
      } else if (ch == sep) {
        row.add('$f');
        f = StringBuffer();
      } else if (ch == '\n') {
        row.add('$f'.replaceAll('\r', ''));
        rows.add(row);
        row = [];
        f = StringBuffer();
      } else {
        f.write(ch);
      }
    }
    if (f.isNotEmpty || row.isNotEmpty) rows.add([...row, '$f']);
    return rows;
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final rows = _rows();
    if (rows.isEmpty) return const SizedBox.shrink();
    final n = rows.map((r) => r.length).fold(0, (a, b) => a > b ? a : b);
    TableRow tr(List<String> r, bool head) => TableRow(
          decoration: BoxDecoration(color: head ? t.s1 : null, border: Border(bottom: BorderSide(color: t.line))),
          children: [
            for (var i = 0; i < n; i++)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                child: Text(i < r.length ? r[i] : '', style: body(context, size: .8125, w: head ? FontWeight.w600 : FontWeight.w400, color: head ? t.bright : t.text)),
              ),
          ],
        );
    return SingleChildScrollView(
      padding: const EdgeInsets.all(12),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: Table(defaultColumnWidth: const IntrinsicColumnWidth(), children: [tr(rows.first, true), for (final r in rows.skip(1)) tr(r, false)]),
      ),
    );
  }
}
