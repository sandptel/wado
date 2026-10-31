// Quick (port of ui/computer/quick.rs + computer.css .qt): volume with mute, brightness, and the
// Keep awake / Wi-Fi / Bluetooth tiles. Every change is a HostDo; the next state answers it.
import 'package:flutter/material.dart';

import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../theme/look.dart';
import '../theme/tokens.dart';
import '../widgets/basics.dart';
import '../motion/motion.dart';
import '../widgets/slider.dart';

/// The default output (ui/host.rs `speaker`).
Map<String, dynamic>? speaker(Map<String, dynamic> h) {
  final a = h['audio'] as Map<String, dynamic>?;
  final sinks = (a?['sinks'] as List? ?? []).cast<Map<String, dynamic>>();
  return sinks.where((s) => s['name'] == a?['default_sink']).firstOrNull ?? sinks.firstOrNull;
}

class Quick extends StatefulWidget {
  const Quick({super.key, required this.h, required this.name});
  final Map<String, dynamic> h;
  final String name;
  @override
  State<Quick> createState() => _QuickState();
}

class _QuickState extends State<Quick> {
  bool askWifi = false;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final h = widget.h;
    final cfg = Host.I.config.value;
    final g = cfg?['grants'] as Map<String, dynamic>?;
    if (g != null && g['host'] == false) {
      return WCard(children: [
        Text("This phone can't change ${widget.name}'s sound, media, brightness or radios.", style: body(context)),
        const Why('Allow it on the computer: wado tui → Devices, or `wado allow <this phone> host`.'),
      ]);
    }
    final owner = g == null || g['settings'] != false;
    final s = speaker(h);
    final wifi = h['wifi'] as Map<String, dynamic>?;
    final wifiOn = wifi?['enabled'] == true;
    final wifiSub = wifi == null ? 'None here' : (!wifiOn ? 'Off' : ((wifi['connected'] ?? 'On') as String));
    final bt = h['bluetooth'] as Map<String, dynamic>?;
    final btOn = bt?['powered'] == true;
    final btSub = bt == null
        ? 'None here'
        : (!btOn
            ? 'Off'
            : ((bt['devices'] as List? ?? []).cast<Map<String, dynamic>>().where((d) => d['connected'] == true).map((d) => '${d['name']}').firstOrNull ?? 'On'));
    final awake = h['awake'] == true;
    final b = h['brightness'];
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (s != null) ...[
        Row(children: [
          IconBtn(s['muted'] == true ? 'mute' : 'sound',
              label: s['muted'] == true ? 'Unmute ${widget.name}' : 'Mute ${widget.name}',
              color: s['muted'] == true ? t.bad : t.bright,
              onTap: () => Host.I.act({'do': 'sink_mute', 'id': s['id'], 'muted': s['muted'] != true})),
          const SizedBox(width: 8),
          Expanded(
            child: ThickSlider(
              label: 'Volume',
              value: ((s['volume'] ?? 0) as num).toDouble().clamp(0, 1),
              shown: s['muted'] == true ? 'muted' : '',
              onChange: (v) => Host.I.act({'do': 'sink_volume', 'id': s['id'], 'volume': v}),
            ),
          ),
        ]),
        const SizedBox(height: 8),
      ],
      if (b is num)
        ThickSlider(label: 'Brightness', icon: 'sun', value: b.toDouble(), step: .1, ticks: 10, onChange: (v) => Host.I.act({'do': 'brightness', 'level': v}))
      else
        Container(
          padding: const EdgeInsets.all(14),
          decoration: BoxDecoration(color: t.s1, borderRadius: BorderRadius.circular(t.r3)),
          child: Row(children: [
            WIcon('sun', color: t.dim),
            const SizedBox(width: 12),
            Expanded(child: Text('${widget.name} has no screen brightness wado can set.', style: body(context, size: .8125, color: t.dim))),
          ]),
        ),
      const SizedBox(height: 8),
      Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Expanded(child: QTile(icon: 'eye', title: 'Keep awake', on: awake, sub: awake ? "Won't sleep" : 'Sleeps as usual', onFlip: () => Host.I.act({'do': 'keep_awake', 'on': !awake}))),
        const SizedBox(width: 8),
        Expanded(
          child: QTile(
            icon: 'wifi',
            title: 'Wi-Fi',
            on: wifiOn,
            sub: wifiSub,
            disabled: wifi == null || !owner,
            onFlip: () => wifiOn ? setState(() => askWifi = true) : Host.I.act({'do': 'wifi_radio', 'on': true, 'minutes': null}),
            onMore: () => Nav.I.openPage('connection_trust'),
          ),
        ),
        const SizedBox(width: 8),
        Expanded(
          child: QTile(
            icon: 'bt',
            title: 'Bluetooth',
            on: btOn,
            sub: btSub,
            disabled: bt == null,
            onFlip: () => Host.I.act({'do': 'bt_power', 'on': !btOn}),
            onMore: () => Nav.I.openPage('sound_devices'),
          ),
        ),
      ]),
      if (wifi != null && !owner) ...[const SizedBox(height: 8), Why("Only the owner device can change ${widget.name}'s Wi-Fi.")],
      if (askWifi) ...[
        const SizedBox(height: 8),
        WifiOff(uplink: wifi?['uplink'] == true, onDone: () => setState(() => askWifi = false)),
      ],
    ]);
  }
}

/// Turning Wi-Fi off from afar can cut the very link you are on (pages/network.rs `WifiOff`).
class WifiOff extends StatelessWidget {
  const WifiOff({super.key, required this.uplink, required this.onDone});
  final bool uplink;
  final VoidCallback onDone;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return WCard(alert: uplink, children: [
      Text('Turn Wi-Fi off?', style: body(context, w: FontWeight.w600, color: t.bright)),
      Why(uplink
          ? 'Wi-Fi is how this computer reaches the internet: turning it off cuts this connection. wado turns it back on in 2 minutes unless you keep it off from the computer.'
          : 'The computer stays reachable over its other connection.'),
      Row(children: [
        Expanded(child: Btn('Keep it on', onTap: onDone)),
        const SizedBox(width: 8),
        Expanded(
          child: Btn('Turn off', danger: true, onTap: () {
            Host.I.act({'do': 'wifi_radio', 'on': false, 'minutes': uplink ? 2 : null});
            onDone();
          }),
        ),
      ]),
    ]);
  }
}

/// `.qt`: a toggle tile that morphs rounder and fills with the accent when on.
class QTile extends StatelessWidget {
  const QTile({super.key, required this.icon, required this.title, required this.on, required this.sub, required this.onFlip, this.disabled = false, this.onMore});
  final String icon, title, sub;
  final bool on, disabled;
  final VoidCallback onFlip;
  final VoidCallback? onMore;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final fg = on ? t.onAccent : t.bright;
    return Press(
      onTap: disabled ? null : onFlip,
      onLongPress: onMore,
      enabled: !disabled,
      label: title,
      child: AnimatedContainer(
        duration: t.durSheet,
        curve: Tokens.spring,
        constraints: const BoxConstraints(minHeight: 104),
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(color: on ? t.accent : t.s2, borderRadius: BorderRadius.circular(on ? t.r3 + 8 : t.r3)),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          AnimatedContainer(
            duration: t.durSheet,
            curve: Tokens.spring,
            width: 40,
            height: 40,
            decoration: BoxDecoration(color: on ? fade(t.onAccent, 84) : t.s3, borderRadius: BorderRadius.circular(on ? 20 : t.r2)),
            alignment: Alignment.center,
            child: Bump(on: on, child: WIcon(icon, color: fg)),
          ),
          const SizedBox(height: 10),
          Text(title, style: body(context, size: .8125, w: FontWeight.w600, color: fg)),
          Text(sub, maxLines: 2, overflow: TextOverflow.ellipsis, style: body(context, size: .7188, color: on ? t.onAccent : t.dim)),
        ]),
      ),
    );
  }
}
