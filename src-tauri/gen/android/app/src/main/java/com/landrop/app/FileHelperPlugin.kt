package com.landrop.app

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import app.tauri.plugin.JSObject
import java.io.File

@TauriPlugin
class FileHelperPlugin(private val activity: Activity) : Plugin(activity) {

    /**
     * Open a local file using the system's default viewer via FileProvider.
     * Args: { "path": "/storage/emulated/0/Download/LanDrop/photo.jpg" }
     */
    @Command
    fun openFile(invoke: Invoke) {
        val args = invoke.parseArgs(JSObject::class.java)
        val path = args.getString("path") ?: run {
            invoke.reject("No path provided")
            return
        }

        try {
            val file = File(path)
            if (!file.exists()) {
                invoke.reject("File not found: $path")
                return
            }

            val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)

            val mime = when (file.extension.lowercase()) {
                "jpg", "jpeg" -> "image/jpeg"
                "png" -> "image/png"
                "gif" -> "image/gif"
                "webp" -> "image/webp"
                "bmp" -> "image/bmp"
                "mp4" -> "video/mp4"
                "mkv" -> "video/x-matroska"
                "avi" -> "video/x-msvideo"
                "mov" -> "video/quicktime"
                "webm" -> "video/webm"
                "pdf" -> "application/pdf"
                "txt" -> "text/plain"
                "zip" -> "application/zip"
                else -> "*/*"
            }

            val intent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, mime)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            activity.startActivity(intent)
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("Failed to open file: ${e.message}")
        }
    }

    /**
     * Get the display name and MIME type for a content:// URI.
     * Args: { "uri": "content://..." }
     * Returns: { "name": "photo.jpg", "mimeType": "image/jpeg" }
     */
    @Command
    fun getFileName(invoke: Invoke) {
        val args = invoke.parseArgs(JSObject::class.java)
        val uriString = args.getString("uri") ?: run {
            invoke.reject("No URI provided")
            return
        }

        try {
            val uri = Uri.parse(uriString)
            var name = ""
            var mimeType = activity.contentResolver.getType(uri) ?: ""

            activity.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (idx >= 0) {
                        cursor.getString(idx)?.let { name = it }
                    }
                }
            }

            // If name is empty, generate from URI type hints
            if (name.isEmpty()) {
                val decoded = java.net.URLDecoder.decode(uriString, "UTF-8")
                name = when {
                    decoded.contains("image:") || mimeType.startsWith("image/") -> {
                        val id = decoded.substringAfterLast("image:").take(20)
                        val ext = when (mimeType) {
                            "image/png" -> "png"
                            "image/gif" -> "gif"
                            "image/webp" -> "webp"
                            else -> "jpg"
                        }
                        "IMG_$id.$ext"
                    }
                    decoded.contains("video:") || mimeType.startsWith("video/") -> {
                        val id = decoded.substringAfterLast("video:").take(20)
                        val ext = when (mimeType) {
                            "video/webm" -> "webm"
                            "video/x-matroska" -> "mkv"
                            else -> "mp4"
                        }
                        "VID_$id.$ext"
                    }
                    else -> "file_${System.currentTimeMillis()}"
                }
            }

            val result = JSObject()
            result.put("name", name)
            result.put("mimeType", mimeType)
            invoke.resolve(result)
        } catch (e: Exception) {
            invoke.reject("Failed to get filename: ${e.message}")
        }
    }
}
