import Flutter
import UIKit

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    let channel = FlutterMethodChannel(
      name: "rpi_work/clipboard_attachments",
      binaryMessenger: engineBridge.applicationRegistrar.messenger()
    )
    channel.setMethodCallHandler { call, result in
      guard call.method == "read" else {
        result(FlutterMethodNotImplemented)
        return
      }
      do {
        result(try Self.readClipboardAttachments())
      } catch let failure as ClipboardReadError {
        result(FlutterError(code: "clipboard", message: failure.message, details: nil))
      } catch {
        result(FlutterError(code: "clipboard", message: "无法读取剪贴板中的文件。请重新复制后再试。", details: nil))
      }
    }
  }

  private struct ClipboardReadError: Error {
    let message: String
  }

  private static func readClipboardAttachments() throws -> [[String: Any]] {
    let pasteboard = UIPasteboard.general
    let png = "public.png"
    let jpeg = "public.jpeg"
    let fileURL = "public.file-url"
    let types = [png, jpeg, fileURL]
    let itemTypes = pasteboard.types(forItemSet: nil) ?? []
    let indexes = itemTypes.indices.filter { index in
      itemTypes[index].contains(where: types.contains)
    }
    guard indexes.count <= 8 else {
      throw ClipboardReadError(message: "最多可添加 8 个附件。")
    }
    var remaining = 2 * 1024 * 1024 + 32 * 1024
    return try indexes.map { index in
      let itemSet = IndexSet(integer: index)
      let limit = min(1024 * 1024, remaining)
      let name: String
      let data: Data
      if itemTypes[index].contains(fileURL),
        let value = pasteboard.values(forPasteboardType: fileURL, inItemSet: itemSet)?.first,
        let url = (value as? URL) ?? (value as? String).flatMap({ URL(string: $0) }),
        url.isFileURL
      {
        name = url.lastPathComponent
        let opened = url.startAccessingSecurityScopedResource()
        defer { if opened { url.stopAccessingSecurityScopedResource() } }
        let file = try FileHandle(forReadingFrom: url)
        defer { try? file.close() }
        data = try file.read(upToCount: limit + 1) ?? Data()
      } else {
        let imageType = itemTypes[index].contains(png) ? png : jpeg
        guard let bytes = pasteboard.data(forPasteboardType: imageType, inItemSet: itemSet)?.first else {
          throw ClipboardReadError(message: "无法读取剪贴板中的文件。请重新复制后再试。")
        }
        name = imageType == png ? "粘贴图片.png" : "粘贴图片.jpg"
        data = bytes
      }
      guard data.count <= limit else {
        throw ClipboardReadError(message: "单个附件超过 1 MiB，或剪贴板附件总大小超出限制。")
      }
      remaining -= data.count
      return ["name": name, "bytes": FlutterStandardTypedData(bytes: data)]
    }
  }
}
