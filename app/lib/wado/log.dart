// The client's log (js/logs.js's W.rlog) and the status line: shown in Diagnostics, kept short.
import 'package:flutter/foundation.dart';

class LogLine {
  LogLine(this.at, this.text);
  final DateTime at;
  final String text;
}

final logs = ValueNotifier<List<LogLine>>([]);
final status = ValueNotifier<String>('');

void rlog(String s) {
  debugPrint('wado: $s');
  final l = [...logs.value, LogLine(DateTime.now(), s)];
  logs.value = l.length > 400 ? l.sublist(l.length - 400) : l;
}

void setStatus(String s) => status.value = s;
