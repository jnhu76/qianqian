package qianqian.desktop.bridge

import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import kotlin.test.Test
import kotlin.test.assertTrue
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import qianqian.desktop.nativebridge.NativePlayerAdapter
import qianqian.desktop.nativebridge.NativeRuntimeLoader
import qianqian.desktop.nativebridge.SongIoSession

/**
 * One bounded empirical measurement of the JNA `song_io` upcall path
 * (DESKTOP-IA-1 residual risk), answering:
 *
 * > Is JNA callback overhead plausibly a bottleneck for normal local
 *   audio playback?
 *
 * Method: drive real decode refills through repeated seeks on a real
 * corpus song, count bytes served by the JVM callbacks, and compare the
 * effective throughput against the consumption rate of real audio
 * (~0.35 MB/s for 44.1 kHz stereo Float32; source bytes are fewer).
 * This is NOT a benchmark suite.
 */
class JnaCallbackPerformanceTest {

    private fun stagedLibrary(): Path {
        val p = Paths.get(
            "build", "native-dev", "linux-x86_64", "libqianqian.so",
        ).toAbsolutePath().normalize()
        check(Files.isRegularFile(p)) {
            "staged runtime missing: $p — run ./gradlew stageNativeRuntime"
        }
        return p
    }

    /** Real committed corpus song (8.8 MB CBR-320 MP3 with artwork). */
    private fun bigSong(): Path =
        Paths.get("..", "..").toAbsolutePath().normalize()
            .resolve("corpus/local/yinxing-de-chibi/mp3-cbr-320-artwork.mp3")

    @Test
    fun callbackPathSustainsFarMoreThanAudioConsumption() = runBlocking {
        val api = NativeRuntimeLoader.load(stagedLibrary())
        val adapter = NativePlayerAdapter.connect(api)
        try {
            adapter.open(bigSong())
            adapter.play()
            delay(300) // initial queue fill through the callbacks

            val bytesBefore = SongIoSession.totalBytes.get()
            val wallStart = System.nanoTime()
            repeat(120) { cycle ->
                adapter.seek(1_000_000L * (cycle % 40))
            }
            delay(500) // let in-flight refills land
            val wallNs = System.nanoTime() - wallStart
            val servedBytes = SongIoSession.totalBytes.get() - bytesBefore

            val seconds = wallNs / 1_000_000_000.0
            val mbPerSecond = servedBytes / (1024.0 * 1024.0) / seconds
            println(
                "JNA song_io measurement: served=${"%.2f".format(servedBytes / (1024.0 * 1024.0))} MB " +
                    "in ${"%.2f".format(seconds)} s " +
                    "=> ${"%.1f".format(mbPerSecond)} MB/s effective callback throughput " +
                    "(reads=${SongIoSession.totalReads.get()}, seeks=${SongIoSession.totalSeeks.get()})",
            )

            // Acceptance: real audio consumption is ~0.35 MB/s at the very
            // most (44.1 kHz stereo Float32); the callback path must
            // sustain at least an order of magnitude more.
            val thresholdMbPerSecond = 5.0
            assertTrue(
                mbPerSecond > thresholdMbPerSecond,
                "JNA callback throughput $mbPerSecond MB/s below the " +
                    "$thresholdMbPerSecond MB/s acceptance threshold",
            )
        } finally {
            adapter.close()
        }
    }
}
