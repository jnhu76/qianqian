package qianqian.desktop.player

import kotlinx.coroutines.flow.StateFlow
import java.nio.file.Path

/**
 * The one narrow application-side player surface over the native runtime.
 *
 * Native state is the single truth: every method issues the corresponding
 * native control call and reports the native result; nothing here invents
 * transitions (no optimistic "set PLAYING before native confirms").
 * [snapshot] is the observation channel (5–10 Hz poller underneath).
 *
 * This port exists so application code and tests can sit above the native
 * adapter deterministically; it deliberately exposes no queue, playlist,
 * library, or navigation policy.
 */
interface PlayerPort {
    /** Latest polled native snapshot. */
    val snapshot: StateFlow<PlayerSnapshot>

    /**
     * Open a local audio file at position 0 (native `pe_open`: never
     * autoplays; state READY on success).
     *
     * @return the SongCore status code carried by the call (`SONG_OK` on
     * success); failures throw [OpenFailure].
     */
    suspend fun open(source: Path)

    /** Native `pe_play`. */
    suspend fun play()

    /** Native `pe_pause`. */
    suspend fun pause()

    /**
     * Native `pe_seek` to [positionUs] (media microseconds).
     * @return the landing the engine rebased on, in media microseconds,
     * or -1 when the landing is genuinely unknown (ESTIMATED).
     */
    suspend fun seek(positionUs: Long): Long

    /** Native `pe_stop`: deterministic rebuild to READY @0. */
    suspend fun stop()

    /**
     * Close the adapter exactly once: stops the snapshot poller, destroys
     * the engine handle, releases the file source and the control
     * dispatcher. Subsequent close calls are no-ops; all other commands
     * afterwards throw [BridgeClosedException].
     */
    suspend fun close()
}
