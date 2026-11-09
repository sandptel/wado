import 'package:flutter_test/flutter_test.dart';
import 'package:wado/wado/files_ops.dart';
import 'package:wado/wado/files_view.dart';

Map<String, dynamic> f(String name, {bool dir = false, int size = 0, int mtime = 0}) => {'name': name, 'dir': dir, 'size': size, 'mtime': mtime};

void main() {
  final list = [f('b.txt', size: 3, mtime: 5), f('A.png', size: 9, mtime: 1), f('zdir', dir: true, mtime: 9), f('.hidden'), f('c.png', size: 1, mtime: 3)];
  List<String> names(List<Map<String, dynamic>> l) => [for (final e in l) '${e['name']}'];

  test('name, folders first, hidden left out; any case', () {
    expect(names(arrange(list, const FilesView())), ['zdir', 'A.png', 'b.txt', 'c.png']);
  });
  test('size and date, either way; folders stay first unless mixed in', () {
    expect(names(arrange(list, const FilesView(sort: 'size', desc: true))), ['zdir', 'A.png', 'b.txt', 'c.png']);
    expect(names(arrange(list, const FilesView(sort: 'mtime', dirsFirst: false))), ['A.png', 'c.png', 'b.txt', 'zdir']);
  });
  test('type groups by extension, then name', () {
    expect(names(arrange(list, const FilesView(sort: 'type', dirsFirst: false))), ['zdir', 'A.png', 'c.png', 'b.txt']);
  });
  test('hidden shown when asked; a filter narrows by name', () {
    expect(names(arrange(list, const FilesView(hidden: true))).contains('.hidden'), isTrue);
    expect(names(arrange(list, const FilesView(), filter: 'PNG')), ['A.png', 'c.png']);
  });
  test('extensions', () {
    expect([extOf('a.TAR.GZ'), extOf('.bashrc'), extOf('noext')], ['gz', '', '']);
  });
  test('undo: a move goes back to each item\'s own folder, a copy to the trash', () {
    String show(List<(String, Map<String, dynamic>)> p) => [for (final (op, args) in p) '$op $args'].join(' | ');
    expect(show(undoPlan('move', ['/a/x.txt', '/b/y.txt', '/a/z'], ['/d/x.txt', '/d/y (1).txt', '/d/z'])),
        'move {paths: [/d/x.txt, /d/z], dest: /a, clash: rename} | move {paths: [/d/y (1).txt], dest: /b, clash: rename}');
    expect(show(undoPlan('copy', ['/a/x'], ['/d/x (1)'])), 'trash {paths: [/d/x (1)]}');
    expect(parentOf('/x'), '/');
  });
}
