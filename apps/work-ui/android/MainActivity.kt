package dev.dioxus.main

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.view.View
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import android.provider.OpenableColumns
import android.provider.Settings
import java.io.File
import java.io.ByteArrayOutputStream
import java.net.URI
import java.security.KeyStore
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONObject

typealias BuildConfig = com.pirust.rpiwork.BuildConfig

class MainActivity : WryActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Android 15 enforces edge-to-edge for targetSdk 35. Fit the WebView's
        // content root to real system/cutout/IME insets, never a guessed bar height.
        WindowCompat.setDecorFitsSystemWindows(window, false)
        val content = findViewById<View>(android.R.id.content)
        ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
            val safe = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or
                    WindowInsetsCompat.Type.displayCutout() or
                    WindowInsetsCompat.Type.ime()
            )
            view.setPadding(safe.left, safe.top, safe.right, safe.bottom)
            // This root owns the inset. Do not let the WebView/CSS apply it again.
            WindowInsetsCompat.CONSUMED
        }
        ViewCompat.requestApplyInsets(content)
    }

    @androidx.annotation.Keep
    fun updateCacheDirectory(): String = File(cacheDir, "rpi-work-updates").absolutePath

    @androidx.annotation.Keep
    fun draftsDirectory(): String = File(filesDir, "rpi-work-drafts").absolutePath

    // Only the non-sensitive root address is plaintext. The complete v1 envelope,
    // including its address/node pin, is authenticated and encrypted by AndroidKeyStore.
    private val rememberedMaxPayload = 32768
    private val rememberedKeyAlias = "rpi-work.remembered-connection.v1"

    private fun rememberedFile(name: String): AtomicFile = AtomicFile(File(noBackupFilesDir, name))
    private fun rememberedSessionFile(): AtomicFile = rememberedFile("remembered-connection-v1.enc")
    private fun rememberedAddressFile(): AtomicFile = rememberedFile("remembered-address-v1")
    private fun rememberedAad(): ByteArray =
        "rpi-work/remembered-connection/SessionEnvelope/v1\u0000$packageName".toByteArray(Charsets.UTF_8)

    private fun rememberedValidAddress(address: String): Boolean {
        if (address.isEmpty() || address.length > 4096 || address.any { it <= ' ' || it >= '\u007f' }) return false
        return try {
            val uri = URI(address).parseServerAuthority()
            (uri.scheme == "https" || uri.scheme == "http") && !uri.isOpaque &&
                !uri.host.isNullOrEmpty() && uri.rawUserInfo == null &&
                uri.rawQuery == null && uri.rawFragment == null &&
                (uri.rawPath.isNullOrEmpty() || uri.rawPath == "/") &&
                (uri.port == -1 || uri.port in 1..65535) &&
                !uri.rawAuthority.endsWith(":")
        } catch (_: Exception) { false }
    }

    private fun rememberedEnvelope(payload: String): JSONObject {
        require(payload.length <= rememberedMaxPayload)
        require(payload.toByteArray(Charsets.UTF_8).size <= rememberedMaxPayload)
        val tokener = org.json.JSONTokener(payload)
        val envelope = JSONObject(tokener)
        require(tokener.nextClean() == '\u0000')
        val fields = envelope.keys().asSequence().toSet()
        require(fields == setOf("version", "address", "nodeId", "session"))
        require(envelope.get("version") is Int && envelope.getInt("version") == 1)
        val address = envelope.get("address")
        require(address is String && rememberedValidAddress(address))
        require(envelope.isNull("nodeId") || envelope.get("nodeId") is String)
        // RememberedSession is opaque here; Rust owns validation of its exact schema.
        require(envelope.isNull("session") || envelope.get("session") is JSONObject)
        return envelope
    }

    private fun rememberedRead(file: AtomicFile, limit: Int): ByteArray = file.openRead().use { input ->
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(1024)
        while (true) {
            val count = input.read(buffer, 0, minOf(buffer.size, limit + 1 - output.size()))
            if (count < 0) break
            output.write(buffer, 0, count)
            require(output.size() <= limit)
        }
        output.toByteArray()
    }

    private fun rememberedExists(file: AtomicFile): Boolean =
        listOf("", ".bak", ".new").any { File(file.baseFile.path + it).exists() }

    private fun rememberedDelete(file: AtomicFile): Boolean = try {
        file.delete() // AtomicFile also removes its backup; check all variants before claiming success.
        listOf("", ".bak", ".new").forEach { suffix ->
            val remaining = File(file.baseFile.path + suffix)
            if (remaining.exists()) remaining.delete()
        }
        !rememberedExists(file)
    } catch (_: Exception) { false }

    private fun rememberedWrite(file: AtomicFile, data: ByteArray) {
        val stream = file.startWrite()
        try {
            stream.write(data)
            file.finishWrite(stream)
            // AtomicFile can report rename failures only in platform logs. Verify commit.
            check(rememberedRead(file, data.size).contentEquals(data))
        } catch (error: Exception) {
            file.failWrite(stream)
            throw error
        }
    }

    private fun rememberedKey(create: Boolean): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val existing = store.getKey(rememberedKeyAlias, null)
        if (existing != null) return existing as SecretKey
        check(create)
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(KeyGenParameterSpec.Builder(
            rememberedKeyAlias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
        ).setKeySize(256)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setRandomizedEncryptionRequired(true)
            .setUserAuthenticationRequired(false)
            .build())
        return generator.generateKey() // Non-exportable; no authentication prompt.
    }

    @Synchronized
    @androidx.annotation.Keep
    fun loadRememberedConnection(): String {
        var address = ""
        var session: JSONObject? = null
        var unavailable = false
        try {
            val addressFile = rememberedAddressFile()
            if (rememberedExists(addressFile)) {
                val stored = rememberedRead(addressFile, 4096).toString(Charsets.UTF_8)
                check(rememberedValidAddress(stored))
                address = stored
            }
        } catch (_: Exception) { unavailable = true }
        try {
            val file = rememberedSessionFile()
            if (rememberedExists(file)) {
                // File format: 12-byte random IV followed by GCM ciphertext + 128-bit tag.
                val encrypted = rememberedRead(file, rememberedMaxPayload + 12 + 16)
                check(encrypted.size >= 12 + 16)
                val cipher = Cipher.getInstance("AES/GCM/NoPadding")
                cipher.init(Cipher.DECRYPT_MODE, rememberedKey(false), GCMParameterSpec(128, encrypted.copyOfRange(0, 12)))
                cipher.updateAAD(rememberedAad())
                val plain = cipher.doFinal(encrypted, 12, encrypted.size - 12)
                try {
                    val envelope = rememberedEnvelope(plain.toString(Charsets.UTF_8))
                    // Never pair an old session with a different latest successful root.
                    check(envelope.getString("address") == address)
                    session = envelope
                } finally { plain.fill(0) }
            }
        } catch (_: Exception) {
            session = null
            unavailable = true
            try { rememberedDelete(rememberedSessionFile()) } catch (_: Exception) { /* generic failure only */ }
        }
        return JSONObject().put("address", address).put("session", session ?: JSONObject.NULL)
            .put("unavailable", unavailable).toString()
    }

    @Synchronized
    @androidx.annotation.Keep
    fun saveRememberedConnection(payload: String): String {
        val envelope = try { rememberedEnvelope(payload) } catch (_: Exception) { return "{\"saved\":false}" }
        var plain: ByteArray? = null
        return try {
            // Commit the latest successful address even when credential storage is unavailable.
            rememberedWrite(rememberedAddressFile(), envelope.getString("address").toByteArray(Charsets.UTF_8))
            // Remove the previous transaction before starting this one: even a process
            // interruption must not restore an older credential from AtomicFile's backup.
            check(rememberedDelete(rememberedSessionFile()))
            plain = payload.toByteArray(Charsets.UTF_8)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            // AndroidKeyStore generates a fresh random IV for every encryption.
            cipher.init(Cipher.ENCRYPT_MODE, rememberedKey(true))
            check(cipher.iv.size == 12)
            cipher.updateAAD(rememberedAad())
            val encrypted = cipher.iv + cipher.doFinal(plain)
            rememberedWrite(rememberedSessionFile(), encrypted)
            "{\"saved\":true}"
        } catch (_: Exception) {
            // Do not let a failed update resurrect a different/stale credential.
            try { rememberedDelete(rememberedSessionFile()) } catch (_: Exception) { /* generic failure only */ }
            "{\"saved\":false}"
        } finally { plain?.fill(0) }
    }

    @Synchronized
    @androidx.annotation.Keep
    fun clearRememberedConnection(keepAddress: Boolean): String {
        return try {
            val sessionCleared = rememberedDelete(rememberedSessionFile())
            val addressCleared = if (keepAddress) true else rememberedDelete(rememberedAddressFile())
            if (sessionCleared && addressCleared) "{\"cleared\":true}" else "{\"cleared\":false}"
        } catch (_: Exception) { "{\"cleared\":false}" }
    }

    @Suppress("DEPRECATION")
    private fun signers(info: PackageInfo): Set<String> {
        val signatures = if (Build.VERSION.SDK_INT >= 28) info.signingInfo?.apkContentsSigners else info.signatures
        return signatures?.map { signature -> signature.toCharsString() }?.toSet() ?: emptySet()
    }

    @Suppress("DEPRECATION")
    @androidx.annotation.Keep
    fun installVerifiedUpdate(path: String): String {
        return try {
            val root = File(updateCacheDirectory()).canonicalFile
            val apk = File(path).canonicalFile
            if (!apk.path.startsWith(root.path + File.separator) || !apk.isFile) return "更新文件不在应用缓存中，请重新下载。"
            val flags = if (Build.VERSION.SDK_INT >= 28) PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
            val incoming = packageManager.getPackageArchiveInfo(apk.path, flags) ?: return "无法读取更新APK。"
            val installed = packageManager.getPackageInfo(packageName, flags)
            if (incoming.packageName != packageName) return "更新APK包名不匹配，已阻止安装。"
            val incomingCode = if (Build.VERSION.SDK_INT >= 28) incoming.longVersionCode else incoming.versionCode.toLong()
            val installedCode = if (Build.VERSION.SDK_INT >= 28) installed.longVersionCode else installed.versionCode.toLong()
            if (incomingCode <= installedCode) return "更新APK版本不高于当前版本，已阻止降级。"
            if (signers(incoming).isEmpty() || signers(incoming) != signers(installed)) return "更新APK签名与当前应用不同。旧临时开发签名版需先卸载，再安装固定签名版。"
            if (Build.VERSION.SDK_INT >= 26 && !packageManager.canRequestPackageInstalls()) {
                runOnUiThread { startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:$packageName"))) }
                return "请允许此应用安装更新，然后返回点击“安装更新”。"
            }
            apk.copyTo(File(root, "update.apk"), overwrite = true)
            val uri = Uri.parse("content://$packageName.updates/update.apk")
            val intent = Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            if (intent.resolveActivity(packageManager) == null) return "此设备没有可用的APK安装程序。"
            runOnUiThread { startActivity(intent) }
            ""
        } catch (error: Exception) { "无法打开更新安装程序：${error.message}" }
    }
}

/** Only exposes the already verified APK; no directory or arbitrary file access. */
class UpdateProvider : ContentProvider() {
    override fun onCreate(): Boolean = true
    private fun apk(uri: Uri): File {
        val context = context ?: throw java.io.FileNotFoundException()
        if (uri.authority != "${context.packageName}.updates" || uri.path != "/update.apk") throw java.io.FileNotFoundException()
        return File(context.cacheDir, "rpi-work-updates/update.apk")
    }
    override fun getType(uri: Uri): String { apk(uri); return "application/vnd.android.package-archive" }
    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        if (mode != "r") throw SecurityException("Read-only update")
        return ParcelFileDescriptor.open(apk(uri), ParcelFileDescriptor.MODE_READ_ONLY)
    }
    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, selectionArgs: Array<out String>?, sortOrder: String?): Cursor {
        val file = apk(uri)
        val columns = projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        return MatrixCursor(columns).apply { addRow(columns.map { column -> when (column) {
            OpenableColumns.DISPLAY_NAME -> "rpi-work-update.apk"
            OpenableColumns.SIZE -> file.length()
            else -> null
        } }.toTypedArray()) }
    }
    override fun insert(uri: Uri, values: ContentValues?): Uri? = throw SecurityException("Read-only update")
    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = throw SecurityException("Read-only update")
    override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int = throw SecurityException("Read-only update")
}
