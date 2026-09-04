package qianqian.desktop.bridge

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import qianqian.desktop.player.AbiMismatch
import qianqian.desktop.nativebridge.NativeRuntimeLoader
import qianqian.desktop.nativebridge.QianqianAbi
import qianqian.desktop.player.RuntimeLoadFailure

/** ABI version gate: mismatch is a typed fail-fast, before any engine. */
class AbiGateTest {

    @Test
    fun matchingVersionsPass() {
        NativeRuntimeLoader.validateAbi(FakeNativeApi())
    }

    @Test
    fun songcoreMismatchFailsFast() {
        val api = FakeNativeApi(songAbi = 99)
        val e = assertFailsWith<AbiMismatch> {
            NativeRuntimeLoader.validateAbi(api)
        }
        assertEquals("SongCore", e.component)
        assertEquals(QianqianAbi.SONGCORE_ABI_VERSION, e.expected)
        assertEquals(99, e.actual)
    }

    @Test
    fun playerEngineMismatchFailsFast() {
        val api = FakeNativeApi(engineAbi = 2)
        val e = assertFailsWith<AbiMismatch> {
            NativeRuntimeLoader.validateAbi(api)
        }
        assertEquals("PlayerEngine", e.component)
        assertEquals(QianqianAbi.PLAYER_ENGINE_ABI_VERSION, e.expected)
        assertEquals(2, e.actual)
    }

    @Test
    fun noEngineIsCreatedAfterMismatch() {
        val api = FakeNativeApi(songAbi = 7)
        assertFailsWith<AbiMismatch> { NativeRuntimeLoader.validateAbi(api) }
        assertTrue(api.callLog.none { it == "pe_create" })
    }

    @Test
    fun missingRuntimeFileIsTypedFailure() {
        val e = assertFailsWith<RuntimeLoadFailure> {
            NativeRuntimeLoader.load(java.nio.file.Path.of("/nonexistent/qianqian/libqianqian.so"))
        }
        assertTrue(e.message!!.contains("staged runtime not found"))
    }
}
