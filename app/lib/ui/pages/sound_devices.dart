// Sound & devices (port of computer/pages/sound_devices.rs + pages/sound.rs + pages/network.rs
// `bluetooth`): the computer's mixer, pavucontrol-style — everything playing, each with its own
// volume, mute and output; every output with its volume and which is the default — and its
// Bluetooth devices. Works with no session: only the relay link is needed.
import 'dart:io';

import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../../wado/session.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../widgets/slider.dart';

/// A sink label short enough for a chip.
String _short(String label) {
  final l = label.replaceAll('High Definition Audio Controller', '').replaceAll('HD Audio Controller', '').trim();
  return l.length > 22 ? '${l.substring(0, 21)}…' : l;
}

String get _here => 'This device · ${Platform.isAndroid ? 'Android' : 'Linux'} · app';

class SoundDevices extends StatelessWidget {
  const SoundDevices({super.key});
  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Host.I.state,
      builder: (context, _) {
        final t = Tok.of(context);
        final h = Host.I.state.value;
        if (h == null) return WCard(children: [const Why('Waiting for the computer…')]);
        final a = (h['audio'] as Map?)?.cast<String, dynamic>() ?? {};
        final sinks = [for (final s in (a['sinks'] as List? ?? [])) Map<String, dynamic>.from(s as Map)];
        final streams = [for (final s in (a['streams'] as List? ?? [])) Map<String, dynamic>.from(s as Map)];
        final phone = a['phone_sink'] as String?;
        final chips = [for (final s in sinks) ('${s['name']}', s['name'] == phone ? _here : _short('${s['label']}'))];
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          const Sect('Playing now'),
          const SizedBox(height: 8),
          if (streams.isEmpty) WCard(children: [const Why('Nothing is playing on the computer.')]),
          for (final st in streams) ...[
            WCard(children: [
              _Head(icon: 'sound', hue: t.base[0xE], title: '${st['app']}', sub: '${st['title'] ?? ''}' == '${st['app']}' ? '' : '${st['title'] ?? ''}'),
              _VolRow(
                muted: st['muted'] == true,
                volume: ((st['volume'] ?? 0) as num).toDouble(),
                label: 'Volume of ${st['app']}',
                onMute: () => Host.I.act({'do': 'stream_mute', 'id': st['id'], 'muted': st['muted'] != true}),
                onVolume: (v) => Host.I.act({'do': 'stream_volume', 'id': st['id'], 'volume': v}),
              ),
              // Chips that wrap, not a segmented row: a computer can have half a dozen outputs,
              // and every name has to stay readable.
              if (chips.length > 1)
                Wrap(spacing: 6, runSpacing: 6, children: [
                  for (final (name, label) in chips)
                    _Chip(
                      icon: name == phone ? 'phone' : 'monitor',
                      label: label,
                      on: st['sink'] == name,
                      onTap: () {
                        if (name == phone) Session.I.listen();
                        Host.I.act({'do': 'stream_to', 'id': st['id'], 'sink': name});
                      },
                    ),
                ]),
            ]),
            const SizedBox(height: 8),
          ],
          const Sect('Outputs'),
          const SizedBox(height: 8),
          for (final s in sinks) ...[
            WCard(children: [
              _Head(
                icon: s['name'] == phone ? 'phone' : 'monitor',
                hue: t.base[0xD],
                title: '${s['label']}',
                trailing: s['name'] == a['default_sink']
                    ? const _Badge('default')
                    : Press(
                        onTap: () => Host.I.act({'do': 'default_sink', 'name': s['name']}),
                        child: Padding(padding: const EdgeInsets.all(8), child: Text('Make default', style: body(context, size: .8125, w: FontWeight.w600, color: t.accent))),
                      ),
              ),
              _VolRow(
                muted: s['muted'] == true,
                volume: ((s['volume'] ?? 0) as num).toDouble(),
                label: 'Volume of ${s['label']}',
                onMute: () => Host.I.act({'do': 'sink_mute', 'id': s['id'], 'muted': s['muted'] != true}),
                onVolume: (v) => Host.I.act({'do': 'sink_volume', 'id': s['id'], 'volume': v}),
              ),
            ]),
            const SizedBox(height: 8),
          ],
          const Why('"This device" plays on the device you are holding. Moving an app there sends its sound here instead of out of the computer.'),
          const Sect('Bluetooth'),
          const SizedBox(height: 8),
          const Bluetooth(),
        ]);
      },
    );
  }
}

/// `.streamhead`: a hue disc, the name, its line; a badge or action at the end.
class _Head extends StatelessWidget {
  const _Head({required this.icon, required this.hue, required this.title, this.sub = '', this.trailing});
  final String icon, title, sub;
  final Color hue;
  final Widget? trailing;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Row(children: [
      Disc(hue: hue, child: WIcon(icon, color: hue)),
      const SizedBox(width: 12),
      Expanded(
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(title, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, w: FontWeight.w600, color: t.bright)),
          if (sub.isNotEmpty) Text(sub, maxLines: 1, overflow: TextOverflow.ellipsis, style: body(context, size: .75, color: t.dim)),
        ]),
      ),
      ?trailing,
    ]);
  }
}

class _Badge extends StatelessWidget {
  const _Badge(this.text);
  final String text;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3),
      decoration: BoxDecoration(color: fade(t.accent, 86), borderRadius: BorderRadius.circular(999)),
      child: Text(text.toUpperCase(), style: body(context, size: .5938, w: FontWeight.w600, color: t.accent).copyWith(letterSpacing: .8)),
    );
  }
}

/// `.volrow`: mute (tinted when on) and a thin, quiet volume slider.
class _VolRow extends StatelessWidget {
  const _VolRow({required this.muted, required this.volume, required this.label, required this.onMute, required this.onVolume});
  final bool muted;
  final double volume;
  final String label;
  final VoidCallback onMute;
  final ValueChanged<double> onVolume;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return Row(children: [
      Press(
        onTap: onMute,
        label: muted ? 'Unmute' : 'Mute',
        child: AnimatedContainer(
          duration: t.dur,
          width: 44,
          height: 44,
          decoration: BoxDecoration(shape: BoxShape.circle, color: muted ? fade(t.bad, 86) : null),
          alignment: Alignment.center,
          child: WIcon(muted ? 'mute' : 'sound', color: muted ? t.bad : t.bright),
        ),
      ),
      const SizedBox(width: 6),
      Expanded(child: ThickSlider(label: label, shown: ' ', thin: true, value: volume.clamp(0, 1), onChange: onVolume)),
    ]);
  }
}

class _Chip extends StatelessWidget {
  const _Chip({required this.icon, required this.label, required this.on, required this.onTap});
  final String icon, label;
  final bool on;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final fg = on ? t.onAccent : t.text;
    return Press(
      onTap: onTap,
      label: label,
      child: AnimatedContainer(
        duration: t.dur,
        height: 36,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(color: on ? t.accent : t.s2, borderRadius: BorderRadius.circular(999)),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          WIcon(icon, size: 16, color: fg),
          const SizedBox(width: 6),
          Text(label, style: body(context, size: .8125, w: FontWeight.w600, color: fg)),
        ]),
      ),
    );
  }
}

/// Bluetooth (network.rs `bluetooth`): on or off, and the paired devices.
class Bluetooth extends StatelessWidget {
  const Bluetooth({super.key});

  static String _icon(String i) => i.startsWith('audio') ? 'sound' : (i.contains('gaming') ? 'pad' : (i.startsWith('input') ? 'kbd' : 'bt'));

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final b = Host.I.state.value?['bluetooth'] as Map?;
    if (b == null) return const SizedBox.shrink();
    final devices = [for (final d in (b['devices'] as List? ?? [])) Map<String, dynamic>.from(d as Map)];
    final connected = devices.where((d) => d['connected'] == true).toList();
    final powered = b['powered'] == true;
    return WCard(children: [
      SwitchRow(
        title: 'Bluetooth',
        sub: switch (connected.length) { 0 => powered ? 'On' : 'Off', 1 => 'Connected to ${connected.first['name']}', _ => '${connected.length} devices connected' },
        on: powered,
        onToggle: (v) => Host.I.act({'do': 'bt_power', 'on': v}),
      ),
      if (powered) ...[
        for (final d in devices)
          Row(children: [
            Disc(hue: t.base[0xD], child: WIcon(_icon('${d['icon'] ?? ''}'), color: t.base[0xD])),
            const SizedBox(width: 12),
            Expanded(child: Text('${d['name']}', style: body(context, w: FontWeight.w600, color: t.bright))),
            Btn(d['connected'] == true ? 'Disconnect' : 'Connect', primary: d['connected'] != true, expand: false, onTap: () => Host.I.act({'do': 'bt_connect', 'addr': d['addr'], 'connect': d['connected'] != true})),
          ]),
        const Why('Paired devices. Pair new ones in the computer’s own settings.'),
      ],
    ]);
  }
}
