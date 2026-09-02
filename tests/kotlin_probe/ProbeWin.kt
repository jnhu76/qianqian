/* ProbeWin.kt — Windows entry: WASAPI-capable runtime (qianqian.dll). */
@file:OptIn(kotlinx.cinterop.ExperimentalForeignApi::class)

import kotlinx.cinterop.*
import platform.windows.Sleep
import qianqian.*
import kotlin.system.exitProcess

private object WinHost : ProbeHost {
    override fun sleep(ms: Int) = Sleep(ms.toUInt())
    /* mingw long is 32-bit */
    override fun seek(f: CPointer<FILE>, offset: Long, whence: Int): Boolean =
        fseek(f, offset.toInt(), whence) == 0
    override fun tell(f: CPointer<FILE>): Long = ftell(f).toLong()
}

fun main(args: Array<String>) {
    if (args.size != 1) {
        println("usage: PlayerEngineProbe <audio-file>")
        exitProcess(2)
    }
    exitProcess(runProbe(WinHost, args[0]))
}
