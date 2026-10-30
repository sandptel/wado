// The clipboard (port of js/cliphost.js + clip_share.js, text): the computer's clipboard history
// (`clip_list` → `clip_history`; one in full with `clip_get` → `clip_data`), this phone's
// clipboard sent there (`clip_push`), and the computer's copies arriving live (`clip_watch` →
// `clip_now`) to copy here.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'relay_link.dart';

class Clip {
  Clip._() {
    final l = RelayLink.I;
    l.on('clip_history', (m) {
      available.value = m['available'] == true;
      entries.value = (m['entries'] as List? ?? []).cast<Map<String, dynamic>>();
      error.value = (m['error'] ?? '') as String;
    });
    l.on('clip_data', (m) async {
      final mime = '${m['mime']}';
      final data = '${m['data']}';
      if (mime.startsWith('text/')) {
        await Clipboard.setData(ClipboardData(text: data));
        said.value = 'Copied to this phone';
      } else if (mime.startsWith('image/')) {
        images.value = {...images.value, '${m['id']}': data};
      }
    });
    l.on('clip_now', (m) async {
      if (!live.value) return;
      final mime = '${m['mime']}';
      if (mime.startsWith('text/')) {
        await Clipboard.setData(ClipboardData(text: '${m['data']}'));
        said.value = 'Copied on ${m['from'] ?? 'the computer'} — on this phone too';
      }
    });
    l.on('clip_pushed', (_) => said.value = 'Sent to the computer’s clipboard');
    l.on('clip_error', (m) => said.value = (m['message'] ?? "The computer's clipboard did not answer.") as String);
    l.on('clip_held', (m) => said.value = (m['message'] ?? '') as String);
    l.on('clipboard_sent', (_) => said.value = 'On the session’s clipboard');
    l.on('__up', (_) {
      if (live.value) l.send({'type': 'clip_watch', 'live': true});
    });
  }
  static final I = Clip._();

  final available = ValueNotifier<bool>(false);
  final entries = ValueNotifier<List<Map<String, dynamic>>>([]);
  final error = ValueNotifier<String>('');
  final said = ValueNotifier<String>('');
  final live = ValueNotifier<bool>(false);
  final images = ValueNotifier<Map<String, String>>({});

  void list() => RelayLink.I.send({'type': 'clip_list'});
  void get(String id) => RelayLink.I.send({'type': 'clip_get', 'id': id});
  void pin(String id, bool on) => RelayLink.I.send({'type': 'clip_pin', 'id': id, 'on': on});
  void delete(String id) => RelayLink.I.send({'type': 'clip_delete', 'id': id});

  void watch(bool on) {
    live.value = on;
    RelayLink.I.send({'type': 'clip_watch', 'live': on});
  }

  /// This phone's clipboard, to the computer's.
  Future<void> sendPhone() async {
    final d = await Clipboard.getData('text/plain');
    final text = d?.text ?? '';
    if (text.isEmpty) {
      said.value = "This phone's clipboard is empty";
      return;
    }
    if (utf8.encode(text).length > 256 * 1024) {
      said.value = 'That text is too long to send';
      return;
    }
    if (!RelayLink.I.send({'type': 'clip_push', 'mime': 'text/plain', 'data': text, 'sensitive': false})) {
      said.value = 'Not connected to the computer right now';
    }
  }
}
