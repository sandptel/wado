// The file manager's first screen (port of files_ui.js `homeView`): it answers "where is that
// thing" — search, categories, recent files, folders, how full the disk is.
import 'package:flutter/material.dart';

import '../../wado/files.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'files_page.dart' show fmtSize, fmtWhen, iconFor;

/// Name, `find` kind, icon, base16 hue.
const cats = [('Photos', 'image', 'image', 0xE), ('Videos', 'video', 'camera', 0x8), ('Music', 'audio', 'music', 0xC), ('Documents', 'doc', 'text', 0xD), ('Archives', 'archive', 'layers', 0x9)];

String placeName(String p, String home) => p == home ? 'Home' : (p == '/' ? 'Computer' : p.split('/').lastWhere((s) => s.isNotEmpty, orElse: () => p));

class FilesOverview extends StatefulWidget {
  const FilesOverview({super.key, required this.quick, required this.onGo, required this.onOpen, required this.onFind, required this.onRecent});
  final Map<String, dynamic> quick;
  final ValueChanged<String> onGo;
  final ValueChanged<Map<String, dynamic>> onOpen;
  final void Function(String title, {String kind, String query}) onFind;
  final VoidCallback onRecent;
  @override
  State<FilesOverview> createState() => _FilesOverviewState();
}

class _FilesOverviewState extends State<FilesOverview> {
  final _q = TextEditingController();

  @override
  void dispose() {
    _q.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final info = Files.I.info ?? const {};
    final home = '${info['home'] ?? ''}';
    final pins = ((widget.quick['pins'] as List?) ?? const []).cast<Map<String, dynamic>>();
    final recent = ((widget.quick['recent'] as List?) ?? const []).cast<Map<String, dynamic>>().take(16).toList();
    final dl = pins.where((p) => '${p['path']}'.endsWith('/Downloads')).firstOrNull;
    final space = ((info['space'] as List?) ?? const []).cast<Map>();
    Widget sect(String s, [Widget? end]) => Padding(padding: const EdgeInsets.fromLTRB(16, 18, 16, 8), child: Row(children: [Expanded(child: Sect(s)), ?end]));

    return ListView(padding: EdgeInsets.only(bottom: 24 + MediaQuery.paddingOf(context).bottom), children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
        child: TextField(
          controller: _q,
          textInputAction: TextInputAction.search,
          onSubmitted: (v) => v.trim().isEmpty ? null : widget.onFind('“${v.trim()}”', query: v.trim()),
          style: body(context, color: t.bright),
          decoration: InputDecoration(
            hintText: 'Search files on the computer',
            hintStyle: body(context, color: t.dim),
            prefixIcon: Padding(padding: const EdgeInsets.all(12), child: WIcon('search', size: 18, color: t.dim)),
            filled: true,
            fillColor: t.s1,
            border: OutlineInputBorder(borderRadius: BorderRadius.circular(999), borderSide: BorderSide.none),
            contentPadding: const EdgeInsets.symmetric(vertical: 12),
          ),
        ),
      ),
      sect('Categories'),
      Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12),
        child: GridView.count(
          crossAxisCount: 3,
          shrinkWrap: true,
          physics: const NeverScrollableScrollPhysics(),
          childAspectRatio: 1.25,
          children: [
            for (final (name, kind, icon, hue) in cats) _Cat(name: name, icon: icon, hue: t.base[hue], onTap: () => widget.onFind(name, kind: kind)),
            if (dl != null) _Cat(name: 'Downloads', icon: 'down', hue: t.base[0xB], onTap: () => widget.onGo('${dl['path']}')),
          ],
        ),
      ),
      if (recent.isNotEmpty) ...[
        sect('Recent', TextButton(onPressed: widget.onRecent, child: Text('See all', style: body(context, size: .8125, w: FontWeight.w600, color: t.accent)))),
        SizedBox(
          height: 118,
          child: ListView(scrollDirection: Axis.horizontal, padding: const EdgeInsets.symmetric(horizontal: 12), children: [
            for (final r in recent)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4),
                child: Press(
                  onTap: () => widget.onOpen(r),
                  label: '${r['name']}',
                  child: Container(
                    width: 112,
                    padding: const EdgeInsets.all(10),
                    decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3), border: Border.all(color: t.line)),
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Disc(hue: t.base[0xD], child: WIcon(iconFor(r), color: t.base[0xD])),
                      const Spacer(),
                      Text('${r['name']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .8125, w: FontWeight.w600, color: t.bright)),
                      Text(fmtWhen((r['mtime'] ?? 0) as num), style: body(context, size: .6875, color: t.dim)),
                    ]),
                  ),
                ),
              ),
          ]),
        ),
      ],
      sect('Folders'),
      Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12),
        child: Wrap(spacing: 8, runSpacing: 8, children: [
          for (final r in ((info['roots'] as List?) ?? const []).map((e) => '$e')) _Chip(icon: r == home ? 'house' : 'server', name: placeName(r, home), onTap: () => widget.onGo(r)),
          for (final p in pins) _Chip(icon: p['kind'] == 'pin' ? 'pin' : 'folder', name: '${p['name']}', onTap: () => widget.onGo('${p['path']}')),
          if (info['trash'] != null) _Chip(icon: 'trash', name: 'Trash', onTap: () => widget.onGo('${info['trash']}')),
        ]),
      ),
      if (space.isNotEmpty) ...[
        sect('Storage'),
        for (final s in space)
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
            child: () {
              final total = (s['total'] ?? 0) as num, free = (s['free'] ?? 0) as num, used = total - free;
              final pct = total > 0 ? used / total : 0.0;
              return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Row(children: [
                  WIcon('server', size: 16, color: t.dim),
                  const SizedBox(width: 8),
                  Expanded(child: Text(placeName('${s['root']}', home), style: body(context, w: FontWeight.w600, color: t.bright))),
                  Text('${fmtSize(free)} free', style: body(context, size: .75, color: t.dim)),
                ]),
                const SizedBox(height: 6),
                ClipRRect(
                  borderRadius: BorderRadius.circular(999),
                  child: LinearProgressIndicator(value: pct.toDouble(), minHeight: 6, color: pct > .9 ? t.bad : t.accent, backgroundColor: t.s2),
                ),
                const SizedBox(height: 4),
                Text('${fmtSize(used)} of ${fmtSize(total)} used', style: body(context, size: .6875, color: t.dim)),
              ]);
            }(),
          ),
      ],
    ]);
  }
}

class _Cat extends StatelessWidget {
  const _Cat({required this.name, required this.icon, required this.hue, required this.onTap});
  final String name, icon;
  final Color hue;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Padding(
      padding: const EdgeInsets.all(4),
      child: Press(
        onTap: onTap,
        label: name,
        child: Container(
          padding: const EdgeInsets.all(10),
          decoration: BoxDecoration(color: Color.alphaBlend(fade(hue, 88), t.s1), borderRadius: BorderRadius.circular(t.r3)),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            WIcon(icon, color: hue),
            const Spacer(),
            Text(name, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .8125, w: FontWeight.w600, color: t.bright)),
          ]),
        ),
      ),
    );
  }
}

class _Chip extends StatelessWidget {
  const _Chip({required this.icon, required this.name, required this.onTap});
  final String icon, name;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Press(
      onTap: onTap,
      label: name,
      child: Container(
        height: 40,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(color: t.s2, borderRadius: BorderRadius.circular(999)),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          WIcon(icon, size: 16, color: t.base[0xA]),
          const SizedBox(width: 6),
          Flexible(child: Text(name, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright))),
        ]),
      ),
    );
  }
}
