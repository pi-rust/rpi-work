import 'dart:async';

// Keep the framework Material theme shared with the Markdown renderer.
// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:image_picker/image_picker.dart';
import 'package:lucide_icons_flutter/lucide_icons_300.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import 'node/work_node.dart';
import 'work_controls.dart';
import 'work_theme.dart';

const _expiredMessage = '授权码已过期，请在电脑上生成新二维码。';

class ConnectScreen extends StatefulWidget {
  const ConnectScreen({
    super.key,
    required this.connect,
    this.notice,
    this.pickImage,
  });

  final Future<void> Function(PairingCode, bool Function()) connect;
  final String? notice;
  final Future<XFile?> Function()? pickImage;

  @override
  State<ConnectScreen> createState() => _ConnectScreenState();
}

class _ConnectScreenState extends State<ConnectScreen>
    with WidgetsBindingObserver {
  final _scanner = MobileScannerController(formats: [BarcodeFormat.qrCode]);
  final _picker = ImagePicker();
  PairingCode? _code;
  Timer? _expiry;
  String? _error;
  bool _scan = false;
  bool _picking = false;
  bool _connecting = false;
  int _attempt = 0;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _expiry?.cancel();
    unawaited(_scanner.dispose());
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed ||
        state == AppLifecycleState.inactive) {
      return;
    }
    _reset();
  }

  void _reset() {
    _expiry?.cancel();
    _attempt++;
    setState(() {
      _scan = false;
      _code = null;
      _error = null;
    });
    unawaited(_scanner.stop());
  }

  void _detect(BarcodeCapture capture) {
    if (!_scan) return;
    _accept(capture);
  }

  void _accept(BarcodeCapture capture) {
    final hits = capture.barcodes
        .map((entry) => entry.rawValue)
        .nonNulls
        .toSet();
    _reset();
    if (hits.length != 1) {
      setState(() => _error = '请选择只包含一个授权二维码的图片，或重新扫码。');
      return;
    }
    try {
      final code = PairingCode.parse(hits.single);
      setState(() => _code = code);
      _expiry = Timer(code.expiresAt.difference(DateTime.now()), () {
        if (!mounted || _code != code || _connecting) return;
        setState(() {
          _code = null;
          _error = _expiredMessage;
        });
      });
    } on ExpiredPairingCode {
      setState(() => _error = _expiredMessage);
    } on FormatException {
      setState(() => _error = '请使用电脑生成的有效 RPI Work 授权二维码。');
    }
  }

  Future<void> _pick() async {
    _reset();
    setState(() => _picking = true);
    try {
      final pickImage = widget.pickImage;
      final file = pickImage == null
          ? await _picker.pickImage(source: ImageSource.gallery)
          : await pickImage();
      if (file == null || !mounted) return;
      final attempt = _attempt;
      if (await file.length() > 20 * 1024 * 1024) {
        if (mounted && attempt == _attempt) {
          setState(() => _error = '图片不能超过 20 MB。');
        }
        return;
      }
      if (!mounted || attempt != _attempt) return;
      final capture = await _scanner.analyzeImage(
        file.path,
        formats: [BarcodeFormat.qrCode],
      );
      if (!mounted || attempt != _attempt) return;
      if (capture == null) {
        setState(() => _error = '图片中没有可识别的二维码。');
      } else {
        _accept(capture);
      }
    } catch (_) {
      if (mounted) setState(() => _error = '无法读取相册中的二维码。');
    } finally {
      if (mounted) setState(() => _picking = false);
    }
  }

  Future<void> _authorize() async {
    final code = _code;
    if (code == null) return;
    _expiry?.cancel();
    setState(() {
      _connecting = true;
      _error = null;
    });
    final attempt = _attempt;
    bool isCurrent() => mounted && attempt == _attempt;
    try {
      await widget.connect(code, isCurrent);
    } on NodeException catch (error) {
      if (isCurrent()) {
        setState(
          () => _error = error.outcomeUnknown
              ? '授权结果未知，请在电脑上生成新二维码后重试。'
              : '连接失败。${error.message}',
        );
      }
    } finally {
      if (mounted) {
        setState(() {
          _connecting = false;
          _code = null;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final code = _code;
    final error = _error ?? widget.notice;
    final busy = _picking || _connecting;
    return Scaffold(
      appBar: AppBar(title: const WorkWordmark()),
      body: SafeArea(
        top: false,
        child: Center(
          child: SingleChildScrollView(
            padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 20),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 420),
              child: DecoratedBox(
                decoration: BoxDecoration(
                  border: Border.all(color: colors.outlineVariant),
                  borderRadius: workPanelRadius,
                ),
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Row(
                        children: [
                          Icon(
                            LucideIcons300.monitor,
                            color: colors.onSurfaceVariant,
                          ),
                          const SizedBox(width: 10),
                          Expanded(
                            child: Semantics(
                              header: true,
                              child: Text(
                                '连接到电脑',
                                style: theme.textTheme.titleMedium,
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 10),
                      Text(
                        '在电脑的 RPI Work 中打开「工作协同」并签发一次性授权。',
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: colors.onSurfaceVariant,
                        ),
                      ),
                      if (_scan) ...[
                        const SizedBox(height: 14),
                        ClipRRect(
                          borderRadius: workControlRadius,
                          child: AspectRatio(
                            aspectRatio: 1.35,
                            child: MobileScanner(
                              controller: _scanner,
                              onDetect: _detect,
                              errorBuilder: (context, error) => ColoredBox(
                                color: colors.surfaceContainer,
                                child: const Center(
                                  child: Padding(
                                    padding: EdgeInsets.all(16),
                                    child: Text(
                                      '无法使用相机。请检查系统权限，或从相册选取二维码。',
                                      textAlign: TextAlign.center,
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          ),
                        ),
                      ],
                      if (code != null) ...[
                        const SizedBox(height: 14),
                        Semantics(
                          header: true,
                          child: Text(
                            '确认一次性授权',
                            style: theme.textTheme.titleSmall,
                          ),
                        ),
                        const SizedBox(height: 6),
                        DecoratedBox(
                          decoration: BoxDecoration(
                            border: Border.all(color: colors.outline),
                            borderRadius: workControlRadius,
                          ),
                          child: Padding(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 12,
                              vertical: 10,
                            ),
                            child: SelectableText(
                              '${code.origin}',
                              semanticsLabel: '节点地址 ${code.origin}',
                              contextMenuBuilder: workEditableMenu,
                            ),
                          ),
                        ),
                        const SizedBox(height: 6),
                        Text(
                          '仅在你确认这台电脑后授权。',
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: colors.onSurfaceVariant,
                          ),
                        ),
                      ],
                      if (error != null) ...[
                        const SizedBox(height: 10),
                        Semantics(
                          liveRegion: true,
                          child: Text(
                            error,
                            style: TextStyle(color: colors.error),
                          ),
                        ),
                      ],
                      const SizedBox(height: 16),
                      Wrap(
                        alignment: WrapAlignment.end,
                        spacing: 8,
                        runSpacing: 8,
                        children: [
                          if (code != null)
                            OutlinedButton(
                              onPressed: busy ? null : _reset,
                              child: const Text('取消'),
                            )
                          else
                            OutlinedButton.icon(
                              onPressed: busy ? null : _pick,
                              icon: const Icon(LucideIcons300.image),
                              label: Text(_picking ? '正在读取…' : '从相册选取'),
                            ),
                          FilledButton.icon(
                            onPressed: busy
                                ? null
                                : code != null
                                ? _authorize
                                : _scan
                                ? _reset
                                : () {
                                    _reset();
                                    setState(() => _scan = true);
                                  },
                            icon: Icon(
                              code != null
                                  ? LucideIcons300.link
                                  : _scan
                                  ? LucideIcons300.x
                                  : LucideIcons300.scanLine,
                            ),
                            label: Text(
                              _connecting
                                  ? '正在授权…'
                                  : code != null
                                  ? '授权并打开'
                                  : _scan
                                  ? '关闭相机'
                                  : '打开相机',
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
