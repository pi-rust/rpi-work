part of 'work_node.dart';

const _realtimeVersion = 1;
const _realtimePath = '/api/v1/realtime';
const _realtimeProtocol = 'rpi-work.realtime.v1';
const _maxRealtimeBytes = 2 * 1024 * 1024;
const _maxRealtimeEvents = 256;
const _resyncRequired = NodeException(
  'Live events require a fresh session snapshot.',
);
const _realtimeDisconnected = NodeException(
  'The live connection to the node ended.',
);

enum _SubscriptionPhase { hello, confirmation, events }

Stream<WireEvent> _events(
  WorkNode node,
  String sessionId,
  int afterSeq,
  void Function()? onSubscribed,
) async* {
  if (!_validRealtimeId(sessionId) || afterSeq < 0) {
    throw const NodeException('Invalid live subscription.');
  }
  final discovery = await node._request(
    'GET',
    ['transports'],
    decode: (value) {
      final json = _object(value);
      if (json['path'] != _realtimePath ||
          json['subprotocol'] != _realtimeProtocol) {
        throw const FormatException();
      }
      return _realtimeIdentity(node, json);
    },
  );
  final socket = await _upgradeRealtime(node);
  if (node._closed) {
    await _closeSocket(socket);
    node._sessionCookie();
  }
  node._sockets.add(socket);
  try {
    socket.pingInterval = _timeout;
    const requestId = 'subscribe';
    const subscriptionId = 'session-events';
    var phase = _SubscriptionPhase.hello;
    var cursor = afterSeq;
    yield* socket
        .timeout(
          _timeout,
          onTimeout: (sink) {
            if (phase != _SubscriptionPhase.events) {
              sink.addError(
                const NodeException(
                  'The node did not confirm the live subscription in time.',
                ),
              );
              sink.close();
            }
          },
        )
        .transform(
          StreamTransformer<dynamic, WireEvent>.fromHandlers(
            handleData: (message, sink) {
              try {
                if (message is! String ||
                    utf8.encode(message).length > _maxRealtimeBytes) {
                  throw const FormatException();
                }
                final frame = _object(jsonDecode(message));
                if (_integer(frame['version']) != _realtimeVersion) {
                  throw const FormatException();
                }
                final type = frame['type'];
                if (type == 'error') {
                  final responseTo = frame['requestId'];
                  if (responseTo != null && responseTo != requestId) {
                    throw const FormatException();
                  }
                  final error = _object(frame['error']);
                  throw NodeException(
                    'The node rejected the live subscription.',
                    status: error['code'] == 'accessDenied' ? 403 : null,
                  );
                }
                if (phase == _SubscriptionPhase.hello) {
                  if (type != 'hello') throw const FormatException();
                  final hello = _object(frame['hello']);
                  if (_realtimeIdentity(node, hello) != discovery ||
                      !_validRealtimeId(_string(hello['connectionId']))) {
                    throw const FormatException();
                  }
                  socket.add(
                    jsonEncode({
                      'type': 'subscribe',
                      'version': _realtimeVersion,
                      'requestId': requestId,
                      'sessionId': sessionId,
                      'subscriptionId': subscriptionId,
                      'afterSeq': afterSeq,
                    }),
                  );
                  phase = _SubscriptionPhase.confirmation;
                  return;
                }
                if (frame['sessionId'] != sessionId ||
                    frame['subscriptionId'] != subscriptionId) {
                  throw const FormatException();
                }
                if (phase == _SubscriptionPhase.confirmation) {
                  if (type != 'subscribed' || frame['requestId'] != requestId) {
                    throw const FormatException();
                  }
                  phase = _SubscriptionPhase.events;
                  onSubscribed?.call();
                  return;
                }
                final batch = frame['events'];
                if (type != 'events' ||
                    batch is! List<Object?> ||
                    batch.isEmpty ||
                    batch.length > _maxRealtimeEvents) {
                  throw const FormatException();
                }
                final events = <WireEvent>[];
                for (final item in batch) {
                  final event = WireEvent._fromJson(_object(item));
                  if (!_validRealtimeId(event.type)) {
                    throw const FormatException();
                  }
                  if (event.type == 'resync_required') throw _resyncRequired;
                  final seq = event.seq;
                  if (seq != null) {
                    if (seq <= cursor) continue;
                    if (seq - cursor != 1) throw _resyncRequired;
                    cursor = seq;
                  }
                  events.add(event);
                }
                for (final event in events) {
                  sink.add(event);
                }
              } on FormatException {
                sink.addError(const NodeException(_invalidResponse));
                sink.close();
              } on NodeException catch (error) {
                sink.addError(error);
                sink.close();
              } on StateError {
                sink.addError(_realtimeDisconnected);
                sink.close();
              }
            },
            handleError: (error, _, sink) {
              sink.addError(
                error is NodeException ? error : _realtimeDisconnected,
              );
              sink.close();
            },
            handleDone: (sink) {
              if (!node._closed) {
                final error = switch (socket.closeCode) {
                  1008 when socket.closeReason == 'authorization ended' =>
                    const NodeException(
                      'Node authorization has ended.',
                      status: 403,
                    ),
                  1013 => _resyncRequired,
                  _ => _realtimeDisconnected,
                };
                sink.addError(error);
              }
              sink.close();
            },
          ),
        );
  } finally {
    node._sockets.remove(socket);
    await _closeSocket(socket);
  }
}

({String ownerEpoch, bool commandQueries}) _realtimeIdentity(
  WorkNode node,
  Json json,
) {
  if (_integer(json['version']) != _realtimeVersion ||
      json['nodeId'] != node.nodeId) {
    throw const FormatException();
  }
  final ownerEpoch = _string(json['ownerEpoch']);
  final capabilities = _object(json['capabilities']);
  if (!_validRealtimeId(ownerEpoch) ||
      capabilities['jsonText'] != true ||
      capabilities['binary'] != false ||
      _integer(capabilities['maxMessageBytes']) != _maxRealtimeBytes ||
      _integer(capabilities['maxEventsPerFrame']) != _maxRealtimeEvents) {
    throw const FormatException();
  }
  final commandQueries = capabilities.containsKey('commandQueries')
      ? _boolean(capabilities['commandQueries'])
      : false;
  return (ownerEpoch: ownerEpoch, commandQueries: commandQueries);
}

Future<WebSocket> _upgradeRealtime(WorkNode node) async {
  // WebSocket.connect follows HTTP redirects. Disable them before attaching the cookie.
  HttpClientRequest? pending;
  Socket? detached;
  try {
    final request = await node._http
        .getUrl(node._url(['realtime']))
        .timeout(_timeout);
    pending = request;
    final random = Random.secure();
    final key = base64Encode(List.generate(16, (_) => random.nextInt(256)));
    request
      ..followRedirects = false
      ..headers.set(HttpHeaders.cookieHeader, node._sessionCookie())
      ..headers.set('x-rpi-web', '1')
      ..headers.set(HttpHeaders.connectionHeader, 'Upgrade')
      ..headers.set(HttpHeaders.upgradeHeader, 'websocket')
      ..headers.set('Sec-WebSocket-Key', key)
      ..headers.set('Sec-WebSocket-Version', '13')
      ..headers.set('Sec-WebSocket-Protocol', _realtimeProtocol);
    final response = await request.close().timeout(_timeout);
    if (response.statusCode != HttpStatus.switchingProtocols) {
      unawaited(response.drain<void>().catchError((Object _) {}));
      throw _httpError(response.statusCode);
    }
    final raw = await response.detachSocket().timeout(_timeout);
    detached = raw;
    final accept = base64Encode(
      sha1
          .convert(ascii.encode('${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11'))
          .bytes,
    );
    if (response.headers.value(HttpHeaders.upgradeHeader)?.toLowerCase() !=
            'websocket' ||
        !(response.headers[HttpHeaders.connectionHeader] ?? const <String>[])
            .expand((value) => value.split(','))
            .any((value) => value.trim().toLowerCase() == 'upgrade') ||
        response.headers.value('Sec-WebSocket-Accept') != accept ||
        response.headers.value('Sec-WebSocket-Protocol') != _realtimeProtocol ||
        response.headers['Sec-WebSocket-Extensions'] != null) {
      throw const FormatException();
    }
    final socket = WebSocket.fromUpgradedSocket(
      raw,
      protocol: _realtimeProtocol,
      serverSide: false,
      compression: CompressionOptions.compressionOff,
      maxPayloadLength: _maxRealtimeBytes,
    );
    detached = null;
    return socket;
  } on NodeException {
    rethrow;
  } on TimeoutException {
    pending?.abort();
    throw const NodeException(
      'The node did not open the live connection in time.',
    );
  } on FormatException {
    throw const NodeException(_invalidResponse);
  } on IOException {
    throw const NodeException('The live connection to the node failed.');
  } finally {
    detached?.destroy();
  }
}

Future<void> _closeSocket(WebSocket socket) async {
  try {
    socket.pingInterval = null;
    await socket.close(WebSocketStatus.normalClosure);
  } on Exception {
    // A failed connection can already be closing.
  }
}
