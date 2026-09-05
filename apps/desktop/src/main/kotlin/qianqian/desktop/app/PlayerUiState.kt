package qianqian.desktop.app

import qianqian.desktop.player.PlayerSnapshot
import qianqian.desktop.player.PlayerState
import java.nio.file.Path

/**
 * The application projection for the player screen — everything the UI
 * renders, and nothing else.
 *
 * Playback truth stays native: [snapshot] is the latest `PlayerSnapshot`
 * verbatim, and nothing in this class re-derives or overrides a playback
 * state. [seekPreviewUs] / [committedSeekUs] are timeline DISPLAY values,
 * clearly separated from snapshot truth:
 *
 *  - [seekPreviewUs] — where the user is currently dragging (never sent
 *    to native during the drag).
 *  - [committedSeekUs] — the landing the engine itself reported from the
 *    last committed seek, shown until a snapshot lands inside that
 *    position's window (so the slider does not snap back between commit
 *    and the next 10 Hz poll). Display reconciliation only — not a claim
 *    that a snapshot causally confirms the seek.
 *
 * Legal-action booleans are a small enablement projection of the native
 * state per `docs/contracts/player-api.md` — a projection, not a second
 * state machine.
 */
data class PlayerUiState(
    /** Last file successfully opened (a failed candidate is NOT promoted). */
    val selectedFile: Path? = null,
    val snapshot: PlayerSnapshot = PlayerSnapshot.empty(),
    val operationInFlight: Boolean = false,
    val seekPreviewUs: Long? = null,
    val committedSeekUs: Long? = null,
    val error: PlayerUiError? = null,
) {
    /** The timeline position the UI should display right now. */
    val displayedPositionUs: Long
        get() = seekPreviewUs ?: committedSeekUs ?: snapshot.positionUs

    val canOpenFile: Boolean get() = !operationInFlight

    /** One toggle control: Play from READY/PAUSED/ENDED, Pause from PLAYING. */
    val canTogglePlayback: Boolean
        get() = !operationInFlight &&
            snapshot.state in TOGGLEABLE_STATES

    /** `stop` is legal from every state except EMPTY (contract table). */
    val canStop: Boolean
        get() = !operationInFlight && snapshot.state != PlayerState.EMPTY

    /**
     * Seek needs a known positive duration and a seekable state; unknown
     * duration never renders as a fake 0-length slider.
     */
    val canSeek: Boolean
        get() = !operationInFlight &&
            snapshot.durationKnown &&
            snapshot.durationUs > 0 &&
            snapshot.state in SEEKABLE_STATES

    val isPlaying: Boolean get() = snapshot.state == PlayerState.PLAYING

    companion object {
        /** States where the play/pause toggle maps to a legal native call. */
        private val TOGGLEABLE_STATES = setOf(
            PlayerState.READY,
            PlayerState.PLAYING,
            PlayerState.PAUSED,
            PlayerState.ENDED,
        )

        private val SEEKABLE_STATES = setOf(
            PlayerState.READY,
            PlayerState.PLAYING,
            PlayerState.PAUSED,
            PlayerState.ENDED,
        )
    }
}

/** Human-readable state label (text, never color-only). */
fun stateLabel(state: PlayerState): String = when (state) {
    PlayerState.EMPTY -> "No track"
    PlayerState.READY -> "Ready"
    PlayerState.PLAYING -> "Playing"
    PlayerState.PAUSED -> "Paused"
    PlayerState.ENDED -> "Ended"
    PlayerState.ERROR -> "Error"
}

/**
 * Product-visible error classification. Typed, minimal categories —
 * machine logic branches only on these; native diagnostic strings
 * ([PlayerSnapshot.lastError], exception messages) are never parsed.
 */
enum class PlayerUiErrorCategory {
    /** The file could not be read JVM-side (missing/unreadable path). */
    FileUnavailable,

    /** `pe_open` rejected the file (unsupported/invalid/corrupt/other). */
    CouldNotOpenTrack,

    /** A play/pause/seek/stop command failed. */
    PlaybackCommandFailed,

    /** The native runtime itself is unavailable (load/ABI/engine). */
    RuntimeUnavailable,

    /** The platform file dialog could not be shown. */
    FileDialogFailed,
}

/**
 * One product error. [detail] is optional, human-oriented context (e.g.
 * the candidate file name); it is display text, never machine input.
 */
data class PlayerUiError(
    val category: PlayerUiErrorCategory,
    val detail: String? = null,
) {
    val message: String
        get() = when (category) {
            PlayerUiErrorCategory.FileUnavailable ->
                withDetail("Could not read this file.")
            PlayerUiErrorCategory.CouldNotOpenTrack ->
                withDetail("Could not open this file.")
            PlayerUiErrorCategory.PlaybackCommandFailed -> "Playback command failed."
            PlayerUiErrorCategory.RuntimeUnavailable -> "Playback runtime unavailable."
            PlayerUiErrorCategory.FileDialogFailed -> "Could not open the file chooser."
        }

    private fun withDetail(base: String): String =
        if (detail.isNullOrBlank()) base else "$base ($detail)"
}
