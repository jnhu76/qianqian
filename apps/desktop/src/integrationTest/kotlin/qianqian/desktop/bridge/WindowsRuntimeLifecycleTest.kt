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
import org.junit.Assume.assumeTrue
import qianqian.desktop.nativebridge.NativePlayerAdapter
import qianqian.desktop.nativebridge.NativeRuntimeLoader
import qianqian.desktop.nativebridge.SongIoSession
import qianqian.desktop.player.BridgeClosedException
import qianqian.desktop.player.PlayerState

/**
 * Real-runtime bridge proof on native Windows against the staged
 * `qianqian.dll` (mingw x86_64, `QN_QIANQIAN_RUNTIME` WASAPI flavor) and
 * real corpus fixtures.
 *
 * Evidence scope: this is the Windows counterpart of
 * [LinuxRuntimeLifecycleTest]. Unlike the Linux engine-only flavor, the
 * Windows runtime composes a real WASAPI renderer, so PLAYING here must
 * show position progression from render truth, and a full playout of a
 * short fixture must reach ENDED (audible confirmation remains the human
 * manual gate; this suite proves the machine half).
 *
 * Requires `gradlew.bat stageNativeRuntime` first.
 */
class WindowsRuntimeLifecycleTest {

    private val repoRoot: Path =
        Paths.get("..", "..").toAbsolutePath().normalize()

    init {
        assumeTrue(
            "Windows runtime proof; Linux truth lives in LinuxRuntimeLifecycleTest",
            System.getProperty("os.name").lowercase().contains("win"),
        )
    }

    private fun stagedLibrary(): Path {
        val p = Paths.get(
            "build", "native-dev", "windows-x86_64", "qianqian.dll",
        ).toAbsolutePath().normalize()
        check(Files.isRegularFile(p)) {
            "staged runtime missing: $p — run gradlew.bat stageNativeRuntime"
        }
        return p
    }

    /** Real stereo FLAC fixture (corpus-tracked, committed), 44.1 kHz. */
    private fun flacFixture(): Path =
        repoRoot.resolve("corpus/fixtures/flac-16-44-stereo.flac")

    /** Tiny real MP3 for repeated cycles and full ENDED playout. */
    private fun shortMp3(): Path =
        repoRoot.resolve("corpus/fixtures/mp3-short.mp3")

    private suspend fun connected(): NativePlayerAdapter =
        NativePlayerAdapter.connect(stagedLibrary())

    @Test
    fun realRuntimeLoadsAndPassesAbiGates() {
        val api = NativeRuntimeLoader.load(stagedLibrary())
        // load() validates both frozen ABI versions; reaching here is the pass.
        assertEquals(1, api.songcoreAbiVersion())
        assertEquals(1, api.playerEngineAbiVersion())
    }

    @Test
    fun playProgressesPositionFromRenderTruth() = runBlocking {
        val adapter = connected()
        try {
            adapter.snapshot.first { it.state == PlayerState.EMPTY }
            adapter.open(flacFixture())
            val ready = adapter.snapshot.first { it.state == PlayerState.READY }
            assertTrue(ready.durationKnown && ready.durationUs > 0)
            assertEquals(44_100, ready.sampleRate)

            adapter.play()
            adapter.snapshot.first { it.state == PlayerState.PLAYING }

            // Render truth: the WASAPI renderer only advances the frozen
            // position on proven playout, so PLAYING must move it.
            val p0 = adapter.snapshot.value.positionUs
            delay(3_000)
            val p1 = adapter.snapshot.value.positionUs
            assertTrue(p1 > p0, "position frozen while PLAYING: $p0 -> $p1")
            // Coarse wall-clock sanity (~3s wall -> plausibly ~3s media,
            // tolerating preroll/render latency).
            assertTrue(
                p1 - p0 > 1_000_000L,
                "position barely progressed in 3s wall: $p0 -> $p1",
            )
            assertTrue(
                p1 - p0 < 6_000_000L,
                "position progressed implausibly far in 3s wall: $p0 -> $p1",
            )
        } finally {
            adapter.close()
        }
    }

    @Test
    fun pauseFreezesAndResumeContinuesFromRenderTruth() = runBlocking {
        val adapter = connected()
        try {
            adapter.open(flacFixture())
            adapter.snapshot.first { it.state == PlayerState.READY }
            adapter.play()
            adapter.snapshot.first { it.state == PlayerState.PLAYING }
            delay(1_500)

            adapter.pause()
            val paused = adapter.snapshot.first { it.state == PlayerState.PAUSED }
            delay(2_000)
            val held = adapter.snapshot.value
            assertEquals(PlayerState.PAUSED, held.state)
            assertTrue(
                held.positionUs - paused.positionUs < 500_000L,
                "position advanced while PAUSED: ${paused.positionUs} -> ${held.positionUs}",
            )

            adapter.play()
            val resumed = adapter.snapshot.first { it.state == PlayerState.PLAYING }
            delay(1_000)
            val later = adapter.snapshot.value
            assertTrue(
                later.positionUs >= resumed.positionUs,
                "position did not continue after resume: ${resumed.positionUs} -> ${later.positionUs}",
            )

            adapter.stop()
            val stopped = adapter.snapshot.first { it.state == PlayerState.READY }
            assertEquals(0L, stopped.positionUs)
        } finally {
            adapter.close()
        }
    }

    @Test
    fun seekLandingCoherentWithRenderTruth() = runBlocking {
        val adapter = connected()
        try {
            adapter.open(flacFixture())
            val ready = adapter.snapshot.first { it.state == PlayerState.READY }
            adapter.play()
            adapter.snapshot.first { it.state == PlayerState.PLAYING }

            val landing = adapter.seek(ready.durationUs / 2)
            assertTrue(landing >= 0, "seek landing unknown")
            delay(1_000)
            val playing = adapter.snapshot.value
            // Playback continues from the landing (render progression may
            // already have moved past it, never back before it).
            assertTrue(
                playing.positionUs >= landing - 500_000L,
                "position fell below seek landing: landing=$landing snapshot=$playing",
            )
        } finally {
            adapter.close()
        }
    }

    @Test
    fun fullPlayoutReachesEndedAndReplays() = runBlocking {
        val adapter = connected()
        try {
            adapter.open(shortMp3())
            val ready = adapter.snapshot.first { it.state == PlayerState.READY }
            assertTrue(ready.durationKnown && ready.durationUs > 0)

            adapter.play()
            val ended = withTimeout(90_000) {
                adapter.snapshot.first { it.state == PlayerState.ENDED }
            }
            // ENDED means actual render completion, not decode EOF: the
            // frozen position must sit at the media duration (contract
            // accuracy — coarse bound, never a fake early jump).
            assertTrue(
                ended.positionUs >= ended.durationUs - 2_000_000L,
                "ENDED below duration: snapshot=$ended",
            )

            // Replay from ENDED: native semantics seek 0 -> PLAYING.
            adapter.play()
            val replayed = adapter.snapshot.first { it.state == PlayerState.PLAYING }
            assertTrue(
                replayed.positionUs < 2_000_000L,
                "replay did not restart near 0: $replayed",
            )
        } finally {
            adapter.close()
        }
    }

    @Test
    fun songIoCallbacksReallyFireOnTheJvm() = runBlocking {
        val reads0 = SongIoSession.totalReads.get()
        val adapter = connected()
        try {
            adapter.open(flacFixture())
            adapter.snapshot.first { it.state == PlayerState.READY }
            assertTrue(
                SongIoSession.totalReads.get() > reads0,
                "no JVM read callback observed after open",
            )
            val before = SongIoSession.totalSeeks.get()
            adapter.play()
            delay(400)
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
        val adapter = connected()
        try {
            adapter.open(flacFixture())
            adapter.play()
            delay(200)
            var served = SongIoSession.totalReads.get()
            repeat(3) { round ->
                @Suppress("UNUSED_VARIABLE")
                val pressure = ByteArray(8 * 1024 * 1024)
                System.gc()
                adapter.seek(1_000_000L * (round + 1))
                delay(300)
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
    fun repeatedCyclesStayClean() = runBlocking {
        repeat(5) {
            val adapter = connected()
            adapter.open(shortMp3())
            adapter.play()
            delay(50)
            adapter.stop()
            adapter.close()
        }
        assertTrue(SongIoSession.sessionsClosed.get() >= 5)
    }

    @Test
    fun closeSemanticsRejectLaterCommands() = runBlocking {
        val adapter = connected()
        adapter.open(shortMp3())
        adapter.close()
        adapter.close() // no-op
        assertFailsWith<BridgeClosedException> { adapter.play() }
        assertFailsWith<BridgeClosedException> { adapter.open(shortMp3()) }
        Unit
    }
}
