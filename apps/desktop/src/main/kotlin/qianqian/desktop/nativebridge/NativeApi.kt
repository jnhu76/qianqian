package qianqian.desktop.nativebridge

import com.sun.jna.Pointer
import com.sun.jna.ptr.IntByReference
import com.sun.jna.ptr.LongByReference

/**
 * The seam between the Desktop bridge and the native binding technology.
 *
 * [NativePlayerAdapter] and everything above it depends on this interface;
 * the only production implementation is [JnaNativeApi]. Tests substitute a
 * fake to prove ABI-gate fail-fast, control-call serialization, close
 * semantics, and error mapping without a native runtime.
 *
 * Methods map 1:1 to the frozen public C ABI symbols this stage consumes
 * (10 of the 24 exported symbols; no speculative bindings for unused APIs).
 * `pe_engine*` is an opaque [Pointer]: created by [peCreate], destroyed
 * exactly once by [peDestroy], never dereferenced.
 */
interface NativeApi {
    fun songcoreAbiVersion(): Int
    fun playerEngineAbiVersion(): Int

    /** `pe_create(NULL config)` — the internal default queue. */
    fun peCreate(): Pointer?

    /** `pe_destroy` — NULL is a documented no-op; called exactly once. */
    fun peDestroy(engine: Pointer)

    fun peOpen(engine: Pointer, io: SongIo, outSongStatus: IntByReference): Int
    fun pePlay(engine: Pointer): Int
    fun pePause(engine: Pointer): Int
    fun peStop(engine: Pointer, outSongStatus: IntByReference): Int

    /** `pe_seek`; [outLandingUs] receives the landing (-1 when unknown). */
    fun peSeek(
        engine: Pointer,
        positionUs: Long,
        outLandingUs: LongByReference,
        outSongStatus: IntByReference,
    ): Int

    fun peGetSnapshot(engine: Pointer, out: PeSnapshot): Int
}
