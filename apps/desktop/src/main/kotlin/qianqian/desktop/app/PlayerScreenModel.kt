package qianqian.desktop.app

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelAndJoin
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
     * The model-owned job hosting the snapshot projection and every
     * user-command coroutine. Parented on the caller's job; [close]
     * cancels and drains it BEFORE the port closes.
     */
    private val modelJob = SupervisorJob(parentScope.coroutineContext[Job])
    private val scope = CoroutineScope(modelJob + Dispatchers.Default)

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
                        // A committed seek stays displayed until a snapshot
                        // lands inside its window; then snapshot truth
                        // resumes. Display reconciliation only — the ABI
                        // carries no seek generation to causally confirm.
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
        } catch (e: CancellationException) {
            throw e // shutdown cancellation is control flow, not a product error
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
     * succeeds → READY, never autoplay. Source identity mirrors native
     * ownership truth:
     *
     *  - [SourceOpenFailure] (JVM read failed BEFORE `pe_open`): the
     *    previous source is untouched and stays displayed; only the
     *    candidate failed.
     *  - [OpenFailure] (`pe_open` ran and failed): `pe_open` is
     *    destructive — native dropped the previous source and owns
     *    nothing, so the projection stops displaying it as current.
     *
     * A failed candidate is never promoted to [PlayerUiState.selectedFile].
     */
    suspend fun openFile(path: Path) {
        beginOperation()
        try {
            port.open(path)
        } catch (e: BridgeClosedException) {
            return // shutting down; not a product error
        } catch (e: CancellationException) {
            throw e // shutdown cancellation is control flow, not a product error
        } catch (e: SourceOpenFailure) {
            failOperation(
                PlayerUiError(PlayerUiErrorCategory.FileUnavailable, path.fileName?.toString()),
            )
            return
        } catch (e: PlayerBridgeException) {
            // pe_open already ran and failed → native EMPTY, bridge owns no
            // source. The old file must not stay displayed as current.
            _uiState.update {
                it.copy(selectedFile = null, seekPreviewUs = null, committedSeekUs = null)
            }
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
     * Non-suspending user entrypoint: the model owns the command coroutine
     * and its admission.
     */
    fun togglePlayback() = launchCommand {
        val state = _uiState.value.snapshot.state
        if (state == PlayerState.EMPTY || state == PlayerState.ERROR) return@launchCommand
        beginOperation()
        try {
            when (state) {
                PlayerState.PLAYING -> port.pause()
                else -> port.play() // READY, PAUSED, ENDED (ENDED replays from 0)
            }
        } catch (e: BridgeClosedException) {
            return@launchCommand // shutting down
        } catch (e: PlayerBridgeException) {
            failOperation(commandError(e))
            return@launchCommand
        }
        endOperation()
    }

    /**
     * Native `stop` → READY @0; the UI follows the snapshot, no local reset.
     * Non-suspending user entrypoint: the model owns the command coroutine
     * and its admission.
     */
    fun stopPlayback() = launchCommand {
        beginOperation()
        try {
            port.stop()
        } catch (e: BridgeClosedException) {
            return@launchCommand // shutting down
        } catch (e: PlayerBridgeException) {
            failOperation(commandError(e))
            return@launchCommand
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
     * landing stays displayed until a snapshot reaches the committed
     * landing window (or the requested target when the landing is
     * genuinely unknown) — display reconciliation, not a causal fence:
     * the snapshot ABI carries no seek generation. On failure the preview
     * is cleared, the error is shown, and the native snapshot remains the
     * timeline authority.
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
     * Shut down exactly once, in a strict order: refuse new commands →
     * cancel and DRAIN every model-owned coroutine (commands + snapshot
     * projection) → close the port last, so no command can touch it after
     * (or during) teardown. The port's own close is idempotent;
     * [NonCancellable] keeps the teardown running even when the caller's
     * scope is being cancelled. Must not be called from a coroutine the
     * model itself owns (draining would wait on the caller).
     */
    suspend fun close() {
        if (!closed.compareAndSet(false, true)) return
        modelJob.cancelAndJoin()
        withContext(NonCancellable) { port.close() }
    }

    // ---- internals -------------------------------------------------------

    /**
     * One-command-at-a-time admission, enforced synchronously at the
     * entrypoint so a click storm cannot stack user commands: at most one
     * application-level user command is admitted; later ones are dropped
     * until it finishes. The native bridge remains the serialization
     * authority; this is only click-spam sanity.
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

    /**
     * Clear the committed-seek display once a snapshot lands inside its
     * window (symmetric tolerance). Until then the landing stays shown —
     * including against a stale pre-seek snapshot that is HIGHER than the
     * landing after a backward seek. Positions are non-negative
     * microseconds, so this subtraction cannot overflow.
     */
    private fun clearCaughtUpSeek(state: PlayerUiState, snap: PlayerSnapshot): Long? {
        val committed = state.committedSeekUs ?: return null
        val delta = if (snap.positionUs >= committed) {
            snap.positionUs - committed
        } else {
            committed - snap.positionUs
        }
        return if (delta <= SEEK_CATCHUP_TOLERANCE_US) null else committed
    }

    private fun openError(e: PlayerBridgeException, path: Path): PlayerUiError = when (e) {
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
