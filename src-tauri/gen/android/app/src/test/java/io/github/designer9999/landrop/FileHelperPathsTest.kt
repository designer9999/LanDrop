package io.github.designer9999.landrop

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class FileHelperPathsTest {
    @Test
    fun onlyReadableFilesInsideAttachmentRootsAreAllowed() {
        val directory = Files.createTempDirectory("landrop-file-policy-").toFile()
        try {
            val received = File(directory, "received").apply { mkdir() }
            val document = File(received, "document.txt").apply { writeText("fixture") }
            val unrelated = File(directory, "credentials.json").apply { writeText("not exportable") }
            val sibling = File(directory, "received-extra").apply { mkdir() }
            val siblingFile = File(sibling, "document.txt").apply { writeText("not exportable") }
            assertEquals(document.canonicalFile, authorizedAttachment(document.path, listOf(received)))
            assertThrows(IllegalArgumentException::class.java) {
                authorizedAttachment(unrelated.path, listOf(received))
            }
            assertThrows(IllegalArgumentException::class.java) {
                authorizedAttachment(siblingFile.path, listOf(received))
            }
            assertThrows(IllegalArgumentException::class.java) {
                authorizedAttachment(File(received, "../credentials.json").path, listOf(received))
            }
            assertThrows(IllegalArgumentException::class.java) {
                authorizedAttachment(File(received, "missing.txt").path, listOf(received))
            }
            assertThrows(IllegalArgumentException::class.java) {
                authorizedAttachment(received.path, listOf(received))
            }
        } finally {
            directory.deleteRecursively()
        }
    }
}
