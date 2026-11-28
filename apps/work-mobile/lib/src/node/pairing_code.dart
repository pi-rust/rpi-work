part of 'work_node.dart';

final class PairingCode {
  PairingCode._(this.origin, this.nodeId, this.expiresAt, this._ticket);

  final Uri origin;
  final String nodeId;
  final DateTime expiresAt;
  final String _ticket;

  static PairingCode parse(String raw, {DateTime? now}) {
    final text = raw.trim();
    if (text.length > 8192 || utf8.encode(text).length > 8192) {
      throw const FormatException('Pairing code is too long.');
    }
    try {
      var payload = _object(jsonDecode(text));
      if (payload['type'] == 'rpi-work-qr') {
        payload = _object(jsonDecode(_openPairingEnvelope(payload)));
      }
      if (payload.length != 6 ||
          payload['v'] is! int ||
          payload['v'] != 1 ||
          payload['type'] != 'rpi-work-node-auth') {
        throw const FormatException();
      }
      final ticket = _string(payload['ticket']);
      final nodeId = _string(payload['nodeId']);
      final seconds = _integer(payload['expiresAt']);
      final nowSeconds = (now ?? DateTime.now()).millisecondsSinceEpoch ~/ 1000;
      if (!_secretPattern.hasMatch(ticket) || !_validRealtimeId(nodeId)) {
        throw const FormatException();
      }
      final origin = _normalizePairingOrigin(_string(payload['address']));
      if (seconds > nowSeconds + 180) throw const FormatException();
      if (seconds <= nowSeconds) throw const ExpiredPairingCode();
      return PairingCode._(
        origin,
        nodeId,
        DateTime.fromMillisecondsSinceEpoch(seconds * 1000, isUtc: true),
        ticket,
      );
    } on ExpiredPairingCode {
      rethrow;
    } on FormatException {
      // JSON and URI exceptions can include the original credential-bearing text.
      throw const FormatException('Invalid RPI Work authorization code.');
    }
  }
}

final class ExpiredPairingCode extends FormatException {
  const ExpiredPairingCode() : super('Pairing code has expired.');
}

final _secretPattern = RegExp(r'^[0-9a-f]{64}$');
final _controlPattern = RegExp(r'[\x00-\x1f\x7f-\x9f]');

bool _validRealtimeId(String id) =>
    id.trim().isNotEmpty &&
    utf8.encode(id).length <= 256 &&
    !_controlPattern.hasMatch(id);

Uri _normalizePairingOrigin(String address) {
  final text = address.trim();
  if (text.length > 2048 ||
      _controlPattern.hasMatch(text) ||
      RegExp(r'\s').hasMatch(text)) {
    throw const FormatException();
  }
  final match = RegExp(
    r'^(https?)://(\[[0-9a-f:.]+\]|[a-z0-9.-]+)(?::([0-9]+))?/?$',
    caseSensitive: false,
  ).firstMatch(text);
  if (match == null) throw const FormatException();
  final scheme = match[1]!.toLowerCase();
  final host = match[2]!.toLowerCase();
  final portText = match[3];
  final port = portText == null ? null : int.tryParse(portText);
  if (portText != null && (port == null || port < 1 || port > 65535)) {
    throw const FormatException();
  }
  if (host.startsWith('[')) {
    final bytes = Uri.parseIPv6Address(host.substring(1, host.length - 1));
    final mapped =
        bytes.take(10).every((byte) => byte == 0) &&
        bytes[10] == 255 &&
        bytes[11] == 255;
    final unspecifiedOrLoopback =
        bytes.take(15).every((byte) => byte == 0) && bytes.last <= 1;
    if (unspecifiedOrLoopback ||
        (mapped &&
            (bytes[12] == 127 || bytes.skip(12).every((byte) => byte == 0)))) {
      throw const FormatException();
    }
  } else {
    final name = host.replaceFirst(RegExp(r'\.+$'), '');
    final labels = name.split('.');
    if (labels.any(
          (label) =>
              label.isEmpty || label.startsWith('-') || label.endsWith('-'),
        ) ||
        name == 'localhost' ||
        name.endsWith('.localhost') ||
        name == '0.0.0.0' ||
        name.startsWith('127.')) {
      throw const FormatException();
    }
    final last = labels.last;
    if (RegExp(r'^[0-9]+$').hasMatch(last) || last.startsWith('0x')) {
      if (name != host ||
          labels.length != 4 ||
          labels.any((label) {
            final octet = int.tryParse(label);
            return octet == null ||
                octet < 0 ||
                octet > 255 ||
                '$octet' != label;
          })) {
        throw const FormatException();
      }
    }
  }
  return Uri(scheme: scheme, host: host, port: port);
}

// The Rust envelope uses a public client key. The node enforces authorization.
const _pairingKey = <int>[
  0x5b,
  0x2e,
  0xd7,
  0x41,
  0x9c,
  0x08,
  0xa3,
  0x6f,
  0x14,
  0xb8,
  0x7d,
  0x32,
  0xe9,
  0x05,
  0x46,
  0xcb,
  0x71,
  0x9a,
  0x2c,
  0x63,
  0xf0,
  0x4d,
  0x87,
  0x1e,
  0xaa,
  0x39,
  0x54,
  0x0d,
  0x92,
  0xc6,
  0x3b,
  0x78,
];

String _openPairingEnvelope(Json envelope) {
  if (envelope['v'] is! int || envelope['v'] != 2) {
    throw const FormatException();
  }
  final nonce = _decodePairingField(envelope['n']);
  final cipher = _decodePairingField(envelope['d']);
  final tag = _decodePairingField(envelope['m']);
  if (nonce.length != 12 ||
      cipher.isEmpty ||
      cipher.length > 4096 ||
      tag.length != 16) {
    throw const FormatException();
  }
  final expected = Hmac(
    sha256,
    _pairingKey,
  ).convert([...ascii.encode('rpi-work-qr-mac'), ...nonce, ...cipher]).bytes;
  var difference = 0;
  for (var i = 0; i < tag.length; i++) {
    difference |= tag[i] ^ expected[i];
  }
  if (difference != 0) throw const FormatException();

  final decoded = Uint8List(cipher.length);
  final prefix = [
    ..._pairingKey,
    ...ascii.encode('rpi-work-qr-keystream'),
    ...nonce,
  ];
  for (var offset = 0; offset < cipher.length; offset += 32) {
    final counter = ByteData(4)..setUint32(0, offset ~/ 32);
    final block = sha256.convert([
      ...prefix,
      ...counter.buffer.asUint8List(),
    ]).bytes;
    for (var j = 0; j < 32 && offset + j < cipher.length; j++) {
      decoded[offset + j] = cipher[offset + j] ^ block[j];
    }
  }
  return utf8.decode(decoded);
}

List<int> _decodePairingField(Object? value) {
  final field = _string(value);
  if (!RegExp(r'^[A-Za-z0-9_-]+$').hasMatch(field)) {
    throw const FormatException();
  }
  final bytes = base64Url.decode(base64Url.normalize(field));
  if (base64UrlEncode(bytes).replaceAll('=', '') != field) {
    throw const FormatException();
  }
  return bytes;
}
