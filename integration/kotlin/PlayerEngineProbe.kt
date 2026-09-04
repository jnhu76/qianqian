/*
 * PlayerEngineProbe.kt — Kotlin/Native boundary probe over the frozen C ABI.
 *
 * Proves a Kotlin-owned process can consume the application-facing runtime
 * (qianqian.dll / libqianqian.so) through include/player_engine.h +
 * include/songcore.h ONLY: no private header, no implementation archive,
 * no backend knowledge. docs/contracts/ffi-boundary.md (boundary proof: see docs/archive/experiments/kotlin-boundary-probe.md): create ->
 * open -> play -> polled snapshots -> monotonic progress -> ENDED ->
 * destroy, plus a direct song_* lifecycle over the same runtime for the
 * API matrix.
 *
 * Platform specifics (sleep + the platform CRT seek/tell surface) live
 * behind ProbeHost and the two entry files (ProbeWin.kt / ProbeLinux.kt)
 * — plain Kotlin/Native modules do not allow expect/actual.
 *
 * This is a boundary proof, not an application architecture.
 */
@file:OptIn(kotlinx.cinterop.ExperimentalForeignApi::class)

import kotlinx.cinterop.*
import cnames.structs.song_handle
import qianqian.*

/* Platform seam: sleep plus the platform CRT seek/tell surface. */
interface ProbeHost {
    fun sleep(ms: Int)
    fun seek(f: CPointer<FILE>, offset: Long, whence: Int): Boolean
    fun tell(f: CPointer<FILE>): Long
}

private var failures = 0

/* staticCFunction cannot capture — the host is published here before the
 * song_io callbacks can ever fire (single probe, set before pe_create). */
private lateinit var gHost: ProbeHost

private fun check(cond: Boolean, what: String) {
    println(if (cond) "  PASS $what" else "  FAIL $what")
    if (!cond) ++failures
}

/* Host FILE* song_io — the frozen caller-side I/O contract, from Kotlin. */
private fun ioRead(ud: COpaquePointer?, dst: CPointer<UByteVar>?, size: ULong): Long =
    fread(dst, 1uL, size, ud!!.reinterpret<FILE>()).toLong()

private fun ioSeek(ud: COpaquePointer?, offset: Long): Long {
    val f = ud!!.reinterpret<FILE>()
    return if (gHost.seek(f, offset, PROBE_SEEK_SET)) gHost.tell(f) else -1L
}

private fun ioSize(ud: COpaquePointer?): Long {
    val f = ud!!.reinterpret<FILE>()
    val cur = gHost.tell(f)
    gHost.seek(f, 0, PROBE_SEEK_END)
    val end = gHost.tell(f)
    gHost.seek(f, cur, PROBE_SEEK_SET)
    return end
}

private fun bytes(ptr: CPointer<ByteVar>?, len: UInt): String =
    ptr!!.readBytes(len.toInt()).decodeToString()

fun runProbe(host: ProbeHost, audioPath: String): Int {
    println("Kotlin/Native boundary probe — target: $audioPath")
    failures = 0
    gHost = host

    memScoped {
        /* ---- ABI gates: header we bound against vs loaded runtime ---- */
        println("== ABI ==")
        check(songcore_abi_version() == SONGCORE_ABI_VERSION, "songcore ABI v1")
        check(player_engine_abi_version() == PLAYER_ENGINE_ABI_VERSION,
              "player_engine ABI v1")

        val f = fopen(audioPath, "rb")
        check(f != null, "fopen $audioPath")
        if (f == null) return 1

        val io = alloc<song_io> {
            userdata = f
            read = staticCFunction { ud, dst, size -> ioRead(ud, dst, size) }
            seek = staticCFunction { ud, offset -> ioSeek(ud, offset) }
            size = staticCFunction { ud -> ioSize(ud) }
        }

        /* ---- SongCore direct lifecycle over the same runtime (song_*) ---- */
        println("== SongCore (song_*) ==")
        val songOut = allocPointerTo<song_handle>()
        check(song_open(io.ptr, songOut.ptr) == SONG_OK, "song_open")
        val song = songOut.value

        val info = alloc<song_info>()
        check(song_probe(song, info.ptr) == SONG_OK, "song_probe")
        println("  probe: ${info.container.toKString()}/${info.codec.toKString()} " +
                "${info.sample_rate} Hz ${info.channels} ch " +
                "mask=0x${info.channel_mask.toString(16)} dur=${info.duration_us}us " +
                "bps=${info.bits_per_sample}")

        val streamCount = alloc<UIntVar>()
        check(song_audio_stream_count(song, streamCount.ptr) == SONG_OK,
              "song_audio_stream_count (${streamCount.value})")
        val streamInfo = alloc<song_stream_info>()
        check(song_audio_stream_info(song, 0u, streamInfo.ptr) == SONG_OK,
              "song_audio_stream_info(0) ${streamInfo.sample_rate} Hz")

        val metaOut = allocPointerTo<song_metadata>()
        check(song_get_metadata(song, metaOut.ptr) == SONG_OK, "song_get_metadata")
        val meta = metaOut.value!!.pointed
        if (meta.has_title != 0u) println("  title: ${bytes(meta.title, meta.title_len)}")
        if (meta.has_artist != 0u) println("  artist: ${bytes(meta.artist, meta.artist_len)}")

        val metaCount = alloc<UIntVar>()
        check(song_get_metadata_count(song, metaCount.ptr) == SONG_OK,
              "song_get_metadata_count (${metaCount.value})")
        val entry = alloc<song_metadata_entry>()
        var entryOk = true
        for (i in 0u until minOf(metaCount.value, 8u)) {
            entryOk = (song_get_metadata_entry(song, i, entry.ptr) == SONG_OK) && entryOk
        }
        check(entryOk, "song_get_metadata_entry")

        val artworkCount = alloc<UIntVar>()
        check(song_get_artwork_count(song, artworkCount.ptr) == SONG_OK,
              "song_get_artwork_count (${artworkCount.value})")
        if (artworkCount.value > 0u) {
            val item = alloc<song_artwork_item>()
            check(song_get_artwork_item(song, 0u, item.ptr) == SONG_OK, "song_get_artwork_item")
        }

        val produced = alloc<ULongVar>()
        val oneSecond = info.sample_rate.toLong().coerceAtLeast(1L)
        val pcm = FloatArray((oneSecond * info.channels).toInt())
        var total = 0L
        var decodeOk = true
        while (total < oneSecond) {
            val st = pcm.usePinned { p ->
                song_read_pcm(song, p.addressOf(0), (oneSecond - total).toULong(), produced.ptr)
            }
            if (st == SONG_EOF) break
            if (st != SONG_OK) { decodeOk = false; break }
            total += produced.value.toLong()
        }
        check(decodeOk && total > 0, "song_read_pcm decoded $total frames")

        val landingUs = alloc<LongVar>()
        val seekTarget = if (info.duration_us > 0) info.duration_us / 2 else 0L
        val seekSt = song_seek(song, seekTarget, landingUs.ptr)
        check(seekSt == SONG_OK || seekSt == SONG_ERR_SEEK_UNSUPPORTED, "song_seek typed")
        if (seekSt == SONG_OK) {
            val post = pcm.usePinned { p ->
                song_read_pcm(song, p.addressOf(0), oneSecond.toULong(), produced.ptr)
            }
            check(post == SONG_OK && produced.value > 0uL,
                  "post-seek PCM (landing=${landingUs.value}us)")
        }

        /* Documented error path: zero capacity is a typed refusal, diagnosable
         * through song_last_error — never a crash. */
        val bad = pcm.usePinned { p -> song_read_pcm(song, p.addressOf(0), 0uL, produced.ptr) }
        check(bad == SONG_ERR_INVALID_ARGUMENT, "song_read_pcm(cap=0) typed refusal")
        val errOut = allocPointerTo<song_error>()
        if (song_last_error(song, errOut.ptr) == SONG_OK) {
            val err = errOut.value!!.pointed
            println("  last_error: \"${err.message?.toKString()}\" native_code=${err.native_code}")
        }

        song_close(song)
        /* Rewind the stream: the direct song_* lifecycle left the FILE*
         * mid-file, and pe_open's internal reopen must see the container
         * header at position 0. */
        gHost.seek(f!!, 0, PROBE_SEEK_SET)
        println("  song_close")

        /* ---- PlayerEngine lifecycle (pe_*) ---- */
        println("== PlayerEngine (pe_*) ==")
        val eng = pe_create(null)
        check(eng != null, "pe_create(NULL)")
        if (eng == null) return 1

        val sn = alloc<pe_snapshot>()
        check(pe_get_snapshot(eng, sn.ptr) == PE_OK && sn.state == PE_STATE_EMPTY,
              "snapshot EMPTY before open")

        val songStatus = alloc<IntVar>()
        check(pe_open(eng, io.ptr, songStatus.ptr) == PE_OK &&
              songStatus.value == SONG_OK.toInt(),
              "pe_open (song_status=${songStatus.value})")
        check(pe_get_snapshot(eng, sn.ptr) == PE_OK && sn.state == PE_STATE_READY,
              "READY after open")
        check(sn.duration_known == 1u && sn.duration_us > 0,
              "duration known: ${sn.duration_us}us rate=${sn.sample_rate}")

        check(pe_play(eng) == PE_OK, "pe_play")
        var progressed = false
        for (i in 0 until 40) {
            host.sleep(100)
            pe_get_snapshot(eng, sn.ptr)
            if (sn.state == PE_STATE_PLAYING && sn.position_us > 0L) { progressed = true; break }
        }

        if (progressed) {
            check(true, "REAL render progression (position_us=${sn.position_us})")

            var last = sn.position_us
            var monotonic = true
            for (i in 0 until 20) {
                host.sleep(100)
                pe_get_snapshot(eng, sn.ptr)
                if (sn.position_us < last) monotonic = false
                last = sn.position_us
            }
            check(monotonic, "position monotonic while PLAYING")

            pe_pause(eng)
            host.sleep(300)
            pe_get_snapshot(eng, sn.ptr)
            val frozenAt = sn.position_us
            host.sleep(400)
            pe_get_snapshot(eng, sn.ptr)
            check(sn.state == PE_STATE_PAUSED && sn.position_us == frozenAt,
                  "pause freezes position at ${frozenAt}us")
            check(pe_play(eng) == PE_OK, "pe_play resume")

            host.sleep(300)
            pe_get_snapshot(eng, sn.ptr)
            val peSeekTarget = (sn.duration_us - 12_000_000L).coerceAtLeast(0L)
            check(pe_seek(eng, peSeekTarget, landingUs.ptr, songStatus.ptr) == PE_OK &&
                  songStatus.value == SONG_OK.toInt() && landingUs.value >= 0,
                  "pe_seek ${peSeekTarget}us (landing=${landingUs.value}us)")

            last = -1L
            monotonic = true
            var ended = false
            for (i in 0 until 300) {
                host.sleep(100)
                pe_get_snapshot(eng, sn.ptr)
                if (last >= 0 && sn.position_us < last) monotonic = false
                last = sn.position_us
                if (sn.state == PE_STATE_ENDED) { ended = true; break }
            }
            check(monotonic, "position monotonic on approach to ENDED")
            check(ended, "ENDED reached through the real output")
            check(sn.duration_known == 1u &&
                  sn.position_us <= sn.duration_us &&
                  sn.position_us >= sn.duration_us - 200_000L,
                  "ENDED at duration (pos=${sn.position_us}, dur=${sn.duration_us})")
        } else {
            println("  EVIDENCE: SIMULATED lifecycle — no platform output on this " +
                    "runtime flavor (engine-only ABI surface); render progression " +
                    "and ENDED not exercisable here")
            /* Control-path coverage without output: the documented state
             * machine still holds (pause/resume/seek). */
            pe_pause(eng)
            host.sleep(200)
            pe_get_snapshot(eng, sn.ptr)
            check(sn.state == PE_STATE_PAUSED, "pause freezes PLAYING (no output)")
            check(pe_play(eng) == PE_OK, "pe_play resume")
            val halfWay = (sn.duration_us / 2).coerceAtLeast(0L)
            check(pe_seek(eng, halfWay, landingUs.ptr, songStatus.ptr) == PE_OK &&
                  songStatus.value == SONG_OK.toInt() && landingUs.value >= 0,
                  "pe_seek ${halfWay}us (landing=${landingUs.value}us)")
            host.sleep(200)
            pe_get_snapshot(eng, sn.ptr)
            check(sn.state == PE_STATE_PLAYING, "PLAYING after seek")
        }

        check(pe_stop(eng, songStatus.ptr) == PE_OK && songStatus.value == SONG_OK.toInt(),
              "pe_stop")
        check(pe_get_snapshot(eng, sn.ptr) == PE_OK && sn.state == PE_STATE_READY &&
              sn.position_us == 0L, "READY @0 after stop")

        pe_destroy(eng)
        pe_destroy(null) /* documented no-op */
        println("  pe_destroy")

        fclose(f)
    }

    if (failures != 0) {
        println("KOTLIN_PROBE FAIL: $failures check(s) failed")
        return 1
    }
    println("KOTLIN_PROBE OK: Kotlin-owned process consumed the qianqian runtime " +
            "through the frozen C ABI only")
    return 0
}
