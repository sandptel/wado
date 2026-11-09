// The file manager's bars: where you are as breadcrumbs (tap any part to go there, the last
// always in view), and the paste bar — what was cut or copied, waiting at the bottom while you
// browse to where it should go.
import 'package:flutter/material.dart';

import '../../wado/files_ops.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';

class Crumbs extends StatefulWidget {
  const Crumbs({super.key, required this.path, required this.home, required this.roots, required this.onGo});
  final String path, home;
  final Set<String> roots;
  final ValueChanged<String> onGo;
  @override
  State<Crumbs> createState() => _CrumbsState();
}

class _CrumbsState extends State<Crumbs> {
  final _scroll = ScrollController();

  @override
  void didUpdateWidget(Crumbs old) {
    super.didUpdateWidget(old);
    // The folder you are in stays in view however deep it is.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scroll.hasClients) _scroll.animateTo(_scroll.position.maxScrollExtent, duration: const Duration(milliseconds: 200), curve: Curves.easeOut);
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final p = widget.path;
    // Start from home (shown as "Home") or from the root the folder is under.
    final base = p == widget.home || p.startsWith('${widget.home}/')
        ? widget.home
        : (widget.roots.where((r) => p == r || p.startsWith(r == '/' ? '/' : '$r/')).fold<String>('/', (a, r) => r.length > a.length ? r : a));
    final parts = <(String, String)>[(base == widget.home ? 'Home' : (base == '/' ? 'Computer' : baseOf(base)), base)];
    var at = base;
    for (final seg in p.substring(base.length).split('/').where((s) => s.isNotEmpty)) {
      at = at == '/' ? '/$seg' : '$at/$seg';
      parts.add((seg, at));
    }
    return SingleChildScrollView(
      controller: _scroll,
      scrollDirection: Axis.horizontal,
      child: Row(children: [
        for (var i = 0; i < parts.length; i++) ...[
          if (i > 0) WIcon('right', size: 14, color: t.dim),
          Press(
            onTap: i == parts.length - 1 ? null : () => widget.onGo(parts[i].$2),
            label: parts[i].$1,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 8),
              child: Text(parts[i].$1, style: body(context, w: i == parts.length - 1 ? FontWeight.w700 : FontWeight.w500, color: i == parts.length - 1 ? t.bright : t.dim)),
            ),
          ),
        ],
      ]),
    );
  }
}

class PasteBar extends StatelessWidget {
  const PasteBar({super.key, required this.clip, required this.canPaste, required this.onPaste, required this.onNewFolder});
  final FilesClip clip;
  final bool canPaste;
  final VoidCallback onPaste, onNewFolder;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final n = clip.paths.length;
    final what = n == 1 ? baseOf(clip.paths.first) : '$n items';
    return Container(
      padding: EdgeInsets.fromLTRB(16, 10, 8, 10 + MediaQuery.of(context).padding.bottom),
      decoration: BoxDecoration(color: t.s1, border: Border(top: BorderSide(color: t.line))),
      child: Row(children: [
        WIcon(clip.op == 'move' ? 'move' : 'copy', color: t.accent),
        const SizedBox(width: 10),
        Expanded(
          child: Text(canPaste ? '${clip.op == 'move' ? 'Move' : 'Copy'} $what here?' : 'Open a folder to ${clip.op == 'move' ? 'move' : 'copy'} $what into',
              maxLines: 2, overflow: TextOverflow.ellipsis, style: body(context, size: .8125, color: t.bright)),
        ),
        IconBtn('x', label: 'Cancel', color: t.bright, onTap: () => filesClip.value = null),
        if (canPaste) IconBtn('plus', label: 'New folder here', color: t.bright, onTap: onNewFolder),
        if (canPaste) Padding(padding: const EdgeInsets.only(left: 4), child: Btn('Paste', primary: true, expand: false, icon: 'paste', onTap: onPaste)),
      ]),
    );
  }
}
