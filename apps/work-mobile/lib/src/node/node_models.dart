part of 'work_node.dart';

typedef Json = Map<String, Object?>;

class NodeException implements Exception {
  const NodeException(this.message, {this.status, this.outcomeUnknown = false});

  final String message;
  final int? status;
  final bool outcomeUnknown;
  bool get authorizationLost => status == 401 || status == 403;

  @override
  String toString() => message;
}

class WorkspaceCatalog {
  const WorkspaceCatalog({required this.projects, required this.sessions});

  final List<WorkProject> projects;
  final List<SessionSummary> sessions;
}

class WorkProject {
  const WorkProject({required this.id, required this.name, required this.path});

  factory WorkProject._fromJson(Json json) => WorkProject(
    id: _string(json['id']),
    name: _string(json['name']),
    path: _string(json['path']),
  );

  final String id;
  final String name;
  final String path;
}

class SessionSummary {
  const SessionSummary({
    required this.id,
    required this.title,
    required this.status,
    this.projectId,
    required this.updatedAt,
    required this.lastSeq,
    required this.pinned,
    required this.archived,
  });

  factory SessionSummary._fromJson(Json json) {
    final id = _string(json['sessionId']);
    return SessionSummary(
      id: id,
      title: _optionalString(json['title']) ?? id,
      status: _string(json['status']),
      projectId: _optionalString(json['projectId']),
      updatedAt: _integer(json['updatedAt']),
      lastSeq: _integer(json['lastSeq']),
      pinned: _boolean(json['pinned'] ?? false),
      archived: _boolean(json['archived'] ?? false),
    );
  }

  final String id;
  final String title;
  final String status;
  final String? projectId;
  final int updatedAt;
  final int lastSeq;
  final bool pinned;
  final bool archived;
}

class SessionAccess {
  const SessionAccess({
    required this.commands,
    required this.snapshotLimit,
    required this.dialogReply,
    this.imagePrompts = false,
  });

  factory SessionAccess._fromJson(Json json) => SessionAccess(
    commands: Set.unmodifiable(_array(json['supportedCommands'], _string)),
    snapshotLimit: _boolean(json['snapshotLimit']),
    dialogReply: _boolean(json['dialogReply']),
    imagePrompts: _supportsImages(json['promptContent']),
  );

  static bool _supportsImages(Object? raw) {
    if (raw is! Json || raw['image'] != true) return false;
    final types = raw['imageMimeTypes'];
    if (types is! List ||
        !types.contains('image/png') ||
        !types.contains('image/jpeg')) {
      return false;
    }
    const profile = {
      'contentBlocksVersion': 1,
      'maxImages': 4,
      'maxImageBytes': 1048576,
      'maxTotalImageBytes': 2097152,
      'maxImageBase64Bytes': 1398104,
      'maxPromptBytes': 3145728,
      'maxTextBytes': 32768,
      'maxContentBlocks': 64,
      'maxImageSide': 2000,
      'maxImagePixels': 4000000,
      'imageDecodeAllocBudgetBytes': 67108864,
    };
    final frameBytes = raw['frameBytes'];
    return frameBytes is int &&
        frameBytes >= 3145728 &&
        profile.entries.every((field) => raw[field.key] == field.value);
  }

  final Set<String> commands;
  final bool snapshotLimit;
  final bool dialogReply;
  final bool imagePrompts;
}

class SessionSnapshot {
  const SessionSnapshot({
    required this.sessionId,
    this.status,
    required this.lastSeq,
    required this.messages,
    required this.events,
    required this.pendingDialogs,
    required this.hasMore,
  });

  factory SessionSnapshot._fromJson(Json json) {
    final dialogs = _array(json['pendingDialogs'] ?? const [], _object);
    final legacyDialog = json['pendingDialog'];
    return SessionSnapshot(
      sessionId: _string(json['sessionId']),
      status: _optionalString(json['status']),
      lastSeq: _integer(json['lastSeq'] ?? 0),
      messages: _array(json['messages'] ?? const [], _object),
      events: _array(json['events'] ?? const [], (value) {
        final record = _object(value);
        final event = _object(record['event']);
        return WireEvent(
          type: _optionalString(event['type']) ?? 'unknown',
          seq: _integer(record['seq']),
          data: event,
        );
      }),
      pendingDialogs: dialogs.isEmpty && legacyDialog != null
          ? List.unmodifiable([_object(legacyDialog)])
          : dialogs,
      hasMore: _boolean(json['hasMore'] ?? json['has_more'] ?? false),
    );
  }

  final String sessionId;
  final String? status;
  final int lastSeq;
  final List<Json> messages;
  final List<WireEvent> events;
  final List<Json> pendingDialogs;
  final bool hasMore;
}

class WireEvent {
  const WireEvent({required this.type, this.seq, required this.data});

  factory WireEvent._fromJson(Json json) => WireEvent(
    type: _string(json['event_type']),
    seq: json['seq'] == null ? null : _integer(json['seq']),
    data: _object(json['data']),
  );

  final String type;
  final int? seq;
  final Json data;
}

Json _object(Object? value) {
  if (value is Json) return value;
  throw const FormatException();
}

String _string(Object? value) {
  if (value is String) return value;
  throw const FormatException();
}

String? _optionalString(Object? value) => value == null ? null : _string(value);

int _integer(Object? value) {
  if (value is int && value >= 0) return value;
  throw const FormatException();
}

bool _boolean(Object? value) {
  if (value is bool) return value;
  throw const FormatException();
}

List<T> _array<T>(Object? value, T Function(Object?) decode) {
  if (value is List<Object?>) return List.unmodifiable(value.map(decode));
  throw const FormatException();
}
