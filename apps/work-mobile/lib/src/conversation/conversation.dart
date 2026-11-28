import 'dart:async';
import 'dart:io';

// Keep the framework Material theme shared with the Markdown renderer.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:lucide_icons_flutter/lucide_icons_300.dart';

import '../node/work_node.dart';
import '../work_controls.dart';
import '../work_theme.dart';
import 'draft_attachment.dart';
import 'transcript.dart';

class SessionSubmission extends ChangeNotifier {
  String draft = '';
  int draftRevision = 0;
  final _attachments = <DraftAttachment>[];
  bool _readingAttachments = false;

  void changeDraft(String value) {
    if (draft == value) return;
    draft = value;
    draftRevision++;
    refresh();
  }

  void refresh() => notifyListeners();

  bool sending = false;
  bool unknown = false;
  int? cursor;
}

class ConversationScreen extends StatefulWidget {
  const ConversationScreen({
    super.key,
    required this.node,
    required this.session,
    required this.submission,
    required this.close,
    this.switchSession,
    required this.authorizationLost,
  });

  final WorkNode node;
  final SessionSummary session;
  final SessionSubmission submission;
  final VoidCallback close;
  final VoidCallback? switchSession;
  final VoidCallback authorizationLost;

  @override
  State<ConversationScreen> createState() => _ConversationScreenState();
}

class _ConversationScreenState extends State<ConversationScreen>
    with WidgetsBindingObserver {
  late final _draft = TextEditingController(text: _submission.draft);
  final _scroll = ScrollController();
  StreamSubscription<WireEvent>? _live;
  Timer? _retry;
  Transcript? _transcript;
  SessionAccess? _access;
  String? _connectionError;
  String? _actionError;
  String? _dialogRequest;
  bool _loading = true;
  bool _online = false;
  bool _opened = false;
  bool _active = true;
  bool _stopping = false;
  bool _cancelRequested = false;
  bool _canReadState = false;
  bool _composerFocused = false;
  int _generation = 0;
  int _failures = 0;
  int _limit = 80;
  int _targetSeq = 0;

  String get _id => widget.session.id;
  SessionSubmission get _submission => widget.submission;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _draft.addListener(_draftUpdated);
    _submission.addListener(_submissionChanged);
    unawaited(_sync());
  }

  void _draftUpdated() => _submission.changeDraft(_draft.text);

  void _submissionChanged() {
    if (!mounted) return;
    if (_draft.text != _submission.draft) {
      _draft.value = TextEditingValue(
        text: _submission.draft,
        selection: TextSelection.collapsed(offset: _submission.draft.length),
      );
    }
    setState(() {});
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _retry?.cancel();
    _cancelLive();
    _submission.removeListener(_submissionChanged);
    _draft.dispose();
    _scroll.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed && !_active) {
      _active = true;
      unawaited(_sync());
    } else if (state == AppLifecycleState.hidden ||
        state == AppLifecycleState.paused) {
      if (!_active) return;
      _active = false;
      _generation++;
      _retry?.cancel();
      _cancelLive();
      setState(() => _online = false);
    }
  }

  bool _current(int generation) =>
      mounted && _active && generation == _generation;

  void _cancelLive() {
    final live = _live;
    _live = null;
    // Cancellation stops delivery immediately. Socket cleanup can finish
    // independently of the next snapshot and subscription.
    live?.cancel().ignore();
  }

  Future<void> _sync() async {
    final generation = ++_generation;
    _retry?.cancel();
    _cancelLive();
    setState(() {
      _loading = true;
      _online = false;
      _access = null;
      _connectionError = null;
    });
    try {
      NodeException? openError;
      if (!_opened) {
        try {
          await widget.node.openSession(_id);
          if (!_current(generation)) return;
          _opened = true;
        } on NodeException catch (error) {
          if (error.authorizationLost) rethrow;
          openError = error;
        }
      }
      SessionAccess? access;
      if (openError == null) {
        try {
          access = await widget.node.access(_id);
        } on NodeException catch (error) {
          if (error.authorizationLost) rethrow;
          openError = error;
        }
      }
      if (!_current(generation)) return;
      final snapshot = await widget.node.snapshot(
        _id,
        limit: access?.snapshotLimit == true ? _limit : null,
      );
      if (!_current(generation)) return;
      final projection = Transcript.fromSnapshot(snapshot);
      setState(() {
        _transcript = projection;
        _access = access;
        _targetSeq = snapshot.lastSeq;
        _canReadState = _knownState(snapshot.status);
        _loading = false;
        _connectionError = openError?.message;
        _settledThrough(snapshot);
        if (!projection.running) _cancelRequested = false;
      });
      if (openError != null) return;
      _live = widget.node
          .events(
            _id,
            afterSeq: projection.cursor,
            onSubscribed: () => unawaited(_subscriptionReady(generation)),
          )
          .listen(
            (event) => _onEvent(generation, projection, event),
            onError: (Object error) => _liveFailed(generation, error),
            onDone: () {
              if (!_current(generation) || _retry?.isActive == true) return;
              _liveFailed(generation, const NodeException('实时连接已结束。'));
            },
            cancelOnError: true,
          );
    } on NodeException catch (error) {
      _liveFailed(generation, error);
    }
  }

  void _settledThrough(SessionSnapshot snapshot) {
    final start = _submission.cursor;
    if (start != null &&
        snapshot.events.any(
          (event) => event.type == 'agent_settled' && (event.seq ?? -1) > start,
        )) {
      _settled();
    }
  }

  void _settled() {
    _submission.cursor = null;
    _submission.unknown = false;
    _cancelRequested = false;
  }

  bool _knownState(String? status) => switch (status) {
    'ready' ||
    'idle' ||
    'running' ||
    'awaiting_input' ||
    'waiting_for_input' ||
    'cancelling' ||
    'stopping' => true,
    _ => false,
  };

  void _onEvent(int generation, Transcript projection, WireEvent event) {
    if (!_current(generation)) return;
    final nearEnd = !_scroll.hasClients || _scroll.position.extentBefore < 120;
    try {
      projection.apply(event);
    } on TranscriptResyncRequired catch (error) {
      _liveFailed(generation, error);
      return;
    }
    setState(() {
      if (event.type == 'ready') {
        final target = event.data['lastSeq'];
        if (target is int && target > _targetSeq) _targetSeq = target;
      }
      if (event.type == 'operation_accepted' || event.type == 'agent_start') {
        _canReadState = true;
      }
      if (event.type == 'agent_settled') {
        _canReadState = true;
        if (_submission.cursor case final start?) {
          if ((event.seq ?? projection.cursor) > start) _settled();
        }
        _cancelRequested = false;
      }
    });
    if (nearEnd) _scrollToEnd();
  }

  Future<void> _subscriptionReady(int generation) async {
    try {
      final summary = await widget.node.getSession(_id);
      if (!_current(generation) || _live == null) return;
      setState(() {
        if (summary.lastSeq > _targetSeq) _targetSeq = summary.lastSeq;
        _failures = 0;
        _online = true;
        _connectionError = null;
      });
    } on NodeException catch (error) {
      _liveFailed(generation, error);
    }
  }

  void _liveFailed(int generation, Object error) {
    if (!_current(generation)) return;
    if (error is NodeException && error.authorizationLost) {
      widget.authorizationLost();
      return;
    }
    _cancelLive();
    setState(() {
      _loading = false;
      _online = false;
      _connectionError = error is NodeException ? error.message : '会话需要重新同步。';
    });
    _retry?.cancel();
    if (error is NodeException && error.status == 404) return;
    final seconds = 1 << _failures.clamp(0, 4);
    _failures++;
    _retry = Timer(Duration(seconds: seconds), () {
      if (_current(generation)) unawaited(_sync());
    });
  }

  bool get _caughtUp => _online && (_transcript?.cursor ?? -1) >= _targetSeq;
  bool get _canSend =>
      _caughtUp &&
      _canReadState &&
      _transcript?.running == false &&
      _transcript!.dialogs.isEmpty &&
      _access?.commands.contains('prompt') == true &&
      !_submission.sending &&
      _submission.cursor == null;

  bool get _hasDraft =>
      _draft.text.trim().isNotEmpty || _submission._attachments.isNotEmpty;

  bool get _canAddAttachments =>
      _access?.commands.contains('prompt') == true &&
      !_submission.sending &&
      !_submission._readingAttachments;

  Future<void> _pickAttachments() async {
    if (_canAddAttachments) await _loadAttachments(DraftAttachment.pickFiles);
  }

  Future<void> _paste(VoidCallback pasteText) async {
    if (_submission._readingAttachments) return;
    if (_canAddAttachments &&
        await _loadAttachments(DraftAttachment.readClipboard)) {
      return;
    }
    if (mounted) pasteText();
  }

  Future<bool> _loadAttachments(
    Future<List<DraftAttachment>> Function() read,
  ) async {
    final submission = _submission;
    submission._readingAttachments = true;
    submission.refresh();
    try {
      final files = await read();
      if (files.isEmpty) return false;
      DraftAttachment.validateCount([...submission._attachments, ...files]);
      submission._attachments.addAll(files);
      if (mounted) _actionError = null;
    } on FormatException catch (error) {
      if (mounted) _actionError = error.message;
    } on PlatformException catch (error) {
      if (mounted) _actionError = error.message ?? '无法读取附件。';
    } on MissingPluginException {
      if (mounted) _actionError = '此设备无法打开文件选择器。';
    } on FileSystemException {
      if (mounted) _actionError = '无法读取附件，请重新选择文件。';
    } finally {
      submission._readingAttachments = false;
      submission.refresh();
    }
    return true;
  }

  Widget _pasteMenu(BuildContext context, EditableTextState editor) {
    final buttons = editor.contextMenuButtonItems;
    if (_access?.commands.contains('prompt') == true) {
      final paste = ContextMenuButtonItem(
        kind: ContextMenuButtonKind.paste,
        onPressed: () {
          editor.hideToolbar();
          unawaited(
            _paste(() {
              if (editor.mounted) {
                unawaited(editor.pasteText(SelectionChangedCause.toolbar));
              }
            }),
          );
        },
      );
      final index = buttons.indexWhere(
        (item) => item.kind == ContextMenuButtonKind.paste,
      );
      if (index == -1) {
        buttons.insert(0, paste);
      } else {
        buttons[index] = paste;
      }
    }
    return workSelectionMenu(context, editor.contextMenuAnchors, buttons);
  }

  Future<void> _send() async {
    final submission = _submission;
    if (!_canSend || !_hasDraft || submission._readingAttachments) return;
    final files = List.of(submission._attachments);
    final images = files.where((file) => file.mime != null);
    if (images.isNotEmpty && _access?.imagePrompts != true) {
      setState(() => _actionError = '当前会话不支持发送图片。请移除图片或切换会话。');
      return;
    }
    final String text;
    try {
      text = DraftAttachment.promptText(_draft.text, files);
    } on FormatException catch (error) {
      setState(() => _actionError = error.message);
      return;
    }
    final revision = submission.draftRevision;
    final start = _transcript!.cursor;
    setState(() {
      _actionError = null;
      submission.sending = true;
      submission.cursor = start;
      submission.unknown = false;
    });
    try {
      if (images.isEmpty) {
        await widget.node.prompt(_id, text);
      } else {
        await widget.node.promptWithImages(
          _id,
          text,
          images.map((image) => image.imageBlock()).toList(),
        );
      }
      submission._attachments.removeWhere(files.contains);
      if (revision == submission.draftRevision) submission.changeDraft('');
    } on NodeException catch (error) {
      if (submission.cursor == start) {
        if (error.outcomeUnknown) {
          submission.unknown = true;
        } else {
          submission.cursor = null;
        }
      }
      if (!mounted) return;
      if (error.authorizationLost) {
        widget.authorizationLost();
      } else {
        setState(
          () => _actionError = error.outcomeUnknown
              ? '发送结果未知。请刷新并核对会话记录，草稿已保留。'
              : error.message,
        );
      }
    } finally {
      submission.sending = false;
      submission.refresh();
    }
  }

  bool get _canReview =>
      _submission.cursor != null &&
      !_submission.sending &&
      _caughtUp &&
      _canReadState &&
      _transcript?.running == false &&
      _transcript!.dialogs.isEmpty;

  Future<void> _unlockAfterReview() async {
    if (!_canReview) return;
    final cursor = _submission.cursor;
    final proceed = await showDialog<bool>(
      context: context,
      builder: (context) => WorkDialog(
        title: '恢复发送',
        content: const Text('请先核对会话记录。若上一条消息已经送达，再次发送会产生重复消息。'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('返回'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('已核对，恢复发送'),
          ),
        ],
      ),
    );
    if (mounted &&
        proceed == true &&
        _canReview &&
        _submission.cursor == cursor) {
      setState(() {
        _settled();
        _actionError = null;
      });
    }
  }

  bool get _canAbort =>
      _caughtUp &&
      _transcript?.running == true &&
      _access?.commands.contains('abort') == true &&
      !_stopping &&
      !_cancelRequested;

  Future<void> _abort() async {
    if (!_canAbort) return;
    final settled = _transcript?.settlementCount;
    setState(() {
      _stopping = true;
      _actionError = null;
    });
    try {
      await widget.node.abort(_id);
      if (mounted &&
          _transcript?.running == true &&
          _transcript?.settlementCount == settled) {
        setState(() => _cancelRequested = true);
      }
    } on NodeException catch (error) {
      _actionFailed(error);
      if (mounted && error.outcomeUnknown) unawaited(_sync());
    } finally {
      if (mounted) setState(() => _stopping = false);
    }
  }

  Future<void> _respond(
    PendingDialog dialog, {
    Object? value,
    bool cancelled = false,
  }) async {
    if (_dialogRequest != null ||
        !_caughtUp ||
        _access?.dialogReply != true ||
        _transcript?.dialogs.any((item) => item.id == dialog.id) != true) {
      return;
    }
    setState(() {
      _dialogRequest = dialog.id;
      _actionError = null;
    });
    try {
      await widget.node.respondToDialog(
        _id,
        dialog.id,
        value: value,
        cancelled: cancelled,
      );
      if (mounted) await _sync();
    } on NodeException catch (error) {
      _actionFailed(error);
      if (mounted && error.outcomeUnknown) await _sync();
    } finally {
      if (mounted) setState(() => _dialogRequest = null);
    }
  }

  void _actionFailed(NodeException error) {
    if (!mounted) return;
    if (error.authorizationLost) {
      widget.authorizationLost();
    } else {
      setState(
        () => _actionError = error.outcomeUnknown
            ? '操作结果未知。正在重新读取会话状态。'
            : error.message,
      );
    }
  }

  void _scrollToEnd() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _scroll.hasClients) {
        _scroll.jumpTo(_scroll.position.minScrollExtent);
      }
    });
  }

  Widget _title(ThemeData theme, String status, TextStyle? statusStyle) =>
      Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            widget.session.title,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
          Semantics(
            liveRegion: true,
            child: Text(
              status,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: statusStyle?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
        ],
      );

  @override
  Widget build(BuildContext context) {
    final transcript = _transcript;
    final dialogs = transcript?.dialogs ?? const <PendingDialog>[];
    final running = transcript?.running == true;
    final pending = _submission.cursor != null;
    final theme = Theme.of(context);
    final scaler = MediaQuery.textScalerOf(context);
    final titleStyle = theme.appBarTheme.titleTextStyle;
    final statusStyle = theme.textTheme.labelMedium;
    final titleHeight =
        scaler.scale(titleStyle?.fontSize ?? 14) * (titleStyle?.height ?? 1.35);
    final statusHeight =
        scaler.scale(statusStyle?.fontSize ?? 12) *
        (statusStyle?.height ?? 1.35);
    final status = _loading
        ? '正在同步…'
        : _connectionError != null
        ? '实时连接中断'
        : !_online
        ? (_active ? '正在连接…' : '实时连接已暂停')
        : !_caughtUp
        ? '正在追赶会话记录…'
        : _cancelRequested
        ? '正在停止…'
        : dialogs.isNotEmpty
        ? '等待回答'
        : running
        ? '运行中'
        : pending
        ? (_submission.unknown ? '发送待核对' : '等待节点确认')
        : '已连接';
    return Scaffold(
      appBar: AppBar(
        toolbarHeight: (titleHeight + statusHeight + 8).clamp(
          theme.appBarTheme.toolbarHeight ?? kToolbarHeight,
          double.infinity,
        ),
        leading: IconButton(
          tooltip: MaterialLocalizations.of(context).backButtonTooltip,
          onPressed: widget.close,
          icon: const Icon(LucideIcons300.arrowLeftDir),
        ),
        titleSpacing: 0,
        title: widget.switchSession == null
            ? _title(theme, status, statusStyle)
            : Tooltip(
                message: '切换会话',
                child: Semantics(
                  button: true,
                  child: InkWell(
                    onTap: widget.switchSession,
                    borderRadius: workControlRadius,
                    child: Padding(
                      padding: const EdgeInsetsDirectional.only(
                        end: 10,
                        top: 2,
                        bottom: 2,
                      ),
                      child: Row(
                        children: [
                          Flexible(child: _title(theme, status, statusStyle)),
                          const SizedBox(width: 4),
                          Icon(
                            LucideIcons300.chevronsUpDown,
                            size: 15,
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
        actions: [
          IconButton(
            tooltip: '重新同步',
            onPressed: _loading ? null : _sync,
            icon: const Icon(LucideIcons300.refreshCw, size: 18),
          ),
          const SizedBox(width: 4),
        ],
      ),
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 760),
            child: LayoutBuilder(
              builder: (context, constraints) => Column(
                children: [
                  if (_loading && transcript != null)
                    const LinearProgressIndicator(),
                  Expanded(child: _timeline(transcript, dialogs)),
                  if (_connectionError != null ||
                      _actionError != null ||
                      _canReview)
                    ConstrainedBox(
                      constraints: BoxConstraints(
                        maxHeight: (constraints.maxHeight - 74).clamp(
                          0.0,
                          constraints.maxHeight * 0.35,
                        ),
                      ),
                      child: SingleChildScrollView(
                        primary: false,
                        padding: const EdgeInsets.symmetric(horizontal: 16),
                        child: Column(
                          children: [
                            if (_canReview)
                              TextButton(
                                onPressed: _unlockAfterReview,
                                child: const Text('已核对会话，恢复发送'),
                              ),
                            for (final message in [
                              ?_connectionError,
                              ?_actionError,
                            ])
                              Padding(
                                padding: const EdgeInsets.symmetric(
                                  vertical: 4,
                                ),
                                child: Semantics(
                                  liveRegion: true,
                                  child: Text(
                                    message,
                                    style: TextStyle(
                                      color: theme.colorScheme.error,
                                    ),
                                  ),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
                  _composer(running, constraints.maxHeight),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _timeline(Transcript? transcript, List<PendingDialog> dialogs) {
    if (transcript == null) {
      return Center(
        child: _loading
            ? const CircularProgressIndicator()
            : const Text('会话记录暂时无法读取。'),
      );
    }
    final messages = transcript.messages;
    final hasMore = transcript.hasMore;
    if (messages.isEmpty && dialogs.isEmpty && !hasMore) {
      return const Center(child: Text('此会话暂无消息'));
    }
    final itemCount = messages.length + dialogs.length + (hasMore ? 1 : 0);
    return ListView.builder(
      controller: _scroll,
      // Anchor to the latest row without estimating the height of older rows.
      reverse: true,
      keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
      padding: const EdgeInsets.fromLTRB(18, 18, 18, 12),
      itemCount: itemCount,
      itemBuilder: (context, reversedIndex) {
        final index = itemCount - 1 - reversedIndex;
        if (hasMore && index == 0) {
          return Align(
            child: TextButton.icon(
              onPressed:
                  _loading || _limit >= 2000 || _access?.snapshotLimit != true
                  ? null
                  : () {
                      _limit = (_limit + 100).clamp(80, 2000);
                      unawaited(_sync());
                    },
              icon: const Icon(LucideIcons300.history),
              label: Text(_limit >= 2000 ? '已达可加载的历史上限' : '加载更早的消息'),
            ),
          );
        }
        final position = index - (hasMore ? 1 : 0);
        if (position < messages.length) {
          return ChatMessageTile(message: messages[position]);
        }
        final dialog = dialogs[position - messages.length];
        return _QuestionCard(
          key: ValueKey(dialog.id),
          dialog: dialog,
          enabled:
              _caughtUp &&
              _access?.dialogReply == true &&
              _dialogRequest == null,
          answer: (value) => _respond(dialog, value: value),
          cancel: () => _respond(dialog, cancelled: true),
        );
      },
    );
  }

  Widget _composer(bool running, double availableHeight) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final showAbort = running && _access?.commands.contains('abort') == true;
    final action = showAbort
        ? (_canAbort ? _abort : null)
        : (_canSend && _hasDraft && !_submission._readingAttachments
              ? _send
              : null);
    final primary = action != null && !showAbort;
    final compact = availableHeight < 300;
    final files = _submission._attachments;
    const paperclip = Icon(LucideIcons300.paperclip, size: 19);
    final attach = IconButton(
      tooltip: '添加附件',
      onPressed: _canAddAttachments || files.isNotEmpty
          ? _showAttachments
          : null,
      icon: compact && files.isNotEmpty
          ? Badge.count(
              count: files.length,
              backgroundColor: colors.onSurface,
              textColor: colors.surface,
              child: paperclip,
            )
          : paperclip,
    );
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 4, 12, 12),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: colors.surface,
          borderRadius: workComposerRadius,
          border: Border.all(
            color: _composerFocused ? colors.onSurfaceVariant : colors.outline,
          ),
        ),
        child: Padding(
          padding: EdgeInsets.fromLTRB(12, compact ? 3 : 6, 6, 3),
          child: Flex(
            direction: compact ? Axis.horizontal : Axis.vertical,
            crossAxisAlignment: compact
                ? CrossAxisAlignment.center
                : CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              if (compact) attach,
              if (!compact && files.isNotEmpty)
                SizedBox(
                  height: 48,
                  child: ListView(
                    primary: false,
                    scrollDirection: Axis.horizontal,
                    children: [for (final file in files) _attachmentTile(file)],
                  ),
                ),
              Flexible(
                flex: compact ? 1 : 0,
                child: ConstrainedBox(
                  constraints: BoxConstraints(
                    maxHeight: compact
                        ? (availableHeight * 0.65 - 22).clamp(48.0, 112.0)
                        : double.infinity,
                  ),
                  child: Focus(
                    onFocusChange: (focused) =>
                        setState(() => _composerFocused = focused),
                    child: Actions(
                      actions: {
                        PasteTextIntent: _AttachmentPasteAction(_paste),
                      },
                      child: TextField(
                        controller: _draft,
                        contextMenuBuilder: _pasteMenu,
                        minLines: 1,
                        maxLines: compact ? 2 : 4,
                        textInputAction: TextInputAction.newline,
                        style: theme.textTheme.bodyLarge,
                        keyboardType: TextInputType.multiline,
                        decoration: InputDecoration(
                          isCollapsed: true,
                          constraints: const BoxConstraints(minHeight: 48),
                          contentPadding: const EdgeInsetsDirectional.only(
                            end: 6,
                          ),
                          border: InputBorder.none,
                          enabledBorder: InputBorder.none,
                          focusedBorder: InputBorder.none,
                          hintText:
                              _access != null &&
                                  !_access!.commands.contains('prompt')
                              ? '当前会话仅供查看'
                              : '输入消息',
                          hintStyle: theme.textTheme.bodyLarge?.copyWith(
                            color: colors.onSurfaceVariant,
                          ),
                          filled: false,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
              Row(
                mainAxisAlignment: compact
                    ? MainAxisAlignment.end
                    : MainAxisAlignment.spaceBetween,
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (!compact) attach,
                  IconButton(
                    tooltip: showAbort ? '停止生成' : '发送消息',
                    onPressed: action,
                    style: IconButton.styleFrom(
                      padding: EdgeInsets.zero,
                      minimumSize: const Size.square(48),
                      shape: const CircleBorder(),
                    ),
                    icon: DecoratedBox(
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: primary
                            ? colors.onSurface
                            : colors.surfaceContainerHighest,
                      ),
                      child: SizedBox.square(
                        dimension: 32,
                        child: Center(
                          child: !showAbort && _submission.sending
                              ? SizedBox.square(
                                  dimension: 15,
                                  child: CircularProgressIndicator(
                                    strokeWidth: 2,
                                    color: colors.onSurfaceVariant,
                                  ),
                                )
                              : Icon(
                                  showAbort
                                      ? LucideIcons300.square
                                      : LucideIcons300.arrowUp,
                                  size: 17,
                                  color: primary
                                      ? colors.surface
                                      : colors.onSurfaceVariant,
                                ),
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _attachmentTile(DraftAttachment file) => Padding(
    padding: const EdgeInsetsDirectional.only(end: 8),
    child: DecoratedBox(
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Padding(
        padding: const EdgeInsetsDirectional.only(start: 8),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (file.mime == null)
              const Icon(LucideIcons300.fileText, size: 18)
            else
              Image.memory(
                file.bytes,
                width: 32,
                height: 32,
                cacheWidth: 96,
                cacheHeight: 96,
                fit: BoxFit.cover,
                errorBuilder: (_, _, _) =>
                    const Icon(LucideIcons300.image, size: 18),
              ),
            const SizedBox(width: 6),
            Flexible(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 140),
                child: Text(
                  file.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ),
            IconButton(
              tooltip: '移除 ${file.name}',
              onPressed: _submission.sending
                  ? null
                  : () {
                      _submission._attachments.remove(file);
                      _actionError = null;
                      _submission.refresh();
                    },
              icon: const Icon(LucideIcons300.x, size: 16),
            ),
          ],
        ),
      ),
    ),
  );

  Future<void> _showAttachments() => showWorkPanel<void>(
    context,
    title: '附件',
    builder: (context) => ListenableBuilder(
      listenable: _submission,
      builder: (context, _) {
        final theme = Theme.of(context);
        final files = _submission._attachments;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            WorkPanelAction(
              icon: LucideIcons300.folderOpen,
              label: _submission._readingAttachments ? '正在读取附件…' : '选择文件',
              description: 'PNG、JPEG 或 UTF-8 文本文件',
              onPressed: _canAddAttachments ? _pickAttachments : null,
            ),
            if (_actionError case final error?)
              Padding(
                padding: const EdgeInsets.all(8),
                child: Semantics(
                  liveRegion: true,
                  child: Text(
                    error,
                    style: TextStyle(color: theme.colorScheme.error),
                  ),
                ),
              ),
            if (files.isNotEmpty) ...[
              const Padding(
                padding: EdgeInsets.symmetric(vertical: 8),
                child: Divider(),
              ),
              for (final file in files)
                Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: _attachmentTile(file),
                ),
            ],
          ],
        );
      },
    ),
  );
}

class _AttachmentPasteAction extends Action<PasteTextIntent> {
  _AttachmentPasteAction(this.paste);

  final Future<void> Function(VoidCallback) paste;

  @override
  Object? invoke(PasteTextIntent intent) {
    final defaultAction = callingAction;
    if (defaultAction != null) {
      unawaited(paste(() => defaultAction.invoke(intent)));
    }
    return null;
  }
}

class _QuestionCard extends StatefulWidget {
  const _QuestionCard({
    super.key,
    required this.dialog,
    required this.enabled,
    required this.answer,
    required this.cancel,
  });
  final PendingDialog dialog;
  final bool enabled;
  final ValueChanged<Object?> answer;
  final VoidCallback cancel;

  @override
  State<_QuestionCard> createState() => _QuestionCardState();
}

class _QuestionCardState extends State<_QuestionCard> {
  final _input = TextEditingController();

  @override
  void dispose() {
    _input.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final dialog = widget.dialog;
    final enabled = widget.enabled;
    final typed = dialog.method == 'input' || dialog.method == 'editor';
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              dialog.title ?? '节点请求回答',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            if (dialog.message.isNotEmpty) ...[
              const SizedBox(height: 8),
              SelectableText(
                dialog.message,
                contextMenuBuilder: workEditableMenu,
              ),
            ],
            if (dialog.method == 'unsupported') ...[
              const SizedBox(height: 8),
              const Text('请在电脑上处理此请求。'),
            ],
            if (typed) ...[
              const SizedBox(height: 10),
              TextField(
                controller: _input,
                contextMenuBuilder: workEditableMenu,
                enabled: enabled,
                onChanged: (_) => setState(() {}),
                maxLines: dialog.method == 'editor' ? 4 : 1,
                decoration: const InputDecoration(labelText: '回答'),
              ),
            ],
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                if (typed && _input.text.trim().isNotEmpty)
                  FilledButton(
                    onPressed: !enabled
                        ? null
                        : () {
                            final text = _input.text;
                            widget.answer({
                              'kind': 'input',
                              'text': text,
                              'value': text,
                              'values': [text],
                            });
                          },
                    child: const Text('提交回答'),
                  ),
                for (final choice in dialog.choices)
                  OutlinedButton(
                    onPressed: !enabled
                        ? null
                        : () => widget.answer(choice.value),
                    child: Text(choice.label),
                  ),
                if (dialog.method == 'confirm') ...[
                  OutlinedButton(
                    onPressed: !enabled ? null : () => widget.answer(true),
                    child: const Text('确认'),
                  ),
                  OutlinedButton(
                    onPressed: !enabled ? null : () => widget.answer(false),
                    child: const Text('拒绝'),
                  ),
                ],
                TextButton(
                  onPressed: enabled ? widget.cancel : null,
                  child: const Text('取消请求'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
