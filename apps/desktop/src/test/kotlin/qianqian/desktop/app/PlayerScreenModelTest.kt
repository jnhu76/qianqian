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
import kotlinx.coroutines.CompletableDeferred
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

        m.togglePlayback() // model-owned command: wait for its effect
        awaitUntil { port.playCalls.get() == 1 }

        // Native truth has not been re-published yet: the UI must NOT claim
        // PLAYING on its own.
        assertEquals(PlayerState.READY, m.uiState.value.snapshot.state)
        assertFalse(m.uiState.value.isPlaying)
        awaitUntil { !m.uiState.value.operationInFlight }

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
        awaitUntil { port.pauseCalls.get() == 1 }
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
        awaitUntil { port.stopCalls.get() == 1 }

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
        awaitUntil { !m.uiState.value.operationInFlight }
    }

    @Test
    fun `backward seek keeps committed landing across a stale high snapshot`() = runBlocking {
        val port = FakePlayerPort()
        port.emit(snap(PlayerState.PLAYING, positionUs = 50_000_000))
        val m = model(port)
        awaitUntil { m.uiState.value.snapshot.positionUs == 50_000_000L }

        // Backward seek: 50s → 10s; the engine reports landing 10s.
        m.onSeekPreview(10_000_000)
        port.seekLandingUs = 10_000_000
        m.onSeekCommit()
        awaitUntil { m.uiState.value.committedSeekUs == 10_000_000L }

        // A stale pre-seek snapshot (~50s) arrives before the engine lands:
        // it is far ABOVE the 10s landing, so it must NOT clear the display.
        port.emit(snap(PlayerState.PLAYING, positionUs = 50_100_000))
        awaitUntil { m.uiState.value.snapshot.positionUs == 50_100_000L }
        assertEquals(10_000_000L, m.uiState.value.committedSeekUs)
        assertEquals(10_000_000L, m.uiState.value.displayedPositionUs)

        // A snapshot inside the landing window clears the override; pure
        // snapshot truth resumes.
        port.emit(snap(PlayerState.PLAYING, positionUs = 10_050_000))
        awaitUntil { m.uiState.value.committedSeekUs == null }
        assertEquals(10_050_000L, m.uiState.value.displayedPositionUs)
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

    @Test
    fun `source open failure preserves the current track`() = runBlocking {
        val port = FakePlayerPort()
        val m = model(port)
        m.openFile(fileA)
        assertEquals(fileA, m.uiState.value.selectedFile)

        // JVM-side read of B fails BEFORE pe_open: native still holds A.
        port.openError = SourceOpenFailure(IOException("unreadable"))
        m.openFile(fileB)

        assertEquals(fileA, m.uiState.value.selectedFile) // A remains current
        assertEquals(PlayerUiErrorCategory.FileUnavailable, m.uiState.value.error!!.category)
        assertEquals(fileB.fileName.toString(), m.uiState.value.error!!.detail) // error names B
        assertFalse(m.uiState.value.operationInFlight)
    }

    @Test
    fun `open failure clears the current track and references the candidate`() = runBlocking {
        val port = FakePlayerPort()
        val m = model(port)
        m.openFile(fileA)
        assertEquals(fileA, m.uiState.value.selectedFile)

        // pe_open(B) ran and failed: native is EMPTY and owns no source.
        port.openError = OpenFailure(peStatus = 1, songStatus = 7)
        m.openFile(fileB)

        assertNull(m.uiState.value.selectedFile) // A must NOT stay displayed
        assertEquals(PlayerUiErrorCategory.CouldNotOpenTrack, m.uiState.value.error!!.category)
        assertEquals(fileB.fileName.toString(), m.uiState.value.error!!.detail) // names B, not A
        assertNull(m.uiState.value.seekPreviewUs)
        assertNull(m.uiState.value.committedSeekUs)
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

    // ---- command admission ---------------------------------------------------

    @Test
    fun `rapid toggles admit at most one command until it completes`() = runBlocking {
        val port = FakePlayerPort()
        val playStarted = CompletableDeferred<Unit>()
        val releasePlay = CompletableDeferred<Unit>()
        port.playGate = {
            playStarted.complete(Unit)
            releasePlay.await() // command stays in flight until the test releases it
        }
        val m = model(port)
        port.emit(snap(PlayerState.READY))
        awaitUntil { m.uiState.value.snapshot.state == PlayerState.READY }

        repeat(32) { m.togglePlayback() }
        playStarted.await() // the first admitted command is suspended inside play

        // Admission is synchronous at the entrypoint: the other 31 clicks
        // are dropped, deterministically, while the command is in flight.
        assertEquals(1, port.playCalls.get())

        releasePlay.complete(Unit)
        awaitUntil { !m.uiState.value.operationInFlight }

        // Admission is a gate, not a wedge: the next command is admitted.
        m.togglePlayback()
        awaitUntil { port.playCalls.get() == 2 }
        awaitUntil { !m.uiState.value.operationInFlight }
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
        awaitUntil { !m.uiState.value.operationInFlight }
    }

    // ---- cancellation / shutdown ---------------------------------------------

    @Test
    fun `picker cancellation during shutdown is not a product error`() = runBlocking {
        val port = FakePlayerPort()
        val pickerEntered = CompletableDeferred<Unit>()
        var pickerCancelled = false
        val stuckPicker = FilePicker {
            pickerEntered.complete(Unit)
            try {
                CompletableDeferred<Path?>().await() // suspends until scope cancellation
            } finally {
                pickerCancelled = true
            }
        }
        val m = model(port, stuckPicker)

        m.openFileViaPicker()
        pickerEntered.await() // the "dialog" is open

        m.close() // must cancel AND drain the picker coroutine before port close

        assertTrue(pickerCancelled) // drain completed before close() returned
        assertNull(m.uiState.value.error) // no FileDialogFailed during shutdown
        assertEquals(1, port.closeCalls.get()) // port closed exactly once, after drain
    }

    @Test
    fun `close drains an in-flight command before closing the port`() = runBlocking {
        val port = FakePlayerPort()
        val playStarted = CompletableDeferred<Unit>()
        port.playGate = {
            playStarted.complete(Unit)
            CompletableDeferred<Unit>().await() // never released: shutdown cancels it
        }
        val m = model(port)
        port.emit(snap(PlayerState.READY))
        awaitUntil { m.uiState.value.snapshot.state == PlayerState.READY }

        m.togglePlayback()
        playStarted.await() // command suspended in play()

        m.close()

        assertEquals(listOf("play", "port:close"), port.events) // drain BEFORE port close
        assertEquals(1, port.closeCalls.get())
        assertNull(m.uiState.value.error) // cancellation never became a fake command error

        // The drained model projects nothing anymore — no late UI mutation.
        val frozen = m.uiState.value
        port.emit(snap(PlayerState.PLAYING, positionUs = 99_000_000))
        delay(50) // bounded settle: absence-of-event check
        assertEquals(frozen, m.uiState.value)
    }

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
