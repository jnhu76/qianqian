package qianqian.desktop.app

import kotlinx.coroutines.flow.MutableStateFlow
import qianqian.desktop.player.PlayerBridgeException
import qianqian.desktop.player.PlayerPort
import qianqian.desktop.player.PlayerSnapshot
import java.nio.file.Path
import java.util.Collections
import java.util.concurrent.atomic.AtomicInteger

/**
 * The smallest application-test fake over [PlayerPort]: records commands,
 * publishes ONLY the snapshots a test explicitly injects via [emit],
 * and fails operations when a test configures a typed error. It simulates
 * no playback engine semantics.
 */
class FakePlayerPort : PlayerPort {

    override val snapshot = MutableStateFlow(PlayerSnapshot.empty())

    val openCalls: MutableList<Path> = Collections.synchronizedList(ArrayList())
    val playCalls = AtomicInteger(0)
    val pauseCalls = AtomicInteger(0)
    val stopCalls = AtomicInteger(0)
    val seekCalls = AtomicInteger(0)
    val closeCalls = AtomicInteger(0)

    /** Landing value `seek` reports on success (native: actual landing). */
    var seekLandingUs: Long = 5_000_000

    var openError: PlayerBridgeException? = null
    var seekError: PlayerBridgeException? = null

    /** Inject the next observed native snapshot (the poller's stand-in). */
    fun emit(snap: PlayerSnapshot) {
        snapshot.value = snap
    }

    override suspend fun open(source: Path) {
        openCalls.add(source)
        openError?.let { throw it }
    }

    override suspend fun play() {
        playCalls.incrementAndGet()
    }

    override suspend fun pause() {
        pauseCalls.incrementAndGet()
    }

    override suspend fun seek(positionUs: Long): Long {
        seekCalls.incrementAndGet()
        seekError?.let { throw it }
        return seekLandingUs
    }

    override suspend fun stop() {
        stopCalls.incrementAndGet()
    }

    override suspend fun close() {
        closeCalls.incrementAndGet()
    }
}
