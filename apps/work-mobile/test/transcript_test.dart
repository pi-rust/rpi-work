// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:work_mobile/main.dart';
import 'package:work_mobile/src/conversation/transcript.dart';
import 'package:work_mobile/src/node/work_node.dart';

const _attachmentMessage = <String, Object?>{
  'role': 'user',
  'entryId': 'desktop-prompt',
  'content': [
    {'type': 'text', 'text': '请检查\n\n引用项目文件 · "docs/release notes.md"'},
    {'type': 'image', 'data': 'opaque image payload', 'mimeType': 'image/png'},
    {'type': 'text', 'text': '\n\n附件 · checks.txt\n- Run smoke tests\n'},
    {
      'type': 'image',
      'data': 'another image payload',
      'mimeType': 'image/jpeg',
    },
    {'type': 'custom', 'text': 'opaque text'},
  ],
};

const _attachmentDisplay =
    '请检查\n\n引用项目文件 · "docs/release notes.md"'
    '\n\n附件 · checks.txt\n- Run smoke tests\n\n[图片附件]';

Transcript _snapshot({
  List<Json> messages = const [],
  List<Json> dialogs = const [],
}) => Transcript.fromSnapshot(
  SessionSnapshot(
    sessionId: 'session',
    status: 'ready',
    lastSeq: 4,
    messages: messages,
    events: const [],
    pendingDialogs: dialogs,
    hasMore: false,
  ),
);

void main() {
  test('same text in different persisted messages keeps both identities', () {
    final transcript = _snapshot(
      messages: const [
        {'role': 'user', 'entryId': 'first', 'content': 'Run the check'},
        {'role': 'user', 'entryId': 'second', 'content': 'Run the check'},
      ],
    );
    expect(transcript.messages.map((message) => message.id), [
      'first',
      'second',
    ]);
  });

  test('snapshot and live prompts use desktop attachment display', () {
    final history = _snapshot(
      messages: const [
        _attachmentMessage,
        {
          'role': 'user',
          'entryId': 'image-only',
          'content': {'type': 'image', 'data': 'opaque image payload'},
        },
      ],
    );
    expect(history.messages.map((message) => message.text), [
      _attachmentDisplay,
      '[图片附件]',
    ]);

    final live = _snapshot();
    live.apply(
      const WireEvent(
        type: 'message_end',
        seq: 5,
        data: {'message': _attachmentMessage},
      ),
    );
    expect(live.messages.single.text, _attachmentDisplay);
  });

  testWidgets(
    'desktop attachment text and inert Markdown images render inline',
    (tester) async {
      final transcript = _snapshot(
        messages: const [
          _attachmentMessage,
          {
            'role': 'assistant',
            'content': [
              {'type': 'image', 'text': 'opaque assistant image'},
              {
                'type': 'text',
                'text':
                    '预览 ![发布 **截图**](https://example.org/screenshot.png) 完成。'
                    '\n\n![](<>)\n\n[发布文档](https://example.org/release.pdf)',
              },
            ],
          },
        ],
      );
      await tester.pumpWidget(
        WorkMobileApp(
          home: Scaffold(
            body: ListView(
              children: transcript.messages
                  .map((message) => ChatMessageTile(message: message))
                  .toList(),
            ),
          ),
        ),
      );
      expect(find.text(_attachmentDisplay), findsOneWidget);
      expect(
        find.text('预览 图片（未加载）：发布 截图 完成。', findRichText: true),
        findsOneWidget,
      );
      expect(find.text('图片（未加载）：', findRichText: true), findsOneWidget);
      expect(find.byType(Image), findsNothing);
      expect(find.textContaining('opaque'), findsNothing);

      const channel = MethodChannel('plugins.flutter.io/url_launcher');
      final messenger = tester.binding.defaultBinaryMessenger;
      final opened = <String>[];
      messenger.setMockMethodCallHandler(channel, (call) async {
        opened.add(call.arguments['url'] as String);
        return true;
      });
      addTearDown(() => messenger.setMockMethodCallHandler(channel, null));
      final markdown = tester.widget<MarkdownBody>(find.byType(MarkdownBody));
      for (final href in [
        'docs/release.pdf',
        'file:///tmp/release.pdf',
        'https:///example.org/release.pdf',
        'https://user@example.org/release.pdf',
        'https://example.org\\\\release.pdf',
      ]) {
        markdown.onTapLink!('', href, '');
      }
      await tester.tap(find.text('发布文档', findRichText: true));
      await tester.pumpAndSettle();
      expect(opened, ['https://example.org/release.pdf']);
    },
  );

  test('projects a streamed turn and requires resync for a sequence gap', () {
    final transcript = _snapshot();
    transcript.apply(const WireEvent(type: 'agent_start', seq: 5, data: {}));
    final delta = const WireEvent(
      type: 'message_update',
      seq: 6,
      data: {
        'message': {'role': 'assistant', 'entryId': 'reply'},
        'assistantMessageEvent': {
          'type': 'text_delta',
          'contentIndex': 0,
          'delta': 'Hel',
        },
      },
    );
    transcript.apply(delta);
    transcript.apply(delta);
    expect(transcript.messages.single.text, 'Hel');
    transcript.apply(
      const WireEvent(
        type: 'message_end',
        seq: 7,
        data: {
          'message': {
            'role': 'assistant',
            'entryId': 'reply',
            'content': 'Hello',
          },
        },
      ),
    );
    transcript.apply(const WireEvent(type: 'agent_end', seq: 8, data: {}));
    expect(transcript.running, true);
    expect(transcript.messages.single.text, 'Hello');
    expect(
      () => transcript.apply(
        const WireEvent(type: 'agent_settled', seq: 10, data: {}),
      ),
      throwsA(isA<TranscriptResyncRequired>()),
    );
    expect(transcript.cursor, 8);
    transcript.apply(const WireEvent(type: 'agent_settled', seq: 9, data: {}));
    expect(transcript.running, false);
    expect(transcript.settlementCount, 1);
  });

  test('projected ACP dialog answers retain the negotiated option label', () {
    final transcript = _snapshot(
      dialogs: const [
        {
          'requestId': 'request',
          'ui': {
            'kind': 'selector',
            'question': 'Run tool?',
            'acpPermission': {
              'options': [
                {
                  'name': 'Allow once',
                  'optionId': 'allow-123',
                  'kind': 'allow_once',
                },
              ],
            },
          },
        },
      ],
    );
    final choice = transcript.dialogs.single.choices.single;
    expect(choice.label, 'Allow once');
    expect(choice.value, {
      'kind': 'selector',
      'values': ['Allow once'],
    });
    transcript.apply(
      const WireEvent(
        type: 'ui_dialog_closed',
        seq: 5,
        data: {'requestId': 'request'},
      ),
    );
    expect(transcript.dialogs, isEmpty);
  });
}
