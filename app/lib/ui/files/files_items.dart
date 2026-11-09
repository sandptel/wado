// One file or folder as the list shows it (a row) and as the grid does (a tile): its picture —
// a thumbnail from the computer for images and videos, its kind's icon otherwise — name and
// details; picked, a check takes the picture's place. Tap, long-press and ⋯ are the page's.
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../../wado/files.dart';
import '../../wado/files_view.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import 'files_page.dart' show fmtSize, fmtWhen, iconFor;

/// Thumbnails asked once per file version, for as long as the app runs.
final _thumbs = <String, Future<Uint8List?>>{};

const _thumbed = {'png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'heic', 'avif', 'mp4', 'mov', 'mkv', 'webm', 'pdf'};

Future<Uint8List?> _thumb(String path, num mtime) => _thumbs['$path@$mtime'] ??= Files.I
    .req('thumb', {'path': path})
    .then((r) => r['png'] is String ? base64Decode(r['png'] as String) : null)
    .catchError((_) => null);

class FilePic extends StatelessWidget {
  const FilePic({super.key, required this.e, required this.path, required this.picked, this.size = 40});
  final Map<String, dynamic> e;
  final String path;
  final bool picked;
  final double size;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final isDir = e['dir'] == true;
    final hue = isDir ? t.base[0xA] : t.base[0xD];
    final icon = Disc(key: ValueKey(picked), hue: picked ? t.accent : hue, child: WIcon(picked ? 'check' : iconFor(e), color: picked ? t.accent : hue));
    final Widget pic = picked || isDir || !_thumbed.contains(extOf('${e['name']}'))
        ? icon
        : FutureBuilder<Uint8List?>(
            future: _thumb(path, (e['mtime'] ?? 0) as num),
            builder: (context, s) => s.data == null
                ? icon
                : ClipRRect(
                    borderRadius: BorderRadius.circular(t.r1),
                    child: Image.memory(s.data!, width: size, height: size, fit: BoxFit.cover, gaplessPlayback: true),
                  ),
          );
    // Picked: the picture gives way to a check that pops in (and back without the pop).
    return AnimatedSwitcher(
      duration: t.dur,
      switchInCurve: t.pop,
      switchOutCurve: Curves.easeIn,
      transitionBuilder: (child, a) => ScaleTransition(scale: Tween(begin: .6, end: 1.0).animate(a), child: FadeTransition(opacity: a, child: child)),
      child: SizedBox(key: ValueKey(picked), width: size, height: size, child: Center(child: pic)),
    );
  }
}

String details(Map<String, dynamic> e, {String where = ''}) =>
    [if (e['dir'] != true) fmtSize((e['size'] ?? 0) as num), fmtWhen((e['mtime'] ?? 0) as num), where].where((x) => x.isNotEmpty).join(' · ');

class FileRow extends StatelessWidget {
  const FileRow({super.key, required this.e, required this.path, required this.picked, required this.onTap, required this.onLong, this.onMore, this.where = ''});
  final Map<String, dynamic> e;
  final String path, where;
  final bool picked;
  final VoidCallback onTap, onLong;
  final VoidCallback? onMore;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Semantics(
      selected: picked,
      child: Press(
        onTap: onTap,
        onLongPress: onLong,
        label: '${e['name']}${picked ? ', selected' : ''}',
        child: AnimatedContainer(
          duration: t.durPress,
          curve: Tokens.spring,
          color: picked ? fade(t.accent, 88) : fade(t.accent, 100),
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
          child: Row(children: [
            FilePic(e: e, path: path, picked: picked),
            const SizedBox(width: 12),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text('${e['name']}', maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright)),
                Text(details(e, where: where), maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, color: t.dim)),
              ]),
            ),
            if (onMore != null) IconBtn('dots', label: 'More', onTap: onMore),
          ]),
        ),
      ),
    );
  }
}

class FileTile extends StatelessWidget {
  const FileTile({super.key, required this.e, required this.path, required this.picked, required this.onTap, required this.onLong, required this.width});
  final Map<String, dynamic> e;
  final String path;
  final bool picked;
  final VoidCallback onTap, onLong;
  final double width;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Semantics(
      selected: picked,
      child: Press(
        onTap: onTap,
        onLongPress: onLong,
        label: '${e['name']}${picked ? ', selected' : ''}',
        child: AnimatedContainer(
          duration: t.durPress,
          curve: Tokens.spring,
          decoration: BoxDecoration(
            color: picked ? fade(t.accent, 85) : t.s1,
            borderRadius: BorderRadius.circular(t.r3),
            border: Border.all(color: picked ? t.accent : t.line),
          ),
          padding: const EdgeInsets.all(8),
          child: Column(children: [
            Expanded(child: LayoutBuilder(builder: (context, box) => FilePic(e: e, path: path, picked: picked, size: box.biggest.shortestSide))),
            const SizedBox(height: 6),
            Text('${e['name']}', maxLines: width < 110 ? 1 : 2, textAlign: TextAlign.center, overflow: TextOverflow.ellipsis, style: body(context, size: .75, w: FontWeight.w600, color: t.bright)),
          ]),
        ),
      ),
    );
  }
}
