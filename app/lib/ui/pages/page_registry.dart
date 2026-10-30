// Every settings page by id, with its title (port of ui/pages/mod.rs `Page`).
import 'package:flutter/material.dart';

import '../theme/look.dart';
import '../widgets/basics.dart';
import 'add_computer.dart';
import 'appearance.dart';
import 'diagnostics.dart';

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
