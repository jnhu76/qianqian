package qianqian.desktop.nativebridge

import com.sun.jna.Callback

import com.sun.jna.Pointer
import com.sun.jna.Structure

/**
 * JNA mappings for the frozen public C ABI
 * (`native/include/songcore.h` + `native/include/player_engine.h`, v1).
 * This file mirrors header layout only; no private native knowledge.
 *
 * Type mapping (64-bit Linux/Windows, proven against the headers):
 *  - `uint32_t` / enum  -> Kotlin `Int` (frozen values are small, non-negative)
 *  - `int64_t`/`uint64_t` -> Kotlin `Long`
 *  - `size_t`           -> Kotlin `Long` (64-bit on both x86-64 targets)
 *  - `void*`            -> `Pointer`
 *  - `uint8_t`          -> `Byte`
 *  - fixed char arrays  -> `ByteArray`
 *  - function pointers  -> [Callback] subinterfaces (single `invoke` method)
 *  - `pe_engine*`       -> opaque `Pointer` (never dereferenced)
 *
 * Calling convention: the headers export plain C functions with no
 * `__stdcall`/`WINAPI` decoration; x86-64 Windows has a single calling
 * convention and Linux x86-64 uses the SysV C ABI — JNA's default
 * (cdecl-equivalent) mapping is correct on both.
 */

/** Frozen ABI version gates (songcore.h / player_engine.h). */
object QianqianAbi {
    const val SONGCORE_ABI_VERSION: Int = 1
    const val PLAYER_ENGINE_ABI_VERSION: Int = 1
}

/** Frozen `pe_status` values (player_engine.h). Machine logic uses these. */
object PeStatus {
    const val OK: Int = 0
    const val ERR_ILLEGAL_CALL: Int = 1
    const val ERR_OPEN_FAILED: Int = 2
    const val ERR_SEEK_FAILED: Int = 3
    const val ERR_INVALID_ARGUMENT: Int = 4
    const val ERR_NO_MEMORY: Int = 5
    const val ERR_INTERNAL: Int = 6

    fun name(status: Int): String = when (status) {
        OK -> "PE_OK"
        ERR_ILLEGAL_CALL -> "PE_ERR_ILLEGAL_CALL"
        ERR_OPEN_FAILED -> "PE_ERR_OPEN_FAILED"
        ERR_SEEK_FAILED -> "PE_ERR_SEEK_FAILED"
        ERR_INVALID_ARGUMENT -> "PE_ERR_INVALID_ARGUMENT"
        ERR_NO_MEMORY -> "PE_ERR_NO_MEMORY"
        ERR_INTERNAL -> "PE_ERR_INTERNAL"
        else -> "PE_STATUS_$status"
    }
}

/** Frozen `pe_state` numeric values (player_engine.h). */
object PeState {
    const val EMPTY: Int = 0
    const val READY: Int = 1
    const val PLAYING: Int = 2
    const val PAUSED: Int = 3
    const val ENDED: Int = 4
    const val ERROR: Int = 5
}

/** Frozen `PE_QUALITY_*` values (player_engine.h). */
object PeQuality {
    const val CONFIRMED: Int = 0
    const val ESTIMATED: Int = 1
}

/** `song_read_fn`: returns bytes read (>0), 0 at EOF, <0 on host error. */
interface SongReadFn : Callback {
    fun invoke(userdata: Pointer?, dst: Pointer?, size: Long): Long
}

/** `song_seek_fn`: returns the resulting absolute offset, <0 on host error. */
interface SongSeekFn : Callback {
    fun invoke(userdata: Pointer?, absoluteOffset: Long): Long
}

/** `song_size_fn`: returns total source size in bytes, <0 when unavailable. */
interface SongSizeFn : Callback {
    fun invoke(userdata: Pointer?): Long
}

/**
 * C `song_io` — field order, widths, and alignment mirror songcore.h.
 * The instance passed to `pe_open` must stay strongly referenced by the
 * caller for the duration of the call; the engine copies its contents.
 */
open class SongIo : Structure {
    @JvmField var userdata: Pointer? = null
    @JvmField var read: SongReadFn? = null
    @JvmField var seek: SongSeekFn? = null
    @JvmField var size: SongSizeFn? = null

    constructor() : super()
    constructor(p: Pointer) : super(p)

    override fun getFieldOrder(): List<String> =
        listOf("userdata", "read", "seek", "size")
}

/**
 * C `pe_snapshot` — one coherent polled instant. Field order mirrors
 * player_engine.h; `position_quality` is a 1-byte field followed by a
 * 64-bit field, the platform C alignment rules apply and match JNA's
 * default alignment on both target platforms.
 */
open class PeSnapshot : Structure {
    @JvmField var state: Int = 0
    @JvmField var positionUs: Long = 0
    @JvmField var durationUs: Long = 0
    @JvmField var durationKnown: Int = 0
    @JvmField var positionQuality: Byte = 0
    @JvmField var bufferedFrames: Long = 0
    @JvmField var underrunCount: Long = 0
    @JvmField var sampleRate: Int = 0
    @JvmField var lastError: ByteArray = ByteArray(96)

    constructor() : super()
    constructor(p: Pointer) : super(p)

    override fun getFieldOrder(): List<String> = listOf(
        "state", "positionUs", "durationUs", "durationKnown",
        "positionQuality", "bufferedFrames", "underrunCount",
        "sampleRate", "lastError",
    )
}
