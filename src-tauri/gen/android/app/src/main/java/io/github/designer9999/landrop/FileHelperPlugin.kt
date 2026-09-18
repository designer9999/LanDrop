package io.github.designer9999.landrop

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.content.Intent
import android.content.ContentValues
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.util.Base64
import androidx.core.content.FileProvider
import androidx.activity.result.ActivityResult
import androidx.appcompat.app.AppCompatActivity
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import app.tauri.plugin.JSObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

internal fun authorizedAttachment(path: String, roots: List<File>): File {
    val file = File(path).canonicalFile
    require(file.isFile && file.canRead()) { "Attachment is not a readable file" }
    require(roots.any { root ->
        // java.nio.file.Path is API 26+, while this app also supports API 24/25.
        file.path.startsWith(root.canonicalPath + File.separator)
    }) { "File is outside LanDrop attachment storage" }
    return file
}

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
    private val pendingExport = AtomicReference<Invoke?>(null)
    private val exportWorker = ThreadPoolExecutor(
        1, 1, 30, TimeUnit.SECONDS, ArrayBlockingQueue<Runnable>(1)
    ).apply { allowCoreThreadTimeOut(true) }

    private fun attachment(path: String): File = authorizedAttachment(path, listOf(
        File(activity.filesDir, "received"),
        File(activity.filesDir, "media_history"),
        File(activity.cacheDir, "send_cache")
    ))

    override fun onDestroy(activity: AppCompatActivity) {
        pendingExport.getAndSet(null)?.reject("Save cancelled because LanDrop closed")
        exportWorker.shutdownNow()
        super.onDestroy(activity)
    }

    @Command
    fun openFile(invoke: Invoke) {
        val args = invoke.parseArgs(PathArgs::class.java)
        val path = args.path

        try {
            val file = attachment(path)

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
            val source = attachment(args.path)
            if (!pendingExport.compareAndSet(null, invoke)) {
                invoke.reject("Another file is being saved; try again when it finishes")
                return
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                exportWorker.execute { exportToMediaStore(invoke, source) }
            } else {
                val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = exportMime(source)
                    putExtra(Intent.EXTRA_TITLE, source.name)
                }
                startActivityForResult(invoke, intent, "saveDocumentResult")
            }
        } catch (e: Exception) {
            pendingExport.compareAndSet(invoke, null)
            invoke.reject("Unable to save attachment: ${e.message}")
        }
    }

    @ActivityCallback
    fun saveDocumentResult(invoke: Invoke, result: ActivityResult) {
        if (pendingExport.get() !== invoke) return
        val destination = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || destination == null) {
            finishExport(invoke, null, "Save cancelled")
            return
        }
        try {
            val source = attachment(invoke.parseArgs(PathArgs::class.java).path)
            exportWorker.execute {
                try {
                    copyAttachment(source, destination)
                    finishExport(invoke, destination.toString(), null)
                } catch (e: Exception) {
                    runCatching { DocumentsContract.deleteDocument(activity.contentResolver, destination) }
                    finishExport(invoke, null, "Unable to save attachment: ${e.message}")
                }
            }
        } catch (e: Exception) {
            runCatching { DocumentsContract.deleteDocument(activity.contentResolver, destination) }
            finishExport(invoke, null, "Unable to save attachment: ${e.message}")
        }
    }

    private fun exportMime(source: File): String = getMimeFromExtension(source.extension)
        .let { if (it == "*/*") "application/octet-stream" else it }

    @android.annotation.TargetApi(Build.VERSION_CODES.Q)
    private fun exportToMediaStore(invoke: Invoke, source: File) {
        var destination: Uri? = null
        try {
            val resolver = activity.contentResolver
            val mime = exportMime(source)
            val image = mime.startsWith("image/")
            val collection = if (image) MediaStore.Images.Media.EXTERNAL_CONTENT_URI
                else MediaStore.Downloads.EXTERNAL_CONTENT_URI
            val directory = if (image) Environment.DIRECTORY_PICTURES else Environment.DIRECTORY_DOWNLOADS
            val values = ContentValues().apply {
                put(MediaStore.MediaColumns.DISPLAY_NAME, source.name)
                put(MediaStore.MediaColumns.MIME_TYPE, mime)
                put(MediaStore.MediaColumns.RELATIVE_PATH, "$directory/LanDrop")
                put(MediaStore.MediaColumns.IS_PENDING, 1)
            }
            val uri = resolver.insert(collection, values)
                ?: throw IllegalStateException("Android could not create the saved file")
            destination = uri
            copyAttachment(source, uri)
            check(pendingExport.get() === invoke) { "Save cancelled" }
            val published = resolver.update(uri, ContentValues().apply {
                put(MediaStore.MediaColumns.IS_PENDING, 0)
            }, null, null)
            check(published == 1) { "Android could not finish saving the file" }
            finishExport(invoke, uri.toString(), null)
        } catch (e: Exception) {
            destination?.let { uri -> runCatching { activity.contentResolver.delete(uri, null, null) } }
            finishExport(invoke, null, "Unable to save attachment: ${e.message}")
        }
    }

    private fun copyAttachment(source: File, destination: Uri) {
        // Bounded streaming runs on one owned worker, never on the UI thread.
        source.inputStream().use { input ->
            activity.contentResolver.openOutputStream(destination, "w")?.use { output ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    check(!Thread.currentThread().isInterrupted && pendingExport.get() != null) { "Save cancelled" }
                    val read = input.read(buffer)
                    if (read < 0) break
                    output.write(buffer, 0, read)
                }
                output.flush()
            } ?: throw IllegalStateException("Android could not open the destination")
        }
    }

    private fun finishExport(invoke: Invoke, destination: String?, error: String?) {
        if (!pendingExport.compareAndSet(invoke, null)) return
        if (error != null) invoke.reject(error)
        else invoke.resolve(JSObject().apply { put("savedPath", destination) })
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
