// The file manager's sheets: "Sort & view" (how every folder is shown, one place for it, as
// Google Files has), the actions for an item or a folder, a name to type, what to do when a
// name is taken, and an item's properties.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../wado/files.dart';
import '../../wado/files_view.dart';
import '../../wado/files_ops.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';
import 'files_page.dart' show fmtSize, fmtWhen;

/// One line of an actions sheet.
class FileAction {
  const FileAction(this.icon, this.label, this.run, {this.danger = false});
  final String icon, label;
  final VoidCallback run;
  final bool danger;
}

void showActions(BuildContext context, String title, List<FileAction> actions) {
  final t = Tok.of(context);
  showModalBottomSheet(
    context: context,
    backgroundColor: t.s1,
    isScrollControlled: true,
    builder: (s) => SafeArea(
      child: SingleChildScrollView(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          ListTile(title: Text(title, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright))),
          for (final a in actions)
            ListTile(
              leading: WIcon(a.icon, color: a.danger ? t.bad : t.bright),
              title: Text(a.label, style: TextStyle(color: a.danger ? t.bad : null)),
              onTap: () {
                Navigator.pop(s);
                a.run();
              },
            ),
        ]),
      ),
    ),
  );
}

/// Sort, order, folders first, hidden files, list or grid and the grid's size — each change
/// shows at once behind the sheet.
void showViewSheet(BuildContext context) {
  final t = Tok.of(context);
  showModalBottomSheet(
    context: context,
    backgroundColor: t.s1,
    builder: (s) => ListenableBuilder(
      listenable: Settings.I,
      builder: (context, _) {
        final v = FilesView.saved;
        Widget chip(String label, bool on, VoidCallback tap) => Padding(
              padding: const EdgeInsets.only(right: 8, bottom: 8),
              child: ChoiceChip(label: Text(label), selected: on, onSelected: (_) => tap()),
            );
        Widget head(String s) => Padding(padding: const EdgeInsets.fromLTRB(16, 12, 16, 6), child: Text(s, style: body(context, size: .75, w: FontWeight.w600, color: t.dim)));
        void put(String k, Object v) => Settings.I.set(k, v);
        return SafeArea(
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
            head('SORT BY'),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Wrap(children: [
                for (final (k, l) in const [('name', 'Name'), ('mtime', 'Date modified'), ('size', 'Size'), ('type', 'Type')]) chip(l, v.sort == k, () => put(kFilesSort, k)),
              ]),
            ),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Wrap(children: [
                chip(switch (v.sort) { 'mtime' => 'Oldest first', 'size' => 'Smallest first', _ => 'A to Z' }, !v.desc, () => put(kFilesDesc, false)),
                chip(switch (v.sort) { 'mtime' => 'Newest first', 'size' => 'Largest first', _ => 'Z to A' }, v.desc, () => put(kFilesDesc, true)),
              ]),
            ),
            head('VIEW'),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Wrap(children: [
                chip('List', !v.grid, () => put(kFilesGrid, false)),
                chip('Grid', v.grid, () => put(kFilesGrid, true)),
                if (v.grid) for (final (i, l) in const [(0, 'Small'), (1, 'Medium'), (2, 'Large')]) chip(l, v.tile == i, () => put(kFilesTile, i)),
              ]),
            ),
            SwitchListTile(title: const Text('Folders first'), value: v.dirsFirst, onChanged: (x) => put(kFilesDirsFirst, x)),
            SwitchListTile(title: const Text('Show hidden files'), value: v.hidden, onChanged: (x) => put(kFilesHidden, x)),
          ]),
        );
      },
    ),
  );
}

Future<String?> askText(BuildContext context, String title, String initial, {String ok = 'OK'}) {
  final c = TextEditingController(text: initial);
  // A name is selected up to its extension, so typing replaces the name and keeps the kind.
  final dot = initial.lastIndexOf('.');
  c.selection = TextSelection(baseOffset: 0, extentOffset: dot > 0 ? dot : initial.length);
  return showDialog<String>(
    context: context,
    builder: (d) => AlertDialog(
      title: Text(title),
      content: TextField(controller: c, autofocus: true, onSubmitted: (x) => Navigator.pop(d, x.trim())),
      actions: [TextButton(onPressed: () => Navigator.pop(d), child: const Text('Cancel')), TextButton(onPressed: () => Navigator.pop(d, c.text.trim()), child: Text(ok))],
    ),
  );
}

/// Some names are taken where things are going: keep both (`rename`), replace (`replace` — the
/// old ones go to the Trash), or nothing.
Future<String?> askClash(BuildContext context) => showDialog<String>(
      context: context,
      builder: (d) => AlertDialog(
        title: const Text('Some names already exist here'),
        content: const Text('Replacing moves the ones already here to the computer’s Trash.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(d), child: const Text('Cancel')),
          TextButton(onPressed: () => Navigator.pop(d, 'replace'), child: const Text('Replace')),
          TextButton(onPressed: () => Navigator.pop(d, 'rename'), child: const Text('Keep both')),
        ],
      ),
    );

/// What an item is: kind, size (a folder: how many items, how big one level down), dates, where.
void showProperties(BuildContext context, Map<String, dynamic> e, String path) {
  final t = Tok.of(context);
  final isDir = e['dir'] == true;
  final inside = isDir ? Files.I.req('list', {'path': path}).catchError((_) => <String, dynamic>{}) : null;
  Widget row(String k, String v, {bool copy = false}) => ListTile(
        dense: true,
        title: Text(k, style: body(context, size: .75, color: t.dim)),
        subtitle: SelectableText(v, style: copy ? mono(context, color: t.bright) : body(context, color: t.bright)),
        trailing: copy ? IconBtn('copy', label: 'Copy', onTap: () => Clipboard.setData(ClipboardData(text: v))) : null,
      );
  showModalBottomSheet(
    context: context,
    backgroundColor: t.s1,
    isScrollControlled: true,
    builder: (s) => SafeArea(
      child: SingleChildScrollView(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          ListTile(title: Text('${e['name']}', style: body(context, w: FontWeight.w600, color: t.bright))),
          row('Kind', isDir ? 'Folder' : (extOf('${e['name']}').isEmpty ? 'File' : '${extOf('${e['name']}').toUpperCase()} file')),
          if (!isDir) row('Size', '${fmtSize((e['size'] ?? 0) as num)} (${e['size'] ?? 0} bytes)'),
          if (inside != null)
            FutureBuilder<Map<String, dynamic>>(
              future: inside,
              builder: (context, r) {
                final list = ((r.data?['entries'] as List?) ?? const []).cast<Map<String, dynamic>>();
                if (r.data == null) return row('Contains', '…');
                final bytes = list.fold<num>(0, (a, x) => a + ((x['size'] ?? 0) as num));
                final dirs = list.where((x) => x['dir'] == true).length;
                return row('Contains', '${list.length - dirs} files, $dirs folders · ${fmtSize(bytes)} not counting subfolders');
              },
            ),
          row('Modified', fmtWhen((e['mtime'] ?? 0) as num)),
          row('Location', parentOf(path), copy: true),
        ]),
      ),
    ),
  );
}
