package qianqian.desktop.nativebridge

import com.sun.jna.Memory
import com.sun.jna.Native

import com.sun.jna.Pointer
import java.nio.ByteBuffer
import java.nio.channels.FileChannel
import java.nio.file.Path
import java.nio.file.StandardOpenOption
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

/**
 * One `song_io` session over one open local file: the JVM-side file source
 * that native SongCore reads through the three frozen host-I/O callbacks.
 *
 * Userdata strategy: `song_io.userdata` points at a JVM-owned 8-byte native
 * slot holding a unique token; a JVM-side registry maps the token to this
 * session. The callbacks (companion-held singletons — unreachable by GC for
 * the process lifetime) decode the token and dispatch. No JVM object
 * reference is ever stored in native memory.
 *
 * Lifetime model: the engine copies `song_io` at `pe_open` and may reuse
 * the copy (stop-recovery reopen), so the token slot, registry entry, file
 * channel, and this structure stay strongly referenced by the owning
 * adapter until AFTER `pe_destroy` returns. `close()` then removes the
 * registry entry and releases the channel; a late native callback finding
 * no registry entry fails closed (returns -1) instead of crashing.
 *
 * Threading: callbacks arrive on native (decode worker) threads; JNA
 * attaches those threads to the JVM automatically. Each callback performs
 * bounded positional file I/O only — no Compose state, no UI, and NEVER a
 * reentrant `song_*`/`pe_*` call back into the runtime. The engine
 * serializes handle access internally; the session is additionally
 * synchronized, so a stray concurrent upcall cannot interleave.
 *
 * Read/seek/size counters are instrumentation for bridge proof (callback
 * reality, GC stress, JNA overhead measurement) — cheap atomics, not a
 * product feature.
 */
class SongIoSession private constructor(val path: Path) : AutoCloseable {

    private val tokenSlot = Memory(Native.LONG_SIZE.toLong())
    private val channel: FileChannel = FileChannel.open(
        path,
        StandardOpenOption.READ,
    )
    private val ioLock = Any()

    init {
        sessionsOpened.incrementAndGet()
    }

    val readCount = AtomicLong(0)
    val bytesRead = AtomicLong(0)
    val seekCount = AtomicLong(0)
    val sizeCount = AtomicLong(0)

    /** The structure handed to `pe_open`; strongly referenced by this session. */
    val io: SongIo = SongIo().apply {
        tokenSlot.setLong(0, TOKEN_ALLOCATOR.incrementAndGet())
        userdata = tokenSlot
        read = READ_CB
        seek = SEEK_CB
        size = SIZE_CB
    }

    private val token: Long get() = tokenSlot.getLong(0)

    fun read(dst: Pointer, size: Long): Long {
        synchronized(ioLock) {
            totalReads.incrementAndGet()
            readCount.incrementAndGet()
            if (size <= 0) return 0
            return try {
                val buffer: ByteBuffer = dst.getByteBuffer(0, size).slice()
                val n = channel.read(buffer)
                if (n < 0) 0 // FileChannel EOF -> contract EOF 0
                else {
                    totalBytes.addAndGet(n.toLong())
                    bytesRead.addAndGet(n.toLong())
                    n.toLong()
                }
            } catch (e: Exception) {
                -1L
            }
        }
    }

    fun seek(absoluteOffset: Long): Long {
        synchronized(ioLock) {
            totalSeeks.incrementAndGet()
            seekCount.incrementAndGet()
            return try {
                if (absoluteOffset < 0) return -1L
                channel.position(absoluteOffset)
                absoluteOffset
            } catch (e: Exception) {
                -1L
            }
        }
    }

    fun size(): Long {
        synchronized(ioLock) {
            totalSizes.incrementAndGet()
            sizeCount.incrementAndGet()
            return try {
                channel.size()
            } catch (e: Exception) {
                -1L
            }
        }
    }

    override fun close() {
        registry.remove(token, this)
        channel.close()
        sessionsClosed.incrementAndGet()
    }

    companion object {
        private val TOKEN_ALLOCATOR = AtomicLong(0)

        // Process-wide callback instrumentation (test/proof probe, not
        // product state): how many host-I/O upcalls the JVM actually served.
        val totalReads = AtomicLong(0)
        val totalSeeks = AtomicLong(0)
        val totalSizes = AtomicLong(0)
        val totalBytes = AtomicLong(0)
        val sessionsOpened = AtomicLong(0)
        val sessionsClosed = AtomicLong(0)
        
        

        // Token -> session. Entries live exactly as long as their session;
        // the adapter removes them only after the engine is destroyed.
        private val registry = ConcurrentHashMap<Long, SongIoSession>()

        // Companion-held callback singletons: strongly reachable forever,
        // so the native trampolines embedded in the engine's copied song_io
        // can never dangle. GC stress cannot collect them.
        private val READ_CB = object : SongReadFn {
            override fun invoke(userdata: Pointer?, dst: Pointer?, size: Long): Long {
                val session = resolve(userdata) ?: return -1L
                return if (dst != null) session.read(dst, size) else -1L
            }
        }
        private val SEEK_CB = object : SongSeekFn {
            override fun invoke(userdata: Pointer?, absoluteOffset: Long): Long {
                val session = resolve(userdata) ?: return -1L
                return session.seek(absoluteOffset)
            }
        }
        private val SIZE_CB = object : SongSizeFn {
            override fun invoke(userdata: Pointer?): Long {
                val session = resolve(userdata) ?: return -1L
                return session.size()
            }
        }

        private fun resolve(userdata: Pointer?): SongIoSession? {
            if (userdata == null) return null
            return try {
                registry[userdata.getLong(0)]
            } catch (e: Exception) {
                null
            }
        }

        fun open(path: Path): SongIoSession {
            val session = SongIoSession(path)
            registry[session.token] = session
            return session
        }
    }
}
