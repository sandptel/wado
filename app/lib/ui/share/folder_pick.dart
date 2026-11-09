// "Other folder…" for a share: the computer's folders, one level at a time, and "Save here".
// Only folders are listed; it starts at home and goes up to the top of what this phone may see.
import 'package:flutter/material.dart';

import '../../wado/files.dart';
import '../../wado/files_ops.dart' show parentOf, baseOf;
import '../theme/look.dart';
import '../widgets/basics.dart';

Future<String?> pickFolder(BuildContext context, String start) => showModalBottomSheet<String>(
      context: context,
      backgroundColor: Tok.of(context).s1,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (_) => SizedBox(height: MediaQuery.of(context).size.height * .7, child: _Pick(start)),
    );

class _Pick extends StatefulWidget {
  const _Pick(this.start);
  final String start;
  @override
  State<_Pick> createState() => _PickState();
}

class _PickState extends State<_Pick> {
  late String _at = widget.start;
  List<String> _dirs = [];
  String _err = '';
  bool _busy = true;

  @override
  void initState() {
    super.initState();
    _go(_at);
  }

  Future<void> _go(String p) async {
    setState(() {
      _busy = true;
      _err = '';
    });
    try {
      final r = await Files.I.req('list', {'path': p});
      final names = [
        for (final e in (r['entries'] as List).cast<Map<String, dynamic>>())
          if (e['dir'] == true && !'${e['name']}'.startsWith('.')) '${e['name']}',
      ]..sort((a, b) => a.toLowerCase().compareTo(b.toLowerCase()));
      setState(() {
        _at = (r['path'] ?? p) as String;
        _dirs = names;
        _busy = false;
      });
    } catch (e) {
      setState(() {
        _err = '$e'.replaceFirst('Exception: ', '');
        _busy = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final roots = ((Files.I.info?['roots'] as List?) ?? const []).map((e) => '$e').toSet();
    final top = roots.contains(_at) || _at == '/';
    return SafeArea(
      child: Column(children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(8, 0, 16, 8),
          child: Row(children: [
            IconBtn('back', label: 'Up a folder', color: top ? t.dim : t.bright, onTap: top ? null : () => _go(parentOf(_at))),
            Expanded(child: Text(_at, maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(context, color: t.bright))),
          ]),
        ),
        Expanded(
          child: _busy
              ? const Center(child: CircularProgressIndicator(strokeWidth: 2))
              : _err.isNotEmpty
                  ? Center(child: Text(_err, style: body(context, color: t.bad)))
                  : ListView(children: [
                      for (final d in _dirs)
                        ListTile(leading: WIcon('folder', color: t.base[0xA]), title: Text(d), onTap: () => _go('${_at == '/' ? '' : _at}/$d')),
                      if (_dirs.isEmpty) Padding(padding: const EdgeInsets.all(24), child: Center(child: Text('No folders inside', style: body(context, color: t.dim)))),
                    ]),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
          child: Btn('Save in ${_at == Files.I.info?['home'] ? 'Home' : baseOf(_at)}', primary: true, onTap: () => Navigator.pop(context, _at)),
        ),
      ]),
    );
  }
}
