// The work theme also applies to framework controls and text selection.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:work_mobile/main.dart';
import 'package:work_mobile/src/work_controls.dart';
import 'package:work_mobile/src/work_theme.dart';

void main() {
  test('091 light Codex palette matches GPUI chrome and contrast', () {
    final theme = workTheme(Brightness.light);
    final colors = theme.colorScheme;
    expect(theme.scaffoldBackgroundColor, const Color(0xffffffff));
    expect(theme.appBarTheme.backgroundColor, const Color(0xfff0f0f0));
    expect(colors.surfaceContainerLow, const Color(0xfffafafa));
    expect(colors.surfaceContainer, const Color(0xfff2f2f2));
    expect(colors.outlineVariant, const Color(0xffe3e3e3));
    expect(colors.onSurface, const Color(0xff1f1f1f));
    expect(colors.onSurfaceVariant, const Color(0xff666666));
    expect(
      _contrast(colors.onSurfaceVariant, colors.surfaceContainerLow),
      greaterThanOrEqualTo(4.5),
    );
  });

  test('092 dark Codex palette matches GPUI chrome and contrast', () {
    final theme = workTheme(Brightness.dark);
    final colors = theme.colorScheme;
    expect(theme.scaffoldBackgroundColor, const Color(0xff212121));
    expect(theme.appBarTheme.backgroundColor, const Color(0xff191919));
    expect(colors.surfaceContainerLow, const Color(0xff1b1b1b));
    expect(colors.surfaceContainer, const Color(0xff2c2c2c));
    expect(colors.outlineVariant, const Color(0xff373737));
    expect(colors.onSurface, const Color(0xffececec));
    expect(colors.onSurfaceVariant, const Color(0xffb0b0b0));
    expect(
      _contrast(colors.onSurfaceVariant, colors.surfaceContainer),
      greaterThanOrEqualTo(4.5),
    );
  });

  testWidgets(
    '093 keyboard focus reaches outlined controls with visible contrast',
    (tester) async {
      final focus = FocusNode();
      await tester.pumpWidget(
        WorkMobileApp(
          home: Scaffold(
            body: Center(
              child: OutlinedButton(
                focusNode: focus,
                onPressed: () {},
                child: const Text('取消'),
              ),
            ),
          ),
        ),
      );
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pump();
      expect(focus.hasPrimaryFocus, isTrue);
      final theme = Theme.of(tester.element(find.byType(OutlinedButton)));
      final style = theme.outlinedButtonTheme.style!;
      expect(
        style.side!.resolve({WidgetState.focused})!.color,
        theme.colorScheme.onSurfaceVariant,
      );
      final overlay = style.overlayColor!.resolve({WidgetState.focused});
      expect(overlay?.a, greaterThan(0.1));
      expect(
        style.overlayColor!.resolve({
          WidgetState.focused,
          WidgetState.disabled,
        }),
        isNull,
      );
      await tester.pumpWidget(const SizedBox.shrink());
      focus.dispose();
    },
  );

  testWidgets('094 dark filled and icon focus use opposite surface tones', (
    tester,
  ) async {
    await _dark(tester);
    final context = tester.element(find.text('打开相机'));
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final filled = theme.filledButtonTheme.style!.overlayColor!;
    final icons = theme.iconButtonTheme.style!.overlayColor!;
    expect(
      filled.resolve({WidgetState.focused})!.withValues(alpha: 1),
      colors.surface,
    );
    expect(
      icons.resolve({WidgetState.focused})!.withValues(alpha: 1),
      colors.onSurface,
    );
    expect(filled.resolve({WidgetState.disabled}), isNull);
  });

  testWidgets(
    '095 light editor cursor and selection handle follow foreground',
    (tester) async {
      await tester.pumpWidget(
        const WorkMobileApp(home: Scaffold(body: TextField())),
      );
      await tester.enterText(find.byType(TextField), '保留这段文字');
      await tester.pump();
      final editor = tester.widget<EditableText>(find.byType(EditableText));
      final context = tester.element(find.byType(EditableText));
      final foreground = Theme.of(context).colorScheme.onSurface;
      expect(editor.cursorColor, foreground);
      expect(TextSelectionTheme.of(context).selectionHandleColor, foreground);
    },
  );

  testWidgets('096 selected text stays legible in the dark prompt surface', (
    tester,
  ) async {
    await _dark(
      tester,
      home: const Scaffold(body: SelectableText('需要复制的项目相对路径')),
    );
    final context = tester.element(find.byType(EditableText));
    final colors = Theme.of(context).colorScheme;
    final selection = TextSelectionTheme.of(context).selectionColor!;
    final active = Color.alphaBlend(selection, colors.surfaceContainer);
    expect(active, isNot(colors.surfaceContainer));
    expect(_contrast(colors.onSurface, active), greaterThanOrEqualTo(4.5));
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).selectionColor,
      selection,
    );
  });

  testWidgets('097 status-bar icons follow the live system mode', (
    tester,
  ) async {
    await _dark(tester);
    SystemUiOverlayStyle overlay() =>
        Theme.of(tester.element(find.byType(AppBar)))
            .appBarTheme
            .systemOverlayStyle!;
    expect(overlay().statusBarIconBrightness, Brightness.light);
    tester.platformDispatcher.platformBrightnessTestValue = Brightness.light;
    await tester.pumpAndSettle();
    expect(overlay().statusBarIconBrightness, Brightness.dark);
    expect(overlay().statusBarColor, Colors.transparent);
    expect(overlay().systemNavigationBarContrastEnforced, isFalse);
  });

  testWidgets(
    '098 enlarged dark dialog uses the palette and keeps actions reachable',
    (tester) async {
      _viewport(tester, const Size(320, 568));
      tester.platformDispatcher.textScaleFactorTestValue = 2;
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
      await _dark(
        tester,
        home: Scaffold(
          body: Builder(
            builder: (context) => Center(
              child: TextButton(
                onPressed: () => showDialog<void>(
                  context: context,
                  builder: (context) => WorkDialog(
                    title: '重新核对',
                    content: Text(List.filled(10, '请先核对节点上的会话记录。').join('\n')),
                    actions: [
                      TextButton(
                        onPressed: () => Navigator.pop(context),
                        child: const Text('返回'),
                      ),
                    ],
                  ),
                ),
                child: const Text('核对'),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('核对'));
      await tester.pumpAndSettle();
      final dialog = tester.widget<Dialog>(
        find.descendant(
          of: find.byType(WorkDialog),
          matching: find.byType(Dialog),
        ),
      );
      expect(dialog.backgroundColor, const Color(0xff212121));
      expect(find.text('返回').hitTestable(), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('099 landscape pairing avoids the bottom system gesture inset', (
    tester,
  ) async {
    _viewport(tester, const Size(568, 320));
    tester.view.padding = const FakeViewPadding(bottom: 38);
    tester.view.viewPadding = const FakeViewPadding(bottom: 38);
    addTearDown(tester.view.resetPadding);
    addTearDown(tester.view.resetViewPadding);
    await tester.pumpWidget(const WorkMobileApp());
    await tester.pumpAndSettle();
    for (final button in [
      find.byType(OutlinedButton),
      find.byType(FilledButton),
    ]) {
      await tester.ensureVisible(button);
      await tester.pumpAndSettle();
      expect(tester.getRect(button).bottom, lessThanOrEqualTo(320 - 38));
      expect(button.hitTestable(), findsOneWidget);
    }
  });

  testWidgets('100 wide pairing keeps the desktop-style readable panel width', (
    tester,
  ) async {
    _viewport(tester, const Size(1024, 768));
    await tester.pumpWidget(const WorkMobileApp());
    await tester.pumpAndSettle();
    final viewport = tester.getRect(find.byType(SingleChildScrollView));
    expect(viewport.center.dx, closeTo(512, 1));
    expect(viewport.width, lessThanOrEqualTo(460));
    expect(find.text('打开相机').hitTestable(), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}

Future<void> _dark(WidgetTester tester, {Widget? home}) async {
  tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
  addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);
  await tester.pumpWidget(
    home == null ? const WorkMobileApp() : WorkMobileApp(home: home),
  );
  await tester.pumpAndSettle();
}

void _viewport(WidgetTester tester, Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.resetDevicePixelRatio);
  addTearDown(tester.view.resetPhysicalSize);
}

double _contrast(Color a, Color b) {
  final luma = [a.computeLuminance(), b.computeLuminance()]..sort();
  return (luma.last + 0.05) / (luma.first + 0.05);
}
