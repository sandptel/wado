// Settings for the input surfaces: Touchpad (port of computer/input/touchpad.rs + pad_prefs.js —
// typed in the computer's config under `device "<this phone>" { trackpad { … } }`), and this
// phone's Input feel for the session (pages/input.rs).
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/settings.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';
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
    Host.I.configSet(key, v is bool ? (v ? 'true' : 'false') : v);
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
          if (Host.I.config.value == null) Padding(padding: const EdgeInsets.only(top: 8), child: Text('Saved on the computer once it answers.', style: body(context, size: .75, color: t.dim))),
        ]);
      },
    );
  }
}

class InputFeel extends StatelessWidget {
  const InputFeel({super.key});
  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Settings.I,
      builder: (context, _) {
        final s = Settings.I;
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          const Sect('Touch on the picture'),
          const SizedBox(height: 8),
          Seg(options: const [('pointer', 'Gestures'), ('touch', 'Raw touch')], value: s.s('touch_mode'), onPick: (v) => s.set('touch_mode', v)),
          const SizedBox(height: 6),
          Why(s.s('touch_mode') == 'touch'
              ? 'Fingers reach the app as touch, for apps made for it.'
              : 'Tap clicks, drag scrolls, hold right-clicks, hold and drag drags — the same in every app.'),
          const Sect('Scrolling'),
          const SizedBox(height: 8),
          WCard(children: [
            ThickSlider(label: 'Scroll speed', value: s.d('scroll_speed'), min: .1, max: 2, step: .05, shown: '${s.d('scroll_speed').toStringAsFixed(2)}×', onChange: (v) => s.set('scroll_speed', v)),
            SwitchRow(title: 'Natural scrolling', sub: 'Content follows your finger', on: s.b('natural_scroll'), onToggle: (v) => s.set('natural_scroll', v)),
          ]),
          const Sect('Edges'),
          const SizedBox(height: 8),
          SwitchRow(title: 'Edge swipes', sub: 'Swipe in from the side for the dock and the control centre', on: s.b('edge_swipe'), onToggle: (v) => s.set('edge_swipe', v)),
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
