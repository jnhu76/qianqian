package qianqian.desktop.app

import java.util.concurrent.atomic.AtomicInteger
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import qianqian.desktop.player.PlayerPort
import qianqian.desktop.player.RuntimeLoadFailure

/**
 * Deterministic lifecycle tests for [AppRuntime], the composition root:
 * Ready exit, Unavailable exit, double-exit idempotence, and exit during
 * a still-pending startup connection. No Compose, no native.
 */
class AppRuntimeTest {

    private val scopes = mutableListOf<CoroutineScope>()

    @AfterTest
    fun cleanup() {
        scopes.forEach { it.cancel() }
    }

    private fun newScope(): CoroutineScope =
        CoroutineScope(SupervisorJob() + Dispatchers.Default).also { scopes.add(it) }

    private suspend fun awaitUntil(timeoutMs: Long = 5_000, condition: () -> Boolean) {
        withTimeout(timeoutMs) {
            while (!condition()) delay(10)
        }
    }

    // ---- ready path ------------------------------------------------------

    @Test
    fun `ready runtime closes model and port once on exit`() = runBlocking {
        val scope = newScope()
        val port = FakePlayerPort()
        val runtime = AppRuntime(
            appScope = scope,
            portDeferred = CompletableDeferred<PlayerPort>().apply { complete(port) },
        )
        awaitUntil { runtime.status.value is AppRuntime.Status.Ready }

        val exits = AtomicInteger(0)
        runtime.requestExit { exits.incrementAndGet() }
        awaitUntil { exits.get() == 1 }

        assertEquals(1, port.closeCalls.get()) // model drained, port closed once
    }

    // ---- failure path ------------------------------------------------------

    @Test
    fun `failed connection becomes Unavailable and exit still runs once`() = runBlocking {
        val scope = newScope()
        val deferred = CompletableDeferred<PlayerPort>()
        deferred.completeExceptionally(RuntimeLoadFailure("libqianqian.so", "missing"))
        val runtime = AppRuntime(appScope = scope, portDeferred = deferred)

        awaitUntil { runtime.status.value is AppRuntime.Status.Unavailable }

        val exits = AtomicInteger(0)
        runtime.requestExit { exits.incrementAndGet() }
        awaitUntil { exits.get() == 1 }
    }

    // ---- double exit -------------------------------------------------------

    @Test
    fun `double requestExit shuts down and exits exactly once`() = runBlocking {
        val scope = newScope()
        val port = FakePlayerPort()
        val runtime = AppRuntime(
            appScope = scope,
            portDeferred = CompletableDeferred<PlayerPort>().apply { complete(port) },
        )
        awaitUntil { runtime.status.value is AppRuntime.Status.Ready }

        val exits = AtomicInteger(0)
        runtime.requestExit { exits.incrementAndGet() }
        runtime.requestExit { exits.incrementAndGet() } // second call must be a no-op
        awaitUntil { exits.get() == 1 }

        delay(50) // bounded settle: absence-of-event check
        assertEquals(1, exits.get())
        assertEquals(1, port.closeCalls.get())
    }

    // ---- loading exit --------------------------------------------------------

    @Test
    fun `exit during pending connection waits for the outcome then closes the port`() =
        runBlocking {
            val scope = newScope()
            val port = FakePlayerPort()
            val deferred = CompletableDeferred<PlayerPort>()
            val runtime = AppRuntime(appScope = scope, portDeferred = deferred)
            assertTrue(runtime.status.value is AppRuntime.Status.Loading)

            val exits = AtomicInteger(0)
            runtime.requestExit { exits.incrementAndGet() }

            // MVP semantics: the exit waits for the connection outcome — a
            // hung startup connection delays exit (documented limitation).
            delay(50)
            assertEquals(0, exits.get())

            deferred.complete(port)
            awaitUntil { exits.get() == 1 }
            assertEquals(1, port.closeCalls.get())
        }
}
