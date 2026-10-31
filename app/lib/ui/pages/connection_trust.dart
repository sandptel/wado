// Connection & trust (port of computer/pages/connection_trust.rs + pages/connection.rs +
// pages/network.rs `wifi` + fetch.rs + computers.rs `forget`): how this computer is reached, what
// this phone may do there and how to allow more, the passkey, the computer's Wi-Fi (it is how the
// computer itself is reached), what only its owner may change, who it is, and forgetting it.
// Works offline on purpose: a computer that is off can still be renamed and forgotten.
import 'package:flutter/material.dart';

import '../../core/core_host.dart';
import '../../wado/host.dart';
import '../../wado/nav.dart';
import '../../wado/relay_link.dart';
import '../computer/quick.dart' show WifiOff;
import '../passkey/passkey_card.dart';
import '../theme/look.dart';
import '../widgets/basics.dart';
import '../widgets/fields.dart';
import 'display_session.dart' show HeadCard;
import 'host_settings.dart';

class ConnectionTrust extends StatelessWidget {
  const ConnectionTrust({super.key, required this.computerKey});
  final String? computerKey;
  @override
  Widget build(BuildContext context) {
    final c = CoreHost.I.computer(computerKey ?? '');
    if (c == null) return const SizedBox.shrink();
    return ListenableBuilder(
      listenable: Listenable.merge([Host.I.state, Host.I.config, Host.I.note, RelayLink.I.pool]),
      builder: (context, _) {
        final info = Host.I.state.value?['info'] as Map?;
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          HeadCard(children: [
            Field(label: 'Name', value: c.name, number: false, hint: c.remoteId, onCommit: (v) {
              if (v.isNotEmpty) CoreHost.I.core.rename(key: c.key, name: v);
            }),
            Text('Reached', style: body(context, w: FontWeight.w600, color: Tok.of(context).bright)),
            const Seg(options: [('relay', 'Via relay'), ('direct', 'Direct')], value: 'relay', onPick: _noop, disabled: true),
            Field(label: 'Remote ID', value: c.remoteId, number: false, disabled: true, onCommit: _noopS),
            Field(label: 'Relay', value: c.relayUrl, number: false, disabled: true, onCommit: _noopS),
            const Why('Set when this phone was paired, and pinned with the computer’s key. To reach it another way, pair again with a code from it.'),
          ]),
          const SizedBox(height: 12),
          const _Grants(),
          const SizedBox(height: 12),
          const PasskeyCard(),
          const Sect('Its Wi-Fi'),
          const SizedBox(height: 8),
          const _Wifi(),
          const SizedBox(height: 12),
          const HostNotes(),
          const OwnerOnly(),
          if (info != null && '${info['hostname'] ?? ''}'.isNotEmpty) ...[const SizedBox(height: 12), Fetch(info: info.cast<String, dynamic>())],
          const SizedBox(height: 12),
          _Forget(name: c.name, k: c.key),
        ]);
      },
    );
  }
}

void _noop(String _) {}
void _noopS(String _) {}

/// What this phone may do on the computer, one line per grant, with the command for the rest.
class _Grants extends StatelessWidget {
  const _Grants();
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final g = Host.I.config.value?['grants'] as Map?;
    if (g == null) return const SizedBox.shrink();
    final files = '${g['files']}';
    final rows = [
      ('Files', files != 'none', 'files-rw', switch (files) { 'ro' => 'read only', 'rw' => 'read and write', _ => 'no' }),
      ('Shells', g['shells'] == true, 'shells', ''),
      ('Sound, media, brightness, Bluetooth', g['host'] == true, 'host', ''),
      ('Trackpad and typing', g['input'] == true, 'input', ''),
      ('Clipboard history', g['clipboard'] == true, 'clipboard', ''),
      ('Settings and Wi-Fi', g['settings'] == true, 'settings', ''),
    ];
    final missing = [for (final r in rows) if (!r.$2) r.$3];
    return HeadCard(title: 'This phone may', children: [
      for (final (label, yes, _, note) in rows)
        SizedBox(
          height: 32,
          child: Row(children: [
            Container(width: 8, height: 8, decoration: BoxDecoration(shape: BoxShape.circle, color: yes ? t.good : t.s3)),
            const SizedBox(width: 10),
            Expanded(child: Text(label, style: body(context, color: t.bright))),
            Text(note.isNotEmpty && yes ? note : (yes ? 'yes' : 'no'), style: body(context, size: .75, color: t.dim)),
          ]),
        ),
      if (missing.isNotEmpty)
        Text.rich(
          TextSpan(children: [
            const TextSpan(text: 'To allow more, run on the computer: '),
            TextSpan(text: 'wado allow ${g['name']} ${missing.join(' ')}', style: mono(context, size: .75, color: t.bright)),
            const TextSpan(text: ' — or tick them in '),
            TextSpan(text: 'wado tui', style: mono(context, size: .75, color: t.bright)),
            const TextSpan(text: '.'),
          ]),
          style: body(context, size: .75, color: t.dim),
        ),
    ]);
  }
}

/// Wi-Fi, asked first and on probation (network.rs `wifi`): the one control that can cut the
/// connection it is used over, so each change says what it risks; a network that does not work
/// is undone by the daemon after a minute, and "off" can be for five minutes only.
class _Wifi extends StatefulWidget {
  const _Wifi();
  @override
  State<_Wifi> createState() => _WifiState();
}

class _WifiState extends State<_Wifi> {
  bool _off = false;
  String? _join;
  final _pw = TextEditingController();

  @override
  void dispose() {
    _pw.dispose();
    super.dispose();
  }

  static String _bars(int s) => s >= 75 ? 'Strong' : (s >= 50 ? 'Good' : (s >= 25 ? 'Fair' : 'Weak'));

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final h = Host.I.state.value;
    if (h == null) return WCard(children: [const Why('Waiting for the computer…')]);
    final owner = Host.I.config.value?['owner'] == true;
    final w = h['wifi'] as Map?;
    final revert = h['wifi_revert_in'];
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (revert != null) ...[
        WCard(children: [
          Text('Keep this Wi-Fi?', style: body(context, w: FontWeight.w600, color: t.bright)),
          Why('The computer goes back to how it was in $revert s unless you keep it.'),
          Btn('Keep', primary: true, onTap: () => Host.I.act({'do': 'wifi_keep'})),
        ]),
        const SizedBox(height: 12),
      ],
      if (w == null)
        WCard(children: [const Why('This computer has no NetworkManager, so its Wi-Fi cannot be changed from here.')])
      else
        WCard(children: [
          SwitchRow(
            title: 'Wi-Fi',
            sub: w['connected'] != null ? 'Connected to ${w['connected']}' : (w['enabled'] == true ? 'On, not connected' : 'Off'),
            on: w['enabled'] == true,
            disabled: !owner,
            onToggle: (on) => on ? Host.I.act({'do': 'wifi_radio', 'on': true, 'minutes': null}) : setState(() => _off = true),
          ),
          if (!owner) const Why("Only the owner device can change the computer's Wi-Fi."),
          if (w['uplink'] == true) const Why('This computer is online through Wi-Fi alone — the connection you are using right now goes through it.'),
          if (_off) WifiOff(uplink: w['uplink'] == true, onDone: () => setState(() => _off = false)),
          if (w['enabled'] == true)
            for (final n in [for (final x in (w['networks'] as List? ?? [])) Map<String, dynamic>.from(x as Map)]) ...[
              NavRow(
                icon: 'net',
                hue: '0C',
                title: '${n['ssid']}',
                sub: [_bars((n['signal'] ?? 0) as int), if (n['secure'] == true) 'secured', if (n['known'] == true) 'saved', if (n['active'] == true) 'connected'].join(' · '),
                onOpen: !owner || n['active'] == true
                    ? () {}
                    : () => setState(() {
                          _pw.clear();
                          _join = '${n['ssid']}';
                        }),
              ),
              if (_join == n['ssid']) ...[
                Text('Join ${n['ssid']}?', style: body(context, w: FontWeight.w600, color: t.bright)),
                Why(w['uplink'] == true
                    ? 'You will be disconnected while the computer switches. If it cannot get online through this network, it goes back to the one it was on after a minute.'
                    : 'The computer switches its Wi-Fi to this network.'),
                if (n['secure'] == true && n['known'] != true)
                  TextField(
                    controller: _pw,
                    obscureText: true,
                    onChanged: (_) => setState(() {}),
                    style: body(context, size: 1, color: t.bright),
                    decoration: InputDecoration(hintText: 'Password', filled: true, fillColor: t.s0, border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.r1))),
                  ),
                Row(children: [
                  Expanded(child: Btn('Cancel', onTap: () => setState(() => _join = null))),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Btn('Join', primary: true, onTap: n['secure'] == true && n['known'] != true && _pw.text.isEmpty
                        ? null
                        : () {
                            Host.I.act({'do': 'wifi_connect', 'ssid': n['ssid'], 'password': _pw.text.isEmpty ? null : _pw.text});
                            setState(() => _join = null);
                          }),
                  ),
                ]),
              ],
            ],
        ]),
    ]);
  }
}

/// The computer, neofetch-style (fetch.rs): a logo for its OS beside `user@host` and the facts,
/// with the scheme's colour blocks underneath.
class Fetch extends StatelessWidget {
  const Fetch({super.key, required this.info});
  final Map<String, dynamic> info;

  static String _logo(String id) => switch (id) {
        'nixos' => '  \\\\  \\\\ //\n ==\\\\__\\\\/ //\n   //   \\\\//\n==//     //==\n //\\\\___//\n// /\\\\  \\\\==\n  // \\\\  \\\\',
        'arch' || 'endeavouros' || 'manjaro' => '      /\\\n     /  \\\n    /\\   \\\n   /  __  \\\n  /  (  )  \\\n / __|  |__\\\n/.`        `.\\',
        'ubuntu' || 'pop' => '        _\n    ---(_)\n_/  ---  \\\n(_) |   |\n  \\  --- _/\n     ---(_)',
        'debian' => '  _____\n /  __ \\\n|  /    |\n|  \\___-\n-_\n  --_',
        'fedora' => '      _____\n     /   __)\\\n     |  /  \\ \\\n  ___|  |__/ /\n / (_    _)_/\n/ /  |  |\n\\ \\__/  |\n \\(_____/',
        _ => '    .--.\n   |o_o |\n   |:_/ |\n  //   \\ \\\n (|     | )\n/\'\\_   _/`\\\n\\___)=(___/',
      };

  static String _up(int s) {
    final d = s ~/ 86400, h = s ~/ 3600 % 24, m = s ~/ 60 % 60;
    if (d == 0 && h == 0) return '$m mins';
    if (d == 0) return '$h hours, $m mins';
    return '$d days, $h hours';
  }

  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    final user = '${info['user'] ?? ''}';
    final head = '${user.isEmpty ? 'user' : user}@${info['hostname']}';
    final cores = (info['cores'] ?? 0) as int;
    final rows = [
      ('OS', '${info['os'] ?? ''}'),
      ('Kernel', '${info['kernel'] ?? ''}'),
      ('Uptime', _up((info['uptime_s'] ?? 0) as int)),
      ('Shell', '${info['shell'] ?? ''}'),
      ('DE', '${info['desktop'] ?? ''}'),
      ('CPU', cores > 0 ? '${info['cpu']} ($cores)' : '${info['cpu'] ?? ''}'),
      ('GPU', '${info['gpu'] ?? ''}'),
      if (((info['mem_total_mb'] ?? 0) as int) > 0) ('Memory', '${info['mem_used_mb']} MiB / ${info['mem_total_mb']} MiB'),
    ].where((r) => r.$2.trim().isNotEmpty);
    final m = mono(context, size: .75, color: t.text).copyWith(height: 1.35);
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(color: t.s0, borderRadius: BorderRadius.circular(t.r3), border: Border.all(color: t.line)),
      child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        if (MediaQuery.sizeOf(context).width > 380) ...[
          Text(_logo('${info['os_id'] ?? ''}'), style: mono(context, size: .6562, w: FontWeight.w700, color: t.accent).copyWith(height: 1.25)),
          const SizedBox(width: 14),
        ],
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(head, style: m.copyWith(color: t.accent, fontWeight: FontWeight.w700)),
            Text('-' * head.length, maxLines: 1, overflow: TextOverflow.clip, style: m.copyWith(color: t.dim)),
            for (final (k, v) in rows)
              Text.rich(TextSpan(children: [TextSpan(text: k, style: TextStyle(color: t.accent, fontWeight: FontWeight.w700)), TextSpan(text: ': $v')]), style: m),
            const SizedBox(height: 6),
            Row(children: [for (final i in const [0, 8, 9, 0xA, 0xB, 0xC, 0xD, 0xE, 0xF, 5]) Container(width: 16, height: 12, color: t.base[i])]),
          ]),
        ),
      ]),
    );
  }
}

class _Forget extends StatefulWidget {
  const _Forget({required this.name, required this.k});
  final String name, k;
  @override
  State<_Forget> createState() => _ForgetState();
}

class _ForgetState extends State<_Forget> {
  bool _ask = false;
  @override
  Widget build(BuildContext context) {
    final t = Tok.of(context);
    return WCard(children: [
      if (!_ask)
        Btn('Forget this computer', danger: true, icon: 'x', onTap: () => setState(() => _ask = true))
      else ...[
        Text('Forget ${widget.name}?', style: body(context, w: FontWeight.w600, color: t.bright)),
        const Why('It goes from this device’s list, with its widgets. The computer itself is not touched, and you can add it again with its link or QR code.'),
        Row(children: [
          Expanded(child: Btn('Keep', onTap: () => setState(() => _ask = false))),
          const SizedBox(width: 8),
          Expanded(
            child: Btn('Forget it', danger: true, onTap: () {
              if (RelayLink.I.key == widget.k) RelayLink.I.drop();
              CoreHost.I.forget(widget.k);
              Nav.I.view.value = const HomeView();
            }),
          ),
        ]),
      ],
    ]);
  }
}
