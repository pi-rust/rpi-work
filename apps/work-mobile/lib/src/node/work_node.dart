import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';

part 'node_models.dart';
part 'pairing_code.dart';
part 'realtime.dart';

const _timeout = Duration(seconds: 20);
const _invalidResponse = 'The node returned an invalid response.';

class WorkNode {
  WorkNode._(this.origin, this.nodeId)
    : _cookieName =
          'rpi_work_desktop_${sha256.convert(utf8.encode(nodeId)).toString().substring(0, 16)}',
      _http = HttpClient()
        ..connectionTimeout = _timeout
        ..findProxy = ((_) => 'DIRECT');

  final Uri origin;
  final String nodeId;
  final HttpClient _http;
  final String _cookieName;
  Cookie? _cookie;
  DateTime? _cookieExpiresAt;
  final _sockets = <WebSocket>{};
  bool _closed = false;

  static Future<WorkNode> connect(
    PairingCode code, {
    bool Function()? isCurrent,
  }) async {
    final node = WorkNode._(code.origin, code.nodeId);
    try {
      void checkCode() {
        if (isCurrent?.call() == false) {
          throw const NodeException('授权已取消，请重新扫码。');
        }
        if (!code.expiresAt.isAfter(DateTime.now())) {
          throw const NodeException(
            'Pairing code has expired. Scan a new code.',
          );
        }
      }

      checkCode();
      final health = await node._request(
        'GET',
        ['health'],
        authenticated: false,
        decode: _object,
      );
      if (health['sharingEnabled'] != true || health['nodeId'] != code.nodeId) {
        throw const NodeException(
          'The pairing code does not match a node with sharing enabled.',
        );
      }
      checkCode();
      await node._request(
        'POST',
        ['auth', 'qr', 'redeem'],
        authenticated: false,
        mutation: true,
        body: {'ticket': code._ticket},
        captureCookie: true,
        decode: (value) {
          if (_object(value)['authenticated'] != true) {
            throw const FormatException();
          }
        },
      );
      await node._request(
        'GET',
        ['auth'],
        decode: (value) {
          if (_object(value)['authenticated'] != true) {
            throw const NodeException(
              'The node did not confirm authorization.',
              status: 401,
            );
          }
        },
      );
      return node;
    } finally {
      if (node._cookie == null || !node._confirmed) node.close();
    }
  }

  bool _confirmed = false;

  Future<WorkspaceCatalog> catalog() async {
    final projects = await _request(
      'GET',
      ['projects'],
      decode: (value) => _array(
        _object(value)['projects'],
        (item) => WorkProject._fromJson(_object(item)),
      ),
    );
    final sessions = await _request(
      'GET',
      ['sessions'],
      decode: (value) => _array(
        _object(value)['sessions'],
        (item) => SessionSummary._fromJson(_object(item)),
      ),
    );
    return WorkspaceCatalog(projects: projects, sessions: sessions);
  }

  Future<WorkProject> createProject(String name) => _request(
    'POST',
    ['projects'],
    mutation: true,
    body: {'name': name},
    decode: (value) =>
        WorkProject._fromJson(_object(_object(value)['project'])),
  );

  Future<SessionSummary> getSession(String id) => _sessionSummary('GET', id);

  Future<SessionSummary> openSession(String id) =>
      _sessionSummary('POST', id, 'open');

  Future<SessionSummary> _sessionSummary(
    String method,
    String id, [
    String? action,
  ]) => _request(
    method,
    _session(id, action),
    mutation: action != null,
    decode: (value) {
      final session = SessionSummary._fromJson(_object(value));
      if (session.id != id) throw const FormatException();
      return session;
    },
  );

  Future<String> createSession(String projectId) => _request(
    'POST',
    ['sessions'],
    mutation: true,
    body: {'projectId': projectId},
    decode: (value) {
      final id = _string(_object(value)['sessionId']);
      if (!_validRealtimeId(id)) throw const FormatException();
      return id;
    },
  );

  Future<void> renameSession(String id, String title) => _request(
    'POST',
    _session(id, 'rename'),
    mutation: true,
    body: {'title': title},
    decode: (value) => _checkSessionMutation(value, id),
  );

  Future<void> setSessionPreferences(
    String id, {
    bool? pinned,
    bool? archived,
  }) => _request(
    'POST',
    _session(id, 'metadata'),
    mutation: true,
    body: {'archived': archived, 'pinned': pinned},
    decode: (value) => _checkSessionMutation(value, id),
  );

  Future<SessionAccess> access(String id) => _request(
    'GET',
    _session(id, 'access'),
    decode: (value) => SessionAccess._fromJson(_object(value)),
  );

  Future<SessionSnapshot> snapshot(String id, {int? limit}) => _request(
    'GET',
    _session(id, 'snapshot'),
    query: limit == null ? null : {'limit': '$limit'},
    decode: (value) {
      final snapshot = SessionSnapshot._fromJson(_object(value));
      if (snapshot.sessionId != id) throw const FormatException();
      return snapshot;
    },
  );

  Stream<WireEvent> events(
    String id, {
    required int afterSeq,
    void Function()? onSubscribed,
  }) => _events(this, id, afterSeq, onSubscribed);

  Future<void> prompt(String id, String text) =>
      _command(id, 'prompt', {'message': text});

  Future<void> promptWithImages(String id, String text, List<Json> images) =>
      _command(id, 'prompt', {
        'contentBlocks': [
          if (text.trim().isNotEmpty) {'type': 'text', 'text': text},
          ...images,
        ],
      });

  Future<void> abort(String id) => _command(id, 'abort', const {});

  Future<void> _command(String id, String command, Json data) => _request(
    'POST',
    _session(id, 'commands'),
    mutation: true,
    body: {'type': command, ...data},
    decode: (value) {
      final reply = _object(value);
      if (reply['type'] != 'response' || reply['command'] != command) {
        throw const FormatException();
      }
      final success = _boolean(reply['success']);
      final data = reply['data'];
      final accepted = data is Json ? data['accepted'] : null;
      if (!success || accepted == false) {
        throw const NodeException('节点拒绝了会话操作。');
      }
      // RPI acknowledges immediately. An ACP prompt replies at the end of the turn.
      final acpCompleted = data is Json && data['stopReason'] is String;
      if (command == 'prompt' && accepted != true && !acpCompleted) {
        throw const FormatException();
      }
    },
  );

  Future<void> respondToDialog(
    String id,
    String requestId, {
    Object? value,
    bool cancelled = false,
  }) => _request(
    'POST',
    _session(id, 'dialogs'),
    mutation: true,
    body: {
      'requestId': requestId,
      if (cancelled) 'cancelled': true else 'answer': value,
    },
    decode: (value) {
      final receipt = _object(value);
      if (!_boolean(receipt['accepted'] ?? receipt['ok'])) {
        throw const NodeException('The node rejected the dialog response.');
      }
    },
  );

  Future<void> logout() async {
    if (_closed) return;
    try {
      await _request(
        'DELETE',
        ['auth'],
        mutation: true,
        decode: (value) {
          if (_object(value)['authenticated'] != false) {
            throw const FormatException();
          }
        },
      );
    } finally {
      close();
    }
  }

  void close() {
    if (_closed) return;
    _closed = true;
    _cookie = null;
    _cookieExpiresAt = null;
    for (final socket in _sockets) {
      unawaited(_closeSocket(socket));
    }
    _sockets.clear();
    _http.close(force: true);
  }

  String _sessionCookie() {
    final cookie = _cookie;
    final expires = _cookieExpiresAt;
    if (_closed ||
        cookie == null ||
        (expires != null && !expires.isAfter(DateTime.now()))) {
      throw const NodeException('Node authorization has ended.', status: 401);
    }
    return '${cookie.name}=${cookie.value}';
  }

  Uri _url(List<String> path, [Map<String, String>? query]) => origin.replace(
    pathSegments: ['api', 'v1', ...path],
    queryParameters: query,
  );

  Future<T> _request<T>(
    String method,
    List<String> path, {
    Json? body,
    Map<String, String>? query,
    bool authenticated = true,
    bool mutation = false,
    bool captureCookie = false,
    required T Function(Object?) decode,
  }) async {
    final cookie = authenticated ? _sessionCookie() : null;
    final String? encoded;
    try {
      encoded = body == null ? null : jsonEncode(body);
    } on JsonUnsupportedObjectError {
      throw const NodeException('The request contains an unsupported value.');
    }
    var sent = false;
    HttpClientRequest? pending;
    try {
      final request = await _http
          .openUrl(method, _url(path, query))
          .timeout(_timeout);
      pending = request;
      request
        ..followRedirects = false
        ..headers.set('x-rpi-web', '1')
        ..headers.set(HttpHeaders.acceptHeader, 'application/json');
      if (cookie != null) request.headers.set(HttpHeaders.cookieHeader, cookie);
      if (encoded != null) {
        request.headers.contentType = ContentType.json;
        request.write(encoded);
      }
      sent = true;
      final response = await request.close().timeout(_timeout);
      final status = response.statusCode;
      if (status < 200 || status >= 300) {
        unawaited(response.drain<void>().catchError((Object _) {}));
        throw _httpError(status, mutation: mutation);
      }
      final result = decode(
        jsonDecode(await utf8.decoder.bind(response).join().timeout(_timeout)),
      );
      if (captureCookie) _captureCookie(response);
      if (method == 'GET' && path.length == 1 && path.single == 'auth') {
        _confirmed = true;
      }
      return result;
    } on FormatException {
      throw NodeException(_invalidResponse, outcomeUnknown: mutation && sent);
    } on TimeoutException {
      pending?.abort();
      throw NodeException(
        'The node did not respond in time.',
        outcomeUnknown: mutation && sent,
      );
    } on IOException {
      throw NodeException(
        'The connection to the node failed.',
        outcomeUnknown: mutation && sent,
      );
    }
  }

  void _captureCookie(HttpClientResponse response) {
    for (final cookie in response.cookies) {
      if (cookie.name == _cookieName &&
          (cookie.domain == null || cookie.domain == origin.host) &&
          cookie.path == '/' &&
          (!cookie.secure || origin.scheme == 'https') &&
          (cookie.maxAge == null || cookie.maxAge! > 0) &&
          _secretPattern.hasMatch(cookie.value)) {
        final age = cookie.maxAge;
        _cookieExpiresAt = age == null
            ? cookie.expires
            : DateTime.now().add(Duration(seconds: age));
        _cookie = cookie;
        return;
      }
    }
    throw const FormatException();
  }
}

List<String> _session(String id, [String? action]) {
  if (!_validRealtimeId(id) || id == '.' || id == '..') {
    throw const NodeException('Invalid session ID.');
  }
  return ['sessions', id, ?action];
}

void _checkSessionMutation(Object? value, String id) {
  if (_object(value)['sessionId'] != id) throw const FormatException();
}

NodeException _httpError(int status, {bool mutation = false}) {
  final message = switch (status) {
    401 || 403 => 'Node authorization was denied or has ended.',
    >= 300 && < 400 => 'The node returned a redirect.',
    429 => 'The node received too many requests.',
    _ => 'The node request failed (HTTP $status).',
  };
  return NodeException(
    message,
    status: status,
    outcomeUnknown:
        mutation && (status == 408 || (status >= 500 && status != 501)),
  );
}
