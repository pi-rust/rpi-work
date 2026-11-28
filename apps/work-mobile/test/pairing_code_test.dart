import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:work_mobile/src/node/work_node.dart';

const _ticket =
    'abababababababababababababababababababababababababababababababab';
// Version 2 envelope using the Rust QR algorithm and nonce bytes 00 through 0b.
const _sealed = <String, Object>{
  'v': 2,
  'type': 'rpi-work-qr',
  'n': 'AAECAwQFBgcICQoL',
  'd':
      'a1CAo2g13V_L5sQNB_gFzREIasWJXFBnWGO_f3ru9d70CvskhGVnaKVfj9TW1J774'
      'BcQmIPHmTrzVnqPWVjIYf7TYUtarVTMTAPxOeocXEbvl5slv3MrouUWfXSZO2PZRy9'
      'F0b0xThlo5Zy_Z1ziZsECWkik8LdBOnuTfOk6ak1oDNa_JYdwZR9h8DbojcY5QuVYH'
      'UpnLjbYMBIZxadOeGH4XENoYlb1LhJQH3XsB9ZK61HJxQ',
  'm': 'o-FxEiMUBIz4PPCFg_UVYw',
};

final _now = DateTime.fromMillisecondsSinceEpoch(1000000, isUtc: true);

String _plaintext({
  String address = 'http://10.1.2.3:13031/',
  int expiresAt = 1120,
}) => jsonEncode({
  'v': 1,
  'type': 'rpi-work-node-auth',
  'address': address,
  'ticket': _ticket,
  'expiresAt': expiresAt,
  'nodeId': 'node-a',
});

void main() {
  test('current envelope and legacy plaintext parse to the same pairing', () {
    for (final text in [jsonEncode(_sealed), _plaintext()]) {
      final code = PairingCode.parse(text, now: _now);
      expect(code.origin, Uri.parse('http://10.1.2.3:13031'));
      expect(code.nodeId, 'node-a');
      expect(code.expiresAt, _now.add(const Duration(seconds: 120)));
    }
  });

  test('rejects tampered envelope and redacts malformed credential input', () {
    for (final text in [
      jsonEncode({..._sealed, 'm': 'p-FxEiMUBIz4PPCFg_UVYw'}),
      jsonEncode({..._sealed, 'd': 'invalid='}),
      '${_plaintext()}$_ticket',
    ]) {
      expect(
        () => PairingCode.parse(text, now: _now),
        throwsA(
          isA<FormatException>().having(
            (error) => error.toString(),
            'redacted error',
            'FormatException: Invalid RPI Work authorization code.',
          ),
        ),
      );
    }
  });

  test('rejects expired codes and unsafe node addresses', () {
    expect(
      () => PairingCode.parse(_plaintext(expiresAt: 1000), now: _now),
      throwsFormatException,
    );
    expect(
      () => PairingCode.parse(_plaintext(expiresAt: 1181), now: _now),
      throwsFormatException,
    );
    for (final address in [
      'http://localhost:13031',
      'http://127.0.0.1:13031',
      'http://[::ffff:127.0.0.1]:13031',
      'https://node.test@other.test',
      'https://node.test/path',
      'https://node.test/?ticket=$_ticket',
    ]) {
      expect(
        () => PairingCode.parse(_plaintext(address: address), now: _now),
        throwsFormatException,
      );
    }
  });
}
