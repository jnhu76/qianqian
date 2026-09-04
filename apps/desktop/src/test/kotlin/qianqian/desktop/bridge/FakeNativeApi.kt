package qianqian.desktop.bridge

import com.sun.jna.Pointer
import com.sun.jna.ptr.IntByReference
import com.sun.jna.ptr.LongByReference
import qianqian.desktop.nativebridge.NativeApi
import qianqian.desktop.nativebridge.PeQuality
import qianqian.desktop.nativebridge.PeSnapshot
import qianqian.desktop.nativebridge.PeState
import qianqian.desktop.nativebridge.PeStatus
import qianqian.desktop.nativebridge.QianqianAbi
import qianqian.desktop.nativebridge.SongIo
import java.util.Collections
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong

/**
 * Deterministic in-process [NativeApi] fake for pure JVM bridge tests:
 * records every call, detects illegal control-call overlap (the frozen
 * caller-serialization rule; `pe_get_snapshot` is excluded — the contract
 * allows it concurrently), and simulates the frozen state machine so the
 * adapter's confinement, close semantics, error mapping, and poller
 * lifecycle can be proven without a native runtime.
 */
class FakeNativeApi(
    songAbi: Int = QianqianAbi.SONGCORE_ABI_VERSION,
    engineAbi: Int = QianqianAbi.PLAYER_ENGINE_ABI_VERSION,
) : NativeApi {

    var fakeSongAbi: Int = songAbi
    var fakeEngineAbi: Int = engineAbi

    var createResult: Pointer? = FAKE_ENGINE
    var openStatus: Int = PeStatus.OK
    var openSongStatus: Int = 0
    var playStatus: Int = PeStatus.OK
    var pauseStatus: Int = PeStatus.OK
    var stopStatus: Int = PeStatus.OK
    var stopSongStatus: Int = 0
    var seekStatus: Int = PeStatus.OK
    var seekLandingUs: Long = 123_456L
    var snapshotStatus: Int = PeStatus.OK

    // Simulated engine observation surface.
    var snapshotState: Int = PeState.EMPTY
    var snapshotPositionUs: Long = 0L
    var snapshotDurationUs: Long = 60_000_000L
    var snapshotDurationKnown: Int = 1
    var snapshotQuality: Int = PeQuality.CONFIRMED

    val callLog: MutableList<String> = Collections.synchronizedList(ArrayList())
    val snapshotCallCount = AtomicLong(0)
    val destroyCount = AtomicInteger(0)
    val controlOverlaps: MutableList<String> =
        Collections.synchronizedList(ArrayList())
    private val activeControlCalls = AtomicInteger(0)

    override fun songcoreAbiVersion(): Int = fakeSongAbi
    override fun playerEngineAbiVersion(): Int = fakeEngineAbi

    override fun peCreate(): Pointer? {
        callLog.add("pe_create")
        return createResult
    }

    override fun peDestroy(engine: Pointer) {
        callLog.add("pe_destroy")
        destroyCount.incrementAndGet()
    }

    override fun peOpen(engine: Pointer, io: SongIo, outSongStatus: IntByReference): Int =
        exclusive("pe_open") {
            outSongStatus.value = openSongStatus
            if (openStatus == PeStatus.OK) snapshotState = PeState.READY
            openStatus
        }

    override fun pePlay(engine: Pointer): Int = exclusive("pe_play") {
        if (playStatus == PeStatus.OK) snapshotState = PeState.PLAYING
        playStatus
    }

    override fun pePause(engine: Pointer): Int = exclusive("pe_pause") {
        if (pauseStatus == PeStatus.OK) snapshotState = PeState.PAUSED
        pauseStatus
    }

    override fun peStop(engine: Pointer, outSongStatus: IntByReference): Int =
        exclusive("pe_stop") {
            outSongStatus.value = stopSongStatus
            if (stopStatus == PeStatus.OK) {
                snapshotState = PeState.READY
                snapshotPositionUs = 0
            }
            stopStatus
        }

    override fun peSeek(
        engine: Pointer,
        positionUs: Long,
        outLandingUs: LongByReference,
        outSongStatus: IntByReference,
    ): Int = exclusive("pe_seek") {
        outLandingUs.value = seekLandingUs
        outSongStatus.value = 0
        seekStatus
    }

    override fun peGetSnapshot(engine: Pointer, out: PeSnapshot): Int {
        snapshotCallCount.incrementAndGet()
        callLog.add("pe_get_snapshot")
        if (snapshotStatus != PeStatus.OK) return snapshotStatus
        out.state = snapshotState
        out.positionUs = snapshotPositionUs
        out.durationUs = snapshotDurationUs
        out.durationKnown = snapshotDurationKnown
        out.positionQuality = snapshotQuality.toByte()
        out.sampleRate = 44_100
        return PeStatus.OK
    }

    private fun <T> exclusive(name: String, block: () -> T): T {
        if (activeControlCalls.incrementAndGet() != 1) {
            controlOverlaps.add(name)
        }
        try {
            callLog.add(name)
            return block()
        } finally {
            activeControlCalls.decrementAndGet()
        }
    }

    companion object {
        /** Opaque stand-in handle; never dereferenced. */
        val FAKE_ENGINE: Pointer = Pointer.createConstant(0x51L)
    }
}
