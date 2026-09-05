package qianqian.desktop.app

import java.io.IOException
import java.nio.file.Path
import java.nio.file.Paths
import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import qianqian.desktop.player.ControlFailure
import qianqian.desktop.player.OpenFailure
import qianqian.desktop.player.PlayerSnapshot
import qianqian.desktop.player.PlayerState
import qianqian.desktop.player.SourceOpenFailure

/**
 * Deterministic pure-JVM coordination tests for [PlayerScreenModel] over a
 * [FakePlayerPort] (no JNA, no native, no Compose): command routing, the
 * no-optimistic-state rule, seek preview/commit, error classification,
 * and shutdown.
 *
 * Suspend commands are called directly (sequentially); async paths
 * (picker flow, seek commit) and snapshot projection use [awaitUntil].
 */
class PlayerScreenModelTest {

    private val models = mutableListOf<PlayerScreenModel>()

    private val fileA: Path = Paths.get("music", "a.flac")
    private val fileB: Path = Paths.get("music", "b.flac")

    @AfterTest
    fun cleanup() = runBlocking {
        models.forEach { it.close() }
    }

    private fun model(
        port: FakePlayerPort = FakePlayerPort(),
        picker: FilePicker = FilePicker { null },
    ): PlayerScreenModel {
        val m = PlayerScreenModel(port, picker)
        models.add(m)
        return m
    }

    /** Await an async transition (snapshot collection / launched command). */
    private suspend fun awaitUntil(timeoutMs: Long = 5_000, condition: () -> Boolean) {
        withTimeout(timeoutMs) {
            while (!condition()) delay(10)
        }
    }

    private fun snap(
        state: PlayerState,
        positionUs: Long = 0,
        durationUs: Long = 60_000_000,
        durationKnown: Boolean = true,
    ): PlayerSnapshot = PlayerSnapshot(
        state = state,
        positionUs = positionUs,
        durationUs = durationUs,
        durationKnown = durationKnown,
        positionEstimated = false,
        sampleRate = 44_100,
        bufferedFrames = 0,
        underrunCount = 0,
        lastError = "",
    )

    // ---- open ------------------------------------------------------------

    @Test
    fun `open success issues open, promotes current track, never autoplays`() = runBlocking {
        val port = FakePlayerPort()
        val m = model(port)

        m.openFile(fileA) // suspend: completes before the assertions

        assertEquals(listOf(fileA), port.openCalls.toList())
        assertEquals(fileA, m.uiState.value.selectedFile)
        assertEquals(0, port.playCalls.get()) // open → READY, no autoplay
        assertNull(m.uiState.value.error)
        assertFalse(m.uiState.value.operationInFlight)
    }

    @Test
    fun `open failure keeps application alive with a product error`() = runBlocking {
        val port = FakePlayerPort()
        port.openError = SourceOpenFailure(IOException("permission denied"))
        val m = model(port)

        m.openFile(fileA)

        assertEquals(PlayerUiErrorCategory.FileUnavailable, m.uiState.value.error!!.category)
        assertNull(m.uiState.value.selectedFile) // failed candidate NOT promoted
        assertFalse(m.uiState.value.operationInFlight)

        // The application survives: a subsequent good open still works.
        port.openError = null
        m.openFile(fileB)
        assertEquals(fileB, m.uiState.value.selectedFile)
        assertNull(m.uiState.value.error)
    }

    @Test
    fun `pe_open failure classifies as CouldNotOpenTrack`() = runBlocking {
        val port = FakePlayerPort()
        port.openError = OpenFailure(peStatus = 1, songStatus = 7)
        val m = model(port)

        m.openFile(fileA)

        assertEquals(PlayerUiErrorCategory.CouldNotOpenTrack, m.uiState.value.error!!.category)
        assertEquals(fileA.fileName.toString(), m.uiState.value.error!!.detail)
        assertNull(m.uiState.value.selectedFile)
    }

    // ---- play / pause / stop ----------------------------------------------

    @Test
    fun `play issues play and shows NO optimistic PLAYING state`() = runBlocking {
        val port = FakePlayerPort()
        port.emit(snap(PlayerState.READY))
        val m = model(port)
        awaitUntil { m.uiState.value.snapshot.state == PlayerState.READY }

        m.togglePlayback()
        assertEquals(1, port.playCalls.get())

        // Native truth has not been re-published yet: the UI must NOT claim
        // PLAYING on its own.
        assertEquals(PlayerState.READY, m.uiState.value.snapshot.state)
        assertFalse(m.uiState.value.isPlaying)
        assertFalse(m.uiState.value.operationInFlight)

        port.emit(snap(PlayerState.PLAYING, positionUs = 500_000))
        awaitUntil { m.uiState.value.isPlaying }
    }

    @Test
    fun `pause issues pause and projection follows snapshot`() = runBlocking {
        val port = FakePlayerPort()
        port.emit(snap(PlayerState.PLAYING, positionUs = 3_000_000))
        val m = model(port)
        awaitUntil { m.uiState.value.isPlaying }

        m.togglePlayback()
        assertEquals(1, port.pauseCalls.get())
        assertEquals(0, port.playCalls.get()) // toggle routed to pause, not play

        assertEquals(PlayerState.PLAYING, m.uiState.value.snapshot.state) // no optimistic PAUSED
        port.emit(snap(PlayerState.PAUSED, positionUs = 3_000_000))
        awaitUntil { m.uiState.value.snapshot.state == PlayerState.PAUSED }
        assertTrue(m.uiState.value.canTogglePlayback) // resume stays available
    }

    @Test
    fun `stop issues stop and the snapshot remains the timeline authority`() = runBlocking {
        val port = FakePlayerPort()
        port.emit(snap(PlayerState.PLAYING, positionUs = 42_000_000))
        val m = model(port)
        awaitUntil { m.uiState.value.snapshot.positionUs == 42_000_000L }

        m.stopPlayback()
        assertEquals(1, port.stopCalls.get())

        port.emit(snap(PlayerState.READY, positionUs = 0))
        awaitUntil { m.uiState.value.snapshot.positionUs == 0L }
        assertEquals(0, m.uiState.value.displayedPositionUs) // no local timeline reset
    }

    // ---- seek ------------------------------------------------------------

    @Test
    fun `hundred drag previews make zero native seeks, commit makes exactly one`() = runBlocking {
        val port = FakePlayerPort()
        port.emit(snap(PlayerState.READY))
        val m = model(port)
        awaitUntil { m.uiState.value.snapshot.state == PlayerState.READY }

        repeat(100) { i -> m.onSeekPreview(i * 100_000L) }
        assertEquals(0, port.seekCalls.get()) // preview is local-only
        assertEquals(99 * 100_000L, m.uiState.value.displayedPositionUs)

        m.onSeekCommit()
        awaitUntil { port.seekCalls.get() == 1 }
        awaitUntil { m.uiState.value.committedSeekUs == port.seekLandingUs }
        assertEquals(1, port.seekCalls.get())

        // Snapshot catches up to the committed landing → display override clears.
        port.emit(snap(PlayerState.PLAYING, positionUs = port.seekLandingUs))
        awaitUntil { m.uiState.value.committedSeekUs == null }
        assertEquals(port.seekLandingUs, m.uiState.value.displayedPositionUs)
    }

    @Test
    fun `seek failure clears preview, records error, snapshot stays authority`() = runBlocking {
        val port = FakePlayerPort()
        port.emit(snap(PlayerState.PLAYING, positionUs = 42_000_000))
        port.seekError = ControlFailure("seek", peStatus = 1, songStatus = 5)
        val m = model(port)
        awaitUntil { m.uiState.value.snapshot.positionUs == 42_000_000L }

        m.onSeekPreview(50_000_000)
        m.onSeekCommit()
        awaitUntil { m.uiState.value.error != null }

        assertEquals(PlayerUiErrorCategory.PlaybackCommandFailed, m.uiState.value.error!!.category)
        assertNull(m.uiState.value.seekPreviewUs)
        assertNull(m.uiState.value.committedSeekUs)
        assertEquals(42_000_000, m.uiState.value.displayedPositionUs) // native snapshot truth
        assertFalse(m.uiState.value.operationInFlight)
    }

    // ---- source replacement ------------------------------------------------

    @Test
    fun `opening another source replaces current track without stale name`() = runBlocking {
        val port = FakePlayerPort()
        val m = model(port)

        m.openFile(fileA)
        port.emit(snap(PlayerState.PLAYING, positionUs = 1_000_000))
        awaitUntil { m.uiState.value.isPlaying }

        m.openFile(fileB)

        assertEquals(listOf(fileA, fileB), port.openCalls.toList())
        assertEquals(fileB, m.uiState.value.selectedFile)
        assertEquals("b.flac", m.uiState.value.selectedFile?.fileName.toString())
    }

    // ---- error clearing -----------------------------------------------------

    @Test
    fun `a new command attempt clears the previous error`() = runBlocking {
        val port = FakePlayerPort()
        port.openError = OpenFailure(peStatus = 1, songStatus = 7)
        val m = model(port)

        m.openFile(fileA)
        assertEquals(PlayerUiErrorCategory.CouldNotOpenTrack, m.uiState.value.error!!.category)

        port.openError = null
        m.openFile(fileB)
        assertEquals(fileB, m.uiState.value.selectedFile)
        assertNull(m.uiState.value.error)
    }

    // ---- picker -------------------------------------------------------------

    @Test
    fun `cancelled file picker is a no-op`() = runBlocking {
        val port = FakePlayerPort()
        val pickCalls = AtomicInteger(0)
        val m = model(port, picker = FilePicker { pickCalls.incrementAndGet(); null })

        m.openFileViaPicker()
        awaitUntil { !m.uiState.value.operationInFlight && pickCalls.get() == 1 }

        assertTrue(port.openCalls.isEmpty())
        assertNull(m.uiState.value.error)
        assertNull(m.uiState.value.selectedFile)
    }

    @Test
    fun `picker path flows into open without autoplay`() = runBlocking {
        val port = FakePlayerPort()
        val m = model(port, picker = FilePicker { fileA })

        m.openFileViaPicker()
        awaitUntil { m.uiState.value.selectedFile == fileA }

        assertEquals(listOf(fileA), port.openCalls.toList())
        assertEquals(0, port.playCalls.get())
        assertFalse(m.uiState.value.operationInFlight)
    }

    // ---- shutdown -------------------------------------------------------------

    @Test
    fun `close closes the port exactly once even when called twice`() = runBlocking {
        val port = FakePlayerPort()
        val m = model(port)

        m.close()
        m.close()
        assertEquals(1, port.closeCalls.get())
        // A brief settle to catch a would-be asynchronous double close.
        delay(50)
        assertEquals(1, port.closeCalls.get())
    }
}
