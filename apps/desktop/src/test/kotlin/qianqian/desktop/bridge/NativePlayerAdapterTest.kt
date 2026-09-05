package qianqian.desktop.bridge

import java.nio.file.Files
import java.nio.file.Path
import java.util.Collections
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import qianqian.desktop.nativebridge.NativePlayerAdapter
import qianqian.desktop.nativebridge.PeState
import qianqian.desktop.nativebridge.PeStatus
import qianqian.desktop.nativebridge.SongIoSession
import qianqian.desktop.player.AbiMismatch
import qianqian.desktop.player.BridgeClosedException
import qianqian.desktop.player.ControlFailure
import qianqian.desktop.player.EngineCreationFailure
import qianqian.desktop.player.OpenFailure
import qianqian.desktop.player.PlayerState
import qianqian.desktop.player.SourceOpenFailure

/**
 * Pure JVM adapter behavior over the [FakeNativeApi] seam: control-call
 * serialization, close semantics, poller lifetime, and typed error
 * mapping — no native artifact required.
 */
class NativePlayerAdapterTest {

    private val tempFiles = mutableListOf<Path>()

    private fun tempFile(): Path =
        Files.createTempFile("bridge-test", ".bin").also { tempFiles.add(it) }

    @AfterTest
    fun cleanup() {
        tempFiles.forEach { Files.deleteIfExists(it) }
    }

    @Test
    fun controlCallsAreSerializedByTheConfinedDispatcher() = runBlocking {
        val api = FakeNativeApi()
        val adapter = NativePlayerAdapter.connect(api)
        val file = tempFile()
        coroutineScope {
            repeat(32) {
                launch(Dispatchers.Default) {
                    adapter.open(file)
                    adapter.play()
                    adapter.pause()
                    adapter.seek(1_000)
                    adapter.stop()
                }
            }
        }
        assertEquals(emptyList<String>(), api.controlOverlaps)
        adapter.close()
    }

    @Test
    fun commandsRacingCloseNeverReachNativeAfterDestroy() = runBlocking {
        val api = FakeNativeApi()
        val adapter = NativePlayerAdapter.connect(api)
        val errors = Collections.synchronizedList(mutableListOf<Throwable>())
        coroutineScope {
            launch(Dispatchers.Default) { adapter.close() }
            repeat(32) {
                launch(Dispatchers.Default) {
                    try {
                        adapter.play()
                    } catch (e: Throwable) {
                        errors.add(e)
                    }
                }
            }
        }
        // Destroy exactly once, and NOTHING touches the engine afterwards:
        // the poller is joined before destroy, and any command that lost
        // the race must have been gated on the control thread (no fake
        // call logged) instead of reaching a destroyed handle.
        assertEquals(1, api.destroyCount.get())
        val destroyIdx = api.callLog.indexOf("pe_destroy")
        assertTrue(destroyIdx >= 0)
        assertTrue(
            api.callLog.drop(destroyIdx + 1).isEmpty(),
            "native calls observed after pe_destroy: ${api.callLog.drop(destroyIdx + 1)}",
        )
        assertTrue(
            errors.all { it is BridgeClosedException },
            "racing commands failed with non-bridge errors: $errors",
        )
    }

    @Test
    fun closeIsIdempotentAndRejectsLaterCommands() = runBlocking {
        val api = FakeNativeApi()
        val adapter = NativePlayerAdapter.connect(api)
        adapter.close()
        adapter.close() // defined no-op
        assertFailsWith<BridgeClosedException> { adapter.play() }
        assertFailsWith<BridgeClosedException> { adapter.open(tempFile()) }
        assertFailsWith<BridgeClosedException> { adapter.seek(0) }
        assertFailsWith<BridgeClosedException> { adapter.stop() }
        assertEquals(1, api.destroyCount.get()) // destroy exactly once
    }

    @Test
    fun pollerPublishesSnapshotsAndStopsAtClose() = runBlocking {
        val api = FakeNativeApi()
        val adapter = NativePlayerAdapter.connect(api)
        adapter.snapshot.first { it.state == PlayerState.EMPTY }

        api.snapshotState = PeState.PLAYING
        withTimeout(3_000) {
            adapter.snapshot.first { it.state == PlayerState.PLAYING }
        }

        adapter.close()
        val callsAtClose = api.snapshotCallCount.get()
        delay(400) // two poll ticks worth of time
        assertTrue(
            api.snapshotCallCount.get() <= callsAtClose,
            "poller kept polling after close",
        )
    }

    @Test
    fun snapshotFlowNeverInventsTransitions() = runBlocking {
        val api = FakeNativeApi()
        api.playStatus = PeStatus.ERR_ILLEGAL_CALL
        val adapter = NativePlayerAdapter.connect(api)
        // play() fails natively; the flow must NOT optimistically report
        // PLAYING — truth comes only from pe_get_snapshot.
        assertFailsWith<ControlFailure> { adapter.play() }
        delay(300)
        assertEquals(PlayerState.EMPTY, adapter.snapshot.value.state)
        adapter.close()
    }

    @Test
    fun controlFailuresMapToTypedErrors() = runBlocking {
        val api = FakeNativeApi()
        api.playStatus = PeStatus.ERR_ILLEGAL_CALL
        val adapter = NativePlayerAdapter.connect(api)
        val e = assertFailsWith<ControlFailure> { adapter.play() }
        assertEquals("play", e.operation)
        assertEquals(PeStatus.ERR_ILLEGAL_CALL, e.peStatus)
        adapter.close()
    }

    @Test
    fun openFailureCarriesBothStatuses() = runBlocking {
        val api = FakeNativeApi()
        api.openStatus = PeStatus.ERR_OPEN_FAILED
        api.openSongStatus = 107 // SONG_ERR_CORRUPT_DATA
        val adapter = NativePlayerAdapter.connect(api)
        val e = assertFailsWith<OpenFailure> { adapter.open(tempFile()) }
        assertEquals(PeStatus.ERR_OPEN_FAILED, e.peStatus)
        assertEquals(107, e.songStatus)
        adapter.close()
    }

    @Test
    fun destructiveOpenFailureReleasesBothSessionsAndOwnsNeither() = runBlocking {
        val api = FakeNativeApi()
        val adapter = NativePlayerAdapter.connect(api)
        adapter.open(tempFile()) // A becomes the active source
        val openedBefore = SongIoSession.sessionsOpened.get()
        val closedBefore = SongIoSession.sessionsClosed.get()

        api.openStatus = PeStatus.ERR_OPEN_FAILED
        api.openSongStatus = 107 // SONG_ERR_CORRUPT_DATA
        assertFailsWith<OpenFailure> { adapter.open(tempFile()) }

        // Native truth: the previous source was dropped before the probe
        // (fake models the same EMPTY); the bridge must release BOTH the
        // failed candidate and the destroyed predecessor.
        assertEquals(PeState.EMPTY, api.snapshotState)
        assertEquals(openedBefore + 1, SongIoSession.sessionsOpened.get())
        assertEquals(closedBefore + 2, SongIoSession.sessionsClosed.get())
        val closedAtFailure = SongIoSession.sessionsClosed.get()

        // Subsequent close must not double-close sessions.
        adapter.close()
        assertEquals(closedAtFailure, SongIoSession.sessionsClosed.get())
    }

    @Test
    fun sourceAcquisitionFailureNeverReachesNativeAndPreservesActiveSource() = runBlocking {
        val api = FakeNativeApi()
        val adapter = NativePlayerAdapter.connect(api)
        adapter.open(tempFile()) // A is the active source
        val peOpensBefore = api.callLog.count { it == "pe_open" }
        val closedBefore = SongIoSession.sessionsClosed.get()

        assertFailsWith<SourceOpenFailure> {
            adapter.open(Path.of("/nonexistent/dir/other.flac"))
        }

        // pe_open was NEVER invoked for B, and A remains owned: the old
        // native source is intact, so the old JVM session stays valid.
        assertEquals(peOpensBefore, api.callLog.count { it == "pe_open" })
        assertEquals(closedBefore, SongIoSession.sessionsClosed.get())

        // A's session is released exactly once — at adapter teardown.
        adapter.close()
        assertEquals(closedBefore + 1, SongIoSession.sessionsClosed.get())
    }

    @Test
    fun seekReturnsTheNativeLanding() = runBlocking {
        val api = FakeNativeApi()
        api.seekLandingUs = 987_654L
        val adapter = NativePlayerAdapter.connect(api)
        assertEquals(987_654L, adapter.seek(1_000_000L))
        adapter.close()
    }

    @Test
    fun failedCreateIsTypedAndDestroysNothing() = runBlocking {
        val api = FakeNativeApi()
        api.createResult = null
        assertFailsWith<EngineCreationFailure> { NativePlayerAdapter.connect(api) }
        assertEquals(0, api.destroyCount.get())
    }

    @Test
    fun connectRefusesWrongSongcoreAbiBeforeEngineCreation() = runBlocking {
        val api = FakeNativeApi(songAbi = 99)
        val e = assertFailsWith<AbiMismatch> { NativePlayerAdapter.connect(api) }
        assertEquals("SongCore", e.component)
        // "before any engine can exist" is literal: pe_create never runs.
        assertEquals(0, api.callLog.count { it == "pe_create" })
        assertEquals(0, api.destroyCount.get())
    }

    @Test
    fun connectRefusesWrongEngineAbiBeforeEngineCreation() = runBlocking {
        val api = FakeNativeApi(engineAbi = 2)
        val e = assertFailsWith<AbiMismatch> { NativePlayerAdapter.connect(api) }
        assertEquals("PlayerEngine", e.component)
        assertEquals(0, api.callLog.count { it == "pe_create" })
        assertEquals(0, api.destroyCount.get())
    }

    @Test
    fun unreadableSourceIsTypedFailure() = runBlocking {
        val api = FakeNativeApi()
        val adapter = NativePlayerAdapter.connect(api)
        assertFailsWith<SourceOpenFailure> {
            adapter.open(Path.of("/nonexistent/dir/song.flac"))
        }
        adapter.close()
    }
}
