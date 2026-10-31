// Markdown, read (port of js/viewer/markdown.js): headings, paragraphs, lists with task boxes,
// quotes, fenced code, rules, and inline bold, italic, code and links. Pictures are named, not
// fetched — a document from the computer does not reach out to the internet from here.
import 'package:flutter/material.dart';

import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';

final _inline = RegExp(r'(\*\*|__)(.+?)\1|(\*)(?!\s)(.+?)\3|`([^`]+)`|!?\[([^\]]*)\]\(([^)\s]+)[^)]*\)');

List<InlineSpan> _spans(BuildContext c, String text, TextStyle base) {
  final t = Tok.of(c);
  final out = <InlineSpan>[];
  var i = 0;
  for (final m in _inline.allMatches(text)) {
    if (m.start > i) out.add(TextSpan(text: text.substring(i, m.start)));
    if (m[2] != null) {
      out.add(TextSpan(style: base.copyWith(fontWeight: FontWeight.w700, color: t.bright), children: _spans(c, m[2]!, base)));
    } else if (m[4] != null) {
      out.add(TextSpan(style: base.copyWith(fontStyle: FontStyle.italic), children: _spans(c, m[4]!, base)));
    } else if (m[5] != null) {
      out.add(TextSpan(text: ' ${m[5]} ', style: TextStyle(fontFamily: Tokens.mono, fontSize: rem(c, .8125), color: t.bright, backgroundColor: t.s2)));
    } else if (m[0]!.startsWith('!')) {
      out.add(TextSpan(text: '[picture: ${m[6]!.isEmpty ? m[7] : m[6]}]', style: base.copyWith(color: t.dim)));
    } else {
      out.add(TextSpan(style: base.copyWith(color: t.accent), children: _spans(c, m[6]!.isEmpty ? m[7]! : m[6]!, base)));
    }
    i = m.end;
  }
  if (i < text.length) out.add(TextSpan(text: text.substring(i)));
  return out;
}

class MarkdownView extends StatelessWidget {
  const MarkdownView(this.src, {super.key});
  final String src;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final base = body(context, size: .9375, color: t.text).copyWith(height: 1.65);
    Widget para(String s, [TextStyle? st]) => Text.rich(TextSpan(style: st ?? base, children: _spans(context, s, st ?? base)));
    final out = <Widget>[];
    final lines = src.replaceAll(RegExp(r'\r\n?'), '\n').split('\n');
    final p = <String>[];
    void flush() {
      if (p.isNotEmpty) out.add(Padding(padding: const EdgeInsets.only(bottom: 14), child: para(p.join(' '))));
      p.clear();
    }

    for (var i = 0; i < lines.length; i++) {
      final l = lines[i];
      RegExpMatch? m;
      if ((m = RegExp(r'^\s*(```|~~~)').firstMatch(l)) != null) {
        flush();
        final code = <String>[];
        while (++i < lines.length && !lines[i].trim().startsWith(m![1]!)) {
          code.add(lines[i]);
        }
        out.add(Container(
          margin: const EdgeInsets.only(bottom: 16),
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
          decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r2)),
          child: SingleChildScrollView(scrollDirection: Axis.horizontal, child: Text(code.join('\n'), style: mono(context, size: .8125, color: t.bright))),
        ));
      } else if ((m = RegExp(r'^(#{1,6})\s+(.*?)\s*#*\s*$').firstMatch(l)) != null) {
        flush();
        final lv = m![1]!.length + 1;
        final size = lv == 2 ? 1.625 : (lv == 3 ? 1.25 : 1.0);
        out.add(Padding(padding: EdgeInsets.only(top: out.isEmpty ? 0 : 20, bottom: 6), child: para(m[2]!, display(context, rem(context, size), ls: -.16).copyWith(height: 1.2))));
      } else if (RegExp(r'^\s*([-*_])(\s*\1){2,}\s*$').hasMatch(l)) {
        flush();
        out.add(Divider(height: 40, color: t.line));
      } else if ((m = RegExp(r'^\s*>\s?(.*)$').firstMatch(l)) != null) {
        flush();
        final q = [m![1]!];
        while (i + 1 < lines.length && RegExp(r'^\s*>').hasMatch(lines[i + 1])) {
          q.add(lines[++i].replaceFirst(RegExp(r'^\s*>\s?'), ''));
        }
        out.add(Container(
          margin: const EdgeInsets.only(bottom: 16),
          padding: const EdgeInsets.fromLTRB(14, 4, 0, 4),
          decoration: BoxDecoration(border: Border(left: BorderSide(color: t.accent, width: 3))),
          child: para(q.join(' ')),
        ));
      } else if ((m = RegExp(r'^\s*([-*+]|\d+[.)])\s+(\[[ xX]\]\s+)?(.*)$').firstMatch(l)) != null) {
        flush();
        final ordered = RegExp(r'\d').hasMatch(m![1]!);
        final box = m[2] == null ? '' : (RegExp('[xX]').hasMatch(m[2]!) ? '☑ ' : '☐ ');
        out.add(Padding(
          padding: const EdgeInsets.only(bottom: 4),
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            SizedBox(width: 22, child: Text(ordered ? m[1]! : '•', style: base)),
            Expanded(child: para('$box${m[3]}')),
          ]),
        ));
      } else if (l.trim().isEmpty) {
        flush();
      } else {
        p.add(l.trim());
      }
    }
    flush();
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 640),
        child: Padding(padding: const EdgeInsets.fromLTRB(20, 18, 20, 40), child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: out)),
      ),
    );
  }
}
