import 'dart:async';

// Keep the framework Material theme shared with the Markdown renderer.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons_300.dart';

import 'conversation/conversation.dart';
import 'node/work_node.dart';
import 'work_controls.dart';
import 'work_theme.dart';

class WorkspaceScreen extends StatefulWidget {
  const WorkspaceScreen({
    super.key,
    required this.node,
    required this.disconnect,
    required this.authorizationLost,
  });

  final WorkNode node;
  final VoidCallback disconnect;
  final VoidCallback authorizationLost;

  @override
  State<WorkspaceScreen> createState() => _WorkspaceScreenState();
}

class _WorkspaceScreenState extends State<WorkspaceScreen>
    with WidgetsBindingObserver {
  final _submissions = <String, SessionSubmission>{};
  final _search = TextEditingController();
  WorkspaceCatalog? _catalog;
  SessionSummary? _selected;
  String? _projectId;
  String? _error;
  Timer? _refreshTimer;
  Future<void>? _loading;
  bool _includeArchived = false;
  bool _busy = false;
  int _catalogRevision = 0;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    unawaited(_refresh());
    _startRefreshTimer();
  }

  void _startRefreshTimer() {
    _refreshTimer?.cancel();
    _refreshTimer = Timer.periodic(const Duration(seconds: 5), (_) {
      unawaited(_refresh());
    });
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      _startRefreshTimer();
      unawaited(_refresh());
    } else if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.hidden) {
      _refreshTimer?.cancel();
    }
  }

  @override
  void dispose() {
    _search.dispose();
    _refreshTimer?.cancel();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  Future<void> _refresh({bool manual = false}) async {
    if (_busy) return;
    final active = _loading;
    if (active != null) {
      if (!manual) return active;
      await active;
      if (!mounted || _busy) return;
      final queued = _loading;
      if (queued != null) return queued;
    }
    if (_catalog == null && _error != null) {
      setState(() => _error = null);
    }
    final revision = _catalogRevision;
    final pending = _loadCatalog(revision);
    _loading = pending;
    try {
      await pending;
    } finally {
      if (identical(_loading, pending)) _loading = null;
    }
  }

  Future<void> _loadCatalog(int revision) async {
    try {
      final catalog = await widget.node.catalog();
      if (!mounted || revision != _catalogRevision) return;
      setState(() {
        _catalog = catalog;
        _error = null;
        if (!catalog.projects.any((project) => project.id == _projectId)) {
          _projectId = null;
        }
      });
    } on NodeException catch (error) {
      if (!mounted || revision != _catalogRevision) return;
      if (error.authorizationLost) {
        widget.authorizationLost();
      } else {
        setState(() {
          final detail = _catalog == null ? '' : '显示上次读取的结果。';
          _error = '无法刷新项目和会话。$detail${error.message}';
        });
      }
    }
  }

  Future<void> _createProject() async {
    final name = await showDialog<String>(
      context: context,
      builder: (context) => const _NameDialog(heading: '新建项目', label: '项目名称'),
    );
    if (!mounted || name == null) return;
    await _mutate(() async {
      final project = await widget.node.createProject(name);
      if (!mounted) return;
      setState(() {
        _catalog = WorkspaceCatalog(
          projects: [
            ...?_catalog?.projects.where((item) => item.id != project.id),
            project,
          ],
          sessions: _catalog?.sessions ?? const [],
        );
        _projectId = project.id;
      });
    });
  }

  Future<void> _newSession() async {
    final projects = _catalog?.projects ?? const <WorkProject>[];
    if (_busy || projects.isEmpty) return;
    final projectId = _projectId ?? await _chooseProject(projects);
    if (!mounted || projectId == null || _busy) return;
    await _mutate(() async {
      final id = await widget.node.createSession(projectId);
      final summary = await widget.node.getSession(id);
      if (mounted) setState(() => _selected = summary);
    });
  }

  Future<String?> _chooseProject(
    List<WorkProject> projects, {
    bool filter = false,
  }) => showWorkPanel<String>(
    context,
    title: filter ? '筛选项目' : '选择会话所属项目',
    builder: (context) => Column(
      children: [
        if (filter)
          WorkPanelAction(
            icon: LucideIcons300.folders,
            label: '全部项目',
            selected: _projectId == null,
            onPressed: () => Navigator.pop(context, ''),
          ),
        for (final project in projects)
          WorkPanelAction(
            icon: LucideIcons300.folder,
            label: project.name,
            description: project.path,
            selected: filter && project.id == _projectId,
            onPressed: () => Navigator.pop(context, project.id),
          ),
      ],
    ),
  );

  Future<void> _filterProjects(List<WorkProject> projects) async {
    final id = await _chooseProject(projects, filter: true);
    if (!mounted || id == null) return;
    setState(
      () => _projectId =
          _catalog?.projects.any((project) => project.id == id) == true
          ? id
          : null,
    );
  }

  Future<void> _mutate(Future<void> Function() operation) async {
    if (_busy) return;
    _catalogRevision++;
    setState(() => _busy = true);
    try {
      await operation();
    } on NodeException catch (error) {
      if (!mounted) return;
      if (error.authorizationLost) {
        widget.authorizationLost();
        return;
      }
      _snack(error.outcomeUnknown ? '操作结果未知。请核对最新的会话列表。' : error.message);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
    if (mounted) await _refresh(manual: true);
  }

  void _snack(String message) =>
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(message)));

  Future<void> _rename(SessionSummary session) async {
    final title = await showDialog<String>(
      context: context,
      builder: (context) => _NameDialog(
        heading: '重命名会话',
        label: '会话名称',
        initialValue: session.title,
      ),
    );
    if (!mounted || title == null || title == session.title) return;
    await _mutate(() => widget.node.renameSession(session.id, title));
  }

  void _closeConversation() {
    setState(() => _selected = null);
    unawaited(_refresh(manual: true));
  }

  Future<void> _showSessionSwitcher() async {
    final sessions = [...?_catalog?.sessions]
      ..sort(
        (a, b) => a.pinned == b.pinned
            ? b.updatedAt.compareTo(a.updatedAt)
            : a.pinned
            ? -1
            : 1,
      );
    final projectNames = {
      for (final project in _catalog?.projects ?? const <WorkProject>[])
        project.id: project.name,
    };
    final picked = await showWorkPanel<SessionSummary>(
      context,
      title: '切换会话',
      builder: (context) => sessions.isEmpty
          ? const Padding(padding: EdgeInsets.all(12), child: Text('暂无可切换的会话。'))
          : Column(
              children: [
                for (final session in sessions)
                  WorkPanelAction(
                    key: ValueKey('switch-${session.id}'),
                    icon: session.pinned
                        ? LucideIcons300.pin
                        : LucideIcons300.messageSquare,
                    label: session.title,
                    description: [
                      ?projectNames[session.projectId],
                      if (session.status == 'running') '运行中',
                      if (session.archived) '已归档',
                    ].join(' · '),
                    selected: session.id == _selected?.id,
                    onPressed: () => Navigator.pop(context, session),
                  ),
              ],
            ),
    );
    if (!mounted || picked == null || picked.id == _selected?.id) return;
    setState(() => _selected = picked);
  }

  Future<void> _nodeDetails() => showWorkPanel<void>(
    context,
    title: '当前节点',
    builder: (context) => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.all(10),
          child: SelectableText(
            '${widget.node.origin}',
            contextMenuBuilder: workEditableMenu,
          ),
        ),
        const Divider(),
        WorkPanelAction(
          icon: LucideIcons300.scanLine,
          label: '断开并重新扫码',
          onPressed: () {
            Navigator.pop(context);
            widget.disconnect();
          },
        ),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) {
    final selected = _selected;
    if (selected != null) {
      final current =
          _catalog?.sessions
              .where((session) => session.id == selected.id)
              .firstOrNull ??
          selected;
      return PopScope(
        canPop: false,
        onPopInvokedWithResult: (didPop, _) {
          if (!didPop && mounted) _closeConversation();
        },
        child: _ConversationReveal(
          key: ValueKey('reveal-${selected.id}'),
          child: ConversationScreen(
            key: ValueKey(selected.id),
            node: widget.node,
            session: current,
            submission: _submissions.putIfAbsent(
              selected.id,
              SessionSubmission.new,
            ),
            close: _closeConversation,
            switchSession: _showSessionSwitcher,
            authorizationLost: widget.authorizationLost,
          ),
        ),
      );
    }

    final catalog = _catalog;
    final projects = catalog?.projects ?? const <WorkProject>[];
    final projectNames = {
      for (final project in projects) project.id: project.name,
    };
    final query = _search.text.trim().toLowerCase();
    final filtered = query.isNotEmpty || _projectId != null || _includeArchived;
    final sessions =
        (catalog?.sessions ?? const <SessionSummary>[]).where((session) {
          final projectName = projectNames[session.projectId] ?? '';
          return (_includeArchived || !session.archived) &&
              (_projectId == null || session.projectId == _projectId) &&
              (query.isEmpty ||
                  session.title.toLowerCase().contains(query) ||
                  projectName.toLowerCase().contains(query));
        }).toList()..sort(
          (a, b) => a.pinned == b.pinned
              ? b.updatedAt.compareTo(a.updatedAt)
              : a.pinned
              ? -1
              : 1,
        );
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final pinned = sessions.where((session) => session.pinned);
    final recent = sessions.where((session) => !session.pinned);
    return Scaffold(
      backgroundColor: colors.surfaceContainerLow,
      appBar: AppBar(
        title: const WorkWordmark(),
        actions: [
          IconButton(
            onPressed: catalog == null || _busy ? null : _createProject,
            tooltip: '新建项目',
            icon: const Icon(LucideIcons300.folderPlus),
          ),
          IconButton(
            onPressed: _nodeDetails,
            tooltip: '节点连接',
            icon: const Icon(LucideIcons300.monitor),
          ),
          const SizedBox(width: 4),
        ],
      ),
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 720),
            child: RefreshIndicator(
              onRefresh: () => _refresh(manual: true),
              child: ListView(
                physics: const AlwaysScrollableScrollPhysics(),
                padding: const EdgeInsets.fromLTRB(16, 16, 16, 28),
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Text('会话', style: theme.textTheme.titleMedium),
                      ),
                      OutlinedButton.icon(
                        onPressed: _busy || projects.isEmpty
                            ? null
                            : _newSession,
                        icon: const Icon(LucideIcons300.squarePen, size: 16),
                        label: const Text('新建会话'),
                      ),
                    ],
                  ),
                  const SizedBox(height: 12),
                  TextField(
                    controller: _search,
                    contextMenuBuilder: workEditableMenu,
                    onChanged: (_) => setState(() {}),
                    decoration: InputDecoration(
                      labelText: '搜索会话',
                      prefixIcon: const Icon(LucideIcons300.search, size: 16),
                      suffixIcon: _search.text.isEmpty
                          ? null
                          : IconButton(
                              tooltip: '清除搜索',
                              onPressed: () => setState(_search.clear),
                              icon: const Icon(LucideIcons300.x),
                            ),
                    ),
                  ),
                  const SizedBox(height: 8),
                  Row(
                    children: [
                      Expanded(
                        child: OutlinedButton(
                          key: const ValueKey('project-filter'),
                          onPressed: () => _filterProjects(projects),
                          child: Row(
                            children: [
                              const Icon(LucideIcons300.folder, size: 16),
                              const SizedBox(width: 10),
                              Expanded(
                                child: Padding(
                                  padding: const EdgeInsets.symmetric(
                                    vertical: 4,
                                  ),
                                  child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Text(
                                        '筛选项目',
                                        style: theme.textTheme.labelMedium
                                            ?.copyWith(
                                              color: colors.onSurfaceVariant,
                                            ),
                                      ),
                                      Text(
                                        projectNames[_projectId] ?? '全部项目',
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                      ),
                                    ],
                                  ),
                                ),
                              ),
                              const SizedBox(width: 8),
                              const Icon(LucideIcons300.chevronDown, size: 16),
                            ],
                          ),
                        ),
                      ),
                      const SizedBox(width: 4),
                      IconButton(
                        onPressed: _busy ? null : () => _refresh(manual: true),
                        tooltip: '刷新会话',
                        icon: const Icon(LucideIcons300.refreshCw),
                      ),
                    ],
                  ),
                  Align(
                    alignment: AlignmentDirectional.centerStart,
                    child: MergeSemantics(
                      child: Semantics(
                        selected: _includeArchived,
                        child: OutlinedButton.icon(
                          label: const Text('包含归档'),
                          icon: Icon(
                            _includeArchived
                                ? LucideIcons300.check
                                : LucideIcons300.archive,
                            size: 16,
                          ),
                          style: OutlinedButton.styleFrom(
                            backgroundColor: _includeArchived
                                ? colors.surfaceContainerHighest
                                : null,
                          ),
                          onPressed: () => setState(
                            () => _includeArchived = !_includeArchived,
                          ),
                        ),
                      ),
                    ),
                  ),
                  const SizedBox(height: 8),
                  if (_busy || (catalog == null && _error == null))
                    LinearProgressIndicator(
                      semanticsLabel: _busy ? '正在保存更改' : '正在读取项目和会话',
                    ),
                  if (_error case final error?) ...[
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 8),
                      child: Semantics(
                        liveRegion: true,
                        child: Text(
                          error,
                          style: TextStyle(color: colors.error),
                        ),
                      ),
                    ),
                    if (catalog == null)
                      Align(
                        alignment: AlignmentDirectional.centerStart,
                        child: OutlinedButton(
                          onPressed: () => _refresh(manual: true),
                          child: const Text('重试'),
                        ),
                      ),
                  ],
                  if (catalog != null && sessions.isEmpty)
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 24),
                      child: Column(
                        children: [
                          Semantics(
                            liveRegion: true,
                            child: Text(
                              catalog.sessions.isEmpty
                                  ? (projects.isEmpty
                                        ? '节点还没有项目。请先新建项目。'
                                        : '暂无会话。')
                                  : filtered
                                  ? '没有符合条件的会话。'
                                  : '暂无未归档会话。',
                              textAlign: TextAlign.center,
                              style: theme.textTheme.bodyLarge,
                            ),
                          ),
                          if (filtered && catalog.sessions.isNotEmpty)
                            TextButton(
                              onPressed: () => setState(() {
                                _search.clear();
                                _projectId = null;
                                _includeArchived = false;
                              }),
                              child: const Text('清除筛选'),
                            ),
                        ],
                      ),
                    ),
                  if (pinned.isNotEmpty) ...[
                    _sectionHeading('置顶', theme),
                    for (final session in pinned)
                      _sessionTile(session, projectNames[session.projectId]),
                  ],
                  if (recent.isNotEmpty) ...[
                    _sectionHeading(query.isEmpty ? '最近' : '搜索结果', theme),
                    for (final session in recent)
                      _sessionTile(session, projectNames[session.projectId]),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _sectionHeading(String title, ThemeData theme) => Padding(
    padding: const EdgeInsets.fromLTRB(10, 6, 10, 4),
    child: Text(
      title,
      style: theme.textTheme.labelMedium?.copyWith(
        color: theme.colorScheme.onSurfaceVariant,
      ),
    ),
  );

  Widget _sessionTile(SessionSummary session, String? projectName) {
    final subject = projectName == null
        ? session.title
        : '$projectName · ${session.title}';
    final details = [
      if (session.status == 'running') '运行中',
      if (session.archived) '已归档',
      if (_projectId == null && projectName != null) projectName,
    ];
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 2, vertical: 1),
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          key: ValueKey(session.id),
          borderRadius: workControlRadius,
          onTap: () => setState(() => _selected = session),
          child: Padding(
            padding: const EdgeInsetsDirectional.only(
              start: 10,
              end: 2,
              top: 5,
              bottom: 5,
            ),
            child: Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Row(
                        children: [
                          Flexible(
                            child: Text(
                              session.title,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                              style: theme.textTheme.bodyMedium,
                            ),
                          ),
                          if (session.pinned) ...[
                            const SizedBox(width: 5),
                            Semantics(
                              container: true,
                              label: '已置顶',
                              child: Icon(
                                LucideIcons300.pin,
                                size: 13,
                                color: colors.onSurfaceVariant,
                              ),
                            ),
                          ],
                        ],
                      ),
                      if (details.isNotEmpty) ...[
                        const SizedBox(height: 1),
                        Text(
                          details.join(' · '),
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.labelMedium?.copyWith(
                            color: colors.onSurfaceVariant,
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
                IconButton(
                  onPressed: _busy ? null : () => _sessionActions(session),
                  tooltip: '$subject：会话操作',
                  icon: const Icon(LucideIcons300.ellipsis, size: 18),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Future<void> _sessionActions(SessionSummary session) async {
    final action = await showWorkPanel<String>(
      context,
      title: '会话操作',
      builder: (context) => Column(
        children: [
          WorkPanelAction(
            icon: LucideIcons300.squarePen,
            label: '重命名',
            onPressed: () => Navigator.pop(context, 'rename'),
          ),
          WorkPanelAction(
            icon: session.pinned ? LucideIcons300.pinOff : LucideIcons300.pin,
            label: session.pinned ? '取消置顶' : '置顶',
            onPressed: () => Navigator.pop(context, 'pin'),
          ),
          WorkPanelAction(
            icon: session.archived
                ? LucideIcons300.archiveRestore
                : LucideIcons300.archive,
            label: session.archived ? '恢复会话' : '归档',
            onPressed: () => Navigator.pop(context, 'archive'),
          ),
        ],
      ),
    );
    if (!mounted || action == null || _busy) return;
    switch (action) {
      case 'rename':
        await _rename(session);
      case 'pin':
        await _mutate(
          () => widget.node.setSessionPreferences(
            session.id,
            pinned: !session.pinned,
          ),
        );
      case 'archive':
        await _mutate(
          () => widget.node.setSessionPreferences(
            session.id,
            archived: !session.archived,
          ),
        );
    }
  }
}

class _ConversationReveal extends StatefulWidget {
  const _ConversationReveal({super.key, required this.child});

  final Widget child;

  @override
  State<_ConversationReveal> createState() => _ConversationRevealState();
}

class _ConversationRevealState extends State<_ConversationReveal>
    with SingleTickerProviderStateMixin {
  late final _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 190),
  );
  late final _curve = CurvedAnimation(
    parent: _controller,
    curve: Curves.easeOutCubic,
  );
  late final _slide = Tween(
    begin: const Offset(0.035, 0),
    end: Offset.zero,
  ).animate(_curve);
  bool _started = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_started) return;
    _started = true;
    if (MediaQuery.of(context).disableAnimations) {
      _controller.value = 1;
    } else {
      _controller.forward();
    }
  }

  @override
  void dispose() {
    _curve.dispose();
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => FadeTransition(
    opacity: _curve,
    child: SlideTransition(position: _slide, child: widget.child),
  );
}

class _NameDialog extends StatefulWidget {
  const _NameDialog({
    required this.heading,
    required this.label,
    this.initialValue = '',
  });
  final String heading;
  final String label;
  final String initialValue;

  @override
  State<_NameDialog> createState() => _NameDialogState();
}

class _NameDialogState extends State<_NameDialog> {
  late final _controller = TextEditingController(text: widget.initialValue);

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _submit() {
    final title = _controller.text.trim();
    if (title.isNotEmpty) Navigator.pop(context, title);
  }

  @override
  Widget build(BuildContext context) => WorkDialog(
    title: widget.heading,
    content: TextField(
      controller: _controller,
      contextMenuBuilder: workEditableMenu,
      autofocus: true,
      maxLength: 120,
      onSubmitted: (_) => _submit(),
      decoration: InputDecoration(labelText: widget.label),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('取消'),
      ),
      FilledButton(onPressed: _submit, child: const Text('保存')),
    ],
  );
}
