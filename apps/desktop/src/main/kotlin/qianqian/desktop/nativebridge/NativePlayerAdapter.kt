package qianqian.desktop.nativebridge

import com.sun.jna.Pointer
import com.sun.jna.ptr.IntByReference
import com.sun.jna.ptr.LongByReference
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExecutorCoroutineDispatcher
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import qianqian.desktop.player.BridgeClosedException
import qianqian.desktop.player.ControlFailure
import qianqian.desktop.player.EngineCreationFailure
import qianqian.desktop.player.OpenFailure
import qianqian.desktop.player.PlayerBridgeException
import qianqian.desktop.player.PlayerPort
import qianqian.desktop.player.PlayerSnapshot
import qianqian.desktop.player.PlayerState
import qianqian.desktop.player.PositionQuality
import qianqian.desktop.player.SourceOpenFailure
import java.nio.file.Path
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Production [PlayerPort] over the frozen native runtime.
 *
 * Threading model (DESKTOP-IA-1 frozen rules):
 *  - ALL control calls ([open]/[play]/[pause]/[seek]/[stop]/engine
 *    create/destroy) run serialized on ONE confined single-thread
 *    dispatcher ("qianqian-native-control"); they never race and never run
 *    on a Compose/UI thread — callers reach them through the suspend
 *    boundary only.
 *  - `pe_get_snapshot` is contract-legal concurrently with control calls;
 *    a dedicated poller coroutine reads it at 10 Hz (frozen 5–10 Hz band)
 *    and publishes immutable [PlayerSnapshot] values into a StateFlow.
 *    There are no native->Kotlin callbacks for observation.
 *  - `song_io` upcalls arrive on native decode threads; they touch only
 *    bounded file I/O inside [SongIoSession] (see its lifetime model).
 *
 * State truth: native owns playback state. This adapter never invents a
 * transition; the StateFlow only ever carries what `pe_get_snapshot`
 * reported. Errors are typed on native status codes; diagnostic strings
 * are never parsed. The adapter reports failure — it applies no product
 * recovery policy (retry/skip/dialog decisions belong above the bridge).
 */
class NativePlayerAdapter private constructor(
    private val api: NativeApi,
    private val engine: Pointer,
) : PlayerPort {

    private val controlDispatcher: ExecutorCoroutineDispatcher =
        Executors.newSingleThreadExecutor { r ->
            Thread(r, "qianqian-native-control").apply { isDaemon = true }
        }.asCoroutineDispatcher()

    /** Internal scope hosting the snapshot poller; cancelled at close. */
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    private val _snapshot = MutableStateFlow(PlayerSnapshot.empty())
    override val snapshot: StateFlow<PlayerSnapshot> = _snapshot.asStateFlow()

    @Volatile
    private var session: SongIoSession? = null

    private val closed = AtomicBoolean(false)

    private var poller: Job? = null

    init {
        poller = scope.launch { pollSnapshots() }
    }

    // ---- PlayerPort -----------------------------------------------------

    override suspend fun open(source: Path): Unit = withControl {
        val newSession = try {
            SongIoSession.open(source)
        } catch (e: Exception) {
            throw SourceOpenFailure(e)
        }
        val songStatus = IntByReference()
        val st = api.peOpen(engine, newSession.io, songStatus)
        if (st != PeStatus.OK) {
            newSession.close()
            throw OpenFailure(st, songStatus.value)
        }
        // The engine quiesced and dropped the previous handle during
        // pe_open; only now may the previous session's file be released.
        session?.close()
        session = newSession
    }

    override suspend fun play(): Unit = control("play") {
        api.pePlay(engine) to SONG_OK
    }

    override suspend fun pause(): Unit = control("pause") {
        api.pePause(engine) to SONG_OK
    }

    override suspend fun seek(positionUs: Long): Long = withControl {
        val landing = LongByReference(-1L)
        val songStatus = IntByReference()
        val st = api.peSeek(engine, positionUs, landing, songStatus)
        if (st != PeStatus.OK) {
            throw ControlFailure("seek", st, songStatus.value)
        }
        landing.value
    }

    override suspend fun stop(): Unit = control("stop") {
        val songStatus = IntByReference()
        val st = api.peStop(engine, songStatus)
        st to songStatus.value
    }

    override suspend fun close() {
        if (!closed.compareAndSet(false, true)) return
        // Stop observation FIRST: no pe_get_snapshot may fire after (or
        // during) destroy.
        poller?.cancelAndJoin()
        withContext(controlDispatcher) {
            api.peDestroy(engine)
        }
        session?.close()
        session = null
        scope.cancel()
        controlDispatcher.close()
    }

    // ---- internals ------------------------------------------------------

    /**
     * Run a control call on the confined dispatcher and map a non-OK
     * `pe_status` (with its accompanying SongCore status) to
     * [ControlFailure]. Rejects closed adapters predictably.
     */
    private suspend inline fun control(
        operation: String,
        crossinline call: () -> Pair<Int, Int>,
    ): Unit = withControl {
        val (st, songStatus) = call()
        if (st != PeStatus.OK) throw ControlFailure(operation, st, songStatus)
    }

    private suspend inline fun <T> withControl(crossinline block: () -> T): T {
        if (closed.get()) throw BridgeClosedException() // fast path only
        try {
            return withContext(controlDispatcher) {
                // Authoritative lifecycle gate: re-checked ON the control
                // thread, in FIFO order against the queued pe_destroy. A
                // command that raced close() before the flag flip either
                // runs strictly before destroy (dispatcher serialization)
                // or sees `closed` here and never reaches native.
                if (closed.get()) throw BridgeClosedException()
                block()
            }
        } catch (e: PlayerBridgeException) {
            throw e
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            // e.g. dispatch into the already-shut-down executor after close
            if (closed.get()) throw BridgeClosedException()
            throw e
        }
    }

    /**
     * 10 Hz observation loop. Tolerates a transient snapshot failure
     * (keeps the last good snapshot); after repeated failures it stops
     * itself — the poller must never keep touching an engine that is
     * failing or being torn down. [close] cancels it before destroy.
     */
    private suspend fun pollSnapshots() {
        var consecutiveFailures = 0
        while (currentCoroutineContext().isActive) {
            delay(SNAPSHOT_PERIOD_MS)
            if (closed.get()) return
            val snap = PeSnapshot()
            val st = api.peGetSnapshot(engine, snap)
            if (st != PeStatus.OK) {
                if (++consecutiveFailures >= MAX_CONSECUTIVE_SNAPSHOT_FAILURES) return
                continue
            }
            consecutiveFailures = 0
            _snapshot.value = mapSnapshot(snap)
        }
    }

    companion object {
        /** 10 Hz — inside the frozen 5–10 Hz UI cadence band. */
        const val SNAPSHOT_PERIOD_MS: Long = 100

        private const val MAX_CONSECUTIVE_SNAPSHOT_FAILURES = 3

        /** The SongCore status accompanying pe_* calls without an out param. */
        private const val SONG_OK = 0

        /**
         * Create the engine (on a background thread) and start observation.
         *
         * This is the engine-creation authority: BOTH frozen ABI versions
         * are validated here, before any engine can exist — no matter how
         * the caller obtained the [NativeApi]. A mismatch is a typed
         * fail-fast with `pe_create` never invoked.
         */
        suspend fun connect(api: NativeApi): NativePlayerAdapter {
            NativeRuntimeLoader.validateAbi(api)
            return withContext(Dispatchers.Default) {
                val engine = api.peCreate() ?: throw EngineCreationFailure()
                try {
                    NativePlayerAdapter(api, engine)
                } catch (e: Throwable) {
                    api.peDestroy(engine)
                    throw e
                }
            }
        }

        /** Load the staged runtime, gate the ABI, and connect. */
        suspend fun connect(libraryPath: Path): NativePlayerAdapter =
            connect(NativeRuntimeLoader.load(libraryPath))

        internal fun mapSnapshot(snap: PeSnapshot): PlayerSnapshot = PlayerSnapshot(
            state = mapState(snap.state),
            positionUs = snap.positionUs,
            durationUs = snap.durationUs,
            durationKnown = snap.durationKnown != 0,
            positionEstimated = snap.positionQuality.toInt() == PeQuality.ESTIMATED,
            sampleRate = snap.sampleRate,
            bufferedFrames = snap.bufferedFrames,
            underrunCount = snap.underrunCount,
            lastError = terminatedString(snap.lastError),
        )

        /** `last_error` is a NUL-terminated UTF-8 char[96] (diagnostic only). */
        private fun terminatedString(bytes: ByteArray): String {
            val end = bytes.indexOf(0).let { if (it < 0) bytes.size else it }
            return String(bytes, 0, end, Charsets.UTF_8)
        }

        internal fun mapState(state: Int): PlayerState = when (state) {
            PeState.EMPTY -> PlayerState.EMPTY
            PeState.READY -> PlayerState.READY
            PeState.PLAYING -> PlayerState.PLAYING
            PeState.PAUSED -> PlayerState.PAUSED
            PeState.ENDED -> PlayerState.ENDED
            PeState.ERROR -> PlayerState.ERROR
            else -> PlayerState.ERROR // unknown native state: fail closed
        }
    }
}
