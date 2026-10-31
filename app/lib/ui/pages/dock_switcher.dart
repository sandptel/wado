// Dock & switcher (port of ui/pages/arrangement.rs + switcher.rs): how a session opens and which
// input face its deck starts on; how windows are laid out; and the chrome that manages them — the
// workspace bar, the dock's fade, edge swipes and the quick rail.
import 'package:flutter/material.dart';

import '../../wado/session.dart';
import '../../wado/settings.dart';
import '../widgets/basics.dart';
import 'display_session.dart' show HeadCard;

class DockSwitcher extends StatelessWidget {
  const DockSwitcher({super.key});
  @override
  Widget build(BuildContext context) {
    final s = Settings.I;
    final open = s.s('console_open') == 'full' ? 'full' : 'console';
    final face = switch (s.s('console_face')) { 'touchpad' => 'pad', '' => 'auto', final f => f };
    final place = s.s('placement');
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      HeadCard(title: 'Open sessions in', children: [
        Seg(options: const [('console', 'Console'), ('full', 'Full screen')], value: open, onPick: (v) => s.set('console_open', v)),
        Hint(open == 'full' ? 'The picture fills the screen and the controls float over it.' : 'The picture sits inset, with the controls around it. Full screen is one tap away.'),
      ]),
      const SizedBox(height: 12),
      HeadCard(title: 'Input face', children: [
        Seg(
          options: const [('auto', 'Auto'), ('pad', 'Touchpad'), ('keys', 'Keys'), ('quick', 'Quick'), ('gamepad', 'Gamepad')],
          value: face,
          onPick: (v) => s.set('console_face', v == 'pad' ? 'touchpad' : v),
        ),
        Hint(switch (face) {
          'pad' => 'A touchpad under the picture moves the computer\'s pointer.',
          'keys' => 'Super, Ctrl, Alt, Shift, Esc, Tab and the arrows under the picture.',
          'quick' => 'Workspaces and playback under the picture.',
          'gamepad' => 'The on-screen gamepad under the picture.',
          _ => 'The touchpad on a touchscreen, quick actions with a mouse.',
        }),
      ]),
      const SizedBox(height: 12),
      HeadCard(title: 'Layout', children: [
        Seg(
          options: const [('auto', 'Auto'), ('strip', 'Sliding'), ('tile', 'Tiling'), ('maximized', 'Full'), ('center', 'Floating')],
          value: const {'cascade', 'top_left'}.contains(place) ? 'center' : place,
          onPick: (v) {
            s.set('placement', v);
            // Applied at once when a session runs: a layout is something to try and see.
            Session.I.reconfigure();
          },
        ),
        Hint(switch (place) {
          'strip' => 'Columns side by side; swipe the strip to move between them.',
          'tile' => 'Every window on the workspace shares the screen.',
          'maximized' => 'Each window fills the screen; the workspace bar switches.',
          'center' || 'cascade' || 'top_left' => 'Windows float where they open; drag them by the title.',
          _ => 'Sliding on a phone, floating on a big screen.',
        }),
      ]),
      const SizedBox(height: 12),
      HeadCard(children: [
        SwitchRow(title: 'Workspace bar', sub: 'Numbered workspaces and their apps, left of the dock', on: s.b('ws_bar'), onToggle: (v) => s.set('ws_bar', v)),
        SwitchRow(title: 'Pin the dock', sub: 'Keep it on screen instead of fading after 2 s', on: s.b('dock_pin'), onToggle: (v) => s.set('dock_pin', v)),
        SwitchRow(title: 'Edge swipes', sub: 'Swipe in from a side edge for the next workspace', on: s.b('edge_swipe'), onToggle: (v) => s.set('edge_swipe', v)),
      ]),
      const SizedBox(height: 12),
      HeadCard(title: 'Quick rail', children: [
        Seg(options: const [('left', 'Left'), ('right', 'Right'), ('off', 'Off')], value: s.s('rail'), onPick: (v) => s.set('rail', v)),
        const Hint('Rotate, fullscreen and the keyboard, on the screen\'s edge.'),
      ]),
      const SizedBox(height: 12),
      const HeadCard(title: 'Moving windows', children: [
        Hint('Hold an app\'s icon in the workspace bar: move it to another workspace, or to a new one. Tap a number to switch; + opens an empty workspace.'),
      ]),
    ]);
  }
}
