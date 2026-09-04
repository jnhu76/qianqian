package qianqian.desktop.player

/**
 * Application-safe projection of one native `pe_snapshot` instant.
 *
 * Units are the native media timeline units (microseconds) preserved without
 * conversion — [positionUs] / [durationUs] are `pe_snapshot`'s values as the
 * engine reported them; `durationUs == -1` keeps the native meaning
 * "unknown" (never remapped to 0). Raw JNA structures never escape the
 * bridge; this is the only shape the application sees.
 */
data class PlayerSnapshot(
    val state: PlayerState,
    val positionUs: Long,
    val durationUs: Long,
    val durationKnown: Boolean,
    val positionEstimated: Boolean,
    val sampleRate: Int,
    val bufferedFrames: Long,
    val underrunCount: Long,
    /** Diagnostic only ("", or a normalized native category); never parsed. */
    val lastError: String,
) {
    companion object {
        /** The snapshot of an engine that has no song yet (`PE_STATE_EMPTY`). */
        fun empty(): PlayerSnapshot = PlayerSnapshot(
            state = PlayerState.EMPTY,
            positionUs = 0L,
            durationUs = -1L,
            durationKnown = false,
            positionEstimated = false,
            sampleRate = 0,
            bufferedFrames = 0L,
            underrunCount = 0L,
            lastError = "",
        )
    }
}

/** Frozen public `pe_state` values, projected 1:1 from native truth. */
enum class PlayerState {
    EMPTY,
    READY,
    PLAYING,
    PAUSED,
    ENDED,
    ERROR,
}

/** `PE_QUALITY_*`: whether the reported position is anchored on a real
 *  SongCore landing (CONFIRMED) or on the requested target (ESTIMATED). */
enum class PositionQuality {
    CONFIRMED,
    ESTIMATED,
}
