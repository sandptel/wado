// Every settings page by id, with its title (port of ui/pages/mod.rs `Page`).
import 'package:flutter/material.dart';

import '../theme/look.dart';
import '../widgets/basics.dart';
import 'add_computer.dart';
import 'appearance.dart';
import 'diagnostics.dart';
import 'input_pages.dart';
import 'tiles.dart';
import 'display_session.dart';
import 'sound_devices.dart';
import 'connection_trust.dart';
import '../../wado/nav.dart';
import '../input/hub.dart';
import '../shell/shell_page.dart';
import '../files/files_page.dart';

class PageDef {
  const PageDef(this.title, this.build, {this.fullBleed = false});
  final String title;
  final Widget Function(BuildContext) build;
  final bool fullBleed;
}

final _pages = <String, PageDef>{
  'add_computer': PageDef('Add a computer', (_) => const AddComputer()),
  'appearance': PageDef('Appearance', (_) => const Appearance()),
  'diagnostics': PageDef('Diagnostics', (_) => const Diagnostics()),
  'logs': PageDef('Server log', (_) => const LogsPage()),
  'display_session': PageDef('Display & session', (_) => const DisplaySession()),
  'sound_devices': PageDef('Sound & devices', (_) => const SoundDevices()),
  'connection_trust': PageDef('Connection & trust', (_) => ConnectionTrust(computerKey: Nav.I.view.value.computer)),
  'trackpad': PageDef('Touchpad', (_) => const TouchpadSettings()),
  'input_feel': PageDef('Input feel', (_) => const InputFeel()),
  'tiles': PageDef('Quick tiles', (_) => const TilesPage()),
  'about': PageDef('About', (_) => const AboutPage()),
  'input': PageDef('Virtual input', (_) => const InputHub()),
  'shells': PageDef('Shell', (_) => const ShellPage(), fullBleed: true),
  'files': PageDef('Files', (_) => const FilesPage(), fullBleed: true),
};

/// Register a page (each feature module adds its own).
void registerPage(String id, PageDef p) => _pages[id] = p;

PageDef pageFor(String id) => _pages[id] ?? PageDef(id, (c) => _Soon(id));

class _Soon extends StatelessWidget {
  const _Soon(this.id);
  final String id;
  @override
  Widget build(BuildContext context) => WCard(children: [Text('This page is coming to the app.', style: body(context, color: Tok.of(context).dim))]);
}
