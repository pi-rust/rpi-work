import 'dart:convert';
import 'dart:typed_data' show BytesBuilder;
import 'dart:ui' as ui;

import 'package:file_selector/file_selector.dart';
import 'package:flutter/services.dart';

const _channel = MethodChannel('rpi_work/clipboard_attachments');
const _invalidFile = '仅支持 PNG、JPEG 图片及 UTF-8 文本文件。';
const _maxAttachments = 8;
const _maxTextBytes = 32768;
const _maxImageBytes = 1048576;

class DraftAttachment {
  DraftAttachment._(this.name, this.bytes, {this.mime, this.text});

  final String name;
  final Uint8List bytes;
  final String? mime;
  final String? text;

  static Future<List<DraftAttachment>> readClipboard() async {
    final List<Map<Object?, Object?>>? items;
    try {
      items = await _channel.invokeListMethod<Map<Object?, Object?>>('read');
    } on MissingPluginException {
      return const [];
    }
    if (items == null || items.isEmpty) return const [];
    _checkAttachmentCount(items.length);
    return [
      for (final item in items)
        await DraftAttachment.fromBytes(
          item['name'] as String,
          item['bytes'] as Uint8List,
        ),
    ];
  }

  static Future<List<DraftAttachment>> pickFiles() async {
    final files = await openFiles();
    _checkAttachmentCount(files.length);
    return [for (final file in files) await _readFile(file)];
  }

  static Future<DraftAttachment> _readFile(XFile file) async {
    final buffer = BytesBuilder(copy: false);
    await for (final chunk in file.openRead()) {
      if (buffer.length + chunk.length > _maxImageBytes) {
        throw const FormatException('单个附件超过 1 MiB。');
      }
      buffer.add(chunk);
    }
    return fromBytes(file.name, buffer.takeBytes());
  }

  static Future<DraftAttachment> fromBytes(
    String rawName,
    Uint8List bytes,
  ) async {
    final name = rawName
        .split(RegExp(r'[/\\]'))
        .last
        .replaceAll(RegExp(r'[\x00-\x1f\x7f\[\]]'), '_')
        .trim();
    final label = name.isEmpty
        ? '附件'
        : String.fromCharCodes(name.runes.take(160));
    final mime = _imageMime(bytes);
    if (mime != null) {
      if (bytes.length > _maxImageBytes) {
        throw FormatException('$label 超过单张图片 1 MiB 上限。');
      }
      final buffer = await ui.ImmutableBuffer.fromUint8List(bytes);
      try {
        final image = await ui.ImageDescriptor.encoded(buffer);
        try {
          if (image.width > 2000 || image.height > 2000) {
            throw FormatException('$label 的宽高须在 2000 像素以内。');
          }
        } finally {
          image.dispose();
        }
      } on FormatException {
        rethrow;
      } on Exception {
        throw FormatException('$label 无法读取为图片。');
      } finally {
        buffer.dispose();
      }
      return DraftAttachment._(label, bytes, mime: mime);
    }

    final String content;
    try {
      content = utf8.decode(bytes);
    } on FormatException {
      throw const FormatException(_invalidFile);
    }
    if (RegExp(r'[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]').hasMatch(content) ||
        content.startsWith('%PDF-') ||
        RegExp(
          r'\.(pdf|docx?|xlsx?|pptx?|zip|rar|7z|gif|webp|heic|heif|bmp)$',
          caseSensitive: false,
        ).hasMatch(label)) {
      throw const FormatException(_invalidFile);
    }
    if (bytes.length > _maxTextBytes) {
      throw FormatException('$label 超过文本附件 32 KiB 上限。');
    }
    return DraftAttachment._(label, bytes, text: content);
  }

  static String? _imageMime(Uint8List bytes) {
    const png = [137, 80, 78, 71, 13, 10, 26, 10];
    if (bytes.length >= png.length &&
        Iterable<int>.generate(png.length).every((i) => bytes[i] == png[i])) {
      return 'image/png';
    }
    if (bytes.length >= 3 &&
        bytes[0] == 255 &&
        bytes[1] == 216 &&
        bytes[2] == 255) {
      return 'image/jpeg';
    }
    return null;
  }

  static void _checkAttachmentCount(int count) {
    if (count > _maxAttachments) {
      throw const FormatException('最多可添加 8 个附件。');
    }
  }

  static void validateCount(List<DraftAttachment> files) {
    _checkAttachmentCount(files.length);
    final images = files.where((file) => file.mime != null);
    if (images.length > 4) {
      throw const FormatException('最多可添加 4 张图片。');
    }
    if (images.fold(0, (sum, image) => sum + image.bytes.length) > 2097152) {
      throw const FormatException('图片合计超过 2 MiB。');
    }
  }

  static String promptText(String text, List<DraftAttachment> files) {
    final result = [
      if (text.isNotEmpty) text,
      for (final file in files)
        if (file.text != null) '[附件：${file.name}]\n${file.text}',
    ].join('\n');
    if (utf8.encode(result).length > _maxTextBytes) {
      throw const FormatException('消息和文本附件合计超过 32 KiB。');
    }
    return result;
  }

  Map<String, Object> imageBlock() => {
    'type': 'image',
    'mimeType': mime!,
    'data': base64Encode(bytes),
  };
}
