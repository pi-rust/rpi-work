import 'dart:async';

// Keep widget tests on the same Material theme as WorkMobileApp.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:work_mobile/main.dart';
import 'package:work_mobile/src/conversation/conversation.dart';
import 'package:work_mobile/src/node/work_node.dart';
import 'package:work_mobile/src/work_controls.dart';
import 'package:work_mobile/src/workspace.dart';

void main() {
  testWidgets('025 initial read shows progress until sessions arrive', (
    tester,
  ) async {
    final pending = Completer<WorkspaceCatalog>();
    final node = _Node(_catalog(sessions: [_session('a', 'First session')]))
      ..nextCatalog = () => pending.future;
    await _mount(tester, node);

    final progress = find.byType(LinearProgressIndicator);
    expect(
      tester.widget<LinearProgressIndicator>(progress).semanticsLabel,
      '正在读取项目和会话',
    );
    expect(find.text('暂无会话。'), findsNothing);
    expect(find.text('First session'), findsNothing);
    pending.complete(node.data);
    await tester.pumpAndSettle();
    expect(progress, findsNothing);
    expect(find.text('First session'), findsOneWidget);
  });

  testWidgets(
    '026 initial failure offers retry and recovers from a pending read',
    (tester) async {
      final node = _Node(_catalog(sessions: [_session('a', 'Recovered')]))
        ..nextCatalog = () async => throw const NodeException('连接超时');
      await _show(tester, node);

      expect(find.textContaining('连接超时'), findsOneWidget);
      expect(find.text('暂无会话。'), findsNothing);
      expect(find.byType(LinearProgressIndicator), findsNothing);
      final pending = Completer<WorkspaceCatalog>();
      node.nextCatalog = () => pending.future;
      await tester.tap(find.widgetWithText(OutlinedButton, '重试'));
      await tester.pump();
      expect(find.byType(LinearProgressIndicator), findsOneWidget);
      expect(find.textContaining('连接超时'), findsNothing);
      pending.complete(node.data);
      await tester.pumpAndSettle();
      expect(find.text('Recovered'), findsOneWidget);
      expect(node.catalogCalls, 2);
    },
  );

  testWidgets('027 a node without projects can create a project', (
    tester,
  ) async {
    final node = _Node(const WorkspaceCatalog(projects: [], sessions: []));
    await _show(tester, node);

    expect(find.text('节点还没有项目。请先新建项目。'), findsOneWidget);
    expect(
      tester
          .widget<OutlinedButton>(find.widgetWithText(OutlinedButton, '新建会话'))
          .onPressed,
      isNull,
    );
    await tester.tap(find.byTooltip('新建项目'));
    await tester.pumpAndSettle();
    expect(find.widgetWithText(TextField, '项目名称'), findsOneWidget);
  });

  testWidgets('028 an empty catalog with projects invites a first session', (
    tester,
  ) async {
    await _show(tester, _Node(_catalog()));

    expect(find.text('暂无会话。'), findsOneWidget);
    expect(find.text('没有符合条件的会话。'), findsNothing);
    expect(
      tester
          .widget<OutlinedButton>(find.widgetWithText(OutlinedButton, '新建会话'))
          .onPressed,
      isNotNull,
    );
  });

  testWidgets('029 project filter uses paths to distinguish namesakes', (
    tester,
  ) async {
    await _show(
      tester,
      _Node(
        WorkspaceCatalog(
          projects: const [
            WorkProject(id: 'alpha', name: 'Shared', path: '/work/alpha'),
            WorkProject(id: 'beta', name: 'Shared', path: '/work/beta'),
          ],
          sessions: [
            _session('a', 'Alpha only'),
            _session('b', 'Beta only', project: 'beta'),
          ],
        ),
      ),
    );

    expect(find.text('筛选项目'), findsOneWidget);
    await _selectProject(tester, '/work/beta');
    expect(find.text('Alpha only'), findsNothing);
    expect(find.text('Beta only'), findsOneWidget);
    await _selectProject(tester, '全部项目');
    expect(find.text('Alpha only'), findsOneWidget);
  });

  testWidgets(
    '030 search trims whitespace, ignores case, and clears in place',
    (tester) async {
      await _show(
        tester,
        _Node(
          _catalog(
            sessions: [
              _session('a', 'Refactor Flutter'),
              _session('b', 'Release notes'),
            ],
          ),
        ),
      );

      await _search(tester, '  fLuTtEr  ');
      expect(find.text('Refactor Flutter'), findsOneWidget);
      expect(find.text('Release notes'), findsNothing);
      await tester.tap(find.byTooltip('清除搜索'));
      await tester.pumpAndSettle();
      expect(find.text('Release notes'), findsOneWidget);
      await _search(tester, '   ');
      expect(find.byTooltip('清除搜索'), findsOneWidget);
      expect(find.text('Release notes'), findsOneWidget);
    },
  );

  testWidgets('031 search matches project names within the selected scope', (
    tester,
  ) async {
    await _show(
      tester,
      _Node(
        _catalog(
          sessions: [
            _session('a', 'Code review'),
            _session('b', 'Beta release', project: 'beta'),
          ],
        ),
      ),
    );

    await _selectProject(tester, 'Alpha');
    await _search(tester, 'alpha');
    expect(find.text('Code review'), findsOneWidget);
    await _search(tester, 'beta');
    expect(find.text('没有符合条件的会话。'), findsOneWidget);
    expect(find.text('Beta release'), findsNothing);
  });

  testWidgets('032 clearing a no-match state resets the active filters', (
    tester,
  ) async {
    await _show(
      tester,
      _Node(
        _catalog(
          sessions: [
            _session('a', 'Current work'),
            _session('b', 'Old work', project: 'beta', archived: true),
          ],
        ),
      ),
    );

    await _selectProject(tester, 'Beta');
    await tester.tap(find.text('包含归档'));
    await tester.pumpAndSettle();
    await _search(tester, 'unmatched');
    expect(find.text('没有符合条件的会话。'), findsOneWidget);
    await tester.tap(find.text('清除筛选'));
    await tester.pumpAndSettle();
    expect(find.text('Current work'), findsOneWidget);
    expect(find.text('Old work'), findsNothing);
    expect(find.text('全部项目'), findsOneWidget);
    expect(
      tester.widget<TextField>(find.byType(TextField)).controller!.text,
      isEmpty,
    );
  });

  testWidgets(
    '033 archive visibility has a visible label and toggle semantics',
    (tester) async {
      final semantics = tester.ensureSemantics();
      try {
        await _show(
          tester,
          _Node(
            _catalog(
              sessions: [_session('a', 'Archived work', archived: true)],
            ),
          ),
        );

        final control = find.bySemanticsLabel('包含归档');
        expect(find.text('包含归档').hitTestable(), findsOneWidget);
        expect(
          tester.getSemantics(control),
          isSemantics(hasSelectedState: true, isSelected: false),
        );
        expect(find.text('Archived work'), findsNothing);
        expect(find.text('暂无未归档会话。'), findsOneWidget);
        await tester.tap(find.text('包含归档'));
        await tester.pumpAndSettle();
        expect(
          tester.getSemantics(control),
          isSemantics(hasSelectedState: true, isSelected: true),
        );
        expect(find.text('Archived work'), findsOneWidget);
        expect(find.text('已归档 · Alpha'), findsOneWidget);
        await tester.tap(find.text('包含归档'));
        await tester.pumpAndSettle();
        expect(find.text('Archived work'), findsNothing);
      } finally {
        semantics.dispose();
      }
    },
  );

  testWidgets('034 archive updates only the chosen session and hides its row', (
    tester,
  ) async {
    final node = _Node(
      _catalog(sessions: [_session('a', 'Review'), _session('b', 'Wrap up')]),
    );
    await _show(tester, node);

    await _sessionAction(tester, 'Alpha · Review：会话操作', '归档');
    expect(node.preferences, [(id: 'a', pinned: null, archived: true)]);
    expect(find.text('Review'), findsNothing);
    expect(find.text('Wrap up'), findsOneWidget);
  });

  testWidgets(
    '035 restoring an archived session returns it to the default list',
    (tester) async {
      final node = _Node(
        _catalog(sessions: [_session('a', 'Old work', archived: true)]),
      );
      await _show(tester, node);
      await tester.tap(find.text('包含归档'));
      await tester.pumpAndSettle();

      await _sessionAction(tester, 'Alpha · Old work：会话操作', '恢复会话');
      expect(node.preferences, [(id: 'a', pinned: null, archived: false)]);
      await tester.tap(find.text('包含归档'));
      await tester.pumpAndSettle();
      expect(find.text('Old work'), findsOneWidget);
      expect(find.textContaining('已归档'), findsNothing);
    },
  );

  testWidgets('036 pinned section precedes recents sorted newest first', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    try {
      await _show(
        tester,
        _Node(
          _catalog(
            sessions: [
              _session('a', 'Older', updated: 1),
              _session('b', 'Pinned work', pinned: true),
              _session('c', 'Newer', updated: 5),
            ],
          ),
        ),
      );

      final order = ['置顶', 'Pinned work', '最近', 'Newer', 'Older'];
      final positions = [
        for (final title in order) tester.getTopLeft(find.text(title)).dy,
      ];
      expect(positions, orderedEquals([...positions]..sort()));
      expect(find.bySemanticsLabel('已置顶'), findsOneWidget);
    } finally {
      semantics.dispose();
    }
  });

  testWidgets('037 pin and unpin move a session between sections', (
    tester,
  ) async {
    final node = _Node(_catalog(sessions: [_session('a', 'Focus work')]));
    await _show(tester, node);

    await _sessionAction(tester, 'Alpha · Focus work：会话操作', '置顶');
    expect(find.text('置顶'), findsOneWidget);
    expect(find.text('最近'), findsNothing);
    await _sessionAction(tester, 'Alpha · Focus work：会话操作', '取消置顶');
    expect(find.text('最近'), findsOneWidget);
    expect(node.preferences, [
      (id: 'a', pinned: true, archived: null),
      (id: 'a', pinned: false, archived: null),
    ]);
  });

  testWidgets('038 menu names disambiguate duplicate titles for rename', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    try {
      final node = _Node(
        _catalog(
          sessions: [
            _session('a', 'Daily'),
            _session('b', 'Daily', project: 'beta'),
          ],
        ),
      );
      await _show(tester, node);
      const betaMenu = 'Beta · Daily：会话操作';
      expect(find.byTooltip('Alpha · Daily：会话操作'), findsOneWidget);
      expect(
        find.semantics.byPredicate((node) => node.tooltip == betaMenu),
        findsOne,
      );

      await _sessionAction(tester, betaMenu, '重命名');
      final input = find.widgetWithText(TextField, '会话名称');
      expect(tester.widget<TextField>(input).controller!.text, 'Daily');
      await tester.enterText(input, '  Beta daily  ');
      await tester.tap(find.text('保存'));
      await tester.pumpAndSettle();
      expect(node.renames, [('b', 'Beta daily')]);
      expect(find.text('Daily'), findsOneWidget);
      expect(find.text('Beta daily'), findsOneWidget);
    } finally {
      semantics.dispose();
    }
  });

  testWidgets('039 creating a project selects the returned project', (
    tester,
  ) async {
    final node = _Node(_catalog(sessions: [_session('a', 'Existing work')]));
    await _show(tester, node);

    await _beginProject(tester, '  Gamma  ');
    await tester.pumpAndSettle();
    expect(node.createdProjects, ['Gamma']);
    expect(find.text('Gamma'), findsOneWidget);
    expect(find.text('Existing work'), findsNothing);
    expect(node.catalogCalls, 2);
  });

  testWidgets(
    '040 project picker paths disambiguate new-session destinations',
    (tester) async {
      final node = _Node(
        const WorkspaceCatalog(
          projects: [
            WorkProject(id: 'one', name: 'Shared', path: '/work/first'),
            WorkProject(id: 'two', name: 'Shared', path: '/work/second'),
          ],
          sessions: [],
        ),
      );
      await _show(tester, node);

      await tester.tap(find.text('新建会话'));
      await tester.pumpAndSettle();
      expect(find.text('选择会话所属项目'), findsOneWidget);
      expect(node.sessionProjects, isEmpty);
      await tester.tap(find.text('/work/second'));
      await tester.pumpAndSettle();
      expect(node.sessionProjects, ['two']);
      expect(node.openedSessions, ['new']);
      expect(find.byType(ConversationScreen), findsOneWidget);
      await tester.tap(find.byTooltip('返回'));
      await tester.pumpAndSettle();
      expect(find.text('New conversation'), findsOneWidget);
    },
  );

  testWidgets(
    'switching conversations keeps the list state and marks the current session',
    (tester) async {
      final semantics = tester.ensureSemantics();
      try {
        final node = _Node(
          _catalog(
            sessions: [
              _session('a', 'First work'),
              _session('b', 'Second work', project: 'beta', status: 'running'),
            ],
          ),
        );
        await _show(tester, node);
        await tester.tap(find.text('First work'));
        await tester.pumpAndSettle();
        expect(node.openedSessions, ['a']);

        await tester.tap(find.byTooltip('切换会话'));
        await tester.pumpAndSettle();
        expect(find.byType(WorkPanel), findsOneWidget);
        final second = find.byKey(const ValueKey('switch-b'));
        expect(second, findsOneWidget);
        expect(find.widgetWithText(TextButton, 'Second work'), findsOneWidget);
        expect(
          tester.getSemantics(
            find.descendant(
              of: find.byType(WorkPanel),
              matching: find.bySemanticsLabel(RegExp('First work')),
            ),
          ),
          isSemantics(hasSelectedState: true, isSelected: true),
        );

        await tester.tap(second);
        await tester.pumpAndSettle();
        expect(find.byType(WorkPanel), findsNothing);
        expect(node.openedSessions, ['a', 'b']);
        expect(find.text('Second work'), findsOneWidget);
        expect(find.text('First work'), findsNothing);
      } finally {
        semantics.dispose();
      }
    },
  );

  testWidgets(
    '041 new session uses the selected project without another picker',
    (tester) async {
      final node = _Node(_catalog());
      await _show(tester, node);
      await _selectProject(tester, 'Beta');

      await tester.tap(find.text('新建会话'));
      await tester.pumpAndSettle();
      expect(find.text('选择会话所属项目'), findsNothing);
      expect(node.sessionProjects, ['beta']);
      expect(find.byType(ConversationScreen), findsOneWidget);
    },
  );

  testWidgets(
    '042 stale and resumed catalog reads cannot overwrite a mutation',
    (tester) async {
      final old = _catalog(sessions: [_session('a', 'Existing work')]);
      final node = _Node(old);
      await _show(tester, node);
      final stale = Completer<WorkspaceCatalog>();
      final creating = Completer<void>();
      node.nextCatalog = () => stale.future;
      node.projectGate = creating.future;
      await tester.pump(const Duration(seconds: 5));
      expect(node.catalogCalls, 2);

      await _beginProject(tester, 'Gamma');
      await tester.pump();
      // Finish the dialog exit while the indeterminate progress bar animates.
      await tester.pump(const Duration(milliseconds: 200));
      expect(find.byType(WorkDialog), findsNothing);
      expect(node.createdProjects, ['Gamma']);
      expect(find.byType(LinearProgressIndicator), findsOneWidget);
      stale.complete(const WorkspaceCatalog(projects: [], sessions: []));
      await tester.pump();
      expect(find.text('Existing work'), findsOneWidget);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await tester.pump();
      expect(node.catalogCalls, 2);

      final selectedProject = find.descendant(
        of: _projectFilter,
        matching: find.text('Gamma'),
      );
      final fresh = Completer<WorkspaceCatalog>();
      node.nextCatalog = () => fresh.future;
      creating.complete();
      await tester.pump();
      expect(selectedProject, findsOneWidget);
      expect(node.catalogCalls, 3);
      fresh.complete(node.data);
      await tester.pumpAndSettle();
      expect(selectedProject, findsOneWidget);
      expect(find.byType(LinearProgressIndicator), findsNothing);
    },
  );

  testWidgets('043 failed background refresh retains the list and recovers', (
    tester,
  ) async {
    final node = _Node(_catalog(sessions: [_session('a', 'Cached session')]));
    await _show(tester, node);
    node.nextCatalog = () async => throw const NodeException('节点暂时离线');
    await tester.pump(const Duration(seconds: 5));
    await tester.pump();

    expect(find.textContaining('节点暂时离线'), findsOneWidget);
    expect(find.textContaining('显示上次读取的结果'), findsOneWidget);
    expect(find.text('Cached session'), findsOneWidget);
    node.data = _catalog(sessions: [_session('b', 'Current session')]);
    await tester.tap(find.byTooltip('刷新会话'));
    await tester.pumpAndSettle();
    expect(find.textContaining('节点暂时离线'), findsNothing);
    expect(find.text('Cached session'), findsNothing);
    expect(find.text('Current session'), findsOneWidget);
  });

  testWidgets(
    '044 narrow viewport at 2x keeps long labels and controls usable',
    (tester) async {
      _setViewport(tester, const Size(320, 568));
      final platform = tester.platformDispatcher;
      platform.textScaleFactorTestValue = 2;
      addTearDown(platform.clearTextScaleFactorTestValue);
      const name = '多语言协作与移动工作空间体验改进项目';
      const title = '持续检查移动视口下的会话列表与会话操作';
      await _show(
        tester,
        _Node(
          WorkspaceCatalog(
            projects: const [
              WorkProject(id: 'alpha', name: name, path: '/work/alpha'),
            ],
            sessions: [_session('a', title, status: 'running')],
          ),
        ),
      );

      await _selectProject(tester, name);
      expect(find.text('运行中'), findsOneWidget);
      final menu = find.byTooltip('$name · $title：会话操作');
      for (final control in [
        find.byType(TextField),
        _projectFilter,
        find.widgetWithText(OutlinedButton, '包含归档'),
        find.byTooltip('刷新会话'),
        find.widgetWithText(OutlinedButton, '新建会话'),
        menu,
      ]) {
        await tester.ensureVisible(control);
        await tester.pumpAndSettle();
        expect(control.hitTestable(), findsOneWidget);
        expect(tester.getSize(control).shortestSide, greaterThanOrEqualTo(44));
      }
      await tester.tap(menu);
      await tester.pumpAndSettle();
      expect(find.byType(WorkPanel), findsOneWidget);
      expect(find.text('重命名').hitTestable(), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('045 session status text stays readable across theme changes', (
    tester,
  ) async {
    final platform = tester.platformDispatcher;
    platform.platformBrightnessTestValue = Brightness.light;
    addTearDown(platform.clearPlatformBrightnessTestValue);
    await _show(
      tester,
      _Node(
        _catalog(sessions: [_session('a', 'Active task', status: 'running')]),
      ),
    );

    for (final (brightness, expectedBackground) in [
      (Brightness.light, const Color(0xfffafafa)),
      (Brightness.dark, const Color(0xff1b1b1b)),
    ]) {
      platform.platformBrightnessTestValue = brightness;
      await tester.pumpAndSettle();
      final text = find.text('运行中 · Alpha');
      final theme = Theme.of(tester.element(text));
      final surface = tester
          .widget<Scaffold>(find.byType(Scaffold))
          .backgroundColor!;
      final foreground = tester.widget<Text>(text).style!.color!;
      expect(theme.brightness, brightness);
      expect(surface, expectedBackground);
      expect(_contrast(foreground, surface), greaterThanOrEqualTo(4.5));
    }
  });

  testWidgets(
    '046 expired authorization follows the node disconnect callback',
    (tester) async {
      final node = _Node(_catalog())
        ..nextCatalog = () async =>
            throw const NodeException('授权已过期', status: 401);
      var expired = 0;
      await _mount(tester, node, authorizationLost: () => expired++);
      await tester.pump();

      expect(expired, 1);
      expect(find.textContaining('无法刷新项目和会话'), findsNothing);
      expect(find.text('节点还没有项目。请先新建项目。'), findsNothing);
    },
  );
}

const _projects = [
  WorkProject(id: 'alpha', name: 'Alpha', path: '/work/alpha'),
  WorkProject(id: 'beta', name: 'Beta', path: '/work/beta'),
];

WorkspaceCatalog _catalog({List<SessionSummary> sessions = const []}) =>
    WorkspaceCatalog(projects: _projects, sessions: sessions);

SessionSummary _session(
  String id,
  String title, {
  String project = 'alpha',
  String status = 'idle',
  int updated = 0,
  bool pinned = false,
  bool archived = false,
}) => SessionSummary(
  id: id,
  title: title,
  projectId: project,
  status: status,
  updatedAt: updated,
  lastSeq: 0,
  pinned: pinned,
  archived: archived,
);

void _setViewport(WidgetTester tester, Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.resetDevicePixelRatio);
  addTearDown(tester.view.resetPhysicalSize);
}

Future<void> _mount(
  WidgetTester tester,
  _Node node, {
  VoidCallback? authorizationLost,
}) => tester.pumpWidget(
  WorkMobileApp(
    home: WorkspaceScreen(
      node: node,
      disconnect: () => fail('Unexpected disconnect'),
      authorizationLost:
          authorizationLost ?? () => fail('Unexpected authorization loss'),
    ),
  ),
);

Future<void> _show(WidgetTester tester, _Node node) async {
  await _mount(tester, node);
  await tester.pumpAndSettle();
}

final _projectFilter = find.byKey(const ValueKey('project-filter'));

Future<void> _selectProject(WidgetTester tester, String name) async {
  await tester.tap(_projectFilter);
  await tester.pumpAndSettle();
  expect(find.byType(WorkPanel), findsOneWidget);
  await tester.tap(find.text(name).hitTestable().last);
  await tester.pumpAndSettle();
}

Future<void> _search(WidgetTester tester, String query) async {
  await tester.enterText(find.byType(TextField), query);
  await tester.pumpAndSettle();
}

Future<void> _sessionAction(
  WidgetTester tester,
  String menu,
  String action,
) async {
  await tester.tap(find.byTooltip(menu));
  await tester.pumpAndSettle();
  await tester.tap(find.text(action).hitTestable().last);
  await tester.pumpAndSettle();
}

Future<void> _beginProject(WidgetTester tester, String name) async {
  await tester.tap(find.byTooltip('新建项目'));
  await tester.pumpAndSettle();
  await tester.enterText(find.widgetWithText(TextField, '项目名称'), name);
  await tester.tap(find.text('保存'));
}

double _contrast(Color foreground, Color background) {
  final light = foreground.computeLuminance() + 0.05;
  final dark = background.computeLuminance() + 0.05;
  return light > dark ? light / dark : dark / light;
}

class _Node extends Fake implements WorkNode {
  _Node(this.data);

  WorkspaceCatalog data;
  Future<WorkspaceCatalog> Function()? nextCatalog;
  Future<void>? projectGate;
  int catalogCalls = 0;
  final createdProjects = <String>[];
  final sessionProjects = <String>[];
  final openedSessions = <String>[];
  final renames = <(String, String)>[];
  final preferences = <({String id, bool? pinned, bool? archived})>[];

  @override
  Future<WorkspaceCatalog> catalog() async {
    catalogCalls++;
    final next = nextCatalog;
    nextCatalog = null;
    return next == null ? data : next();
  }

  @override
  Future<WorkProject> createProject(String name) async {
    createdProjects.add(name);
    await projectGate;
    final project = WorkProject(id: 'gamma', name: name, path: '/work/gamma');
    data = WorkspaceCatalog(
      projects: [...data.projects, project],
      sessions: data.sessions,
    );
    return project;
  }

  @override
  Future<String> createSession(String projectId) async {
    sessionProjects.add(projectId);
    final session = _session('new', 'New conversation', project: projectId);
    data = WorkspaceCatalog(
      projects: data.projects,
      sessions: [...data.sessions, session],
    );
    return session.id;
  }

  @override
  Future<SessionSummary> getSession(String id) async =>
      data.sessions.singleWhere((session) => session.id == id);

  @override
  Future<SessionSummary> openSession(String id) {
    openedSessions.add(id);
    return getSession(id);
  }

  @override
  Future<void> renameSession(String id, String title) async {
    renames.add((id, title));
    _edit(id, title: title);
  }

  @override
  Future<void> setSessionPreferences(
    String id, {
    bool? pinned,
    bool? archived,
  }) async {
    preferences.add((id: id, pinned: pinned, archived: archived));
    _edit(id, pinned: pinned, archived: archived);
  }

  void _edit(String id, {String? title, bool? pinned, bool? archived}) {
    data = WorkspaceCatalog(
      projects: data.projects,
      sessions: [
        for (final session in data.sessions)
          if (session.id == id)
            SessionSummary(
              id: id,
              title: title ?? session.title,
              projectId: session.projectId,
              status: session.status,
              updatedAt: session.updatedAt,
              lastSeq: session.lastSeq,
              pinned: pinned ?? session.pinned,
              archived: archived ?? session.archived,
            )
          else
            session,
      ],
    );
  }

  @override
  Future<SessionAccess> access(String id) async => const SessionAccess(
    commands: {},
    snapshotLimit: false,
    dialogReply: false,
  );

  @override
  Future<SessionSnapshot> snapshot(String id, {int? limit}) async =>
      SessionSnapshot(
        sessionId: id,
        status: 'idle',
        lastSeq: 0,
        messages: const [],
        events: const [],
        pendingDialogs: const [],
        hasMore: false,
      );

  final _stream = StreamController<WireEvent>.broadcast();

  @override
  Stream<WireEvent> events(
    String id, {
    required int afterSeq,
    void Function()? onSubscribed,
  }) {
    if (onSubscribed != null) scheduleMicrotask(onSubscribed);
    return _stream.stream;
  }
}
