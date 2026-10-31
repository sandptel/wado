// Settings for the input surfaces: Touchpad (port of computer/input/touchpad.rs + pad_prefs.js —
// typed in the computer's config under `device "<this phone>" { trackpad { … } }`), and this
// phone's Input feel for the session (pages/input.rs).
import 'dart:convert';

import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/pad_actions.dart';
import '../../wado/settings.dart';
import '../../wado/session.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';
import '../widgets/fields.dart';
import '../widgets/icon_paths.dart';
import 'display_session.dart' show HeadCard;
import '../widgets/slider.dart';

const _padDefaults = <String, Object?>{
  'landscape': true, 'fullscreen': false, 'rail': 'keys', 'keyboard': false, //
  'speed': 1.0, 'scroll_speed': 1.0, 'natural': null, 'tap': true, 'tap_drag': true, 'drag_lock': 'off',
  'click_bar': false, 'palm': true, 'marks': true, 'gesture_distance': 'medium',
};

class TouchpadSettings extends StatelessWidget {
  const TouchpadSettings({super.key});

  static Map<String, Object?> prefs() {
    final own = Host.I.config.value?['trackpad'];
    return {..._padDefaults, if (own is Map) ...own.cast<String, Object?>()};
  }

  static void set(String field, Object? v) {
    final key = 'device.${CoreHost.I.clientKey}.trackpad.${field.replaceAll('_', '-')}';
    Host.I.configSet(key, v is bool ? (v ? 'true' : 'false') : (v is List ? jsonEncode(v) : v));
    final cfg = Host.I.config.value;
    if (cfg != null) {
      Host.I.config.value = {...cfg, 'trackpad': {...prefs(), field: v}};
    }
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Host.I.config,
      builder: (context, _) {
        final t = Tok.of(context);
        final p = prefs();
        final caps = ((Host.I.config.value?['caps'] as List?) ?? []).map((e) => '$e').toList();
        final hypr = caps.contains('pad-hypr');
        final host = CoreHost.I.computer(Host.I.config.value == null ? '' : '')?.name ?? 'the computer';
        bool b(String k) => p[k] == true;
        double d(String k) => ((p[k] ?? 1) as num).toDouble();
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          const Sect('How you hold it'),
          const SizedBox(height: 8),
          Seg(options: const [('true', 'Sideways'), ('false', 'Upright')], value: '${p['landscape']}', onPick: (v) => set('landscape', v == 'true')),
          const Sect('Beside the pad'),
          const SizedBox(height: 8),
          WCard(children: [
            Seg(options: const [('keys', 'Keys'), ('actions', 'Quick actions'), ('none', 'Nothing')], value: '${p['rail']}', onPick: (v) => set('rail', v)),
            SwitchRow(title: 'Open with the keyboard up', sub: 'The pad shrinks to a strip above it', on: b('keyboard'), onToggle: (v) => set('keyboard', v)),
          ]),
          const Sect('Quick actions'),
          const SizedBox(height: 8),
          _QuickActions(rail: '${p['rail']}'),
          Row(children: [const Expanded(child: Sect('Pointer and scrolling')), if (hypr) WhenBadge(When.host)]),
          const SizedBox(height: 8),
          WCard(children: [
            if (!hypr) Why('These are set by $host itself here — its compositor does not let wado change them for this phone.', tone: 'host'),
            ThickSlider(label: 'Pointer speed', value: d('speed'), min: .5, max: 2, step: .05, shown: '${d('speed').toStringAsFixed(2)}×', disabled: !hypr, onChange: (v) => set('speed', v)),
            const Why('Changes only this phone’s touchpad on the computer, which still accelerates faster slides.'),
            ThickSlider(label: 'Scroll speed', value: d('scroll_speed'), min: .25, max: 2, step: .05, shown: '${d('scroll_speed').toStringAsFixed(2)}×', disabled: !hypr, onChange: (v) => set('scroll_speed', v)),
            SwitchRow(title: 'Natural scrolling', sub: 'Content follows your fingers, as on a phone', on: b('natural'), disabled: !hypr, onToggle: (v) => set('natural', v)),
          ]),
          const Sect('Tapping'),
          const SizedBox(height: 8),
          WCard(children: [
            SwitchRow(title: 'Tap to click', sub: 'One finger taps a left click, two a right click', on: b('tap'), disabled: !hypr, onToggle: (v) => set('tap', v)),
            SwitchRow(title: 'Tap, then drag', sub: 'Tap and put the finger back down to drag', on: b('tap_drag'), disabled: !hypr, onToggle: (v) => set('tap_drag', v)),
            if (b('tap_drag'))
              Seg(options: const [('off', 'Off'), ('briefly', 'Briefly'), ('until-tap', 'Until you tap')], value: '${p['drag_lock']}', disabled: !hypr, onPick: (v) => set('drag_lock', v)),
            if (caps.contains('pad-click')) SwitchRow(title: 'Click bar', sub: 'A bar under the pad that clicks, for when taps do not', on: b('click_bar'), onToggle: (v) => set('click_bar', v)),
          ]),
          const Sect('Fingers'),
          const SizedBox(height: 8),
          WCard(children: [
            if (caps.contains('pad-palm')) SwitchRow(title: 'Ignore resting thumbs', sub: 'A thumb resting on the edge is not a finger', on: b('palm'), onToggle: (v) => set('palm', v)),
            SwitchRow(title: 'Finger marks', sub: 'A ring under each finger on this phone', on: b('marks'), onToggle: (v) {
              set('marks', v);
              Settings.I.set('pad_marks_off', !v);
            }),
          ]),
          const Sect('Phone gestures'),
          const SizedBox(height: 8),
          const _Gestures(),
          if (Host.I.config.value == null) Padding(padding: const EdgeInsets.only(top: 8), child: Text('Saved on the computer once it answers.', style: body(context, size: .75, color: t.dim))),
        ]);
      },
    );
  }
}

class InputFeel extends StatelessWidget {
  const InputFeel({super.key});
  // pages/input.rs `feel`: how a finger and a wheel feel on this phone.
  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Settings.I,
      builder: (context, _) {
        final s = Settings.I;
        void apply() {
          if (Session.I.on.value) Session.I.reconfigure();
        }

        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          HeadCard(title: 'A finger is', children: [
            Seg(options: const [('pointer', 'Gestures'), ('touch', 'Raw touch')], value: s.s('touch_mode'), onPick: (v) {
              s.set('touch_mode', v);
              apply();
            }),
            Why(s.s('touch_mode') == 'touch' ? 'Real multi-touch, for touch-native apps.' : 'Tap clicks, hold right-clicks, drag scrolls.'),
            SwitchRow(title: 'Move windows', sub: 'Dragging moves the window under the finger', on: s.b('move_mode'), onToggle: (v) {
              s.set('move_mode', v);
              apply();
            }),
          ]),
          const SizedBox(height: 12),
          HeadCard(children: [
            ThickSlider(label: 'Scroll speed', value: s.d('scroll_speed'), min: .05, max: 2, step: .05, shown: '${s.d('scroll_speed').toStringAsFixed(2)}×', onChange: (v) => s.set('scroll_speed', v)),
            SwitchRow(title: 'Natural scroll', sub: 'Content follows the finger', on: s.b('natural_scroll'), onToggle: (v) => s.set('natural_scroll', v)),
          ]),
        ]);
      },
    );
  }
}

class AboutPage extends StatelessWidget {
  const AboutPage({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      WCard(children: [
        Text('wado next', style: display(context, rem(context, 1.5))),
        Text('The native app — Flutter over wado’s Rust client core.', style: body(context, color: t.dim)),
        Text('AGPL-3.0-only. Fonts: Bricolage Grotesque, Figtree, JetBrains Mono (OFL). OS logos: Simple Icons (CC0), used only to name each computer’s OS.', style: body(context, size: .8125)),
      ]),
      const SizedBox(height: 8),
      Btn('Open-source licences', onTap: () => showLicensePage(context: context, applicationName: 'wado next')),
    ]);
  }
}

/// The touchpad's quick actions (pad_settings.js `quickSection`): the chosen ones in order with
/// up and down, then everything else the computer offers, by group.
class _QuickActions extends StatelessWidget {
  const _QuickActions({required this.rail});
  final String rail;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([PadActions.I.list, PadActions.I.denied]),
      builder: (context, _) {
        final all = PadActions.I.list.value;
        if (PadActions.I.denied.value.isNotEmpty) return WCard(children: [Why(PadActions.I.denied.value)]);
        if (all.isEmpty) {
          PadActions.I.refresh();
          return const WCard(children: [Why('Asking the computer what it can do…')]);
        }
        final quick = (TouchpadSettings.prefs()['quick'] as List?)?.cast<String>() ?? const ['workspace-prev', 'workspace-next', 'fullscreen', 'play-pause'];
        final chosen = [for (final id in quick.toSet()) if (PadActions.I.byId(id) != null) PadActions.I.byId(id)!];
        final ids = [for (final a in chosen) '${a['id']}'];
        void put(List<String> q) => TouchpadSettings.set('quick', q);
        Widget row(Map<String, dynamic> a, bool on, int i) => Padding(
              padding: const EdgeInsets.symmetric(vertical: 2),
              child: Row(children: [
                WIcon(iconPaths.containsKey(a['icon']) ? '${a['icon']}' : 'dots', color: t.accent),
                const SizedBox(width: 12),
                Expanded(child: Text('${a['name'] ?? a['id']}', style: body(context, w: FontWeight.w600, color: t.bright))),
                if (on) ...[
                  IconBtn('up', label: 'Move up: ${a['name']}', onTap: i == 0 ? null : () => put([...ids]..[i] = ids[i - 1]..[i - 1] = ids[i])),
                  IconBtn('down', label: 'Move down: ${a['name']}', onTap: i == ids.length - 1 ? null : () => put([...ids]..[i] = ids[i + 1]..[i + 1] = ids[i])),
                ],
                Sw(on: on, label: '${a['name']}', onChanged: (v) => put(v ? [...ids, '${a['id']}'] : ids.where((x) => x != a['id']).toList())),
              ]),
            );
        final groups = <String, List<Map<String, dynamic>>>{};
        for (final a in all) {
          if (!ids.contains(a['id'])) (groups['${a['group'] ?? 'More'}'] ??= []).add(a);
        }
        Widget head(String s) => Padding(padding: const EdgeInsets.only(top: 6, bottom: 4), child: Text(s, style: body(context, size: .75, w: FontWeight.w600, color: t.dim)));
        return WCard(children: [
          if (chosen.isNotEmpty) head(rail == 'actions' ? 'On the rail, in this order' : 'Chosen, in this order'),
          for (var i = 0; i < chosen.length; i++) row(chosen[i], true, i),
          for (final g in groups.entries) ...[head(g.key), for (final a in g.value) row(a, false, 0)],
        ]);
      },
    );
  }
}

/// pad_settings.js `gestureSection`: wado's own gestures, each off until it is given one of the
/// computer's actions, and how far they go.
class _Gestures extends StatelessWidget {
  const _Gestures();
  static const _all = [('edge-left', 'Swipe in from the left edge'), ('edge-right', 'Swipe in from the right edge'), ('four-tap', 'Tap with four fingers'), ('two-hold', 'Hold two fingers still')];

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Listenable.merge([PadActions.I.list, PadActions.I.denied]),
      builder: (context, _) {
        final p = TouchpadSettings.prefs();
        final mine = (p['gestures'] as Map?) ?? const {};
        final acts = PadActions.I.list.value;
        return WCard(children: [
          const Why("Off unless you pick one. Your computer's own touchpad gestures always come first."),
          if (acts.isNotEmpty)
            for (final (g, words) in _all)
              Choice(
                label: words,
                value: '${mine[g] ?? ''}',
                options: [('', 'Off'), for (final a in acts) ('${a['id']}', '${a['name']}')],
                onPick: (v) {
                  // Each its own key (`trackpad.gestures.<name>`); empty turns it off.
                  Host.I.configSet('device.${CoreHost.I.clientKey}.trackpad.gestures.$g', v);
                  final cfg = Host.I.config.value;
                  if (cfg != null) Host.I.config.value = {...cfg, 'trackpad': {...p, 'gestures': {...mine, g: v}}};
                },
              )
          else
            Why(PadActions.I.denied.value.isNotEmpty ? PadActions.I.denied.value : 'Open the touchpad once to see what this computer offers.'),
          Choice(
            label: 'How far a gesture goes',
            value: '${p['gesture_distance']}',
            options: const [('short', 'Short'), ('medium', 'Medium'), ('long', 'Long')],
            onPick: (v) => TouchpadSettings.set('gesture_distance', v),
          ),
          const Why("wado's own edge swipes and holds only. The computer's three- and four-finger gestures are its own."),
        ]);
      },
    );
  }
}
