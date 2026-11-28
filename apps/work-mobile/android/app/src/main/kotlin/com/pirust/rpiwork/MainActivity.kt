package com.pirust.rpiwork

import android.content.ClipboardManager
import android.net.Uri
import android.provider.OpenableColumns
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.ByteArrayOutputStream
import java.io.IOException
import kotlin.concurrent.thread

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "rpi_work/clipboard_attachments")
            .setMethodCallHandler { call, result ->
                if (call.method != "read") {
                    result.notImplemented()
                    return@setMethodCallHandler
                }
                val clipboard = getSystemService(ClipboardManager::class.java)
                val clip = clipboard.primaryClip
                val uris = (0 until (clip?.itemCount ?: 0)).mapNotNull { index ->
                    clip?.getItemAt(index)?.uri?.takeIf { it.scheme == "content" }
                }
                if (uris.size > 8) {
                    result.error("clipboard", "最多可添加 8 个附件。", null)
                    return@setMethodCallHandler
                }
                thread(name = "clipboard-attachments") {
                    try {
                        var remaining = 2 * 1024 * 1024 + 32 * 1024
                        val items = uris.map { uri ->
                            val item = readAttachment(uri, remaining)
                            remaining -= (item.getValue("bytes") as ByteArray).size
                            item
                        }
                        runOnUiThread { result.success(items) }
                    } catch (error: Exception) {
                        val message = if (error is AttachmentTooLarge) {
                            "单个附件超过 1 MiB，或剪贴板附件总大小超出限制。"
                        } else {
                            "无法读取剪贴板中的文件。请重新复制后再试。"
                        }
                        runOnUiThread { result.error("clipboard", message, null) }
                    }
                }
            }
    }

    private fun readAttachment(uri: Uri, remaining: Int): Map<String, Any> {
        val name = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
            ?: when (contentResolver.getType(uri)) {
                "image/png" -> "粘贴图片.png"
                "image/jpeg" -> "粘贴图片.jpg"
                else -> uri.lastPathSegment ?: "粘贴附件"
            }
        val limit = minOf(1024 * 1024, remaining)
        val bytes = contentResolver.openInputStream(uri)?.use { input ->
            val output = ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            while (true) {
                val count = input.read(buffer)
                if (count == -1) break
                if (output.size() + count > limit) throw AttachmentTooLarge()
                output.write(buffer, 0, count)
            }
            output.toByteArray()
        } ?: throw IOException("Clipboard item has no readable content")
        return mapOf("name" to name, "bytes" to bytes)
    }

    private class AttachmentTooLarge : IOException()
}
