/* ProbeWin.kt — Windows entry: WASAPI-capable runtime (qianqian.dll). */
@file:OptIn(kotlinx.cinterop.ExperimentalForeignApi::class)

import kotlinx.cinterop.*
import platform.windows.Sleep
import qianqian.*
import kotlin.system.exitProcess

private object WinHost : ProbeHost {
    override fun sleep(ms: Int) = Sleep(ms.toUInt())
    /* _fseeki64/_ftelli64, not fseek/ftell: mingw long is 32-bit and would
     * clip the frozen song_io 64-bit offsets to +/-2GB. */
    override fun seek(f: CPointer<FILE>, offset: Long, whence: Int): Boolean =
        _fseeki64(f, offset, whence) == 0
    override fun tell(f: CPointer<FILE>): Long = _ftelli64(f)
}

fun main(args: Array<String>) {
    if (args.size != 1) {
        println("usage: PlayerEngineProbe <audio-file>")
        exitProcess(2)
    }
    exitProcess(runProbe(WinHost, args[0]))
}
