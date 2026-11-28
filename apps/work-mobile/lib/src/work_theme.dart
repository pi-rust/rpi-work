// Keep the framework Material theme shared with the Markdown renderer.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

const workControlRadius = BorderRadius.all(Radius.circular(8));
const workPanelRadius = BorderRadius.all(Radius.circular(12));
const workPromptRadius = BorderRadius.all(Radius.circular(16));
const workComposerRadius = BorderRadius.all(Radius.circular(22));

ThemeData workTheme(Brightness brightness) {
  final dark = brightness == Brightness.dark;
  // The default GPUI Codex palette combines work-viewmodel tokens with
  // the chrome overrides in apps/work-gpui/src/theme.rs.
  final background = Color(dark ? 0xff212121 : 0xffffffff);
  final sidebar = Color(dark ? 0xff1b1b1b : 0xfffafafa);
  final titlebar = Color(dark ? 0xff191919 : 0xfff0f0f0);
  final subtle = Color(dark ? 0xff2c2c2c : 0xfff2f2f2);
  final selected = Color(dark ? 0xff383838 : 0xffeaeaea);
  final text = Color(dark ? 0xffececec : 0xff1f1f1f);
  final muted = Color(dark ? 0xffb0b0b0 : 0xff666666);
  final line = Color(dark ? 0xff373737 : 0xffe3e3e3);
  final strongLine = Color(dark ? 0xff636363 : 0xffbfbfbf);
  final danger = Color(dark ? 0xfff1aaa2 : 0xffa03c35);
  final onDanger = Color(dark ? 0xff212121 : 0xffffffff);
  final scheme = ColorScheme(
    brightness: brightness,
    primary: text,
    onPrimary: background,
    primaryContainer: subtle,
    onPrimaryContainer: text,
    secondary: muted,
    onSecondary: background,
    secondaryContainer: selected,
    onSecondaryContainer: text,
    error: danger,
    onError: onDanger,
    surface: background,
    onSurface: text,
    onSurfaceVariant: muted,
    surfaceContainerLow: sidebar,
    surfaceContainer: subtle,
    surfaceContainerHigh: titlebar,
    surfaceContainerHighest: selected,
    outline: strongLine,
    outlineVariant: line,
    surfaceTint: Colors.transparent,
    scrim: Colors.black,
  );
  final base = ThemeData(colorScheme: scheme, useMaterial3: true);
  const heading = FontWeight.w600;
  const medium = FontWeight.w500;
  const shape = RoundedRectangleBorder(borderRadius: workControlRadius);
  final textTheme = TextTheme(
    headlineSmall: const TextStyle(
      fontSize: 22,
      height: 1.3,
      fontWeight: heading,
    ),
    titleLarge: const TextStyle(fontSize: 18, height: 1.3, fontWeight: heading),
    titleMedium: const TextStyle(
      fontSize: 16,
      height: 1.3,
      fontWeight: heading,
    ),
    titleSmall: const TextStyle(
      fontSize: 14,
      height: 1.35,
      fontWeight: heading,
    ),
    bodyLarge: const TextStyle(fontSize: 16, height: 1.65),
    bodyMedium: const TextStyle(fontSize: 15, height: 1.55),
    bodySmall: const TextStyle(fontSize: 13, height: 1.45),
    labelLarge: const TextStyle(fontSize: 14, height: 1.25, fontWeight: medium),
    labelMedium: const TextStyle(
      fontSize: 12,
      height: 1.35,
      fontWeight: medium,
    ),
    labelSmall: const TextStyle(fontSize: 11, height: 1.35),
  ).apply(bodyColor: text, displayColor: text);
  final fieldBorder = OutlineInputBorder(
    borderRadius: workControlRadius,
    borderSide: BorderSide(color: strongLine),
  );
  Color? focusOverlay(Set<WidgetState> states, Color foreground) =>
      states.contains(WidgetState.focused) &&
          !states.contains(WidgetState.disabled)
      ? foreground.withValues(alpha: 0.18)
      : null;
  final control = ButtonStyle(
    overlayColor: WidgetStateProperty.resolveWith(
      (states) => focusOverlay(states, text),
    ),
    shape: const WidgetStatePropertyAll(shape),
    minimumSize: const WidgetStatePropertyAll(Size(44, 44)),
    padding: const WidgetStatePropertyAll(EdgeInsets.symmetric(horizontal: 12)),
    textStyle: WidgetStatePropertyAll(textTheme.labelLarge),
    elevation: const WidgetStatePropertyAll(0),
  );
  return base.copyWith(
    scaffoldBackgroundColor: background,
    textTheme: textTheme,
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: text,
      selectionHandleColor: text,
      selectionColor: text.withValues(alpha: 0.25),
    ),
    dividerTheme: DividerThemeData(color: line, space: 1, thickness: 1),
    appBarTheme: AppBarTheme(
      backgroundColor: titlebar,
      foregroundColor: text,
      elevation: 0,
      scrolledUnderElevation: 0,
      toolbarHeight: 52,
      titleSpacing: 16,
      titleTextStyle: textTheme.titleSmall,
      iconTheme: IconThemeData(size: 18, color: muted),
      actionsIconTheme: IconThemeData(size: 18, color: muted),
      centerTitle: false,
      shape: Border(bottom: BorderSide(color: line)),
      systemOverlayStyle:
          (dark ? SystemUiOverlayStyle.light : SystemUiOverlayStyle.dark)
              .copyWith(
                statusBarColor: Colors.transparent,
                systemNavigationBarColor: Colors.transparent,
                systemNavigationBarContrastEnforced: false,
              ),
    ),
    iconTheme: IconThemeData(size: 18, color: muted),
    iconButtonTheme: IconButtonThemeData(
      style:
          IconButton.styleFrom(
            shape: shape,
            minimumSize: const Size(44, 44),
            iconSize: 18,
          ).copyWith(
            overlayColor: WidgetStateProperty.resolveWith(
              (states) => focusOverlay(states, text),
            ),
          ),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: background,
      isDense: true,
      constraints: const BoxConstraints(minHeight: 44),
      contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      border: fieldBorder,
      enabledBorder: fieldBorder,
      focusedBorder: fieldBorder.copyWith(borderSide: BorderSide(color: muted)),
      hintStyle: textTheme.bodyMedium?.copyWith(color: muted),
      prefixIconColor: muted,
      suffixIconColor: muted,
      prefixIconConstraints: const BoxConstraints.tightFor(width: 42),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: control.copyWith(
        overlayColor: WidgetStateProperty.resolveWith(
          (states) => focusOverlay(states, background),
        ),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: control.copyWith(
        side: WidgetStateProperty.resolveWith((states) {
          final color = states.contains(WidgetState.focused)
              ? muted
              : strongLine;
          return BorderSide(
            color: states.contains(WidgetState.disabled) ? line : color,
          );
        }),
      ),
    ),
    textButtonTheme: TextButtonThemeData(style: control),
    cardTheme: CardThemeData(
      elevation: 0,
      color: background,
      shape: RoundedRectangleBorder(
        borderRadius: workPanelRadius,
        side: BorderSide(color: line),
      ),
    ),
    dialogTheme: DialogThemeData(
      elevation: 0,
      backgroundColor: background,
      shape: RoundedRectangleBorder(
        borderRadius: workPanelRadius,
        side: BorderSide(color: line),
      ),
      titleTextStyle: textTheme.titleMedium,
      contentTextStyle: textTheme.bodyMedium,
    ),
    snackBarTheme: SnackBarThemeData(
      behavior: SnackBarBehavior.floating,
      backgroundColor: sidebar,
      contentTextStyle: textTheme.bodySmall,
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: workControlRadius,
        side: BorderSide(color: strongLine),
      ),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(
      color: muted,
      linearMinHeight: 2,
      linearTrackColor: subtle,
    ),
  );
}

class WorkWordmark extends StatelessWidget {
  const WorkWordmark({super.key});

  @override
  Widget build(BuildContext context) => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      Image.asset(
        'assets/rpi.png',
        width: 22,
        height: 22,
        excludeFromSemantics: true,
      ),
      const SizedBox(width: 8),
      const Text('RPI Work'),
    ],
  );
}
