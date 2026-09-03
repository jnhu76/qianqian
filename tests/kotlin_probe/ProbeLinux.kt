/* ProbeLinux.kt — Linux entry: engine-only runtime (libqianqian.so). */
@file:OptIn(kotlinx.cinterop.ExperimentalForeignApi::class)

import kotlinx.cinterop.*
import platform.posix.usleep
import qianqian.*
import kotlin.system.exitProcess

private object LinuxHost : ProbeHost {
    override fun sleep(ms: Int) { usleep((ms * 1000).toUInt()) }
    /* glibc long is 64-bit */
    override fun seek(f: CPointer<FILE>, offset: Long, whence: Int): Boolean =
        fseek(f, offset, whence) == 0
    override fun tell(f: CPointer<FILE>): Long = ftell(f)
}

fun main(args: Array<String>) {
    if (args.size != 1) {
        println("usage: PlayerEngineProbe <audio-file>")
        exitProcess(2)
    }
    exitProcess(runProbe(LinuxHost, args[0]))
}
