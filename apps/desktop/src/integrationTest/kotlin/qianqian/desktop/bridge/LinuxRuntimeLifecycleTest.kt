package qianqian.desktop.bridge

import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import qianqian.desktop.nativebridge.NativePlayerAdapter
import qianqian.desktop.nativebridge.NativeRuntimeLoader
import qianqian.desktop.nativebridge.SongIoSession
import qianqian.desktop.player.BridgeClosedException
import qianqian.desktop.player.PlayerState

/**
 * Real-runtime bridge proof on WSL/Linux against the staged
 * `libqianqian.so` and real corpus fixtures.
 *
 * Evidence scope (honest by construction): the Linux runtime is the
 * engine-only ABI flavor — it has NO real audio output backend. Control
 * lifecycle, `song_io` callbacks, decode, seek, and snapshots are real;
 * audible render progression and ENDED-through-render are NOT exercisable
 * here and are never claimed.
 *
 * Requires `./gradlew stageNativeRuntime` first.
 */
class LinuxRuntimeLifecycleTest {

    private val repoRoot: Path =
        Paths.get("..", "..").toAbsolutePath().normalize()

    private fun stagedLibrary(): Path {
        val p = Paths.get(
            "build", "native-dev", "linux-x86_64", "libqianqian.so",
        ).toAbsolutePath().normalize()
        check(Files.isRegularFile(p)) {
            "staged runtime missing: $p — run ./gradlew stageNativeRuntime"
        }
        return p
    }

    /** Real stereo FLAC fixture (corpus-tracked, committed). */
    private fun flacFixture(): Path =
        repoRoot.resolve("corpus/fixtures/flac-16-44-stereo.flac")

    /** Tiny real MP3 for repeated lifecycle cycles. */
    private fun shortMp3(): Path =
        repoRoot.resolve("corpus/fixtures/mp3-short.mp3")

    private suspend fun connected(): Pair<NativePlayerAdapter, qianqian.desktop.nativebridge.NativeApi> {
        val api = NativeRuntimeLoader.load(stagedLibrary())
        return NativePlayerAdapter.connect(api) to api
    }

    @Test
    fun realRuntimeLoadsAndPassesAbiGates() {
        val api = NativeRuntimeLoader.load(stagedLibrary())
        // load() validates both frozen ABI versions; reaching here is the pass.
        assertEquals(1, api.songcoreAbiVersion())
        assertEquals(1, api.playerEngineAbiVersion())
    }

    @Test
    fun fullLegalControlLifecycleOverRealRuntime() = runBlocking {
        val (adapter, _) = connected()
        try {
            adapter.snapshot.first { it.state == PlayerState.EMPTY }

            adapter.open(flacFixture())
            val ready = adapter.snapshot.first { it.state == PlayerState.READY }
            assertTrue(
                ready.durationKnown && ready.durationUs > 0,
                "duration unknown: $ready",
            )
            assertEquals(44_100, ready.sampleRate)

            adapter.play()
            adapter.snapshot.first { it.state == PlayerState.PLAYING }
            delay(700)
            // Linux engine-only flavor: no audio backend, so no proven
            // render progression — position stays at the segment base.
            // (Observed behavior, recorded per DESKTOP-NATIVE-BRIDGE-1 §32;
            // NOT a WASAPI expectation.)
            val playing = adapter.snapshot.value
            assertEquals(PlayerState.PLAYING, playing.state)
            assertEquals(0L, playing.positionUs)

            adapter.pause()
            adapter.snapshot.first { it.state == PlayerState.PAUSED }
            adapter.play()
            adapter.snapshot.first { it.state == PlayerState.PLAYING }

            val landing = adapter.seek(ready.durationUs / 2)
            assertTrue(landing >= 0, "seek landing unknown: $landing")

            adapter.stop()
            val stopped = adapter.snapshot.first { it.state == PlayerState.READY }
            assertEquals(0L, stopped.positionUs)
        } finally {
            adapter.close()
        }
    }

    @Test
    fun songIoCallbacksReallyFireOnTheJvm() = runBlocking {
        val reads0 = SongIoSession.totalReads.get()
        val sizes0 = SongIoSession.totalSizes.get()
        val (adapter, _) = connected()
        try {
            adapter.open(flacFixture())
            adapter.snapshot.first { it.state == PlayerState.READY }
            // pe_open -> song_open/song_probe already read and sized the file
            // through the JVM callbacks.
            assertTrue(
                SongIoSession.totalReads.get() > reads0,
                "no JVM read callback observed after open",
            )
            assertTrue(SongIoSession.totalSizes.get() > sizes0)

            val before = SongIoSession.totalSeeks.get()
            adapter.play()
            delay(400) // decode worker fills the queue through the callbacks
            adapter.seek(500_000L)
            delay(300)
            assertTrue(
                SongIoSession.totalSeeks.get() > before,
                "no JVM seek callback observed",
            )
        } finally {
            adapter.close()
        }
    }

    @Test
    fun callbacksSurviveBoundedGcStress() = runBlocking {
        val (adapter, _) = connected()
        try {
            adapter.open(flacFixture())
            adapter.play()
            delay(200)
            var served = SongIoSession.totalReads.get()
            repeat(5) { round ->
                @Suppress("UNUSED_VARIABLE")
                val pressure = ByteArray(8 * 1024 * 1024)
                System.gc()
                adapter.seek(1_000_000L * (round + 1))
                delay(250)
                val now = SongIoSession.totalReads.get()
                assertTrue(
                    now > served,
                    "callbacks died under GC pressure at round $round",
                )
                served = now
            }
        } finally {
            adapter.close()
        }
    }

    @Test
    fun repeatedCyclesReleaseResources() = runBlocking {
        val fdBaseline = openFdCount()
        repeat(25) {
            val (adapter, _) = connected()
            adapter.open(shortMp3())
            adapter.play()
            delay(30)
            adapter.stop()
            adapter.close()
        }
        val grown = openFdCount() - fdBaseline
        assertTrue(
            grown < 10,
            "resource leak across 25 lifecycle cycles: +$grown fds",
        )
        assertTrue(SongIoSession.sessionsClosed.get() >= 25)
    }

    @Test
    fun closeSemanticsRejectLaterCommands() = runBlocking {
        val (adapter, _) = connected()
        adapter.open(shortMp3())
        adapter.close()
        adapter.close() // no-op
        assertFailsWith<BridgeClosedException> { adapter.play() }
        assertFailsWith<BridgeClosedException> { adapter.open(shortMp3()) }
        Unit
    }

    private fun openFdCount(): Int =
        Files.list(Paths.get("/proc/self/fd")).use { it.count().toInt() }
}
