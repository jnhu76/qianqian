package qianqian.desktop.app

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import qianqian.desktop.player.AbiMismatch
import qianqian.desktop.player.BridgeClosedException
import qianqian.desktop.player.ControlFailure
import qianqian.desktop.player.EngineCreationFailure
import qianqian.desktop.player.OpenFailure
import qianqian.desktop.player.PlayerBridgeException
import qianqian.desktop.player.PlayerPort
import qianqian.desktop.player.PlayerSnapshot
import qianqian.desktop.player.PlayerState
import qianqian.desktop.player.RuntimeLoadFailure
import qianqian.desktop.player.SourceOpenFailure
import java.nio.file.Path
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The application coordination owner for the player screen: it routes user
 * intent to [PlayerPort] commands, projects native [PlayerSnapshot]s into
 * [PlayerUiState], and owns transient application state (operation in
 * flight, seek preview/commit display, product error classification).
 *
 * Non-negotiable direction (single playback truth — native PlayerEngine):
 *
 * ```text
 * user command → PlayerPort command → native result
 *     → snapshot poller → PlayerSnapshot → this projection → Compose
 * ```
 *
 * No method here ever writes a playback state directly; the UI state's
 * snapshot field ONLY ever carries what the port published. The model owns
 * no native handle, no JNA, no queue/library/settings/navigation — it
 * coordinates one screen's workflow over the port.
 */
class PlayerScreenModel(
    private val port: PlayerPort,
    private val picker: FilePicker,
    parentScope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
) {
    /**
     * Hosts the snapshot projection and in-flight command coroutines.
     * Owned by the model; cancelled by [close] before the port closes.
     */
    private val scope =
        CoroutineScope(SupervisorJob(parentScope.coroutineContext[Job]) + Dispatchers.Default)

    private val _uiState = MutableStateFlow(PlayerUiState())
    val uiState: StateFlow<PlayerUiState> = _uiState.asStateFlow()

    /** Synchronous command admission: at most one command in flight. */
    private val commandActive = AtomicBoolean(false)

    private val closed = AtomicBoolean(false)

    init {
        scope.launch {
            port.snapshot.collect { snap ->
                _uiState.update { state ->
                    state.copy(
                        snapshot = snap,
                        // A committed seek is displayed until a snapshot
                        // reaches its landing; then snapshot truth resumes.
                        committedSeekUs = clearCaughtUpSeek(state, snap),
                    )
                }
            }
        }
    }

    // ---- user commands ---------------------------------------------------

    /**
     * Open File button: pick a file through the platform chooser, then open
     * it. The operation covers the whole chooser visit, so the UI stays in
     * `operationInFlight` (controls disabled) until the dialog closes.
     */
    fun openFileViaPicker() = launchCommand {
        beginOperation()
        val chosen = try {
            picker.selectAudioFile()
        } catch (e: Exception) {
            failOperation(PlayerUiError(PlayerUiErrorCategory.FileDialogFailed))
            return@launchCommand
        }
        if (chosen != null) {
            openFile(chosen)
        } else {
            endOperation() // cancelled: a deliberate no-op
        }
    }

    /**
     * Open a concrete file. Semantics follow the native contract: open
     * succeeds → READY, never autoplay; the candidate is promoted to
     * [PlayerUiState.selectedFile] ONLY on success, so a failed file is
     * never displayed as the current track.
     */
    suspend fun openFile(path: Path) {
        beginOperation()
        try {
            port.open(path)
        } catch (e: BridgeClosedException) {
            return // shutting down; not a product error
        } catch (e: PlayerBridgeException) {
            failOperation(openError(e, path))
            return
        } catch (e: Exception) {
            failOperation(
                PlayerUiError(PlayerUiErrorCategory.CouldNotOpenTrack, path.fileName?.toString()),
            )
            return
        }
        _uiState.update {
            it.copy(
                selectedFile = path,
                operationInFlight = false,
                seekPreviewUs = null,
                committedSeekUs = null,
            )
        }
    }

    /**
     * The one playback control: Play from READY/PAUSED/ENDED, Pause from
     * PLAYING. No optimistic state — the button label follows the snapshot.
     */
    suspend fun togglePlayback() {
        val state = _uiState.value.snapshot.state
        if (state == PlayerState.EMPTY || state == PlayerState.ERROR) return
        beginOperation()
        try {
            when (state) {
                PlayerState.PLAYING -> port.pause()
                else -> port.play() // READY, PAUSED, ENDED (ENDED replays from 0)
            }
        } catch (e: BridgeClosedException) {
            return // shutting down
        } catch (e: PlayerBridgeException) {
            failOperation(commandError(e))
            return
        }
        endOperation()
    }

    /** Native `stop` → READY @0; the UI follows the snapshot, no local reset. */
    suspend fun stopPlayback() {
        beginOperation()
        try {
            port.stop()
        } catch (e: BridgeClosedException) {
            return // shutting down
        } catch (e: PlayerBridgeException) {
            failOperation(commandError(e))
            return
        }
        _uiState.update {
            it.copy(operationInFlight = false, seekPreviewUs = null, committedSeekUs = null)
        }
    }

    /**
     * Slider drag update: local preview ONLY — no native call. A drag
     * gesture may produce hundreds of these; native seek is committed once,
     * on release.
     */
    fun onSeekPreview(positionUs: Long) {
        _uiState.update { it.copy(seekPreviewUs = positionUs.coerceAtLeast(0)) }
    }

    /**
     * Slider release: commit exactly one native seek. The engine-reported
     * landing stays displayed until a snapshot confirms it (or the
     * requested target when the landing is genuinely unknown). On failure
     * the preview is cleared, the error is shown, and the native snapshot
     * remains the timeline authority.
     */
    fun onSeekCommit() {
        val target = _uiState.value.seekPreviewUs ?: return
        launchCommand {
            beginOperation()
            try {
                val landing = port.seek(target)
                _uiState.update {
                    it.copy(
                        operationInFlight = false,
                        seekPreviewUs = null,
                        committedSeekUs = if (landing >= 0) landing else target,
                    )
                }
            } catch (e: BridgeClosedException) {
                return@launchCommand // shutting down
            } catch (e: PlayerBridgeException) {
                failOperation(commandError(e))
            }
        }
    }

    // ---- lifecycle -------------------------------------------------------

    /**
     * Shut down exactly once: stop the model's coroutines first, then close
     * the port (the port's own close is idempotent; [NonCancellable] keeps
     * the teardown running even when called from a cancelled scope).
     */
    suspend fun close() {
        if (!closed.compareAndSet(false, true)) return
        scope.cancel()
        withContext(NonCancellable) { port.close() }
    }

    // ---- internals -------------------------------------------------------

    /**
     * One-command-at-a-time admission, enforced synchronously so a
     * double-click cannot double-dispatch. Rejected clicks are dropped —
     * the native bridge remains the serialization authority; this is only
     * click-spam sanity.
     */
    private fun launchCommand(block: suspend () -> Unit) {
        if (closed.get()) return
        if (!commandActive.compareAndSet(false, true)) return
        scope.launch {
            try {
                block()
            } finally {
                commandActive.set(false)
            }
        }
    }

    /**
     * Beginning a command clears the previous transient error and any seek
     * display override — the chosen MVP error-clearing behavior: a new
     * user command supersedes the last failure.
     */
    private fun beginOperation() {
        _uiState.update {
            it.copy(
                operationInFlight = true,
                error = null,
                seekPreviewUs = null,
                committedSeekUs = null,
            )
        }
    }

    private fun endOperation() {
        _uiState.update { it.copy(operationInFlight = false) }
    }

    private fun failOperation(error: PlayerUiError) {
        _uiState.update { it.copy(operationInFlight = false, error = error) }
    }

    /** Clear the committed-seek display once the snapshot reaches it. */
    private fun clearCaughtUpSeek(state: PlayerUiState, snap: PlayerSnapshot): Long? {
        val committed = state.committedSeekUs ?: return null
        return if (snap.positionUs >= committed - SEEK_CATCHUP_TOLERANCE_US) null else committed
    }

    private fun openError(e: PlayerBridgeException, path: Path): PlayerUiError = when (e) {
        is SourceOpenFailure ->
            PlayerUiError(PlayerUiErrorCategory.FileUnavailable, path.fileName?.toString())
        is OpenFailure ->
            PlayerUiError(PlayerUiErrorCategory.CouldNotOpenTrack, path.fileName?.toString())
        is RuntimeLoadFailure, is AbiMismatch, is EngineCreationFailure ->
            PlayerUiError(PlayerUiErrorCategory.RuntimeUnavailable)
        else ->
            PlayerUiError(PlayerUiErrorCategory.CouldNotOpenTrack, path.fileName?.toString())
    }

    private fun commandError(e: PlayerBridgeException): PlayerUiError = when (e) {
        is ControlFailure -> PlayerUiError(PlayerUiErrorCategory.PlaybackCommandFailed, e.operation)
        else -> PlayerUiError(PlayerUiErrorCategory.PlaybackCommandFailed)
    }

    companion object {
        /**
         * How close a snapshot position must be to a committed seek landing
         * before the UI stops displaying the landing and resumes pure
         * snapshot truth. Small against the 10 Hz poll period; generous
         * against mapping jitter.
         */
        const val SEEK_CATCHUP_TOLERANCE_US: Long = 250_000
    }
}
