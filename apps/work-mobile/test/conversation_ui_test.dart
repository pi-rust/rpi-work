import 'dart:async';
import 'dart:convert';

import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';

// Use the same framework Material widgets as the app.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:work_mobile/src/work_controls.dart';
import 'package:work_mobile/src/work_theme.dart';
import 'package:work_mobile/src/conversation/conversation.dart';
import 'package:work_mobile/src/node/work_node.dart';

final _input = find.byWidgetPredicate(
  (widget) => widget is TextField && widget.decoration?.hintText != null,
);
Finder _icon(String tooltip) => find.byWidgetPredicate(
  (widget) => widget is IconButton && widget.tooltip == tooltip,
);
final _send = _icon('发送消息');
final _stop = _icon('停止生成');
final _review = find.text('已核对会话，恢复发送');
const _unknown = NodeException('回复超时', outcomeUnknown: true);
const _saved = {'role': 'user', 'content': '已保存的消息'};

void main() {
  testWidgets('the paperclip opens the app panel before selecting a file', (
    t,
  ) async {
    final node = _Node();
    final submission = await _open(t, node);
    final picker = _picker('checks.txt', 'lint passed');
    _clipboard(
      t,
      () => throw StateError('A file picker must not read the clipboard.'),
    );
    await t.tap(_icon('添加附件'));
    await t.pumpAndSettle();
    expect(find.byType(WorkPanel), findsOneWidget);
    expect(picker.openCount, 0);
    await t.tap(find.text('选择文件'));
    expect(picker.openCount, 1);
    await t.pumpAndSettle();
    expect(
      find.descendant(
        of: find.byType(WorkPanel),
        matching: find.text('checks.txt'),
      ),
      findsOneWidget,
    );
    await t.tap(_icon('关闭'));
    await t.pumpAndSettle();
    expect(find.text('checks.txt'), findsOneWidget);
    expect(node.prompts, isEmpty);
    expect(_enabled(t, _send), isTrue);
    await t.tap(_send);
    await t.pumpAndSettle();
    expect(node.prompts, ['[附件：checks.txt]\nlint passed']);
    expect(submission.draft, isEmpty);
    expect(find.text('checks.txt'), findsNothing);
  });

  testWidgets('failed send retains the attachment through the compact editor', (
    t,
  ) async {
    _display(t, const Size(320, 510), scale: 2);
    final node = _Node()..promptFailure = const NodeException('节点拒绝发送');
    await _open(t, node, draft: '检查');
    _picker('notes.md', '# 备注');
    t.view.viewInsets = const FakeViewPadding(bottom: 270);
    await t.pumpAndSettle();
    await t.tap(_icon('添加附件'));
    await t.pumpAndSettle();
    await t.tap(find.text('选择文件'));
    await t.pumpAndSettle();
    await t.tap(_icon('关闭'));
    await t.pumpAndSettle();
    expect(_send.hitTestable(), findsOneWidget);
    await t.tap(_send);
    await t.pumpAndSettle();
    expect(node.prompts, ['检查\n[附件：notes.md]\n# 备注']);
    await t.tap(_icon('添加附件'));
    await t.pumpAndSettle();
    expect(find.text('notes.md'), findsOneWidget);
    await t.tap(_icon('移除 notes.md'));
    await t.pumpAndSettle();
    expect(find.text('notes.md'), findsNothing);
    expect(t.takeException(), isNull);
  });

  testWidgets('canceling the file picker keeps the saved draft', (t) async {
    final submission = await _open(t, _Node(), draft: '已有草稿');
    final picker = _picker();
    await t.tap(_icon('添加附件'));
    await t.pumpAndSettle();
    await t.tap(find.text('选择文件'));
    await t.pumpAndSettle();
    expect(picker.openCount, 1);
    expect(find.byType(WorkPanel), findsOneWidget);
    await t.tap(_icon('关闭'));
    await t.pumpAndSettle();
    expect(submission.draft, '已有草稿');
    expect(_enabled(t, _send), isTrue);
  });

  testWidgets(
    'ordinary Paste replaces selected text without creating an attachment',
    (t) async {
      final submission = await _open(t, _Node(), draft: 'one two');
      _clipboard(t, () => []);
      final messenger = t.binding.defaultBinaryMessenger;
      messenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
        return switch (call.method) {
          'Clipboard.getData' => {'text': 'three'},
          'Clipboard.hasStrings' => {'value': true},
          _ => null,
        };
      });
      addTearDown(
        () => messenger.setMockMethodCallHandler(SystemChannels.platform, null),
      );
      await t.showKeyboard(_input);
      _editor(t).controller.selection = const TextSelection(
        baseOffset: 4,
        extentOffset: 7,
      );
      await _pasteShortcut(t);
      await t.pumpAndSettle();
      expect(submission.draft, 'one three');
      expect(find.byTooltip('移除 three'), findsNothing);
    },
  );

  testWidgets('PNG paste is retained when the session lacks image support', (
    t,
  ) async {
    final node = _Node();
    await _open(t, node);
    _clipboard(
      t,
      () => [
        {'name': 'clip.png', 'bytes': base64Decode(_pixel)},
      ],
    );
    await _pasteImage(t);
    expect(find.text('clip.png'), findsOneWidget);
    await t.tap(_send);
    await t.pumpAndSettle();
    expect(node.prompts, isEmpty);
    expect(find.text('当前会话不支持发送图片。请移除图片或切换会话。'), findsOneWidget);
    expect(find.text('clip.png'), findsOneWidget);
  });

  testWidgets('clipboard images are sent as ordered content blocks', (t) async {
    final node = _Node()..imagePrompts = true;
    await _open(t, node, draft: '截图');
    _clipboard(
      t,
      () => [
        {'name': 'clip.png', 'bytes': base64Decode(_pixel)},
        {'name': 'build.txt', 'bytes': Uint8List.fromList(utf8.encode('OK'))},
      ],
    );
    await _pasteImage(t, shortcut: true);
    expect(find.text('clip.png'), findsOneWidget);
    await t.tap(_send);
    await t.pumpAndSettle();
    expect(node.prompts, isEmpty);
    final (text, images) = node.imageRequests.single;
    expect(text, '截图\n[附件：build.txt]\nOK');
    expect(images, [
      {'type': 'image', 'mimeType': 'image/png', 'data': _pixel},
    ]);
    expect(find.text('clip.png'), findsNothing);
  });

  testWidgets('047 initial history loading gates refresh and send', (t) async {
    final node = _Node()..snapshotRequest = Completer<SessionSnapshot>();
    await _open(t, node, draft: '保留草稿');
    expect(find.text('正在同步…'), findsOneWidget);
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    expect(find.text('此会话暂无消息'), findsNothing);
    expect(_enabled(t, _icon('重新同步')), isFalse);
    expect(_enabled(t, _send), isFalse);
    node.snapshotRequest!.complete(node.frame);
    await t.pumpAndSettle();
  });

  testWidgets('048 completed empty history has its own connected state', (
    t,
  ) async {
    await _open(t, _Node());
    expect(find.text('此会话暂无消息'), findsOneWidget);
    expect(find.text('已连接'), findsOneWidget);
    expect(find.byType(CircularProgressIndicator), findsNothing);
  });

  testWidgets('049 an empty limited page still loads older history', (t) async {
    final cleanup = Completer<void>();
    final node = _Node()
      ..frame = _snapshot(hasMore: true)
      ..nextStreamCleanup = cleanup;
    await _open(t, node);
    final previous = node.stream!;
    final earlier = find.text('加载更早的消息');
    expect(earlier.hitTestable(), findsOneWidget);
    node.frame = _snapshot(messages: [_saved]);
    try {
      await t.tap(earlier);
      await t.pumpAndSettle();
      expect(previous.hasListener, isFalse);
      expect(node.stream, isNot(same(previous)));
      expect(node.snapshotLimits, [80, 180]);
      expect(node.afterSequences, [7, 7]);
      expect(find.text(_saved['content']!), findsOneWidget);
    } finally {
      cleanup.complete();
    }
    await t.pump();
  });

  testWidgets('050 dragging saved history dismisses the draft keyboard', (
    t,
  ) async {
    final node = _Node()
      ..frame = _snapshot(
        messages: [
          for (var i = 0; i < 35; i++) {'role': 'user', 'content': '历史消息 $i'},
        ],
      );
    final submission = await _open(t, node, draft: '未发送');
    expect(find.text('历史消息 34').hitTestable(), findsOneWidget);
    await t.showKeyboard(_input);
    await t.drag(find.byType(ListView), const Offset(0, 220));
    await t.pumpAndSettle();
    expect(_editor(t).focusNode.hasFocus, isFalse);
    expect(submission.draft, '未发送');
  });

  testWidgets('051 subscription handshake displays connecting and gates send', (
    t,
  ) async {
    final node = _Node()..autoSubscribe = false;
    await _open(t, node, draft: '待发送');
    expect(find.text('正在连接…'), findsOneWidget);
    expect(find.text('实时连接中断'), findsNothing);
    expect(_enabled(t, _send), isFalse);
    node.subscribe!();
    await t.pump();
    expect(_enabled(t, _send), isTrue);
  });

  testWidgets('052 live replay must reach the readback cursor before send', (
    t,
  ) async {
    final node = _Node()..summarySeq = 9;
    await _open(t, node, draft: '待发送');
    expect(find.text('正在追赶会话记录…'), findsOneWidget);
    node.emit('message_end', 8, {'message': _saved});
    await t.pump();
    expect(_enabled(t, _send), isFalse);
    node.emit('agent_settled', 9);
    await t.pump();
    expect(find.text('已连接'), findsOneWidget);
    expect(_enabled(t, _send), isTrue);
  });

  testWidgets('053 realtime loss retains the history and draft across retry', (
    t,
  ) async {
    final node = _Node()..frame = _snapshot(messages: [_saved]);
    final submission = await _open(t, node, draft: '未发送');
    node.stream!.addError(const NodeException('连接已断开'));
    await t.pump();
    expect(find.text('实时连接中断'), findsOneWidget);
    expect(find.text('连接已断开').hitTestable(), findsOneWidget);
    expect(find.text(_saved['content']!), findsOneWidget);
    expect(_enabled(t, _send), isFalse);
    node.frame = _snapshot(seq: 9, messages: [_saved]);
    await t.pump(const Duration(seconds: 1));
    await t.pumpAndSettle();
    expect(node.afterSequences, [7, 9]);
    expect(find.text(_saved['content']!), findsOneWidget);
    expect(submission.draft, '未发送');
    expect(_enabled(t, _send), isTrue);
  });

  testWidgets('054 read-only access cannot submit an editable saved draft', (
    t,
  ) async {
    final node = _Node()..commands = {};
    final submission = await _open(t, node);
    expect(find.text('当前会话仅供查看'), findsOneWidget);
    await t.enterText(_input, '带走的草稿');
    await t.tap(_send, warnIfMissed: false);
    expect(node.prompts, isEmpty);
    expect(submission.draft, '带走的草稿');
  });

  testWidgets('055 input opens the keyboard on touch and has a 48dp target', (
    t,
  ) async {
    await _open(t, _Node());
    expect(_editor(t).focusNode.hasFocus, isFalse);
    expect(t.testTextInput.isVisible, isFalse);
    expect(t.getSize(_input).height, greaterThanOrEqualTo(48));
    await t.tap(_input);
    await t.pump();
    expect(_editor(t).focusNode.hasFocus, isTrue);
    expect(t.testTextInput.isVisible, isTrue);
  });

  testWidgets('056 IME newline keeps a long draft multiline and never sends', (
    t,
  ) async {
    final node = _Node();
    final submission = await _open(t, node);
    const text = '第一行\n第二行\n第三行\n第四行\n第五行\n第六行';
    await t.showKeyboard(_input);
    expect(
      t.testTextInput.setClientArgs!['inputAction'],
      'TextInputAction.newline',
    );
    t.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: text.length),
      ),
    );
    await t.testTextInput.receiveAction(TextInputAction.newline);
    await t.pump();
    expect(submission.draft, text);
    expect(_editor(t).maxLines, 4);
    expect(_editor(t).focusNode.hasFocus, isTrue);
    expect(node.prompts, isEmpty);
  });

  testWidgets('057 send touch target dispatches once and keeps input focus', (
    t,
  ) async {
    final node = _Node()..promptRequest = Completer<void>();
    final submission = await _open(t, node, draft: '第一行\n第二行');
    await t.showKeyboard(_input);
    expect(t.getSize(_send).shortestSide, greaterThanOrEqualTo(48));
    await t.tapAt(t.getRect(_send).centerRight - const Offset(2, 0));
    await t.pump();
    expect(_enabled(t, _send), isFalse);
    await t.tap(_send, warnIfMissed: false);
    expect(node.prompts, ['第一行\n第二行']);
    expect(_editor(t).focusNode.hasFocus, isTrue);
    node.promptRequest!.complete();
    await t.pumpAndSettle();
    expect(submission.draft, isEmpty);
    expect(find.text('等待节点确认'), findsOneWidget);
  });

  testWidgets('058 Tab reaches Send and hardware activation submits', (
    t,
  ) async {
    final node = _Node();
    await _open(t, node, draft: '硬件键盘发送');
    await t.showKeyboard(_input);
    for (final label in ['添加附件', '发送消息']) {
      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.pump();
      final focusContext = FocusManager.instance.primaryFocus!.context!;
      expect(
        focusContext.findAncestorWidgetOfExactType<Tooltip>()?.message,
        label,
      );
    }
    await t.sendKeyEvent(LogicalKeyboardKey.space);
    await t.pumpAndSettle();
    expect(node.prompts, ['硬件键盘发送']);
  });

  testWidgets(
    '059 320dp at 200% keeps the composer focused above the keyboard',
    (t) async {
      _display(t, const Size(320, 568), scale: 2);
      final node = _Node();
      final submission = await _open(t, node);
      await t.enterText(_input, '第一行\n第二行\n第三行');
      t.view.viewInsets = const FakeViewPadding(bottom: 248);
      await t.pumpAndSettle();
      expect(
        t.getRect(find.text('已连接')).bottom,
        lessThanOrEqualTo(t.getRect(find.byType(AppBar)).bottom),
      );
      expect(_editor(t).focusNode.hasFocus, isTrue);
      expect(submission.draft, '第一行\n第二行\n第三行');
      expect(_editor(t).maxLines, 2);
      expect(_send.hitTestable(), findsOneWidget);
      expect(t.getRect(_send).bottom, lessThanOrEqualTo(320));
      expect(t.takeException(), isNull);
    },
  );

  testWidgets(
    '060 short landscape keeps long notices scrollable and composer usable',
    (t) async {
      _display(t, const Size(568, 320), scale: 2);
      t.view.viewInsets = const FakeViewPadding(bottom: 132);
      final node = _Node()
        ..promptFailure = NodeException(
          List.filled(16, '操作被拒绝，请检查节点配置。').join(),
        );
      final submission = await _open(t, node, draft: '第一行\n第二行\n第三行');
      await t.showKeyboard(_input);
      await t.tap(_send);
      await t.pumpAndSettle();
      final notice = find.byType(SingleChildScrollView);
      final scrollable = find.descendant(
        of: notice,
        matching: find.byType(Scrollable),
      );
      expect(
        t.state<ScrollableState>(scrollable).position.maxScrollExtent,
        greaterThan(0),
      );
      expect(_send.hitTestable(), findsOneWidget);
      expect(_input.hitTestable(), findsOneWidget);
      expect(t.getRect(_send).bottom, lessThanOrEqualTo(188));
      expect(submission.draft, '第一行\n第二行\n第三行');
      expect(t.takeException(), isNull);
    },
  );

  testWidgets(
    '061 stop is touchable, single-flight, and gated until settlement',
    (t) async {
      final node = _Node()
        ..frame = _snapshot(status: 'running')
        ..abortRequest = Completer<void>();
      await _open(t, node, draft: '下一条');
      expect(find.text('运行中'), findsOneWidget);
      expect(t.getSize(_stop).shortestSide, greaterThanOrEqualTo(48));
      await t.tap(_stop);
      await t.pump();
      expect(_enabled(t, _stop), isFalse);
      await t.tap(_stop, warnIfMissed: false);
      expect(node.aborts, 1);
      node.abortRequest!.complete();
      await t.pumpAndSettle();
      expect(find.text('正在停止…'), findsOneWidget);
      expect(_enabled(t, _stop), isFalse);
      node.emit('agent_settled', 8);
      await t.pumpAndSettle();
      expect(find.text('已连接'), findsOneWidget);
      expect(_stop, findsNothing);
      expect(_enabled(t, _send), isTrue);
    },
  );

  testWidgets(
    '062 successful send leaves edits made during the request intact',
    (t) async {
      final node = _Node()..promptRequest = Completer<void>();
      final submission = await _open(t, node, draft: '原消息');
      await t.tap(_send);
      await t.enterText(_input, '新草稿');
      node.promptRequest!.complete();
      await t.pumpAndSettle();
      expect(node.prompts, ['原消息']);
      expect(submission.draft, '新草稿');
      expect(_editor(t).controller.text, '新草稿');
    },
  );

  testWidgets('063 definitive rejection retains the draft and allows a retry', (
    t,
  ) async {
    final node = _Node()..promptFailure = const NodeException('节点拒绝了会话操作。');
    final submission = await _open(t, node, draft: '原消息');
    await t.tap(_send);
    await t.pumpAndSettle();
    expect(find.text('节点拒绝了会话操作。').hitTestable(), findsOneWidget);
    expect(_review, findsNothing);
    expect(submission.draft, '原消息');
    expect(_enabled(t, _send), isTrue);
  });

  testWidgets(
    '064 dismissing uncertain receipt review preserves the send lock',
    (t) async {
      final node = _Node()..promptFailure = _unknown;
      final submission = await _open(t, node, draft: '结果未知的草稿');
      await t.tap(_send);
      await t.pumpAndSettle();
      expect(find.text('发送待核对'), findsOneWidget);
      await t.tap(_review);
      await t.pumpAndSettle();
      expect(find.textContaining('再次发送会产生重复消息'), findsOneWidget);
      await t.tap(find.text('返回'));
      await t.pumpAndSettle();
      expect(_enabled(t, _send), isFalse);
      expect(submission.draft, '结果未知的草稿');
      expect(node.prompts, ['结果未知的草稿']);
    },
  );

  testWidgets('065 explicit review confirmation unlocks without resending', (
    t,
  ) async {
    _display(t, const Size(320, 568), scale: 2);
    final node = _Node()..promptFailure = _unknown;
    final submission = await _open(t, node, draft: '核对后的草稿');
    await t.tap(_send);
    await t.pumpAndSettle();
    await t.tap(_review);
    await t.pumpAndSettle();
    final confirm = find.text('已核对，恢复发送');
    expect(confirm.hitTestable(), findsOneWidget);
    await t.tap(confirm);
    await t.pumpAndSettle();
    expect(_enabled(t, _send), isTrue);
    expect(submission.draft, '核对后的草稿');
    expect(node.prompts, ['核对后的草稿']);
    expect(t.takeException(), isNull);
  });

  testWidgets(
    '066 selector sends its wire value and holds the reply lock after close',
    (t) async {
      final second = _dialog('second', 'confirm');
      final node = _Node()
        ..frame = _snapshot(
          status: 'awaiting_input',
          dialogs: [
            _dialog(
              'first',
              'selector',
              options: [
                {'title': '选第一项', 'name': 'wire-key'},
              ],
            ),
            second,
          ],
        )
        ..replyRequest = Completer<void>();
      await _open(t, node);
      final option = find.text('选第一项');
      await t.ensureVisible(option);
      await t.tap(option);
      await t.pump();
      final (id, value, cancelled) = node.replies.single;
      expect(id, 'first');
      expect(value, {
        'kind': 'selector',
        'values': ['wire-key'],
      });
      expect(cancelled, isFalse);
      node.emit('ui_dialog_closed', 8, {'requestId': 'first'});
      await t.pumpAndSettle();
      final confirm = find.widgetWithText(OutlinedButton, '确认');
      expect(t.widget<OutlinedButton>(confirm).onPressed, isNull);
      await t.tap(confirm, warnIfMissed: false);
      expect(node.replies, hasLength(1));
      node.frame = _snapshot(
        seq: 8,
        status: 'awaiting_input',
        dialogs: [second],
      );
      node.replyRequest!.complete();
      await t.pumpAndSettle();
      expect(t.widget<OutlinedButton>(confirm).onPressed, isNotNull);
    },
  );

  testWidgets(
    '067 negative confirmation and explicit cancellation stay distinct',
    (t) async {
      final node = _Node()
        ..frame = _snapshot(
          status: 'awaiting_input',
          dialogs: [_dialog('approval', 'confirm')],
        );
      await _open(t, node, draft: '暂缓发送');
      expect(find.text('等待回答'), findsOneWidget);
      expect(_send, findsNothing);
      node.frame = _snapshot(
        seq: 8,
        status: 'awaiting_input',
        dialogs: [_dialog('desktop', 'unsupported')],
      );
      await t.tap(find.text('拒绝'));
      await t.pumpAndSettle();
      expect(find.text('请在电脑上处理此请求。'), findsOneWidget);
      expect(find.text('确认'), findsNothing);
      node.frame = _snapshot(seq: 9);
      await t.tap(find.text('取消请求'));
      await t.pumpAndSettle();
      expect(node.replies, [
        ('approval', false, false),
        ('desktop', null, true),
      ]);
    },
  );

  testWidgets(
    '068 editor preserves a multiline reply and checks reply capability',
    (t) async {
      final node = _Node()
        ..canReply = false
        ..frame = _snapshot(
          status: 'awaiting_input',
          dialogs: [_dialog('editor', 'editor')],
        );
      await _open(t, node);
      final answer = find.widgetWithText(TextField, '回答');
      expect(t.widget<TextField>(answer).enabled, isFalse);
      node.canReply = true;
      await t.tap(_icon('重新同步'));
      await t.pumpAndSettle();
      expect(t.widget<TextField>(answer).enabled, isTrue);
      expect(find.text('提交回答'), findsNothing);
      const reply = '保留第一行\n也保留第二行';
      await t.enterText(answer, reply);
      await t.pump();
      final submit = find.text('提交回答');
      await t.ensureVisible(submit);
      node.frame = _snapshot(seq: 8);
      await t.tap(submit);
      await t.pumpAndSettle();
      final (id, value, cancelled) = node.replies.single;
      expect(id, 'editor');
      expect(value, {
        'kind': 'input',
        'text': reply,
        'value': reply,
        'values': [reply],
      });
      expect(cancelled, isFalse);
      expect(answer, findsNothing);
    },
  );
}

const _pixel =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jD1sAAAAASUVORK5CYII=';

Future<void> _pasteImage(WidgetTester t, {bool shortcut = false}) async {
  if (shortcut) {
    await t.showKeyboard(_input);
  } else {
    await t.longPress(_input);
    await t.pumpAndSettle();
  }
  await t.runAsync(() async {
    if (shortcut) {
      await _pasteShortcut(t);
    } else {
      final paste = find.text('粘贴');
      expect(paste, findsOneWidget);
      expect(
        find.ancestor(of: paste, matching: find.byType(TextSelectionToolbar)),
        findsOneWidget,
      );
      expect(find.byType(AdaptiveTextSelectionToolbar), findsNothing);
      await t.tap(paste);
    }
    await t.pumpAndSettle();
    // The image descriptor completes outside widget-test fake time.
    for (
      var attempt = 0;
      attempt < 40 && find.text('clip.png').evaluate().isEmpty;
      attempt++
    ) {
      await Future<void>.delayed(const Duration(milliseconds: 10));
      await t.pump();
    }
  });
}

Future<void> _pasteShortcut(WidgetTester t) async {
  await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  await t.sendKeyEvent(LogicalKeyboardKey.keyV);
  await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
}

_FilePicker _picker([String? name, String? content]) {
  final previous = FileSelectorPlatform.instance;
  final picker = _FilePicker([
    if (name != null)
      XFile.fromData(Uint8List.fromList(utf8.encode(content!)), path: name),
  ]);
  FileSelectorPlatform.instance = picker;
  addTearDown(() => FileSelectorPlatform.instance = previous);
  return picker;
}

class _FilePicker extends FileSelectorPlatform {
  _FilePicker(this.files);

  final List<XFile> files;
  int openCount = 0;

  @override
  Future<List<XFile>> openFiles({
    List<XTypeGroup>? acceptedTypeGroups,
    String? initialDirectory,
    String? confirmButtonText,
  }) async {
    openCount += 1;
    return files;
  }
}

void _clipboard(WidgetTester t, List<Json> Function() read) {
  const channel = MethodChannel('rpi_work/clipboard_attachments');
  final messenger = t.binding.defaultBinaryMessenger;
  messenger.setMockMethodCallHandler(channel, (call) async {
    expect(call.method, 'read');
    return read();
  });
  addTearDown(() => messenger.setMockMethodCallHandler(channel, null));
}

EditableText _editor(WidgetTester t) => t.widget<EditableText>(
  find.descendant(of: _input, matching: find.byType(EditableText)),
);

bool _enabled(WidgetTester t, Finder button) =>
    t.widget<IconButton>(button).onPressed != null;

void _display(WidgetTester t, Size size, {required double scale}) {
  t.view.devicePixelRatio = 1;
  t.view.physicalSize = size;
  t.platformDispatcher.textScaleFactorTestValue = scale;
  addTearDown(t.view.resetDevicePixelRatio);
  addTearDown(t.view.resetPhysicalSize);
  addTearDown(t.view.resetViewInsets);
  addTearDown(t.platformDispatcher.clearTextScaleFactorTestValue);
}

Future<SessionSubmission> _open(
  WidgetTester t,
  _Node node, {
  String draft = '',
}) async {
  final submission = SessionSubmission()..changeDraft(draft);
  addTearDown(() async {
    await t.pumpWidget(const SizedBox.shrink());
    await node.stream?.close();
    submission.dispose();
  });
  await t.pumpWidget(
    MaterialApp(
      locale: const Locale('zh', 'CN'),
      supportedLocales: const [Locale('zh', 'CN')],
      // Match the app's framework Material widgets and localizations.
      // ignore: deprecated_member_use
      localizationsDelegates: GlobalMaterialLocalizations.delegates,
      theme: workTheme(Brightness.light),
      home: ConversationScreen(
        node: node,
        session: node.summary,
        submission: submission,
        close: () {},
        authorizationLost: () => fail('Unexpected authorization loss'),
      ),
    ),
  );
  await t.pump();
  return submission;
}

SessionSnapshot _snapshot({
  int seq = 7,
  String status = 'idle',
  List<Json> messages = const [],
  List<Json> dialogs = const [],
  bool hasMore = false,
}) => SessionSnapshot(
  sessionId: 'conversation',
  status: status,
  lastSeq: seq,
  messages: messages,
  events: const [],
  pendingDialogs: dialogs,
  hasMore: hasMore,
);

Json _dialog(String id, String method, {List<Json>? options}) => {
  'requestId': id,
  'ui': {'method': method, 'title': '请求 $id', 'options': ?options},
};

class _Node extends Fake implements WorkNode {
  StreamController<WireEvent>? stream;
  Completer<void>? nextStreamCleanup;
  final afterSequences = <int>[];
  final snapshotLimits = <int?>[];
  final prompts = <String>[];
  final imageRequests = <(String, List<Json>)>[];
  final replies = <(String, Object?, bool)>[];
  int aborts = 0;
  int? summarySeq;
  Set<String> commands = {'prompt', 'abort'};
  bool canReply = true;
  bool imagePrompts = false;
  bool autoSubscribe = true;
  void Function()? subscribe;
  SessionSnapshot frame = _snapshot();
  Completer<SessionSnapshot>? snapshotRequest;
  Completer<void>? promptRequest;
  Completer<void>? abortRequest;
  Completer<void>? replyRequest;
  NodeException? promptFailure;

  SessionSummary get summary => SessionSummary(
    id: frame.sessionId,
    title: '会话标题',
    status: frame.status!,
    updatedAt: 0,
    lastSeq: summarySeq ?? frame.lastSeq,
    pinned: false,
    archived: false,
  );

  @override
  Future<SessionSummary> openSession(String id) async => summary;

  @override
  Future<SessionSummary> getSession(String id) async => summary;

  @override
  Future<SessionAccess> access(String id) async => SessionAccess(
    commands: commands,
    snapshotLimit: true,
    dialogReply: canReply,
    imagePrompts: imagePrompts,
  );

  @override
  Future<SessionSnapshot> snapshot(String id, {int? limit}) async {
    snapshotLimits.add(limit);
    return snapshotRequest?.future ?? frame;
  }

  @override
  Stream<WireEvent> events(
    String id, {
    required int afterSeq,
    void Function()? onSubscribed,
  }) {
    afterSequences.add(afterSeq);
    subscribe = onSubscribed;
    final cleanup = nextStreamCleanup;
    nextStreamCleanup = null;
    final controller = StreamController<WireEvent>(
      onListen: () {
        if (autoSubscribe && onSubscribed != null) {
          scheduleMicrotask(onSubscribed);
        }
      },
      onCancel: () => cleanup?.future ?? Future<void>.value(),
    );
    stream = controller;
    return controller.stream;
  }

  void emit(String type, int seq, [Json data = const {}]) =>
      stream!.add(WireEvent(type: type, seq: seq, data: data));

  @override
  Future<void> prompt(String id, String text) async {
    prompts.add(text);
    await promptRequest?.future;
    if (promptFailure case final error?) throw error;
  }

  @override
  Future<void> promptWithImages(
    String id,
    String text,
    List<Json> images,
  ) async {
    imageRequests.add((text, images));
  }

  @override
  Future<void> abort(String id) async {
    aborts++;
    await abortRequest?.future;
  }

  @override
  Future<void> respondToDialog(
    String id,
    String requestId, {
    Object? value,
    bool cancelled = false,
  }) async {
    replies.add((requestId, value, cancelled));
    await replyRequest?.future;
  }
}
