// Keep Material for focus, ink, and route behavior.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons_300.dart';

import 'work_theme.dart';

Future<T?> showWorkPanel<T>(
  BuildContext context, {
  required String title,
  required WidgetBuilder builder,
}) {
  return showModalBottomSheet<T>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    showDragHandle: false,
    backgroundColor: Colors.transparent,
    elevation: 0,
    constraints: const BoxConstraints(maxWidth: 560),
    builder: (context) => ConstrainedBox(
      constraints: BoxConstraints(
        maxHeight: MediaQuery.sizeOf(context).height * 0.78,
      ),
      child: WorkPanel(title: title, child: builder(context)),
    ),
  );
}

class WorkPanel extends StatelessWidget {
  const WorkPanel({super.key, required this.title, required this.child});

  final String title;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return Material(
      color: colors.surface,
      shape: RoundedRectangleBorder(
        borderRadius: const BorderRadius.vertical(top: Radius.circular(12)),
        side: BorderSide(color: colors.outlineVariant),
      ),
      clipBehavior: Clip.antiAlias,
      child: SafeArea(
        top: false,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            ColoredBox(
              color: colors.surfaceContainerLow,
              child: Padding(
                padding: const EdgeInsetsDirectional.only(start: 16, end: 6),
                child: Row(
                  children: [
                    Expanded(
                      child: Semantics(
                        header: true,
                        child: Text(title, style: theme.textTheme.titleMedium),
                      ),
                    ),
                    IconButton(
                      tooltip: '关闭',
                      onPressed: () => Navigator.pop(context),
                      icon: const Icon(LucideIcons300.x),
                    ),
                  ],
                ),
              ),
            ),
            const Divider(),
            Flexible(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(12),
                child: child,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class WorkPanelAction extends StatelessWidget {
  const WorkPanelAction({
    super.key,
    required this.icon,
    required this.label,
    this.description,
    this.selected = false,
    required this.onPressed,
  });

  final IconData icon;
  final String label;
  final String? description;
  final bool selected;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return SizedBox(
      width: double.infinity,
      child: MergeSemantics(
        child: TextButton(
          onPressed: onPressed,
          style: TextButton.styleFrom(
            minimumSize: const Size(44, 48),
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
            backgroundColor: selected ? colors.surfaceContainerHighest : null,
            disabledForegroundColor: colors.onSurfaceVariant,
          ),
          child: Semantics(
            selected: selected,
            child: Row(
              children: [
                Icon(icon, size: 17, color: colors.onSurfaceVariant),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(label, maxLines: 2, overflow: TextOverflow.ellipsis),
                      if (description case final detail? when detail.isNotEmpty)
                        Text(
                          detail,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.labelMedium?.copyWith(
                            color: colors.onSurfaceVariant,
                          ),
                        ),
                    ],
                  ),
                ),
                if (selected) const Icon(LucideIcons300.check, size: 16),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class WorkDialog extends StatelessWidget {
  const WorkDialog({
    super.key,
    required this.title,
    required this.content,
    required this.actions,
  });

  final String title;
  final Widget content;
  final List<Widget> actions;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return Dialog(
      backgroundColor: colors.surface,
      elevation: 0,
      insetPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      constraints: const BoxConstraints(maxWidth: 420),
      clipBehavior: Clip.antiAlias,
      shape: RoundedRectangleBorder(
        borderRadius: workPanelRadius,
        side: BorderSide(color: colors.outlineVariant),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ColoredBox(
            color: colors.surfaceContainerLow,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
              child: Semantics(
                header: true,
                child: Text(title, style: theme.textTheme.titleMedium),
              ),
            ),
          ),
          const Divider(),
          Flexible(
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(16),
              child: DefaultTextStyle.merge(
                style: theme.textTheme.bodyMedium,
                child: content,
              ),
            ),
          ),
          const Divider(),
          Padding(
            padding: const EdgeInsets.all(8),
            child: OverflowBar(
              alignment: MainAxisAlignment.end,
              overflowAlignment: OverflowBarAlignment.end,
              spacing: 8,
              overflowSpacing: 4,
              children: actions,
            ),
          ),
        ],
      ),
    );
  }
}

Widget workEditableMenu(BuildContext context, EditableTextState editor) =>
    workSelectionMenu(
      context,
      editor.contextMenuAnchors,
      editor.contextMenuButtonItems,
    );

Widget workReadOnlyMenu(BuildContext context, SelectableRegionState region) =>
    workSelectionMenu(
      context,
      region.contextMenuAnchors,
      region.contextMenuButtonItems,
    );

Widget workSelectionMenu(
  BuildContext context,
  TextSelectionToolbarAnchors anchors,
  List<ContextMenuButtonItem> buttons,
) {
  if (buttons.isEmpty) return const SizedBox.shrink();
  return TextSelectionToolbar(
    anchorAbove: anchors.primaryAnchor,
    anchorBelow: anchors.secondaryAnchor ?? anchors.primaryAnchor,
    toolbarBuilder: (context, child) {
      final colors = Theme.of(context).colorScheme;
      return Material(
        color: colors.surface,
        elevation: 3,
        shadowColor: colors.scrim.withValues(alpha: 0.24),
        shape: RoundedRectangleBorder(
          borderRadius: workControlRadius,
          side: BorderSide(color: colors.outline),
        ),
        clipBehavior: Clip.antiAlias,
        child: child,
      );
    },
    children: [
      for (final button in buttons)
        TextButton(
          onPressed: button.onPressed,
          child: Text(
            AdaptiveTextSelectionToolbar.getButtonLabel(context, button),
          ),
        ),
    ],
  );
}
