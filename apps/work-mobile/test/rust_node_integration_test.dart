import 'dart:async';

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:work_mobile/src/conversation/transcript.dart';
import 'package:work_mobile/src/node/work_node.dart';

void main() {
  final fixturePath = Platform.environment['WORK_MOBILE_NODE_FIXTURE'];
  final rpcPath = Platform.environment['WORK_MOBILE_RPC_FIXTURE'];
  test(
    'pairs, reads saved history, and receives a settled reply from the Rust node',
    skip: fixturePath == null || rpcPath == null
        ? 'Set WORK_MOBILE_NODE_FIXTURE and WORK_MOBILE_RPC_FIXTURE.'
        : false,
    () async {
      const password = 'isolated-flutter-fixture';
      final server = await Process.start(
        fixturePath!,
        [rpcPath!, '--web-proxy'],
        environment: {
          'WORK_FIXTURE_PASSWORD': password,
          'WORK_FIXTURE_ALLOWED_HOST': '192.0.2.1:13031',
        },
      );
      server.stderr.drain<void>();
      addTearDown(() async {
        server.stdin.writeln('{"op":"shutdown"}');
        await server.stdin.close();
        await server.exitCode.timeout(
          const Duration(seconds: 10),
          onTimeout: () {
            server.kill();
            return server.exitCode;
          },
        );
      });
      final ready = jsonDecode(
        await server.stdout
            .transform(utf8.decoder)
            .transform(const LineSplitter())
            .first
            .timeout(const Duration(seconds: 10)),
      ) as Json;
      final url = Uri.parse(ready['url'] as String);
      final nodeId = ready['nodeId'] as String;
      final id = ready['sessionId'] as String;
      final setup = HttpClient();
      addTearDown(() => setup.close(force: true));
      final loginRequest = await setup.postUrl(url.resolve('/api/v1/auth'));
      loginRequest.headers.contentType = ContentType.json;
      loginRequest.headers.set('x-rpi-web', '1');
      loginRequest.write(jsonEncode({'password': password}));
      final login = await loginRequest.close();
      expect(login.statusCode, 200);
      await login.drain<void>();
      final qr = await setup.postUrl(url.resolve('/api/v1/auth/qr'));
      qr.headers.set('x-rpi-web', '1');
      qr.cookies.addAll(login.cookies);
      qr.headers.contentType = ContentType.json;
      qr.write('{}');
      final issued = await qr.close();
      expect(issued.statusCode, 200);
      final ticket = jsonDecode(await utf8.decoder.bind(issued).join()) as Json;

      await HttpOverrides.runWithHttpOverrides(() async {
        final node = await WorkNode.connect(
          PairingCode.parse(
            jsonEncode({
              ...ticket,
              'v': 1,
              'type': 'rpi-work-node-auth',
              'address': 'http://192.0.2.1:13031',
            }),
          ),
        );
        try {
          final catalog = await node.catalog();
          expect(catalog.projects.single.id, ready['projectId']);
          expect(catalog.sessions.single.id, id);
          final project = await node.createProject('Flutter mobile test');
          expect(
            (await node.catalog()).projects.map((item) => item.id),
            contains(project.id),
          );
          await node.renameSession(id, 'Mobile integration');
          await node.setSessionPreferences(id, pinned: true);
          final opened = await node.openSession(id);
          expect(opened.title, 'Mobile integration');
          expect(opened.pinned, true);
          final access = await node.access(id);
          expect(access.commands, contains('prompt'));
          expect(access.imagePrompts, isTrue);
          final snapshot = await node.snapshot(
            id,
            limit: access.snapshotLimit ? 80 : null,
          );
          expect(snapshot.sessionId, id);
          final transcript = Transcript.fromSnapshot(snapshot);
          final live = StreamIterator(
            node.events(id, afterSeq: snapshot.lastSeq),
          );
          try {
            expect(await live.moveNext(), true);
            expect(live.current.type, 'ready');
            await node.promptWithImages(id, 'Flutter integration test', [
              {
                'type': 'image',
                'mimeType': 'image/png',
                'data': 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jD1sAAAAASUVORK5CYII=',
              },
            ]);
            while (await live.moveNext().timeout(const Duration(seconds: 10))) {
              final event = live.current;
              transcript.apply(event);
              if (event.type == 'agent_settled') break;
            }
            expect(
              transcript.messages.map((message) => message.text),
              contains('fixture reply'),
            );
            expect(transcript.running, false);
            expect(transcript.settlementCount, 1);
          } finally {
            await live.cancel();
          }
          final reopened = Transcript.fromSnapshot(await node.snapshot(id));
          expect(reopened.cursor, transcript.cursor);
          final replay = Transcript.fromSnapshot(snapshot);
          await for (final event
              in node
                  .events(id, afterSeq: snapshot.lastSeq)
                  .timeout(const Duration(seconds: 10))) {
            replay.apply(event);
            if (event.type == 'agent_settled') break;
          }
          expect(
            replay.messages.map((message) => message.text),
            contains('fixture reply'),
          );
          expect(replay.cursor, transcript.cursor);
        } finally {
          await node.logout();
        }
      }, _FixtureNetwork(url.port));
      expect(nodeId, isNotEmpty);
    },
  );
}

class _FixtureNetwork extends HttpOverrides {
  _FixtureNetwork(this.port);
  final int port;
  @override
  HttpClient createHttpClient(SecurityContext? context) =>
      super.createHttpClient(context)
        ..connectionFactory = (url, _, _) =>
            Socket.startConnect(InternetAddress.loopbackIPv4, port);
}
