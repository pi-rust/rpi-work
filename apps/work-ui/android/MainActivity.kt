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
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import android.provider.Settings
import java.io.File

typealias BuildConfig = com.pirust.rpiwork.BuildConfig

class MainActivity : WryActivity() {
    @androidx.annotation.Keep
    fun updateCacheDirectory(): String = File(cacheDir, "rpi-work-updates").absolutePath

    @androidx.annotation.Keep
    fun draftsDirectory(): String = File(filesDir, "rpi-work-drafts").absolutePath

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
