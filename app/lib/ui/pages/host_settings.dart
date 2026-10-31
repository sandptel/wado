// What the computer holds for every device (port of pages/host.rs + pages/input.rs `swipes`,
// `keyboard`): stream caps, input pinned for everyone, three-finger swipes, the compositor's
// shortcuts, keyboard repeat — and what only the owner may change. Every change goes to the
// daemon (`config_set`); live keys any trusted device may set, owner keys ask first.
import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';
import '../widgets/fields.dart';
import 'display_session.dart' show HeadCard;

Map<String, dynamic> get _c => Host.I.config.value ?? const {};
void _set(String key, Object? v, {bool confirmed = false}) => Host.I.configSet(key, v, confirmed: confirmed);

/// The file's last reload failed, a restart is owed, or the daemon refused a change.
class HostNotes extends StatelessWidget {
  const HostNotes({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final err = (_c['error'] ?? '') as String;
    final restart = (_c['restart'] as List? ?? []).cast<String>();
    final note = Host.I.note.value;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (err.isNotEmpty) ...[
        WCard(alert: true, children: [
          Text('config.kdl has a mistake', style: body(context, w: FontWeight.w600, color: t.bad)),
          Text(err, style: mono(context, color: t.bright)),
          const Why('The daemon is still running on the last good version.'),
        ]),
        const SizedBox(height: 12),
      ],
      if (restart.isNotEmpty) ...[WCard(children: [Why('Restart the daemon to apply: ${restart.join(', ')}')]), const SizedBox(height: 12)],
      if (note.isNotEmpty) ...[WCard(alert: true, children: [Text(note, style: body(context, color: t.bright))]), const SizedBox(height: 12)],
    ]);
  }
}

/// Stream caps every device's session is held inside, the encoder it must use, and input pinned
/// for everyone.
class HostLimits extends StatelessWidget {
  const HostLimits({super.key});
  @override
  Widget build(BuildContext context) {
    final l = (_c['limits'] as Map?) ?? const {};
    final input = (_c['input'] as Map?) ?? const {};
    String n(Object? v) => v == null ? '' : '$v';
    return HeadCard(title: 'Limits', when: When.host, children: [
      Text('Stream caps', style: body(context, w: FontWeight.w600, color: Tok.of(context).bright)),
      const Why("Every device's session is held inside these. Empty means no limit."),
      Field(label: 'Frame rate', unit: 'fps', hint: 'no limit', value: n(l['max_fps']), onCommit: (v) => _set('stream.max-fps', v)),
      Field(label: 'Bitrate', unit: 'kbps', hint: 'no limit', value: n(l['max_bitrate']), onCommit: (v) => _set('stream.max-bitrate', v)),
      Field(label: 'Width', unit: 'px', hint: 'no limit', value: n(l['max_width']), onCommit: (v) => _set('stream.max-width', v)),
      Field(label: 'Height', unit: 'px', hint: 'no limit', value: n(l['max_height']), onCommit: (v) => _set('stream.max-height', v)),
      Choice(
        label: 'Encoder',
        value: switch (l['encoder']) { 'hardware' => 'hardware', 'software' => 'software', _ => 'auto' },
        options: const [('auto', 'Each device chooses'), ('hardware', 'Always GPU'), ('software', 'Always x264')],
        onPick: (v) => _set('stream.encoder', v),
      ),
      SwitchRow(
        title: 'Pin focus-follows-pointer',
        sub: input['focus_follows_pointer'] != null ? 'Pinned for every device' : 'Each device chooses',
        on: input['focus_follows_pointer'] != null,
        onToggle: (v) => _set('input.focus-follows-pointer', v ? 'true' : ''),
      ),
    ]);
  }
}

const _gestureActions = [
  ('app-drawer', 'App drawer'), ('control-centre', 'Control centre'), ('keyboard', 'Keyboard'), ('back', 'Back'), ('focus-next', 'Next window'),
  ('maximize', 'Maximize'), ('minimize', 'Minimize'), ('close-window', 'Close window'), ('none', 'Nothing'),
];

/// What three-finger swipes do — saved on the computer, for every device.
class HostSwipes extends StatelessWidget {
  const HostSwipes({super.key});
  @override
  Widget build(BuildContext context) {
    final g = (_c['gestures'] as Map?) ?? const {};
    return HeadCard(title: 'Three-finger swipes', when: When.host, children: [
      for (final dir in const ['up', 'down', 'left', 'right'])
        Choice(label: 'Swipe $dir', value: '${g['swipe-3-$dir'] ?? ''}', options: _gestureActions, onPick: (v) => _set('gestures.swipe-3-$dir', v)),
      const Why('Saved on this computer for every device. Gesture mode only — raw touch passes three fingers to the app.'),
    ]);
  }
}

/// Keys the compositor keeps for itself.
class HostShortcuts extends StatefulWidget {
  const HostShortcuts({super.key});
  @override
  State<HostShortcuts> createState() => _HostShortcutsState();
}

class _HostShortcutsState extends State<HostShortcuts> {
  final _combo = TextEditingController();
  String _action = 'close-window';
  @override
  void dispose() {
    _combo.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final mod = (_c['bind_mod'] ?? 'super') as String;
    final binds = (_c['binds'] as Map? ?? {}).cast<String, dynamic>();
    final rules = (_c['window_rules'] ?? 0) as int;
    return HeadCard(title: 'Shortcuts', when: When.host, children: [
      Why('Keys the compositor keeps for itself. Mod is $mod — a browser cannot always capture Super, so ctrl+alt is the fallback.'),
      Choice(label: 'Mod is', value: mod, options: const [('super', 'super'), ('alt', 'alt'), ('ctrl', 'ctrl'), ('ctrl+alt', 'ctrl+alt')], onPick: (v) => _set('binds.mod', v)),
      for (final e in binds.entries)
        Row(children: [
          Text(e.key, style: mono(context, w: FontWeight.w600, color: t.bright)),
          const SizedBox(width: 10),
          Expanded(child: Text('${e.value}', style: body(context, size: .8125, color: t.dim))),
          IconBtn('x', label: 'Remove ${e.key}', onTap: () => _set('binds.${e.key}', '')),
        ]),
      Row(children: [
        Expanded(
          child: TextField(
            controller: _combo,
            onChanged: (_) => setState(() {}),
            style: mono(context, size: 1, color: t.bright),
            decoration: InputDecoration(isDense: true, hintText: 'Mod+Q', filled: true, fillColor: t.s0, border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1))),
          ),
        ),
        const SizedBox(width: 8),
        Expanded(
          child: Choice(
            value: _action,
            options: const [('close-window', 'close-window'), ('maximize', 'maximize'), ('minimize', 'minimize'), ('focus-next', 'focus-next'), ('back', 'back')],
            onPick: (v) => setState(() => _action = v),
          ),
        ),
      ]),
      Btn('Add shortcut', onTap: _combo.text.trim().isEmpty
          ? null
          : () {
              _set('binds.${_combo.text.trim()}', _action);
              setState(() => _combo.clear());
            }),
      if (rules > 0) Why('$rules window rule(s) in config.kdl.'),
    ]);
  }
}

/// Keyboard repeat and focus policy: this phone's choice at Start, unless the computer pins it.
class KeyboardRepeat extends StatelessWidget {
  const KeyboardRepeat({super.key});
  @override
  Widget build(BuildContext context) {
    final s = Settings.I;
    final pin = (_c['input'] as Map?) ?? const {};
    final pinnedRepeat = pin['repeat_rate'] != null || pin['repeat_delay'] != null;
    return HeadCard(title: 'Keyboard repeat', when: pinnedRepeat ? When.host : When.apply, children: [
      Field(label: 'keys / s', value: '${pin['repeat_rate'] ?? s.i('repeat_rate')}', disabled: pin['repeat_rate'] != null, onCommit: (v) {
        final n = int.tryParse(v);
        if (n != null) s.set('repeat_rate', n);
      }),
      Field(label: 'delay ms', value: '${pin['repeat_delay'] ?? s.i('repeat_delay')}', disabled: pin['repeat_delay'] != null, onCommit: (v) {
        final n = int.tryParse(v);
        if (n != null) s.set('repeat_delay', n);
      }),
      SwitchRow(
        title: 'Focus follows pointer',
        sub: 'Hovering a window focuses it',
        on: (pin['focus_follows_pointer'] ?? s.b('focus_follows')) == true,
        disabled: pin['focus_follows_pointer'] != null,
        when: pin['focus_follows_pointer'] != null ? When.host : When.apply,
        onToggle: (v) => s.set('focus_follows', v),
      ),
    ]);
  }
}

/// What only the owner may change: shells, autostart, environment, trust. Each asks first.
class OwnerOnly extends StatefulWidget {
  const OwnerOnly({super.key});
  @override
  State<OwnerOnly> createState() => _OwnerOnlyState();
}

class _OwnerOnlyState extends State<OwnerOnly> {
  (String, String)? _confirm;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final owner = _c['owner'] == true;
    return HeadCard(title: 'Owner only', children: [
      if (!owner)
        const Why('Shells, autostart, environment and trust can only be changed from the owner device — the first one trusted — or in config.kdl.')
      else ...[
        const Why('These run code or change who can connect. Each asks first.'),
        SwitchRow(title: 'Shells', sub: 'Allow terminals on this computer', on: _c['shells'] != false, onToggle: (v) => setState(() => _confirm = ('shells.enabled', '$v'))),
        if (_confirm != null) ...[
          Text('Change ${_confirm!.$1} to ${_confirm!.$2} on this computer?', style: body(context, color: t.bright)),
          Row(children: [
            Expanded(child: Btn('Cancel', onTap: () => setState(() => _confirm = null))),
            const SizedBox(width: 8),
            Expanded(
              child: Btn('Change it', primary: true, onTap: () {
                _set(_confirm!.$1, _confirm!.$2, confirmed: true);
                setState(() => _confirm = null);
              }),
            ),
          ]),
        ],
      ],
    ]);
  }
}
