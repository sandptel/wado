// What a file is, by its name (port of js/viewer/kinds.js): its kind (which viewer), the words
// for its type, and its MIME type for the phone's own apps.

const _kinds = {
  'image': 'png jpg jpeg gif webp avif bmp ico svg heic heif',
  'video': 'mp4 m4v webm mov mkv ogv avi wmv flv mpg mpeg ts m2ts mts 3gp vob divx rmvb asf',
  'audio': 'mp3 m4a aac ogg oga opus flac wav weba wma ape alac aiff aif mka ac3 dts amr',
  'pdf': 'pdf',
  'markdown': 'md markdown mdown',
  'json': 'json jsonl geojson webmanifest',
  'csv': 'csv tsv',
  'text': 'txt log nfo xml yaml yml toml ini conf cfg kdl nix rs js mjs cjs ts tsx jsx py go c h cc cpp hpp java kt rb php sh '
      'fish zsh bash lua sql swift zig hs ml css scss html htm srt vtt diff patch env gitignore dockerfile makefile dart',
  'archive': 'zip jar apk epub docx xlsx pptx odt ods odp tar gz tgz xz bz2 zst 7z rar',
  'font': 'ttf otf woff woff2',
};
final _by = {for (final e in _kinds.entries) for (final x in e.value.split(' ')) x: e.key};
const _bare = {'makefile': 'text', 'dockerfile': 'text', 'readme': 'text', 'license': 'text', 'changelog': 'text'};
const _mime = {
  'png': 'image/png', 'jpg': 'image/jpeg', 'jpeg': 'image/jpeg', 'gif': 'image/gif', 'webp': 'image/webp', 'avif': 'image/avif', //
  'bmp': 'image/bmp', 'svg': 'image/svg+xml', 'heic': 'image/heic', 'heif': 'image/heif',
  'mp4': 'video/mp4', 'm4v': 'video/mp4', 'webm': 'video/webm', 'mov': 'video/quicktime', 'mkv': 'video/x-matroska', 'ogv': 'video/ogg',
  'avi': 'video/x-msvideo', '3gp': 'video/3gpp', 'mpg': 'video/mpeg', 'mpeg': 'video/mpeg',
  'mp3': 'audio/mpeg', 'm4a': 'audio/mp4', 'aac': 'audio/aac', 'ogg': 'audio/ogg', 'oga': 'audio/ogg', 'opus': 'audio/ogg',
  'flac': 'audio/flac', 'wav': 'audio/wav', 'weba': 'audio/webm', 'pdf': 'application/pdf',
  'epub': 'application/epub+zip', 'apk': 'application/vnd.android.package-archive', 'zip': 'application/zip',
  'docx': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  'xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  'pptx': 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  'ttf': 'font/ttf', 'otf': 'font/otf', 'woff': 'font/woff', 'woff2': 'font/woff2',
};
const _source = {'rs', 'js', 'mjs', 'cjs', 'ts', 'tsx', 'jsx', 'py', 'go', 'c', 'h', 'cc', 'cpp', 'hpp', 'java', 'kt', 'rb', 'php', 'sh', 'fish', 'zsh', 'bash', 'lua', 'sql', 'swift', 'zig', 'hs', 'ml', 'css', 'scss', 'html', 'htm', 'nix', 'kdl', 'dart'};
const _named = {
  'txt': 'Text', 'log': 'Log', 'nfo': 'Text', 'md': 'Markdown', 'markdown': 'Markdown', 'mdown': 'Markdown', 'json': 'JSON', //
  'jsonl': 'JSON lines', 'csv': 'CSV table', 'tsv': 'TSV table', 'pdf': 'PDF document', 'diff': 'Patch', 'patch': 'Patch',
  'yaml': 'YAML', 'yml': 'YAML', 'toml': 'TOML', 'xml': 'XML', 'ini': 'Settings', 'conf': 'Settings', 'cfg': 'Settings', 'env': 'Settings',
  'srt': 'Subtitles', 'vtt': 'Subtitles', 'epub': 'EPUB book', 'docx': 'Word document', 'xlsx': 'Excel sheet', 'pptx': 'Slides',
  'odt': 'Document', 'ods': 'Spreadsheet', 'odp': 'Slides', 'apk': 'Android app', 'jar': 'Java archive',
};

/// The text kinds stop at 2 MB (`TEXT_MAX`); a bigger one shows its head.
const textMax = 2 << 20;

String extOf(String name) {
  final i = name.lastIndexOf('.');
  return i > 0 ? name.substring(i + 1).toLowerCase() : '';
}

/// image · video · audio · pdf · markdown · json · csv · text · archive · font · info
String kindOf(String name) => _by[extOf(name)] ?? _bare[name.toLowerCase()] ?? 'info';

String mimeOf(String name) => _mime[extOf(name)] ?? (const {'text', 'markdown', 'json', 'csv'}.contains(kindOf(name)) ? 'text/plain' : 'application/octet-stream');

/// The words for its type: "PDF document", "RS source", "MP4 video", "ZIP archive", "File".
String typeOf(String name) {
  final x = extOf(name), k = kindOf(name), up = x.toUpperCase();
  if (_named[x] != null) return _named[x]!;
  if (k == 'image' || k == 'video' || k == 'audio') return '$up $k';
  if (k == 'text') return _source.contains(x) ? '$up source' : (x.isNotEmpty ? '$up text' : 'Text');
  if (k == 'archive') return '$up archive';
  if (k == 'font') return '$up font';
  return x.isNotEmpty ? '$up file' : 'File';
}
