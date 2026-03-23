package com.landrop.app

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.content.Intent
import android.media.MediaScannerConnection
import android.net.Uri
import android.provider.OpenableColumns
import android.util.Base64
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import app.tauri.plugin.JSObject
import java.io.ByteArrayOutputStream
import java.io.File

@InvokeArg
internal class PathArgs {
    lateinit var path: String
}

@InvokeArg
internal class UriArgs {
    lateinit var uri: String
}

@InvokeArg
internal class ThumbnailArgs {
    lateinit var path: String
    var maxPx: Int = 120
}

@TauriPlugin
class FileHelperPlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun openFile(invoke: Invoke) {
        val args = invoke.parseArgs(PathArgs::class.java)
        val path = args.path

        try {
            val file = File(path)
            if (!file.exists()) {
                invoke.reject("File not found: $path")
                return
            }

            val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
            val mime = getMimeFromExtension(file.extension)

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

    @Command
    fun saveToDownloads(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(PathArgs::class.java)
            val path = args.path

            val srcFile = File(path)
            if (!srcFile.exists()) {
                invoke.reject("not found: $path")
                return
            }

            val mime = getMimeFromExtension(srcFile.extension)

            // Scan in background so file appears in gallery, resolve immediately
            MediaScannerConnection.scanFile(activity, arrayOf(srcFile.absolutePath), arrayOf(mime), null)

            val result = JSObject()
            result.put("savedPath", path)
            invoke.resolve(result)
        } catch (e: Exception) {
            invoke.reject("error: ${e.javaClass.simpleName}: ${e.message}")
        }
    }

    @Command
    fun getThumbnail(invoke: Invoke) {
        val args = invoke.parseArgs(ThumbnailArgs::class.java)
        val path = args.path
        val maxPx = args.maxPx

        try {
            val bitmap: Bitmap? = if (path.startsWith("content://")) {
                val uri = Uri.parse(path)
                activity.contentResolver.openInputStream(uri)?.use { input ->
                    val bytes = input.readBytes()
                    val opts = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts)
                    val scale = maxOf(1, maxOf(opts.outWidth, opts.outHeight) / maxPx)
                    val opts2 = BitmapFactory.Options().apply { inSampleSize = scale }
                    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts2)
                }
            } else {
                val file = File(path)
                if (!file.exists()) { invoke.resolve(JSObject()); return }
                val ext = file.extension.lowercase()
                if (ext !in listOf("jpg", "jpeg", "png", "gif", "webp", "bmp")) {
                    invoke.resolve(JSObject()); return
                }
                val opts = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeFile(path, opts)
                val scale = maxOf(1, maxOf(opts.outWidth, opts.outHeight) / maxPx)
                opts.inJustDecodeBounds = false
                opts.inSampleSize = scale
                BitmapFactory.decodeFile(path, opts)
            }

            if (bitmap == null) {
                invoke.resolve(JSObject())
                return
            }

            val w = bitmap.width
            val h = bitmap.height
            val ratio = minOf(maxPx.toFloat() / w, maxPx.toFloat() / h, 1f)
            val thumb = if (ratio < 1f) {
                Bitmap.createScaledBitmap(bitmap, (w * ratio).toInt(), (h * ratio).toInt(), true)
            } else bitmap

            val baos = ByteArrayOutputStream()
            thumb.compress(Bitmap.CompressFormat.PNG, 80, baos)
            val b64 = Base64.encodeToString(baos.toByteArray(), Base64.NO_WRAP)

            val result = JSObject()
            result.put("data", "data:image/png;base64,$b64")
            invoke.resolve(result)
        } catch (e: Exception) {
            invoke.resolve(JSObject())
        }
    }

    @Command
    fun getFileName(invoke: Invoke) {
        val args = invoke.parseArgs(UriArgs::class.java)
        val uriString = args.uri

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

            if (name.isEmpty()) {
                val decoded = java.net.URLDecoder.decode(uriString, "UTF-8")
                name = when {
                    decoded.contains("image:") || mimeType.startsWith("image/") -> {
                        val ext = when (mimeType) {
                            "image/png" -> "png"
                            "image/gif" -> "gif"
                            "image/webp" -> "webp"
                            else -> "jpg"
                        }
                        "IMG_${System.currentTimeMillis()}.$ext"
                    }
                    decoded.contains("video:") || mimeType.startsWith("video/") -> {
                        val ext = when (mimeType) {
                            "video/webm" -> "webm"
                            "video/x-matroska" -> "mkv"
                            else -> "mp4"
                        }
                        "VID_${System.currentTimeMillis()}.$ext"
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

    private fun getMimeFromExtension(ext: String): String {
        return when (ext.lowercase()) {
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
    }
}
