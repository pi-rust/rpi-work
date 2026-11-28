import 'dart:async';

// The Markdown renderer reads the framework Material theme.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

import 'src/connect.dart';
import 'src/node/work_node.dart';
import 'src/work_theme.dart';
import 'src/workspace.dart';

void main() => runApp(const WorkMobileApp());

class WorkMobileApp extends StatelessWidget {
  const WorkMobileApp({super.key, this.home = const WorkHome()});

  final Widget home;

  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'RPI Work',
    debugShowCheckedModeBanner: false,
    locale: const Locale('zh', 'CN'),
    supportedLocales: const [Locale('zh', 'CN')],
    // Keep localized strings in the framework Material widget tree.
    // ignore: deprecated_member_use
    localizationsDelegates: GlobalMaterialLocalizations.delegates,
    theme: workTheme(Brightness.light),
    darkTheme: workTheme(Brightness.dark),
    home: home,
  );
}

class WorkHome extends StatefulWidget {
  const WorkHome({super.key});

  @override
  State<WorkHome> createState() => _WorkHomeState();
}

class _WorkHomeState extends State<WorkHome> {
  WorkNode? _node;
  String? _notice;

  Future<void> _connect(PairingCode code, bool Function() isCurrent) async {
    final node = await WorkNode.connect(code, isCurrent: isCurrent);
    if (!mounted || !isCurrent()) {
      unawaited(node.logout().catchError((Object _) {}));
      return;
    }
    setState(() {
      _node = node;
      _notice = null;
    });
  }

  void _disconnect(WorkNode node, {bool expired = false}) {
    if (!mounted || !identical(_node, node)) return;
    if (expired) {
      node.close();
    } else {
      unawaited(node.logout().catchError((Object _) {}));
    }
    setState(() {
      _node = null;
      _notice = expired ? '节点授权已结束，请重新扫码。' : null;
    });
  }

  @override
  void dispose() {
    _node?.close();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final node = _node;
    if (node == null) return ConnectScreen(connect: _connect, notice: _notice);
    return WorkspaceScreen(
      key: ObjectKey(node),
      node: node,
      disconnect: () => _disconnect(node),
      authorizationLost: () => _disconnect(node, expired: true),
    );
  }
}
