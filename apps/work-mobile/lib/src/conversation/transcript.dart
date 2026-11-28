import 'dart:collection';

// Keep the framework Material theme shared with the Markdown renderer.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:lucide_icons_flutter/lucide_icons_300.dart';
import 'package:markdown/markdown.dart' as md;
import 'package:url_launcher/url_launcher.dart';

import '../node/work_node.dart';
import '../work_controls.dart';
import '../work_theme.dart';

class TranscriptResyncRequired implements Exception {
  const TranscriptResyncRequired(this.cursor, {this.receivedSeq});

  final int cursor;
  final int? receivedSeq;

  @override
  String toString() => 'The conversation needs a fresh snapshot.';
}

class Transcript {
  Transcript.fromSnapshot(SessionSnapshot snapshot)
    : _sessionId = snapshot.sessionId,
      hasMore = snapshot.hasMore {
    _running = switch (snapshot.status) {
      'running' ||
      'awaiting_input' ||
      'waiting_for_input' ||
      'cancelling' ||
      'stopping' => true,
      _ => false,
    };
    var replayCursor = -1;
    for (final event in snapshot.events) {
      final seq = event.seq;
      if (seq != null && (seq <= replayCursor || seq > snapshot.lastSeq)) {
        continue;
      }
      _project(event, history: true);
      if (seq != null) replayCursor = seq;
    }
    _mergeHistory(snapshot.messages);
    _cursor = snapshot.lastSeq;
    _dialogs.clear();
    for (final data in snapshot.pendingDialogs) {
      _addDialog(data);
    }
  }

  final String _sessionId;
  final bool hasMore;
  final _messages = <ChatMessage>[];
  final _messagePositions = <(String, String), int>{};
  final _toolPositions = <String, int>{};
  final _dialogs = <String, PendingDialog>{};
  late final List<ChatMessage> messages = UnmodifiableListView(_messages);
  List<PendingDialog> get dialogs => List.unmodifiable(_dialogs.values);

  int _cursor = 0;
  int get cursor => _cursor;
  bool _running = false;
  bool get running => _running;
  int _settlementCount = 0;
  int get settlementCount => _settlementCount;
  int? _activeAssistant;

  void _mergeHistory(List<Json> history) {
    if (history.isEmpty) return;
    final replay = _messages.toList();
    final active = _activeAssistant == null ? null : replay[_activeAssistant!];
    final replayTools = _toolPositions.map(
      (id, index) => MapEntry(id, replay[index]),
    );
    _messages.clear();
    _messagePositions.clear();
    _toolPositions.clear();
    _activeAssistant = null;

    final anonymous = <(String, String, Object?), int>{};
    for (final data in history) {
      final role = _role(data);
      if (role == 'tool' || role == 'toolResult' || role == 'tool_result') {
        final tool = replayTools[data['toolCallId']];
        _applyTool(data, 'tool_execution_end', previous: tool);
      } else if (role == 'user' || role == 'assistant') {
        _applyMessage({'message': data}, 'message_end');
        final message = _messages.last;
        if (message.id == null && message._timestamp is num) {
          anonymous.update(
            message._historyKey,
            (count) => count + 1,
            ifAbsent: () => 1,
          );
        }
      }
    }
    for (final message in replay) {
      if (message.isTool && _toolPositions.containsKey(message.id)) continue;
      final id = message.id;
      final position = id == null
          ? null
          : _messagePositions[(message.role, id)];
      if (position != null) {
        if (identical(message, active)) _activeAssistant = position;
        continue;
      }
      final key = message._historyKey;
      if (id == null && message._complete && (anonymous[key] ?? 0) > 0) {
        anonymous.update(key, (count) => count - 1);
        continue;
      }
      // An older anonymous row can precede the active one in this replay.
      final index = _messages.length;
      _messages.add(message);
      if (id != null) {
        if (message.isTool) {
          _toolPositions[id] = index;
        } else {
          _messagePositions[(message.role, id)] = index;
        }
      }
      if (identical(message, active)) _activeAssistant = index;
    }
  }

  void apply(WireEvent event) {
    final seq = event.seq;
    if (event.type == 'resync_required') {
      throw TranscriptResyncRequired(cursor, receivedSeq: seq);
    }
    if (seq != null) {
      if (seq <= cursor) return;
      if (seq != cursor + 1) {
        throw TranscriptResyncRequired(cursor, receivedSeq: seq);
      }
    }
    _project(event);
    if (seq != null) _cursor = seq;
  }

  void _project(WireEvent event, {bool history = false}) {
    final data = event.data;
    switch (event.type) {
      case 'operation_accepted':
      case 'agent_start':
        if (!_running) _activeAssistant = null;
        _running = true;
      case 'agent_end':
        _running = true;
      case 'agent_settled':
        _running = false;
        _activeAssistant = null;
        if (!history) _settlementCount++;
      case 'message_start':
      case 'message_update':
      case 'message_end':
        if (event.type == 'message_update') _running = true;
        _applyMessage(data, event.type, seq: event.seq);
      case 'tool_execution_start':
      case 'tool_execution_update':
      case 'tool_execution_end':
        _running = true;
        _applyTool(data, event.type);
      case 'ui_dialog_request':
        _running = true;
        _addDialog(data);
      case 'ui_dialog_closed':
        _dialogs.remove(data['requestId']);
    }
  }

  void _applyMessage(Json data, String type, {int? seq}) {
    final raw = data['message'];
    final message = raw is Json ? raw : const <String, Object?>{};
    final role = _role(message) ?? 'assistant';
    if (role != 'user' && role != 'assistant') return;

    final update = data['assistantMessageEvent'];
    final delta = update is Json ? update : const <String, Object?>{};
    final kind = data['eventType'] ?? delta['type'];
    final content = message['content'] ?? message['blocks'] ?? message['text'];
    final isDelta = type == 'message_update' && kind == 'text_delta';
    if (content == null && !isDelta && message.isEmpty) return;
    if (type == 'message_update' && content == null && !isDelta) return;

    final id = _messageId(message) ?? _messageId(data);
    final active = role == 'assistant' ? _activeAssistant : null;
    var position = id == null ? null : _messagePositions[(role, id)];
    if (position == null && active != null && type != 'message_start') {
      final activeId = _messages[active].id;
      final sessionId = _string(data['sessionId']);
      // The ACP journal assigns the final assistant entry an ID at settlement.
      final canonicalEnd =
          type == 'message_end' &&
          sessionId != null &&
          seq != null &&
          id == '$sessionId:assistant:$seq';
      if (id == null || activeId == null || canonicalEnd) position = active;
    }
    final previous = position == null ? null : _messages[position];
    final blocks = SplayTreeMap<int, String>();
    var hasImage = role == 'user' && _hasImageContent(content);
    if (isDelta) {
      hasImage |= previous?._hasImage ?? false;
      blocks.addAll(previous?._blocks ?? const {});
      final index = data['contentIndex'] ?? delta['contentIndex'];
      final slot = index is int && index >= 0 ? index : 0;
      final text = _publicText(data['delta'] ?? delta['delta']);
      blocks[slot] = (blocks[slot] ?? '') + text;
    } else {
      blocks.addAll(_textBlocks(content));
      final placeholder = content == '' || (content is List && content.isEmpty);
      if (content == null || (placeholder && type != 'message_end')) {
        blocks.addAll(previous?._blocks ?? const {});
        hasImage = previous?._hasImage ?? false;
      }
    }

    final projected = ChatMessage._(
      id: id ?? previous?.id,
      role: role,
      blocks: blocks,
      hasImage: hasImage,
      complete: type == 'message_end',
      timestamp: message['timestamp'] ?? previous?._timestamp,
    );
    if (position == null) {
      position = _messages.length;
      _messages.add(projected);
    } else {
      _messages[position] = projected;
    }
    final projectedId = projected.id;
    if (projectedId != null) _messagePositions[(role, projectedId)] = position;
    if (role == 'assistant') {
      if (type != 'message_end') {
        _activeAssistant = position;
      } else if (active == position) {
        _activeAssistant = null;
      }
    }
  }

  void _applyTool(Json data, String type, {ChatMessage? previous}) {
    final id = _string(data['toolCallId']);
    if (id == null) return;
    final position = _toolPositions[id];
    if (position != null) previous = _messages[position];
    final result = data['result'] ?? data['partialResult'] ?? data['content'];
    final output = _publicText(result);
    final error = _string(data['error']);
    final failed = error != null || data['isError'] == true;
    final projected = ChatMessage._(
      id: id,
      role: 'tool',
      sessionId: _sessionId,
      blocks: result == null && error == null
          ? previous?._blocks ?? const {}
          : {0: output.trim().isEmpty ? error ?? '' : output},
      toolName:
          _string(data['toolName']) ??
          _string(data['name']) ??
          previous?.toolName ??
          'Tool',
      toolStatus: switch (type) {
        'tool_execution_end' when failed => 'Failed',
        'tool_execution_end' => 'Completed',
        _ => 'Running',
      },
    );
    if (position == null) {
      _toolPositions[id] = _messages.length;
      _messages.add(projected);
    } else {
      _messages[position] = projected;
    }
  }

  void _addDialog(Json data) {
    final dialog = PendingDialog._fromData(data);
    if (dialog != null) _dialogs[dialog.id] = dialog;
  }
}

class ChatMessage {
  ChatMessage._({
    required this.id,
    required this.role,
    required Map<int, String> blocks,
    this.toolName,
    this.toolStatus,
    this._sessionId,
    this._hasImage = false,
    this._complete = false,
    this._timestamp,
  }) : _blocks = Map.unmodifiable(blocks);

  final String? id;
  final String role;
  final Map<int, String> _blocks;
  final String? _sessionId;
  final bool _hasImage;
  final bool _complete;
  final Object? _timestamp;
  (String, String, Object?) get _historyKey => (role, text, _timestamp);
  final String? toolName;
  final String? toolStatus;
  bool get isTool => role == 'tool';

  String get text {
    // Desktop text attachments and project references carry their own spacing.
    final body = _blocks.values
        .where((text) => text.isNotEmpty)
        .join(role == 'assistant' ? '\n\n' : '');
    if (!_hasImage) return body;
    final caption = body.trimRight();
    return caption.isEmpty ? '[图片附件]' : '$caption\n\n[图片附件]';
  }
}

class PendingDialog {
  const PendingDialog._({
    required this.id,
    required this.message,
    required this.title,
    required this.method,
    required this.choices,
  });

  static PendingDialog? _fromData(Json data) {
    final id = _string(data['requestId']);
    final raw = data['ui'] ?? data;
    if (id == null || raw is! Json) return null;
    final permission = raw['acpPermission'];
    final options = permission is Json ? permission['options'] : raw['options'];
    final kind = raw['method'] ?? raw['kind'];
    final unsupported =
        raw['supported'] == false ||
        raw['unsupported'] == true ||
        raw['unsupported'] is String ||
        raw['allowMultiple'] == true ||
        raw['allow_multiple'] == true ||
        raw['multiple'] == true ||
        (raw['questions'] is List && (raw['questions'] as List).isNotEmpty);
    final method = switch (kind) {
      _ when unsupported => 'unsupported',
      'select' || 'selector' => 'select',
      'confirm' || 'confirmation' => 'confirm',
      'input' => 'input',
      'editor' => 'editor',
      null when options is List || permission is Json => 'select',
      _ => 'unsupported',
    };
    final selectorAnswer = permission is Json || kind == 'selector';
    return PendingDialog._(
      id: id,
      message:
          _string(raw['question']) ??
          _string(raw['message']) ??
          _string(raw['prompt']) ??
          '',
      title: _string(raw['title']) ?? _string(raw['header']),
      method: method,
      choices: List.unmodifiable([
        if (method == 'select' && options is List)
          for (final option in options)
            if (_dialogChoice(option, selectorAnswer)
                case final DialogChoice choice)
              choice,
      ]),
    );
  }

  final String id;
  final String message;
  final String? title;
  final String method;
  final List<DialogChoice> choices;
}

class DialogChoice {
  const DialogChoice({required this.label, required this.value});

  final String label;
  final Object? value;
}

DialogChoice? _dialogChoice(Object? option, bool selectorAnswer) {
  final fields = option is Json ? option : const <String, Object?>{};
  final label =
      _string(option) ??
      _string(fields['label']) ??
      _string(fields['title']) ??
      _string(fields['name']) ??
      _string(fields['value']);
  if (label == null) return null;
  return DialogChoice(
    label: label,
    value: selectorAnswer
        ? {
            'kind': 'selector',
            'values': [_string(fields['name']) ?? label],
          }
        : fields.containsKey('value')
        ? fields['value']
        : label,
  );
}

class ChatMessageTile extends StatelessWidget {
  const ChatMessageTile({super.key, required this.message});

  final ChatMessage message;

  @override
  Widget build(BuildContext context) {
    final text = message.text;
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    if (message.isTool) {
      final status = switch (message.toolStatus) {
        'Failed' => '失败',
        'Running' => '运行中',
        'Completed' => '已完成',
        _ => message.toolStatus ?? '',
      };
      final hasOutput = text.trim().isNotEmpty;
      return ExpansionTile(
        key: ValueKey((message._sessionId, message.id)),
        minTileHeight: 48,
        tilePadding: const EdgeInsets.symmetric(horizontal: 6),
        childrenPadding: const EdgeInsets.fromLTRB(12, 0, 12, 10),
        expandedCrossAxisAlignment: CrossAxisAlignment.start,
        shape: const RoundedRectangleBorder(borderRadius: workControlRadius),
        collapsedShape: const RoundedRectangleBorder(
          borderRadius: workControlRadius,
        ),
        iconColor: colors.onSurfaceVariant,
        collapsedIconColor: colors.onSurfaceVariant,
        backgroundColor: colors.surfaceContainerLow,
        leading: Icon(
          LucideIcons300.terminal,
          size: 15,
          color: colors.onSurfaceVariant,
        ),
        title: Text(
          message.toolName ?? '工具',
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
          style: theme.textTheme.bodySmall,
        ),
        subtitle: Text(
          status,
          style: theme.textTheme.labelMedium?.copyWith(
            color: message.toolStatus == 'Failed'
                ? colors.error
                : colors.onSurfaceVariant,
          ),
        ),
        children: [
          const Divider(),
          const SizedBox(height: 8),
          Text(
            hasOutput
                ? '工具结果'
                : message.toolStatus == 'Running'
                ? '暂无输出'
                : '无输出',
            style: theme.textTheme.labelMedium?.copyWith(
              color: colors.onSurfaceVariant,
            ),
          ),
          if (hasOutput) ...[
            const SizedBox(height: 4),
            SelectableText(
              text,
              style: theme.textTheme.bodySmall,
              contextMenuBuilder: workEditableMenu,
            ),
          ],
        ],
      );
    }
    if (text.trim().isEmpty) return const SizedBox.shrink();
    final reading = theme.textTheme.bodyLarge!;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: message.role == 'user'
          ? Align(
              alignment: AlignmentDirectional.centerEnd,
              child: FractionallySizedBox(
                widthFactor: 0.88,
                alignment: AlignmentDirectional.centerEnd,
                child: Align(
                  alignment: AlignmentDirectional.centerEnd,
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      color: colors.surfaceContainer,
                      borderRadius: workPromptRadius,
                    ),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 16,
                        vertical: 11,
                      ),
                      child: SelectableText(
                        text,
                        style: reading,
                        contextMenuBuilder: workEditableMenu,
                      ),
                    ),
                  ),
                ),
              ),
            )
          // Keep text selectable without competing with code and table swipes.
          : SelectionArea(
              contextMenuBuilder: workReadOnlyMenu,
              child: MarkdownBody(
                data: text,
                builders: {'img': _MarkdownImagePlaceholder()},
                onTapLink: (_, href, _) => _openMessageLink(context, href),
                styleSheet: MarkdownStyleSheet.fromTheme(theme).copyWith(
                  p: reading,
                  a: reading.copyWith(decoration: TextDecoration.underline),
                  h1: theme.textTheme.headlineSmall,
                  h2: theme.textTheme.titleLarge,
                  h3: theme.textTheme.titleMedium,
                  h4: theme.textTheme.titleSmall,
                  h5: theme.textTheme.titleSmall,
                  h6: theme.textTheme.titleSmall,
                  listBullet: reading,
                  listIndent: 20,
                  blockSpacing: 10,
                  code: TextStyle(
                    fontFamily: 'monospace',
                    fontSize: 13,
                    backgroundColor: colors.surfaceContainer,
                  ),
                  codeblockDecoration: BoxDecoration(
                    color: colors.surfaceContainer,
                    borderRadius: workControlRadius,
                    border: Border.all(color: colors.outlineVariant),
                  ),
                  blockquote: reading.copyWith(color: colors.onSurfaceVariant),
                  blockquoteDecoration: BoxDecoration(
                    border: BorderDirectional(
                      start: BorderSide(color: colors.outline, width: 2),
                    ),
                  ),
                  tableColumnWidth: const FixedColumnWidth(144),
                  tableBorder: TableBorder.all(color: colors.outlineVariant),
                ),
              ),
            ),
    );
  }
}

class _MarkdownImagePlaceholder extends MarkdownElementBuilder {
  @override
  Widget visitElementAfterWithContext(
    BuildContext context,
    md.Element element,
    TextStyle? preferredStyle,
    TextStyle? parentStyle,
  ) {
    final theme = Theme.of(context);
    return Text.rich(
      TextSpan(
        text: '图片（未加载）：${element.attributes['alt'] ?? ''}',
        style: (parentStyle ?? theme.textTheme.bodyLarge!).copyWith(
          color: theme.colorScheme.onSurfaceVariant,
        ),
      ),
    );
  }
}

final _webAuthority = RegExp(r'^https?://([^/?#]+)', caseSensitive: false);
final _invalidLinkCharacters = RegExp(r'[\\\s\x00-\x1f\x7f-\x9f]');

Future<void> _openMessageLink(BuildContext context, String? href) async {
  if (href == null || _invalidLinkCharacters.hasMatch(href)) return;
  final authority = _webAuthority.firstMatch(href)?.group(1);
  if (authority == null || authority.contains('@')) return;
  final uri = Uri.tryParse(href);
  if (uri == null || uri.host.isEmpty || uri.userInfo.isNotEmpty) return;
  try {
    if (await launchUrl(uri, mode: LaunchMode.externalApplication)) return;
  } on Exception {
    // Report the same failure when no browser can open the link.
  }
  if (context.mounted) {
    ScaffoldMessenger.maybeOf(context)
        ?.showSnackBar(const SnackBar(content: Text('无法打开链接。')));
  }
}

String? _string(Object? value) =>
    value is String && value.isNotEmpty ? value : null;

String? _role(Json data) => _string(data['role']) ?? _string(data['kind']);

String? _messageId(Json data) =>
    _string(data['entryId']) ??
    _string(data['messageId']) ??
    _string(data['id']);

Map<int, String> _textBlocks(Object? content) {
  if (content is! List) return {0: _publicText(content)};
  return {
    for (final (index, part) in content.indexed)
      if (_publicText(part) case final text when text.isNotEmpty) index: text,
  };
}

bool _hasImageContent(Object? value) {
  final content = value is Json && !value.containsKey('type')
      ? value['content'] ?? value
      : value;
  bool isImage(Object? block) => block is Json && block['type'] == 'image';
  return content is List ? content.any(isImage) : isImage(content);
}

String _publicText(Object? part) {
  if (part is String) return part;
  if (part is List) return part.map(_publicText).join();
  if (part is Json &&
      !part.containsKey('thinking') &&
      !part.containsKey('reasoning') &&
      part['redacted'] != true) {
    switch (part['type']) {
      case null when !part.containsKey('type'):
      case 'text':
      case 'input_text':
      case 'output_text':
        return _publicText(
          part.containsKey('content') ? part['content'] : part['text'],
        );
    }
  }
  return '';
}
