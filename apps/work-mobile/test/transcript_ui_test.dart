// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:work_mobile/src/conversation/transcript.dart';
import 'package:work_mobile/src/node/work_node.dart';
import 'package:work_mobile/src/work_theme.dart';

Json _user(Object content, {String id = 'prompt'}) => {
  'role': 'user',
  'entryId': id,
  'content': content,
};

Json _assistant(Object content, {String id = 'reply'}) => {
  'role': 'assistant',
  'entryId': id,
  'content': content,
};

Transcript _snapshot({
  List<Json> messages = const [],
  List<WireEvent> events = const [],
  int cursor = 0,
  String session = 'session',
}) => Transcript.fromSnapshot(
  SessionSnapshot(
    sessionId: session,
    status: 'ready',
    lastSeq: cursor,
    messages: messages,
    events: events,
    pendingDialogs: const [],
    hasMore: false,
  ),
);

WireEvent _event(String type, int seq, Json data) =>
    WireEvent(type: type, seq: seq, data: data);

WireEvent _tool(String stage, int seq, [Json fields = const {}]) =>
    _event('tool_execution_$stage', seq, {'toolCallId': 'read-1', ...fields});

Future<void> _render(
  WidgetTester tester,
  Transcript transcript, {
  Brightness brightness = Brightness.light,
  double scale = 1,
}) async {
  await tester.binding.setSurfaceSize(const Size(320, 568));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(
    MaterialApp(
      theme: workTheme(brightness),
      home: MediaQuery(
        data: MediaQueryData(textScaler: TextScaler.linear(scale)),
        child: Scaffold(
          body: ListView.builder(
            padding: const EdgeInsets.fromLTRB(18, 18, 18, 12),
            itemCount: transcript.messages.length,
            itemBuilder: (_, index) =>
                ChatMessageTile(message: transcript.messages[index]),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

List<MethodCall> _captureLaunches(WidgetTester tester) {
  const channel = MethodChannel('plugins.flutter.io/url_launcher');
  final messenger = tester.binding.defaultBinaryMessenger;
  final calls = <MethodCall>[];
  messenger.setMockMethodCallHandler(channel, (call) async {
    calls.add(call);
    return true;
  });
  addTearDown(() => messenger.setMockMethodCallHandler(channel, null));
  return calls;
}

Finder _text(String text) => find.text(text, findRichText: true);

Finder _styledText(String text) => find.byWidgetPredicate(
  (widget) => widget is Text && widget.textSpan?.toPlainText() == text,
);

Finder _horizontalScroll() => find.byWidgetPredicate(
  (widget) =>
      widget is Scrollable && widget.axisDirection == AxisDirection.right,
);

Iterable<TextSpan> _spans(InlineSpan span) sync* {
  if (span is! TextSpan) return;
  yield span;
  for (final child in span.children ?? const <InlineSpan>[]) {
    yield* _spans(child);
  }
}

void main() {
  testWidgets('069 user text stays literal and end-aligned', (tester) async {
    const prompt = '# 原样显示 **星号**';
    await _render(tester, _snapshot(messages: [_user(prompt)]));
    final content = find.widgetWithText(SelectableText, prompt);
    final row = find.byType(ChatMessageTile);
    expect(content, findsOneWidget);
    expect(
      tester.getRect(content).right,
      closeTo(tester.getRect(row).right - 16, 1),
    );
  });

  testWidgets('070 text attachment preserves order and line breaks', (
    tester,
  ) async {
    const shown = '请检查\n\n附件 · checks.txt\n- 第一行\n  缩进\n';
    final transcript = _snapshot(
      messages: [
        _user([
          {'type': 'text', 'text': '请检查'},
          {'type': 'text', 'text': '\n\n附件 · checks.txt\n- 第一行\n  缩进\n'},
        ]),
      ],
    );
    await _render(tester, transcript);
    expect(transcript.messages.single.text, shown);
    expect(find.widgetWithText(SelectableText, shown), findsOneWidget);
  });

  testWidgets('071 quoted project path renders as inert prompt text', (
    tester,
  ) async {
    final launches = _captureLaunches(tester);
    const prompt = '引用项目文件 · "docs/release notes.md"';
    await _render(tester, _snapshot(messages: [_user(prompt)]));
    await tester.tap(_text(prompt));
    await tester.pumpAndSettle();
    expect(launches, isEmpty);
  });

  testWidgets('072 user image-only prompt renders one placeholder', (
    tester,
  ) async {
    final transcript = _snapshot(
      messages: [
        _user([
          {'type': 'image', 'data': 'private pixels 1'},
          {'type': 'image', 'data': 'private pixels 2'},
        ]),
      ],
    );
    await _render(tester, transcript);
    expect(_text('[图片附件]'), findsOneWidget);
    expect(find.byType(Image), findsNothing);
    expect(transcript.messages.single.text, '[图片附件]');
  });

  testWidgets('073 private and unknown blocks stay out of prompt text', (
    tester,
  ) async {
    final transcript = _snapshot(
      messages: [
        _user([
          {'type': 'input_text', 'text': '公开'},
          {'type': 'custom', 'content': 'opaque custom'},
          {'type': 'text', 'text': 'opaque redacted', 'redacted': true},
          {'reasoning': 'opaque thought', 'content': 'opaque companion'},
        ]),
      ],
    );
    await _render(tester, transcript);
    expect(transcript.messages.single.text, '公开');
    expect(_text('公开'), findsOneWidget);
    expect(find.textContaining('opaque', findRichText: true), findsNothing);
  });

  testWidgets('074 narrow prompt wraps at large text scale', (tester) async {
    const short = '路径';
    final long = List.filled(5, '目录/发布文档及回归记录').join('/');
    await _render(
      tester,
      _snapshot(
        messages: [
          _user(short),
          _user(long, id: 'long'),
        ],
      ),
      scale: 2,
    );
    final shortText = find.widgetWithText(SelectableText, short);
    final longText = find.widgetWithText(SelectableText, long);
    expect(
      tester.getSize(longText).height,
      greaterThan(tester.getSize(shortText).height * 2),
    );
    expect(tester.getRect(longText).right, lessThan(320));
    expect(tester.takeException(), isNull);
  });

  testWidgets('075 assistant Markdown renders selectable formatted prose', (
    tester,
  ) async {
    await _render(
      tester,
      _snapshot(messages: [_assistant('## 发布说明\n\n- **已完成**\n- 待检查')]),
    );
    expect(find.byType(SelectionArea), findsOneWidget);
    final heading = tester.widget<Text>(_styledText('发布说明'));
    expect(
      heading.textSpan!.style!.fontSize,
      workTheme(Brightness.light).textTheme.titleLarge!.fontSize,
    );
    final bold = tester.widget<Text>(_styledText('已完成'));
    expect(
      _spans(bold.textSpan!)
          .any((span) => span.style?.fontWeight == FontWeight.bold),
      isTrue,
    );
    expect(_text('待检查'), findsOneWidget);
    await tester.longPress(_text('待检查'));
    await tester.pumpAndSettle();
    expect(find.text('Copy'), findsOneWidget);
  });

  testWidgets(
    '076 assistant image alt uses muted inline text in both palettes',
    (tester) async {
      for (final brightness in Brightness.values) {
        await _render(
          tester,
          _snapshot(
            messages: [
              _assistant([
                {'type': 'image', 'content': 'opaque assistant image'},
                {
                  'type': 'text',
                  'text': '前 ![发布 **截图**](https://example.org/preview.png) 后',
                },
                {
                  'type': 'text',
                  'text': '<img src="https://example.org/raw.png" />',
                },
              ]),
            ],
          ),
          brightness: brightness,
        );
        final line = tester.widget<Text>(_styledText('前 图片（未加载）：发布 截图 后'));
        final alt = _spans(line.textSpan!)
            .singleWhere((span) => span.text == '图片（未加载）：发布 截图');
        expect(
          alt.style!.color,
          workTheme(brightness).colorScheme.onSurfaceVariant,
        );
        expect(find.byType(Image), findsNothing);
        expect(find.textContaining('opaque', findRichText: true), findsNothing);
      }
    },
  );

  testWidgets('077 linked assistant image alt itself is inert', (tester) async {
    final launches = _captureLaunches(tester);
    const alt = '图片（未加载）：预览';
    await _render(
      tester,
      _snapshot(
        messages: [
          _assistant(
            '[![预览](https://example.org/preview.png)](https://example.org/file.pdf)',
          ),
        ],
      ),
    );
    expect(_text(alt), findsOneWidget);
    await tester.tap(_text(alt));
    await tester.pumpAndSettle();
    expect(launches, isEmpty);
  });

  testWidgets('078 explicit HTTP(S) file links launch only on tap', (
    tester,
  ) async {
    final launches = _captureLaunches(tester);
    await _render(
      tester,
      _snapshot(
        messages: [
          _assistant(
            '[发行稿](https://example.org/release.pdf)\n\n[镜像](http://example.org/release.pdf)',
          ),
        ],
      ),
    );
    expect(launches, isEmpty);
    await tester.tap(_text('发行稿'));
    await tester.tap(_text('镜像'));
    await tester.pumpAndSettle();
    expect(launches.map((call) => call.arguments['url']), [
      'https://example.org/release.pdf',
      'http://example.org/release.pdf',
    ]);
  });

  testWidgets('079 unsafe Markdown file links never launch', (tester) async {
    final launches = _captureLaunches(tester);
    await _render(
      tester,
      _snapshot(
        messages: [
          _assistant(
            '[相对路径](docs/release.pdf)\n\n[本地文件](file:///tmp/release.pdf)\n\n'
            '[协议相对](//example.org/release.pdf)\n\n[带凭据](https://user@example.org/release.pdf)\n\n'
            '[应用协议](custom://host/release.pdf)',
          ),
        ],
      ),
    );
    for (final label in ['相对路径', '本地文件', '协议相对', '带凭据', '应用协议']) {
      await tester.tap(_text(label));
    }
    await tester.pumpAndSettle();
    expect(launches, isEmpty);
  });

  testWidgets('080 whitespace-only messages occupy zero transcript height', (
    tester,
  ) async {
    await _render(
      tester,
      _snapshot(
        messages: [
          _user('前一条', id: 'before'),
          _user(' \n\t '),
          _assistant('\n  '),
          _assistant('后一条', id: 'after'),
        ],
      ),
    );
    final rows = find.byType(ChatMessageTile);
    expect(rows, findsNWidgets(4));
    expect(tester.getSize(rows.at(1)).height, 0);
    expect(tester.getSize(rows.at(2)).height, 0);
    expect(_text('后一条'), findsOneWidget);
  });

  testWidgets('081 wide fenced code scrolls horizontally', (tester) async {
    final code = 'const name = "${List.filled(18, 'long_value').join('_')}";';
    await _render(
      tester,
      _snapshot(messages: [_assistant('```dart\n$code\n```')]),
    );
    final scroll = _horizontalScroll();
    final position = tester.state<ScrollableState>(scroll).position;
    expect(position.maxScrollExtent, greaterThan(200));
    await tester.drag(scroll, const Offset(-175, 0));
    await tester.pumpAndSettle();
    expect(position.pixels, greaterThan(0));
    expect(find.textContaining(code, findRichText: true), findsWidgets);
    expect(tester.takeException(), isNull);
  });

  testWidgets('082 wide Markdown table exposes the last column by scrolling', (
    tester,
  ) async {
    await _render(
      tester,
      _snapshot(
        messages: [
          _assistant(
            '| 项目 | 版本 | 状态 | 末列 |\n| --- | --- | --- | --- |\n| 工作区 | v1 | 完成 | 末列值 |',
          ),
        ],
      ),
    );
    final scroll = _horizontalScroll();
    final position = tester.state<ScrollableState>(scroll).position;
    expect(position.maxScrollExtent, greaterThan(200));
    expect(_text('末列值').hitTestable(), findsNothing);
    await tester.drag(scroll, const Offset(-600, 0));
    await tester.pumpAndSettle();
    expect(_text('末列值').hitTestable(), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('083 tool summary has an accessible collapsed toggle', (
    tester,
  ) async {
    final transcript = _snapshot()
      ..apply(_tool('end', 1, {'toolName': '读取', 'result': '构建结果'}));
    await _render(tester, transcript, scale: 2);
    final header = find.byType(ListTile);
    expect(tester.getSize(header).height, greaterThanOrEqualTo(48));
    expect(
      find.bySemanticsLabel(RegExp('读取.*已完成', dotAll: true)),
      findsWidgets,
    );
    expect(_text('构建结果'), findsNothing);
    await tester.tap(_text('读取'));
    await tester.pumpAndSettle();
    expect(_text('工具结果'), findsOneWidget);
    expect(find.widgetWithText(SelectableText, '构建结果'), findsOneWidget);
    await tester.tap(_text('读取'));
    await tester.pumpAndSettle();
    expect(_text('构建结果'), findsNothing);
  });

  testWidgets('084 tool result allowlist validates outer and nested blocks', (
    tester,
  ) async {
    final transcript = _snapshot();
    for (final (seq, payload) in [
      (1, {'type': 'image', 'content': 'opaque image'}),
      (2, {'type': 'custom', 'content': 'opaque custom'}),
      (3, {'reasoning': 'opaque reasoning', 'content': 'opaque companion'}),
    ]) {
      transcript.apply(_tool('update', seq, {'partialResult': payload}));
      expect(transcript.messages.single.text, isEmpty);
    }
    transcript.apply(
      _tool('end', 4, {
        'toolName': '读取',
        'result': {
          'content': [
            {'type': 'output_text', 'text': '第一段\n'},
            {'type': 'image', 'content': 'opaque nested image'},
            {'type': 'text', 'text': '第二段'},
          ],
        },
      }),
    );
    await _render(tester, transcript);
    await tester.tap(_text('读取'));
    await tester.pumpAndSettle();
    expect(transcript.messages.single.text, '第一段\n第二段');
    expect(_text('第一段\n第二段'), findsOneWidget);
    expect(find.textContaining('opaque', findRichText: true), findsNothing);
  });

  testWidgets('085 failed tool shows public error when result is unusable', (
    tester,
  ) async {
    final transcript = _snapshot()
      ..apply(
        _tool('end', 1, {
          'toolName': '读取',
          'result': {'type': 'binary', 'content': 'opaque payload'},
          'error': '读取失败，请重试',
        }),
      );
    await _render(tester, transcript);
    final status = tester.widget<Text>(find.text('失败'));
    expect(status.style!.color, workTheme(Brightness.light).colorScheme.error);
    await tester.tap(_text('读取'));
    await tester.pumpAndSettle();
    expect(_text('读取失败，请重试'), findsOneWidget);
    expect(find.textContaining('opaque', findRichText: true), findsNothing);
  });

  testWidgets('086 long tool detail scrolls with the rest of the transcript', (
    tester,
  ) async {
    final output = List.generate(80, (index) => '读到第 $index 行').join('\n');
    final transcript = _snapshot()
      ..apply(_tool('end', 1, {'toolName': '读取', 'result': output}))
      ..apply(_event('message_end', 2, {'message': _assistant('读取后的回复')}));
    await _render(tester, transcript);
    await tester.tap(_text('读取'));
    await tester.pumpAndSettle();
    expect(find.widgetWithText(SelectableText, output), findsOneWidget);
    expect(_text('读取后的回复').hitTestable(), findsNothing);
    final timeline = find
        .descendant(
          of: find.byType(ListView),
          matching: find.byType(Scrollable),
        )
        .first;
    await tester.scrollUntilVisible(
      _text('读取后的回复'),
      420,
      scrollable: timeline,
      maxScrolls: 12,
    );
    expect(_text('读取后的回复').hitTestable(), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    '087 expansion survives tool updates and resets across sessions',
    (tester) async {
      final transcript = _snapshot()
        ..apply(_tool('start', 1, {'toolName': '读取'}));
      await _render(tester, transcript);
      await tester.tap(_text('读取'));
      await tester.pumpAndSettle();
      expect(_text('暂无输出'), findsOneWidget);
      transcript.apply(_tool('update', 2, {'partialResult': '部分输出'}));
      await _render(tester, transcript);
      expect(_text('部分输出'), findsOneWidget);
      final next = _snapshot(session: 'next')
        ..apply(_tool('end', 1, {'toolName': '读取', 'result': '另一会话输出'}));
      await _render(tester, next);
      expect(_text('另一会话输出'), findsNothing);
      await tester.tap(_text('读取'));
      await tester.pumpAndSettle();
      expect(_text('另一会话输出'), findsOneWidget);
    },
  );

  testWidgets('088 snapshot history and realtime render the same turn once', (
    tester,
  ) async {
    final prompt = _user([
      {'type': 'text', 'text': '检查附件'},
      {'type': 'image', 'data': 'opaque user image'},
    ]);
    final reply = _assistant('**看过附件**');
    final result = {
      'role': 'toolResult',
      'toolCallId': 'read-1',
      'content': '读取结果',
    };
    final events = [
      _event('message_end', 1, {'message': prompt}),
      _tool('start', 2, {'toolName': '读取'}),
      _tool('end', 3, {'result': '读取结果'}),
      _event('message_end', 4, {'message': reply}),
    ];
    final live = _snapshot();
    events.forEach(live.apply);
    final history = _snapshot(
      messages: [prompt, result, reply],
      events: events,
      cursor: 4,
    );
    for (final transcript in [live, history]) {
      expect(transcript.messages.map((message) => message.id), [
        'prompt',
        'read-1',
        'reply',
      ]);
      expect(transcript.messages.map((message) => message.text), [
        '检查附件\n\n[图片附件]',
        '读取结果',
        '**看过附件**',
      ]);
      await _render(tester, transcript);
      expect(_text('检查附件\n\n[图片附件]'), findsOneWidget);
      expect(_text('读取'), findsOneWidget);
      expect(_text('看过附件'), findsOneWidget);
      await tester.tap(_text('读取'));
      await tester.pumpAndSettle();
      expect(_text('读取结果'), findsOneWidget);
      await tester.tap(_text('读取'));
      await tester.pumpAndSettle();
    }
  });

  test(
    '089 indexed deltas survive canonical acknowledgement without content',
    () {
      final transcript = _snapshot();
      final lateBlock = _event('message_update', 1, {
        'message': {'role': 'assistant', 'id': 'stream'},
        'assistantMessageEvent': {
          'type': 'text_delta',
          'contentIndex': 2,
          'delta': '第二段',
        },
      });
      transcript
        ..apply(lateBlock)
        ..apply(lateBlock)
        ..apply(
          _event('message_update', 2, {
            'message': {'role': 'assistant', 'id': 'stream'},
            'eventType': 'text_delta',
            'contentIndex': 0,
            'delta': '第一段',
          }),
        )
        ..apply(
          _event('message_end', 3, {
            'sessionId': 'session',
            'message': {'role': 'assistant', 'entryId': 'session:assistant:3'},
          }),
        );
      expect(transcript.messages.single.id, 'session:assistant:3');
      expect(transcript.messages.single.text, '第一段\n\n第二段');
    },
  );

  testWidgets('090 resync rejects gaps and renders authoritative replacement', (
    tester,
  ) async {
    final live = _snapshot(messages: [_assistant('过期内容')], cursor: 4);
    final gap = _event('message_end', 6, {'message': _assistant('跳序内容')});
    expect(
      () => live.apply(gap),
      throwsA(
        isA<TranscriptResyncRequired>()
            .having((error) => error.cursor, 'cursor', 4)
            .having((error) => error.receivedSeq, 'receivedSeq', 6),
      ),
    );
    expect(
      () => live.apply(_event('resync_required', 5, {})),
      throwsA(isA<TranscriptResyncRequired>()),
    );
    expect(live.cursor, 4);
    expect(live.messages.single.text, '过期内容');
    final fresh = _snapshot(messages: [_assistant('节点确认内容')], cursor: 6)
      ..apply(gap)
      ..apply(_event('message_end', 7, {'message': _user('继续')}));
    await _render(tester, fresh);
    expect(fresh.cursor, 7);
    expect(_text('节点确认内容'), findsOneWidget);
    expect(_text('继续'), findsOneWidget);
    expect(_text('过期内容'), findsNothing);
    expect(_text('跳序内容'), findsNothing);
  });
}
