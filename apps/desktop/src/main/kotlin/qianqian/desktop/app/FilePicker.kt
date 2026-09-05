package qianqian.desktop.app

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.awt.EventQueue
import java.awt.FileDialog
import java.nio.file.Path
import java.nio.file.Paths

/**
 * The one seam between the player workflow and the platform file chooser,
 * so application/state tests never touch real GUI tooling.
 *
 * Contract: return the chosen file's path, or `null` when the user
 * cancelled. The picker knows nothing about [qianqian.desktop.player.PlayerPort];
 * the application decides what a returned path means.
 */
fun interface FilePicker {
    suspend fun selectAudioFile(): Path?
}

/**
 * Production picker: the JDK's real platform file dialog
 * (`java.awt.FileDialog`, the native chooser on Windows and GTK/Linux).
 * Chosen because it is the simplest mature JVM mechanism — zero new
 * dependencies, single-file selection, cancel-safe (`file == null`).
 *
 * No filename filter is installed: extension is not codec truth (native
 * `pe_open` stays authoritative), and AWT filters are unreliable across
 * platforms — the MVP shows all files.
 *
 * The dialog blocks until dismissed, so it is shown on the AWT event
 * thread (its required context) and awaited from any caller; the Compose
 * UI thread is never blocked.
 */
object AwtFileDialogPicker : FilePicker {

    private const val TITLE = "Open audio file"

    override suspend fun selectAudioFile(): Path? = withContext(Dispatchers.Default) {
        val result = CompletableDeferred<Path?>()
        EventQueue.invokeLater {
            try {
                val dialog = FileDialog(null as java.awt.Frame?, TITLE, FileDialog.LOAD)
                dialog.isVisible = true
                val name = dialog.file
                result.complete(
                    if (name == null) {
                        null // user cancelled
                    } else {
                        Paths.get(dialog.directory, name)
                    },
                )
                dialog.dispose()
            } catch (e: Exception) {
                result.completeExceptionally(e)
            }
        }
        result.await()
    }
}
