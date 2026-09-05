package qianqian.desktop.app

import qianqian.desktop.player.PlayerSnapshot
import qianqian.desktop.player.PlayerState
import java.nio.file.Paths
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * Pure projection rules of [PlayerUiState]: enablement per the frozen
 * player contract, timeline display precedence, state labels, and product
 * error copy. No coroutines, no port.
 */
class PlayerUiStateTest {

    private val someFile = Paths.get("some", "song.flac")

    private fun snapshot(
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

    // ---- enablement ------------------------------------------------------

    @Test
    fun emptyStateDisablesEverything() {
        val state = PlayerUiState(snapshot = snapshot(PlayerState.EMPTY))
        assertFalse(state.canTogglePlayback)
        assertFalse(state.canStop)
        assertFalse(state.canSeek)
        assertTrue(state.canOpenFile)
    }

    @Test
    fun readyEnablesPlayStopAndSeekWhenDurationKnown() {
        val state = PlayerUiState(snapshot = snapshot(PlayerState.READY))
        assertTrue(state.canTogglePlayback)
        assertTrue(state.canStop)
        assertTrue(state.canSeek)
    }

    @Test
    fun playingEnablesToggleForPause() {
        val state = PlayerUiState(snapshot = snapshot(PlayerState.PLAYING, positionUs = 1_000_000))
        assertTrue(state.canTogglePlayback)
        assertTrue(state.isPlaying)
    }

    @Test
    fun endedEnablesReplay() {
        val state = PlayerUiState(snapshot = snapshot(PlayerState.ENDED, positionUs = 60_000_000))
        assertTrue(state.canTogglePlayback)
        assertTrue(state.canStop)
    }

    @Test
    fun errorDisablesPlaybackAndSeekButAllowsStopRecovery() {
        val state = PlayerUiState(snapshot = snapshot(PlayerState.ERROR))
        assertFalse(state.canTogglePlayback)
        assertFalse(state.canSeek)
        assertTrue(state.canStop)
    }

    @Test
    fun unknownDurationDisablesSeek() {
        val state = PlayerUiState(
            snapshot = snapshot(PlayerState.READY, durationUs = -1, durationKnown = false),
        )
        assertFalse(state.canSeek)
        assertTrue(state.canTogglePlayback)
    }

    @Test
    fun zeroDurationNeverEnablesSeek() {
        val state = PlayerUiState(
            snapshot = snapshot(PlayerState.READY, durationUs = 0, durationKnown = true),
        )
        assertFalse(state.canSeek)
    }

    @Test
    fun operationInFlightDisablesAllActions() {
        val state = PlayerUiState(
            snapshot = snapshot(PlayerState.READY),
            operationInFlight = true,
        )
        assertFalse(state.canOpenFile)
        assertFalse(state.canTogglePlayback)
        assertFalse(state.canStop)
        assertFalse(state.canSeek)
    }

    // ---- timeline display precedence --------------------------------------

    @Test
    fun displayedPositionPrefersPreviewThenCommittedLandingThenSnapshot() {
        val base = PlayerUiState(snapshot = snapshot(PlayerState.PLAYING, positionUs = 10_000_000))
        assertEquals(10_000_000, base.displayedPositionUs)

        val committed = base.copy(committedSeekUs = 20_000_000)
        assertEquals(20_000_000, committed.displayedPositionUs)

        val previewing = committed.copy(seekPreviewUs = 30_000_000)
        assertEquals(30_000_000, previewing.displayedPositionUs)
    }

    // ---- labels ------------------------------------------------------------

    @Test
    fun everyNativeStateHasATextLabel() {
        assertEquals("No track", stateLabel(PlayerState.EMPTY))
        assertEquals("Ready", stateLabel(PlayerState.READY))
        assertEquals("Playing", stateLabel(PlayerState.PLAYING))
        assertEquals("Paused", stateLabel(PlayerState.PAUSED))
        assertEquals("Ended", stateLabel(PlayerState.ENDED))
        assertEquals("Error", stateLabel(PlayerState.ERROR))
    }

    // ---- error copy ---------------------------------------------------------

    @Test
    fun errorCopyIsMinimalAndTyped() {
        assertEquals(
            "Could not read this file.",
            PlayerUiError(PlayerUiErrorCategory.FileUnavailable).message,
        )
        assertEquals(
            "Could not open this file. (broken.flac)",
            PlayerUiError(PlayerUiErrorCategory.CouldNotOpenTrack, "broken.flac").message,
        )
        assertEquals(
            "Playback command failed.",
            PlayerUiError(PlayerUiErrorCategory.PlaybackCommandFailed).message,
        )
        assertEquals(
            "Playback runtime unavailable.",
            PlayerUiError(PlayerUiErrorCategory.RuntimeUnavailable).message,
        )
    }

    @Test
    fun selectedFileIsPartOfTheProjection() {
        val state = PlayerUiState(snapshot = snapshot(PlayerState.READY), selectedFile = someFile)
        assertEquals(someFile, state.selectedFile)
    }
}
