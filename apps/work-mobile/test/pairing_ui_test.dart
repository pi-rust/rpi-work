import 'dart:async';
import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:image_picker/image_picker.dart';
import 'package:work_mobile/main.dart';
import 'package:work_mobile/src/connect.dart';
import 'package:work_mobile/src/node/work_node.dart';

const _origin = 'https://studio-node.example:13031';
const _expired = '授权码已过期，请在电脑上生成新二维码。';
const _scanner = MethodChannel('dev.steenbakker.mobile_scanner/scanner/method');

String _code(int seconds) => jsonEncode({
  'v': 1,
  'type': 'rpi-work-node-auth',
  'address': _origin,
  'ticket': 'ab' * 32,
  'expiresAt': DateTime.now().millisecondsSinceEpoch ~/ 1000 + seconds,
  'nodeId': 'studio-01',
});

class _Pairing {
  _Pairing(this.tester);

  final WidgetTester tester;
  var connects = 0;
  Completer<void>? request;
  PairingCode? submitted;
  XFile? image;

  Future<void> open() async {
    await tester.pumpWidget(
      WorkMobileApp(
        home: ConnectScreen(
          pickImage: () async => image,
          connect: (code, isCurrent) async {
            expect(isCurrent(), isTrue);
            connects++;
            submitted = code;
            await request?.future;
          },
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  void mock(
    MethodChannel channel,
    Future<Object?> Function(MethodCall) handle,
  ) {
    final messenger = tester.binding.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(channel, handle);
    addTearDown(() => messenger.setMockMethodCallHandler(channel, null));
  }

  Future<void> select(List<String>? barcodes) async {
    image = XFile.fromData(Uint8List.fromList([0]), name: 'photo.png');
    MethodCall? analyzed;
    mock(_scanner, (call) async {
      if (call.method != 'analyzeImage') return null;
      analyzed = call;
      return barcodes == null
          ? null
          : {
              'data': [
                for (final raw in barcodes) {'rawValue': raw},
              ],
            };
    });
    await tester.tap(find.text('从相册选取'));
    await tester.pumpAndSettle();
    expect(analyzed?.arguments['formats'], [256]);
  }

  Future<void> confirm() async {
    await tester.tap(find.text('授权并打开'));
    await tester.pumpAndSettle();
  }
}

void main() {
  final scenarios = <String, Future<void> Function(_Pairing)>{
    '013 gallery cancel restores the two entry actions': (flow) async {
      await flow.tester.tap(find.text('从相册选取'));
      await flow.tester.pumpAndSettle();
      expect(find.text('打开相机').hitTestable(), findsOneWidget);
      expect(find.text('从相册选取').hitTestable(), findsOneWidget);
      expect(find.text('正在读取…'), findsNothing);
    },
    '014 an unrecognized image offers a retry': (flow) async {
      await flow.select(null);
      expect(find.text('图片中没有可识别的二维码。'), findsOneWidget);
      expect(find.text('从相册选取').hitTestable(), findsOneWidget);
    },
    '015 a malformed authorization gives localized feedback': (flow) async {
      await flow.select(['a generic QR value']);
      expect(find.text('请使用电脑生成的有效 RPI Work 授权二维码。'), findsOneWidget);
      expect(find.text('授权并打开'), findsNothing);
    },
    '016 multiple codes require an unambiguous retry': (flow) async {
      await flow.select([_code(80), _code(90)]);
      expect(find.text('请选择只包含一个授权二维码的图片，或重新扫码。'), findsOneWidget);
      expect(find.text(_origin), findsNothing);
    },
    '017 expired authorization asks for a new code when scanned': (flow) async {
      await flow.select([_code(-1)]);
      expect(find.text(_expired), findsOneWidget);
      expect(find.text('授权并打开'), findsNothing);
    },
    '018 a code that expires on the confirmation screen is cleared':
        (flow) async {
          await flow.select([_code(15)]);
          expect(find.text(_origin), findsOneWidget);
          await flow.tester.pump(const Duration(seconds: 16));
          expect(find.text(_expired), findsOneWidget);
          expect(find.text(_origin), findsNothing);
        },
    '019 the confirmation displays an accessible address before sending':
        (flow) async {
          final semantics = flow.tester.ensureSemantics();
          try {
            await flow.select([_code(90)]);
            expect(find.text('确认一次性授权'), findsOneWidget);
            expect(find.bySemanticsLabel('节点地址 $_origin'), findsOneWidget);
            expect(flow.connects, 0);
          } finally {
            semantics.dispose();
          }
        },
    '020 explicit cancel restores scan and gallery without connecting':
        (flow) async {
          await flow.select([_code(90)]);
          await flow.tester.tap(find.text('取消'));
          await flow.tester.pumpAndSettle();
          expect(find.text(_origin), findsNothing);
          expect(find.text('打开相机').hitTestable(), findsOneWidget);
          expect(find.text('从相册选取').hitTestable(), findsOneWidget);
          expect(flow.connects, 0);
        },
    '021 pending authorization retains the address and prevents duplicate actions':
        (flow) async {
          await flow.select([_code(90)]);
          flow.request = Completer<void>();
          await flow.confirm();
          expect(flow.submitted?.origin.toString(), _origin);
          expect(find.text('正在授权…'), findsOneWidget);
          expect(find.text(_origin), findsOneWidget);
          await flow.tester.tap(find.text('正在授权…'));
          await flow.tester.tap(find.text('取消'));
          expect(flow.connects, 1);
          flow.request!.complete();
          await flow.tester.pumpAndSettle();
        },
    '022 a rejected connection returns to pairing with the node error':
        (flow) async {
          await flow.select([_code(90)]);
          flow.request = Completer<void>();
          await flow.confirm();
          flow.request!.completeError(const NodeException('节点地址不可达。'));
          await flow.tester.pumpAndSettle();
          expect(find.text('连接失败。节点地址不可达。'), findsOneWidget);
          expect(find.text('打开相机').hitTestable(), findsOneWidget);
        },
    '023 an uncertain authorization requests a fresh code instead of retrying':
        (flow) async {
          await flow.select([_code(90)]);
          flow.request = Completer<void>();
          await flow.confirm();
          flow.request!.completeError(
            const NodeException('timeout', outcomeUnknown: true),
          );
          await flow.tester.pumpAndSettle();
          expect(find.text('授权结果未知，请在电脑上生成新二维码后重试。'), findsOneWidget);
          expect(find.text('授权并打开'), findsNothing);
        },
    '024 denied camera permission leaves the gallery available': (flow) async {
      flow.mock(
        _scanner,
        (call) async => switch (call.method) {
          'state' => 0,
          'request' => false,
          _ => null,
        },
      );
      await flow.tester.tap(find.text('打开相机'));
      await flow.tester.pumpAndSettle();
      expect(find.text('无法使用相机。请检查系统权限，或从相册选取二维码。'), findsOneWidget);
      expect(find.text('从相册选取').hitTestable(), findsOneWidget);
      await flow.tester.tap(find.text('关闭相机'));
      await flow.tester.pumpAndSettle();
      expect(find.text('打开相机').hitTestable(), findsOneWidget);
    },
  };
  for (final MapEntry(key: label, value: check) in scenarios.entries) {
    testWidgets(label, (tester) async {
      final flow = _Pairing(tester);
      await flow.open();
      await check(flow);
      await tester.pumpWidget(const WorkMobileApp());
      expect(tester.takeException(), isNull);
    });
  }
}
