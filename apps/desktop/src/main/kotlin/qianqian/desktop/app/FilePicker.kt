package qianqian.desktop.app

import kotlinx.coroutines.suspendCancellableCoroutine
import java.awt.EventQueue
import java.awt.FileDialog
import java.nio.file.Path
import java.nio.file.Paths
import java.util.concurrent.atomic.AtomicReference

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
 * Ownership: the dialog lives entirely inside this object. It is created,
 * shown, and disposed on the AWT EDT; a [suspendCancellableCoroutine] ties
 * the caller's suspension to the dialog's lifetime, so callers never hold
 * a dialog handle. Coroutine cancellation (e.g. application shutdown with
 * the dialog still open) schedules `dispose()` back onto the EDT, which
 * ends the modal loop — no orphan modal window, no JVM hang. Disposal is
 * idempotent and the continuation is resumed at most once (the EDT flow
 * skips the resume when cancellation already claimed the continuation).
 */
object AwtFileDialogPicker : FilePicker {

    private const val TITLE = "Open audio file"

    override suspend fun selectAudioFile(): Path? = suspendCancellableCoroutine { continuation ->
        // Slot protocol: null = dialog pending; FileDialog = published
        // (pending show or shown); [Cancelled] = cancellation arrived
        // before the EDT could publish. This closes the create/show race:
        // whichever side loses the slot swap cleans up its own half.
        val slot = AtomicReference<Any?>(null)
        continuation.invokeOnCancellation {
            val published = slot.getAndSet(Cancelled)
            if (published is FileDialog) {
                // The EDT is parked in the modal loop, which keeps
                // dispatching queued work — this dispose ends the show.
                EventQueue.invokeLater { published.dispose() }
            }
        }
        EventQueue.invokeLater {
            val dialog = FileDialog(null as java.awt.Frame?, TITLE, FileDialog.LOAD)
            if (!slot.compareAndSet(null, dialog)) {
                dialog.dispose() // cancelled before show: never displayed
                return@invokeLater
            }
            try {
                dialog.isVisible = true // modal; returns when dismissed or disposed
                val chosen = dialog.file?.let { name ->
                    val dir = requireNotNull(dialog.directory) {
                        "file dialog returned '$name' without a directory"
                    }
                    Paths.get(dir, name)
                }
                dialog.dispose()
                if (continuation.isActive) continuation.resume(chosen, onCancellation = null)
            } catch (e: Throwable) {
                dialog.dispose()
                if (continuation.isActive) continuation.resumeWith(Result.failure(e))
            }
        }
    }

    /** Slot sentinel: cancellation claimed the dialog slot before the EDT. */
    private object Cancelled
}
