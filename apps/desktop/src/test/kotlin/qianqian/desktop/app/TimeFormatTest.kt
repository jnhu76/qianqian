package qianqian.desktop.app

import kotlin.test.Test
import kotlin.test.assertEquals

class TimeFormatTest {

    @Test
    fun zeroFormatsAsZeroMinutes() {
        assertEquals("0:00", TimeFormat.formatUs(0))
    }

    @Test
    fun subSecondFloorsToZero() {
        assertEquals("0:00", TimeFormat.formatUs(599_000))
        assertEquals("0:00", TimeFormat.formatUs(999_999))
    }

    @Test
    fun singleSecondAndBoundarySeconds() {
        assertEquals("0:01", TimeFormat.formatUs(1_000_000))
        assertEquals("0:59", TimeFormat.formatUs(59_000_000))
        assertEquals("0:59", TimeFormat.formatUs(59_900_000))
    }

    @Test
    fun minuteBoundaries() {
        assertEquals("1:00", TimeFormat.formatUs(60_000_000))
        assertEquals("1:05", TimeFormat.formatUs(65_000_000))
        assertEquals("9:59", TimeFormat.formatUs(599_000_000))
    }

    @Test
    fun justUnderAndAtOneHour() {
        assertEquals("59:59", TimeFormat.formatUs(3_599_000_000))
        assertEquals("1:00:00", TimeFormat.formatUs(3_600_000_000))
    }

    @Test
    fun overOneHourUsesHourClock() {
        assertEquals("1:01:01", TimeFormat.formatUs(3_661_000_000))
        assertEquals("2:00:00", TimeFormat.formatUs(7_200_000_000))
    }

    @Test
    fun negativeMeansUnknown() {
        assertEquals(TimeFormat.UNKNOWN, TimeFormat.formatUs(-1))
        assertEquals("--:--", TimeFormat.UNKNOWN)
    }
}
