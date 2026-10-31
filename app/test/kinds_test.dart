import 'package:flutter_test/flutter_test.dart';
import 'package:wado/ui/files/kinds.dart';

void main() {
  test('kinds, types and mimes by name', () {
    expect((kindOf('a.PDF'), typeOf('a.PDF'), mimeOf('a.pdf')), ('pdf', 'PDF document', 'application/pdf'));
    expect((kindOf('main.rs'), typeOf('main.rs')), ('text', 'RS source'));
    expect((kindOf('clip.mkv'), typeOf('clip.mkv')), ('video', 'MKV video'));
    expect((kindOf('Makefile'), kindOf('noext'), typeOf('noext')), ('text', 'info', 'File'));
    expect(typeOf('x.docx'), 'Word document');
    expect(mimeOf('notes.md'), 'text/plain');
  });
}
