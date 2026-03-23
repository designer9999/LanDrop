package com.landrop.app

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import java.io.File

/**
 * Resolves content:// URIs to real file paths by copying to cache.
 * Called from Rust via JNI for Android file transfers.
 */
object FileResolver {
    @JvmStatic
    fun resolveUri(context: Context, uriString: String, cacheDir: String): String {
        val uri = Uri.parse(uriString)
        val resolver = context.contentResolver

        // Query display name from ContentResolver
        var fileName = "file_${System.currentTimeMillis()}"
        try {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (idx >= 0) {
                        cursor.getString(idx)?.let { fileName = it }
                    }
                }
            }
        } catch (_: Exception) {
            // Fall back to generated name
        }

        // Ensure cache directory exists
        val outDir = File(cacheDir)
        outDir.mkdirs()
        val outFile = File(outDir, fileName)

        // Copy content to cache file
        resolver.openInputStream(uri)?.use { input ->
            outFile.outputStream().use { output ->
                input.copyTo(output, bufferSize = 65536)
            }
        } ?: throw Exception("Cannot open URI: $uriString")

        return outFile.absolutePath
    }
}
