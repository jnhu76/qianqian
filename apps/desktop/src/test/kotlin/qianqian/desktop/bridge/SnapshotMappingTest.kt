package qianqian.desktop.bridge

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import qianqian.desktop.nativebridge.NativePlayerAdapter
import qianqian.desktop.nativebridge.PeQuality
import qianqian.desktop.nativebridge.PeSnapshot
import qianqian.desktop.nativebridge.PeState
import qianqian.desktop.player.PlayerState
import qianqian.desktop.player.PositionQuality

/**
 * Native `pe_snapshot` -> application-safe `PlayerSnapshot` projection:
 * field-by-field mapping, units preserved (media microseconds), NUL-
 * terminated UTF-8 diagnostic decoding, unknown values fail closed.
 */
class SnapshotMappingTest {

    private fun snapshot(block: PeSnapshot.() -> Unit = {}): PeSnapshot =
        PeSnapshot().apply(block)

    @Test
    fun fieldsMapOneToOne() {
        val snap = snapshot {
            state = PeState.PLAYING
            positionUs = 123_456_789L
            durationUs = 456_000_000L
            durationKnown = 1
            positionQuality = PeQuality.CONFIRMED.toByte()
            bufferedFrames = 8_192L
            underrunCount = 2L
            sampleRate = 44_100
        }
        val mapped = NativePlayerAdapter.mapSnapshot(snap)
        assertEquals(PlayerState.PLAYING, mapped.state)
        assertEquals(123_456_789L, mapped.positionUs)
        assertEquals(456_000_000L, mapped.durationUs)
        assertTrue(mapped.durationKnown)
        assertFalse(mapped.positionEstimated)
        assertEquals(PositionQuality.CONFIRMED, mapped.positionQuality())
        assertEquals(44_100, mapped.sampleRate)
        assertEquals(8_192L, mapped.bufferedFrames)
        assertEquals(2L, mapped.underrunCount)
        assertEquals("", mapped.lastError)
    }

    @Test
    fun unknownDurationStaysMinusOne() {
        val mapped = NativePlayerAdapter.mapSnapshot(
            snapshot {
                state = PeState.READY
                durationUs = -1L
                durationKnown = 0
            }
        )
        assertEquals(-1L, mapped.durationUs) // never remapped to 0
        assertFalse(mapped.durationKnown)
    }

    @Test
    fun estimatedQualityIsTyped() {
        val mapped = NativePlayerAdapter.mapSnapshot(
            snapshot { positionQuality = PeQuality.ESTIMATED.toByte() }
        )
        assertTrue(mapped.positionEstimated)
        assertEquals(PositionQuality.ESTIMATED, mapped.positionQuality())
    }

    @Test
    fun lastErrorDecodesUtf8UpToTerminator() {
        val snap = snapshot { state = PeState.ERROR }
        val text = "seek failed: 状态错误"
        val raw = text.toByteArray(Charsets.UTF_8)
        System.arraycopy(raw, 0, snap.lastError, 0, raw.size)
        snap.lastError[raw.size] = 0
        assertEquals(text, NativePlayerAdapter.mapSnapshot(snap).lastError)
    }

    @Test
    fun everyPublicStateMaps() {
        assertEquals(PlayerState.EMPTY, NativePlayerAdapter.mapState(PeState.EMPTY))
        assertEquals(PlayerState.READY, NativePlayerAdapter.mapState(PeState.READY))
        assertEquals(PlayerState.PLAYING, NativePlayerAdapter.mapState(PeState.PLAYING))
        assertEquals(PlayerState.PAUSED, NativePlayerAdapter.mapState(PeState.PAUSED))
        assertEquals(PlayerState.ENDED, NativePlayerAdapter.mapState(PeState.ENDED))
        assertEquals(PlayerState.ERROR, NativePlayerAdapter.mapState(PeState.ERROR))
    }

    @Test
    fun unknownStateFailsClosedAsError() {
        assertEquals(PlayerState.ERROR, NativePlayerAdapter.mapState(42))
    }

    /** Convenience without widening the public snapshot type. */
    private fun qianqian.desktop.player.PlayerSnapshot.positionQuality(): PositionQuality =
        if (positionEstimated) PositionQuality.ESTIMATED else PositionQuality.CONFIRMED
}
