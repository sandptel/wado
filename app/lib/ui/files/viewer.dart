// The viewer (port of js/viewer/*): images zoom and pan on Stage Black; text, code, markdown,
// JSON and CSV read as text; everything else gets the honest card — type, size, Download.
// Files over 8 MB, and all media, are not fetched whole: they are offered as a download.
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../../wado/files.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'files_page.dart';

const _image = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'];
const _text = ['txt', 'md', 'json', 'csv', 'tsv', 'log', 'rs', 'js', 'ts', 'dart', 'py', 'sh', 'nix', 'toml', 'yaml', 'yml', 'kdl', 'conf', 'ini', 'c', 'h', 'cpp', 'go', 'java', 'kt', 'html', 'css', 'xml', 'sql'];

void openViewer(BuildContext context, String path, Map<String, dynamic> e) {
  Navigator.of(context).push(MaterialPageRoute(builder: (_) => _Viewer(path: path, e: e)));
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
  String _err = '';
  double _p = 0;
  late final String name = widget.path.split('/').last;
  late final String ext = name.contains('.') ? name.split('.').last.toLowerCase() : '';
  late final int size = ((widget.e['size'] ?? 0) as num).toInt();
  bool get _shows => (_image.contains(ext) || _text.contains(ext) || ext.isEmpty) && size <= 8 * 1024 * 1024;

  @override
  void initState() {
    super.initState();
    if (_shows) {
      Files.I.get(widget.path, progress: (p) => setState(() => _p = p)).then((b) => setState(() => _bytes = b)).catchError((Object e) => setState(() => _err = '$e'));
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final img = _image.contains(ext);
    return Scaffold(
      backgroundColor: img ? Tokens.stage : t.s0,
      appBar: AppBar(
        backgroundColor: img ? Tokens.stage : t.s0,
        foregroundColor: img ? Colors.white : t.bright,
        title: Text(name, style: display(context, rem(context, 1), color: img ? Colors.white : t.bright)),
        actions: [
          IconBtn('down', label: 'Download to this phone', color: img ? Colors.white : t.bright, onTap: () async {
            final b = _bytes ?? await Files.I.get(widget.path);
            await keepOnPhone(name, b);
          }),
        ],
      ),
      body: _err.isNotEmpty
          ? Center(child: Padding(padding: const EdgeInsets.all(24), child: Text(_err, style: body(context, color: t.bad))))
          : !_shows
              ? Padding(
                  padding: const EdgeInsets.all(16),
                  child: WCard(children: [
                    Text(name, style: display(context, rem(context, 1.125))),
                    Text('${ext.isEmpty ? 'File' : ext.toUpperCase()} · ${fmtSize(size)} · ${fmtWhen((widget.e['mtime'] ?? 0) as num)}', style: body(context, color: t.dim)),
                    const Why('This kind opens on the computer, or download it to this phone.'),
                    Btn('Download to this phone', primary: true, icon: 'down', onTap: () async => keepOnPhone(name, await Files.I.get(widget.path))),
                  ]),
                )
              : _bytes == null
                  ? Center(child: SizedBox(width: 160, child: LinearProgressIndicator(value: _p > 0 ? _p : null, color: t.accent, backgroundColor: t.s2)))
                  : img
                      ? InteractiveViewer(maxScale: 8, child: Center(child: Image.memory(_bytes!)))
                      : SingleChildScrollView(
                          padding: const EdgeInsets.all(12),
                          child: SelectableText(utf8.decode(_bytes!, allowMalformed: true), style: mono(context, size: .8125, color: t.text)),
                        ),
    );
  }
}
