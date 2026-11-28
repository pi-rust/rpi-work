import 'dart:async';
import 'dart:ui';

// ignore: deprecated_member_use
import 'package:flutter/material.dart'
    show FilledButton, InputBorder, OutlinedButton, TextField, Theme;
import 'package:flutter_test/flutter_test.dart';
import 'package:work_mobile/main.dart';
import 'package:work_mobile/src/conversation/conversation.dart';
import 'package:work_mobile/src/node/work_node.dart';

void main() {
  for (final brightness in Brightness.values) {
    for (final size in [
      const Size(320, 568),
      const Size(390, 844),
      const Size(568, 320),
    ]) {
      for (final scale in [1.0, 2.0]) {
        testWidgets('pairing actions fit $size at ${scale}x in $brightness', (
          tester,
        ) async {
          tester.view.devicePixelRatio = 1;
          tester.view.physicalSize = size;
          final platform = tester.platformDispatcher;
          platform.platformBrightnessTestValue = brightness;
          platform.textScaleFactorTestValue = scale;
          addTearDown(tester.view.resetPhysicalSize);
          addTearDown(tester.view.resetDevicePixelRatio);
          addTearDown(platform.clearPlatformBrightnessTestValue);
          addTearDown(platform.clearTextScaleFactorTestValue);
          await tester.pumpWidget(const WorkMobileApp());
          await tester.pumpAndSettle();
          final camera = find.text('打开相机');
          final gallery = find.text('从相册选取');
          for (final label in [camera, gallery]) {
            final button = find.ancestor(
              of: label,
              matching: find.byWidgetPredicate(
                (widget) => widget is FilledButton || widget is OutlinedButton,
              ),
            );
            await tester.ensureVisible(button);
            await tester.pumpAndSettle();
            expect(label.hitTestable(), findsOneWidget);
            expect(
              tester.getSize(button).shortestSide,
              greaterThanOrEqualTo(44),
            );
            if (scale == 1) {
              expect(tester.getRect(button).bottom, lessThan(size.height));
            }
          }
          final theme = Theme.of(tester.element(camera));
          final dark = brightness == Brightness.dark;
          expect(
            theme.scaffoldBackgroundColor,
            Color(dark ? 0xff212121 : 0xffffffff),
          );
          expect(
            theme.appBarTheme.backgroundColor,
            Color(dark ? 0xff191919 : 0xfff0f0f0),
          );
          expect(tester.takeException(), isNull);
        });
      }
    }
  }

  testWidgets(
    'the composer is usable with a keyboard and an idle realtime subscription',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(320, 568);
      tester.view.viewInsets = const FakeViewPadding(bottom: 248);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetViewInsets);
      final node = _IdleNode();
      final submission = SessionSubmission()..changeDraft('Draft to send');
      await tester.pumpWidget(
        WorkMobileApp(
          home: ConversationScreen(
            node: node,
            session: _IdleNode.session,
            submission: submission,
            close: () {},
            authorizationLost: () => fail('Unexpected logout'),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('已连接'), findsOneWidget);
      final input = tester.widget<TextField>(find.byType(TextField));
      expect(input.decoration?.enabledBorder, InputBorder.none);
      expect(input.decoration?.focusedBorder, InputBorder.none);
      final send = find.byTooltip('发送消息');
      expect(send.hitTestable(), findsOneWidget);
      await tester.tap(send);
      await tester.pumpAndSettle();
      expect(node.submitted, 'Draft to send');
      expect(submission.draft, isEmpty);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const WorkMobileApp());
      unawaited(node.stream.close());
      await tester.pump();
    },
  );
}

class _IdleNode extends Fake implements WorkNode {
  static const session = SessionSummary(
    id: 'session',
    title: 'Mobile chat',
    status: 'idle',
    updatedAt: 0,
    lastSeq: 7,
    pinned: false,
    archived: false,
  );
  final stream = StreamController<WireEvent>.broadcast();
  String? submitted;

  @override
  Future<SessionSummary> getSession(String id) async => session;

  @override
  Future<SessionSummary> openSession(String id) async => session;

  @override
  Future<SessionAccess> access(String id) async => const SessionAccess(
    commands: {'prompt'},
    snapshotLimit: false,
    dialogReply: false,
  );

  @override
  Future<SessionSnapshot> snapshot(String id, {int? limit}) async =>
      const SessionSnapshot(
        sessionId: 'session',
        status: 'idle',
        lastSeq: 7,
        messages: [
          {'role': 'assistant', 'content': 'Saved message'},
        ],
        events: [],
        pendingDialogs: [],
        hasMore: false,
      );

  @override
  Stream<WireEvent> events(
    String id, {
    required int afterSeq,
    void Function()? onSubscribed,
  }) {
    if (onSubscribed != null) scheduleMicrotask(onSubscribed);
    return stream.stream;
  }

  @override
  Future<void> prompt(String id, String text) async {
    submitted = text;
  }
}
