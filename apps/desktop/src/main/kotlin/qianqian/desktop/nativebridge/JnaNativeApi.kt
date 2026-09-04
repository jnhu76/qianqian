package qianqian.desktop.nativebridge

import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import com.sun.jna.ptr.IntByReference
import com.sun.jna.ptr.LongByReference
import qianqian.desktop.player.RuntimeLoadFailure
import java.nio.file.Path

/**
 * Raw JNA interface over the frozen public ABI. Only symbols declared in
 * `songcore.h` / `player_engine.h` are bound; symbol names match the
 * headers exactly so the public-ABI-only rule is reviewable from this file.
 */
internal interface QianqianLibrary : Library {
    fun songcore_abi_version(): Int
    fun player_engine_abi_version(): Int
    fun pe_create(config: Pointer?): Pointer?
    fun pe_destroy(engine: Pointer)
    fun pe_open(engine: Pointer, io: SongIo, out_song_status: IntByReference): Int
    fun pe_play(engine: Pointer): Int
    fun pe_pause(engine: Pointer): Int
    fun pe_stop(engine: Pointer, out_song_status: IntByReference): Int
    fun pe_seek(
        engine: Pointer,
        position_us: Long,
        out_landing_us: LongByReference,
        out_song_status: IntByReference,
    ): Int

    fun pe_get_snapshot(engine: Pointer, out: PeSnapshot): Int
}

/**
 * JNA-backed [NativeApi]. The library is loaded from an explicit absolute
 * path (`Native.load` dlopens exactly that file; no `jna.library.path`,
 * `PATH`, or current-directory fallback participates), then ABI-gated.
 */
class JnaNativeApi internal constructor(
    private val lib: QianqianLibrary,
    val loadedFrom: Path,
) : NativeApi {

    companion object {
        /** Load and ABI-gate the runtime at [libraryPath]. */
        fun load(libraryPath: Path): JnaNativeApi {
            val absolute = libraryPath.toAbsolutePath().normalize()
            if (!java.nio.file.Files.isRegularFile(absolute)) {
                throw RuntimeLoadFailure(
                    absolute.toString(),
                    "staged runtime not found (run the stageNativeRuntime task)",
                )
            }
            val lib = try {
                Native.load(absolute.toString(), QianqianLibrary::class.java)
            } catch (e: UnsatisfiedLinkError) {
                throw RuntimeLoadFailure(absolute.toString(), e.message ?: "UnsatisfiedLinkError")
            }
            return JnaNativeApi(lib, absolute)
        }
    }

    override fun songcoreAbiVersion(): Int = lib.songcore_abi_version()
    override fun playerEngineAbiVersion(): Int = lib.player_engine_abi_version()
    override fun peCreate(): Pointer? = lib.pe_create(null)
    override fun peDestroy(engine: Pointer) = lib.pe_destroy(engine)
    override fun peOpen(engine: Pointer, io: SongIo, outSongStatus: IntByReference): Int =
        lib.pe_open(engine, io, outSongStatus)

    override fun pePlay(engine: Pointer): Int = lib.pe_play(engine)
    override fun pePause(engine: Pointer): Int = lib.pe_pause(engine)
    override fun peStop(engine: Pointer, outSongStatus: IntByReference): Int =
        lib.pe_stop(engine, outSongStatus)

    override fun peSeek(
        engine: Pointer,
        positionUs: Long,
        outLandingUs: LongByReference,
        outSongStatus: IntByReference,
    ): Int = lib.pe_seek(engine, positionUs, outLandingUs, outSongStatus)

    override fun peGetSnapshot(engine: Pointer, out: PeSnapshot): Int =
        lib.pe_get_snapshot(engine, out)
}
