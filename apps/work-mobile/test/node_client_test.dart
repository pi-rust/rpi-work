import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:work_mobile/src/node/work_node.dart';

const _nodeId = 'node-fixture';
const _sessionId = 'session-fixture';
const _protocol = 'rpi-work.realtime.v1';
const _capabilities = <String, Object>{
  'jsonText': true,
  'binary': false,
  'commandQueries': true,
  'maxMessageBytes': 2 * 1024 * 1024,
  'maxEventsPerFrame': 256,
};
final _ticket = 'ab' * 32;
final _token = 'cd' * 32;
final _nodeHash = sha256
    .convert(utf8.encode(_nodeId))
    .toString()
    .substring(0, 16);
final _cookie = 'rpi_work_desktop_$_nodeHash=$_token';

void main() {
  test(
    'checks health identity and sharing before posting the pairing ticket',
    () async {
      final fixture = await _Fixture.start();
      await fixture.connect();
      expect(
        fixture.requests.map((request) => '${request.method} ${request.path}'),
        [
          'GET /api/v1/health',
          'POST /api/v1/auth/qr/redeem',
          'GET /api/v1/auth',
        ],
      );
      final [health, redeem, auth] = fixture.requests;
      expect(health.cookie, isNull);
      expect(health.body, isEmpty);
      expect(redeem.cookie, isNull);
      expect(redeem.marker, '1');
      expect(redeem.contentType, 'application/json');
      expect(jsonDecode(redeem.body), {'ticket': _ticket});
      expect(auth.cookie, _cookie);
    },
  );

  test(
    'rejects a mismatched health response before sending any credentials',
    () async {
      final fixture = await _Fixture.start(healthNodeId: 'different-node');
      await expectLater(fixture.connect(), throwsA(isA<NodeException>()));
      expect(fixture.requests.single.path, '/api/v1/health');
      expect(fixture.requests.single.cookie, isNull);
      expect(fixture.requests.single.body, isEmpty);
    },
  );

  test(
    'uses authenticated discovery and subscribes from the snapshot cursor',
    () async {
      final fixture = await _Fixture.start(
        onSubscribe: (socket, request) {
          _sendEvents(socket, request, [
            _event('ready'),
            _event('agent_start', seq: 7),
            _event('message_update', seq: 8),
            _event('message_end', seq: 9),
          ]);
        },
      );
      final node = await fixture.connect();
      final result = await node
          .events(_sessionId, afterSeq: 7)
          .take(3)
          .toList();
      expect(result.map((event) => (event.type, event.seq)), [
        ('ready', null),
        ('message_update', 8),
        ('message_end', 9),
      ]);
      expect(result.last.data, {'type': 'message_end'});
      final [discovery, upgrade] = fixture.requests.skip(3).toList();
      expect(discovery.path, '/api/v1/transports');
      expect(discovery.cookie, _cookie);
      expect(upgrade.path, '/api/v1/realtime');
      expect(upgrade.query, isEmpty);
      expect(upgrade.cookie, _cookie);
      expect(upgrade.protocol, _protocol);
      expect(fixture.subscription, {
        'type': 'subscribe',
        'version': 1,
        'requestId': isA<String>(),
        'sessionId': _sessionId,
        'subscriptionId': isA<String>(),
        'afterSeq': 7,
      });
      await fixture.socketClosed.future.timeout(const Duration(seconds: 3));
    },
  );

  test(
    'rejects discovery for another node before attempting the upgrade',
    () async {
      final fixture = await _Fixture.start(discoveryNodeId: 'different-node');
      final node = await fixture.connect();
      await expectLater(
        node.events(_sessionId, afterSeq: 7),
        emitsError(isA<NodeException>()),
      );
      expect(fixture.requests.last.path, '/api/v1/transports');
    },
  );

  test(
    'requires the owner epoch in hello to match authenticated discovery',
    () async {
      final fixture = await _Fixture.start(helloEpoch: 'changed-epoch');
      final node = await fixture.connect();
      await expectLater(
        node.events(_sessionId, afterSeq: 7),
        emitsInOrder([emitsError(isA<NodeException>()), emitsDone]),
      );
      expect(fixture.requests.last.path, '/api/v1/realtime');
      expect(fixture.subscription, isNull);
    },
  );

  for (final path in ['auth/qr/redeem', 'transports', 'realtime']) {
    test('refuses /$path redirects with credentials', () async {
      final fixture = await _Fixture.start(redirectPath: '/api/v1/$path');
      final Matcher redirect = isA<NodeException>().having(
        (error) => error.status,
        'status',
        307,
      );
      if (path == 'auth/qr/redeem') {
        await expectLater(fixture.connect(), throwsA(redirect));
      } else {
        final node = await fixture.connect();
        await expectLater(
          node.events(_sessionId, afterSeq: 7),
          emitsError(redirect),
        );
      }
      expect(fixture.requests.last.path, '/api/v1/$path');
      expect(
        fixture.requests.where((request) => request.path == '/redirect-target'),
        isEmpty,
      );
    });
  }

  for (final resync in [
    _event('resync_required'),
    _event('message_end', seq: 9),
  ]) {
    test(
      '${resync['event_type']} requests a fresh snapshot and ends the stream',
      () async {
        final fixture = await _Fixture.start(
          onSubscribe: (socket, request) {
            _sendEvents(socket, request, [resync]);
            _sendEvents(socket, request, [_event('message_update', seq: 8)]);
          },
        );
        final node = await fixture.connect();
        await expectLater(
          node.events(_sessionId, afterSeq: 7),
          emitsInOrder([
            emitsError(
              isA<NodeException>().having(
                (error) => error.message,
                'message',
                contains('fresh session snapshot'),
              ),
            ),
            emitsDone,
          ]),
        );
        await fixture.socketClosed.future.timeout(const Duration(seconds: 3));
      },
    );
  }

  test(
    'an unexpected socket close fails the stream without exposing its reason',
    () async {
      final fixture = await _Fixture.start(
        onSubscribe: (socket, _) {
          unawaited(socket.close(WebSocketStatus.goingAway, _token));
        },
      );
      final node = await fixture.connect();
      await expectLater(
        node.events(_sessionId, afterSeq: 7),
        emitsInOrder([
          emitsError(
            isA<NodeException>().having(
              (error) => error.toString(),
              'message',
              'The live connection to the node ended.',
            ),
          ),
          emitsDone,
        ]),
      );
    },
  );
}

Map<String, Object?> _event(String type, {int? seq}) => {
  'id': seq?.toString(),
  'seq': seq,
  'event_type': type,
  'data': {'type': type},
};

void _sendEvents(WebSocket socket, Json request, List<Json> events) {
  socket.add(
    jsonEncode({
      'type': 'events',
      'version': 1,
      'sessionId': request['sessionId'],
      'subscriptionId': request['subscriptionId'],
      'events': events,
    }),
  );
}

class _Fixture {
  _Fixture(
    this.server,
    this.healthNodeId,
    this.discoveryNodeId,
    this.helloEpoch,
    this.redirectPath,
    this.onSubscribe,
  );

  static Future<_Fixture> start({
    String healthNodeId = _nodeId,
    String discoveryNodeId = _nodeId,
    String helloEpoch = 'node-epoch',
    String? redirectPath,
    void Function(WebSocket, Json)? onSubscribe,
  }) async {
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final fixture = _Fixture(
      server,
      healthNodeId,
      discoveryNodeId,
      helloEpoch,
      redirectPath,
      onSubscribe,
    );
    server.listen(fixture.handle);
    addTearDown(() async {
      for (final socket in fixture.sockets) {
        unawaited(socket.close());
      }
      await server.close(force: true);
    });
    return fixture;
  }

  final HttpServer server;
  final String healthNodeId;
  final String discoveryNodeId;
  final String helloEpoch;
  final String? redirectPath;
  final void Function(WebSocket, Json)? onSubscribe;
  final requests =
      <
        ({
          String method,
          String path,
          String query,
          String? cookie,
          String? marker,
          String? contentType,
          String? protocol,
          String body,
        })
      >[];
  final sockets = <WebSocket>[];
  final socketClosed = Completer<void>();
  Json? subscription;

  Future<WorkNode> connect() async {
    final node = await _LoopbackOverride().run(
      () => WorkNode.connect(
        PairingCode.parse(
          jsonEncode({
            'v': 1,
            'type': 'rpi-work-node-auth',
            'address': 'http://192.0.2.1:${server.port}',
            'nodeId': _nodeId,
            'expiresAt': DateTime.now().millisecondsSinceEpoch ~/ 1000 + 120,
            'ticket': _ticket,
          }),
        ),
      ),
    );
    addTearDown(node.close);
    return node;
  }

  Future<void> handle(HttpRequest request) async {
    final path = request.uri.path;
    requests.add((
      method: request.method,
      path: path,
      query: request.uri.query,
      cookie: request.headers.value(HttpHeaders.cookieHeader),
      marker: request.headers.value('x-rpi-web'),
      contentType: request.headers.contentType?.mimeType,
      protocol: request.headers.value('sec-websocket-protocol'),
      body: await utf8.decoder.bind(request).join(),
    ));
    final response = request.response;
    if (path == redirectPath) {
      response.statusCode = HttpStatus.temporaryRedirect;
      response.headers.set(HttpHeaders.locationHeader, '/redirect-target');
      await response.close();
      return;
    }
    if (path == '/api/v1/realtime') {
      final socket = await WebSocketTransformer.upgrade(
        request,
        protocolSelector: (protocols) =>
            protocols.contains(_protocol) ? _protocol : null,
      );
      sockets.add(socket);
      socket.add(
        jsonEncode({
          'type': 'hello',
          'version': 1,
          'hello': {
            'version': 1,
            'nodeId': _nodeId,
            'ownerEpoch': helloEpoch,
            'connectionId': 'node-connection',
            'capabilities': _capabilities,
          },
        }),
      );
      socket.listen((message) {
        final frame = jsonDecode(message as String) as Json;
        subscription = frame;
        socket.add(
          jsonEncode({
            'type': 'subscribed',
            'version': 1,
            'requestId': frame['requestId'],
            'sessionId': frame['sessionId'],
            'subscriptionId': frame['subscriptionId'],
          }),
        );
        onSubscribe?.call(socket, frame);
      }, onDone: () => socketClosed.complete());
      return;
    }
    if (path == '/api/v1/auth/qr/redeem') {
      response.headers.add(
        HttpHeaders.setCookieHeader,
        '$_cookie; HttpOnly; SameSite=Strict; Path=/; Max-Age=600',
      );
    }
    final payload = switch (path) {
      '/api/v1/health' => {'sharingEnabled': true, 'nodeId': healthNodeId},
      '/api/v1/transports' => {
        'version': 1,
        'nodeId': discoveryNodeId,
        'ownerEpoch': 'node-epoch',
        'path': '/api/v1/realtime',
        'subprotocol': _protocol,
        'capabilities': _capabilities,
      },
      '/api/v1/auth' || '/api/v1/auth/qr/redeem' => {'authenticated': true},
      _ => null,
    };
    if (payload == null) response.statusCode = HttpStatus.notFound;
    response.headers.contentType = ContentType.json;
    response.write(jsonEncode(payload));
    await response.close();
  }
}

// Pairing rejects loopback origins. Map the fixture origin to its local server.
class _LoopbackOverride extends HttpOverrides {
  Future<T> run<T>(Future<T> Function() action) =>
      HttpOverrides.runWithHttpOverrides(action, this);

  @override
  HttpClient createHttpClient(SecurityContext? context) =>
      super.createHttpClient(context)
        ..connectionFactory = (uri, _, _) =>
            Socket.startConnect(InternetAddress.loopbackIPv4, uri.port);
}
