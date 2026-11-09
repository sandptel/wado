// How a folder is shown (port of the web's saved view, js/files_ui.js): sort by name, date, size
// or type, either way round, folders first or mixed in, hidden files or not, a list or a grid of
// three sizes. Kept in Settings, so every folder and the next visit look the same.
import 'settings.dart';

/// The settings keys, with their defaults in settings.dart.
const kFilesSort = 'files_sort', kFilesDesc = 'files_desc', kFilesDirsFirst = 'files_dirs_first';
const kFilesHidden = 'files_hidden', kFilesGrid = 'files_grid', kFilesTile = 'files_tile';

class FilesView {
  const FilesView({this.sort = 'name', this.desc = false, this.dirsFirst = true, this.hidden = false, this.grid = false, this.tile = 1});
  /// name · mtime · size · type
  final String sort;
  final bool desc, dirsFirst, hidden, grid;
  /// Grid tiles: 0 small, 1 medium, 2 large.
  final int tile;

  static FilesView get saved {
    final s = Settings.I;
    return FilesView(
      sort: s.s(kFilesSort).isEmpty ? 'name' : s.s(kFilesSort),
      desc: s.b(kFilesDesc),
      dirsFirst: s.b(kFilesDirsFirst),
      hidden: s.b(kFilesHidden),
      grid: s.b(kFilesGrid),
      tile: s.i(kFilesTile).clamp(0, 2),
    );
  }

  /// The widest a grid tile may be, by size.
  double get tileWidth => const [96.0, 128.0, 176.0][tile];
}

/// The part of a name after its last dot, lower case; empty for none (and for `.bashrc`).
String extOf(String name) {
  final i = name.lastIndexOf('.');
  return i <= 0 ? '' : name.substring(i + 1).toLowerCase();
}

/// `entries` as `v` shows them: hidden ones left out unless asked, `filter` narrowing by name
/// (any case), sorted. Ties fall back to the name, so the order never jumps.
List<Map<String, dynamic>> arrange(List<Map<String, dynamic>> entries, FilesView v, {String filter = ''}) {
  final f = filter.trim().toLowerCase();
  final out = [
    for (final e in entries)
      if ((v.hidden || !'${e['name']}'.startsWith('.')) && (f.isEmpty || '${e['name']}'.toLowerCase().contains(f))) e,
  ];
  int byName(Map a, Map b) => '${a['name']}'.toLowerCase().compareTo('${b['name']}'.toLowerCase());
  num n(Map e, String k) => (e[k] ?? 0) as num;
  out.sort((a, b) {
    if (v.dirsFirst && a['dir'] != b['dir']) return a['dir'] == true ? -1 : 1;
    final c = switch (v.sort) {
      'mtime' => n(a, 'mtime').compareTo(n(b, 'mtime')),
      'size' => n(a, 'size').compareTo(n(b, 'size')),
      'type' => extOf('${a['name']}').compareTo(extOf('${b['name']}')),
      _ => 0,
    };
    final r = c != 0 ? c : byName(a, b);
    return v.desc ? -r : r;
  });
  return out;
}
