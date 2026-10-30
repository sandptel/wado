// Connection & trust (port of computer/pages/connection_trust.rs + pages/connection.rs +
// network.rs): this computer's name here, how it is reached, what this phone may do on it, its
// Wi-Fi, and Forget.
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/relay_link.dart';
import '../computer/quick.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';

class ConnectionTrust extends StatefulWidget {
  const ConnectionTrust({super.key, required this.computerKey});
  final String? computerKey;
  @override
  State<ConnectionTrust> createState() => _ConnectionTrustState();
}

class _ConnectionTrustState extends State<ConnectionTrust> {
  bool _forget = false, _wifiOff = false;
  late final _name = TextEditingController(text: CoreHost.I.computer(widget.computerKey ?? '')?.name ?? '');

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final c = CoreHost.I.computer(widget.computerKey ?? '');
    if (c == null) return const SizedBox.shrink();
    return ListenableBuilder(
      listenable: Listenable.merge([Host.I.state, Host.I.config, RelayLink.I.pool, RelayLink.I.words]),
      builder: (context, _) {
        final g = Host.I.config.value?['grants'] as Map<String, dynamic>?;
        final wifi = Host.I.state.value?['wifi'] as Map<String, dynamic>?;
        final owner = g == null || g['settings'] != false;
        String yes(Object? v) => v == true ? 'yes' : 'no';
        Widget kv(String k, String v) => Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              SizedBox(width: 120, child: Text(k, style: body(context, size: .8125, color: t.dim))),
              Expanded(child: Text(v, style: body(context, size: .8125, color: t.bright))),
            ]);
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          const Sect('Name on this phone'),
          const SizedBox(height: 8),
          WCard(children: [
            TextField(
              controller: _name,
              style: body(context, size: 1, color: t.bright),
              decoration: InputDecoration(filled: true, fillColor: t.s0, border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1))),
              onSubmitted: (v) => CoreHost.I.core.rename(key: c.key, name: v),
            ),
            Btn('Save name', onTap: () => CoreHost.I.core.rename(key: c.key, name: _name.text)),
          ]),
          const Sect('How it is reached'),
          const SizedBox(height: 8),
          WCard(children: [
            kv('Remote ID', c.remoteId),
            kv('Relay', c.relayUrl.replaceFirst(RegExp(r'^https?://'), '')),
            kv('Link', RelayLink.I.words.value),
            if (RelayLink.I.pool.value.instance.isNotEmpty) kv('Daemon', RelayLink.I.pool.value.tag),
          ]),
          const Sect('What this phone may do'),
          const SizedBox(height: 8),
          WCard(children: [
            if (g == null)
              Text('Asking the computer…', style: body(context, color: t.dim))
            else ...[
              kv('Files', '${g['files']}'),
              kv('Shells', yes(g['shells'])),
              kv('Settings', yes(g['settings'])),
              kv('Sound & media', yes(g['host'])),
              kv('Touchpad & keys', yes(g['input'])),
              kv('Clipboard', yes(g['clipboard'])),
              const Why('Change these on the computer: wado tui → Devices, or `wado allow` / `wado deny`.'),
            ],
          ]),
          if (wifi != null) ...[
            const Sect('Wi-Fi'),
            const SizedBox(height: 8),
            WCard(children: [
              SwitchRow(
                title: 'Wi-Fi',
                sub: wifi['connected'] == null ? '' : 'On ${wifi['connected']}',
                on: wifi['enabled'] == true,
                disabled: !owner,
                onToggle: (v) => v ? Host.I.act({'do': 'wifi_radio', 'on': true, 'minutes': null}) : setState(() => _wifiOff = true),
              ),
              if (_wifiOff) WifiOff(uplink: wifi['uplink'] == true, onDone: () => setState(() => _wifiOff = false)),
              for (final n in (wifi['networks'] as List? ?? []).cast<Map<String, dynamic>>().take(8))
                Row(children: [
                  WIcon('wifi', color: n['active'] == true ? t.accent : t.dim),
                  const SizedBox(width: 10),
                  Expanded(child: Text('${n['ssid']}${n['secure'] == true ? ' 🔒' : ''}', style: body(context, color: t.bright))),
                  Text('${n['signal']}%', style: mono(context, color: t.dim)),
                  if (n['active'] != true && n['known'] == true && owner)
                    Press(
                      onTap: () => Host.I.act({'do': 'wifi_connect', 'ssid': n['ssid'], 'password': null}),
                      child: Padding(padding: const EdgeInsets.all(8), child: Text('Join', style: body(context, size: .8125, w: FontWeight.w600, color: t.accent))),
                    ),
                ]),
              if (!owner) Why("Only the owner device can change ${c.name}'s Wi-Fi."),
            ]),
          ],
          const Sect('This phone'),
          const SizedBox(height: 8),
          WCard(children: [
            if (!_forget)
              Btn('Forget this computer', danger: true, icon: 'trash', onTap: () => setState(() => _forget = true))
            else ...[
              Text('Forget ${c.name} on this phone?', style: body(context, w: FontWeight.w600, color: t.bright)),
              const Why('To use it again you pair once more with a code from the computer. The computer still trusts this phone until you remove it there.'),
              Row(children: [
                Expanded(child: Btn('Keep it', onTap: () => setState(() => _forget = false))),
                const SizedBox(width: 8),
                Expanded(
                  child: Btn('Forget', danger: true, onTap: () {
                    if (RelayLink.I.key == c.key) RelayLink.I.drop();
                    CoreHost.I.forget(c.key);
                    Nav.I.view.value = const HomeView();
                  }),
                ),
              ]),
            ],
          ]),
        ]);
      },
    );
  }
}
