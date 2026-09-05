package qianqian.desktop.app

/**
 * Minimal media-time display formatting (mm:ss, h:mm:ss for tracks >= 1 h).
 *
 * Input is the native media unit (microseconds) — no locale/time-zone
 * machinery, pure arithmetic. Negative input means "unknown" and formats
 * as [UNKNOWN]; seconds are floored (0.9 s shows as 0:00), matching
 * standard media-player display behavior.
 */
object TimeFormat {

    /** Displayed in place of a value that is not known (`-1` from native). */
    const val UNKNOWN = "--:--"

    fun formatUs(micros: Long): String {
        if (micros < 0) return UNKNOWN
        val totalSeconds = micros / 1_000_000L
        val hours = totalSeconds / 3_600
        val minutes = (totalSeconds % 3_600) / 60
        val seconds = totalSeconds % 60
        return if (hours > 0) {
            "%d:%02d:%02d".format(hours, minutes, seconds)
        } else {
            "%d:%02d".format(minutes, seconds)
        }
    }
}
