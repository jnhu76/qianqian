package qianqian.desktop

import androidx.compose.ui.window.application
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import qianqian.desktop.app.QianqianApp
import qianqian.desktop.nativebridge.NativePlayerAdapter
import qianqian.desktop.nativebridge.NativeRuntimeLoader
import qianqian.desktop.player.PlayerPort
import java.nio.file.Path
import java.nio.file.Paths

/**
 * Application entry point.
 *
 * Note: `onExit` runs on the appScope teardown coroutine — it only mutates
 * a Compose snapshot, and Compose schedules the application closure back
 * onto the main event loop (verified against Compose Desktop 1.12.0
 * behavior). No main-dispatcher abstraction is added for this.
 */
fun main() {
    // The one application-lifetime scope (no GlobalScope). It hosts the
    // startup connection and the exit-path teardown.
    val appScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    // Startup initialization: connect the production player bridge while
    // the window is already visible (it shows "Loading playback runtime…"
    // until the connection lands, and a truthful unavailable state if the
    // staged runtime is missing or rejected by the ABI gates).
    val port: Deferred<PlayerPort> = appScope.async {
        NativePlayerAdapter.connect(runtimePath())
    }

    application {
        QianqianApp(
            appScope = appScope,
            port = port,
            onExit = { exitApplication() },
        )
    }
}

/**
 * The runtime location resolved through the bridge's one explicit resolver:
 * a packaged app image consumes its bundled copy (Compose sets
 * `compose.application.resources.dir`); development (`./gradlew run`, cwd =
 * `apps/desktop`) consumes the app-owned staging location. The repository
 * build output, PATH, and the working directory never participate.
 */
private fun runtimePath(): Path =
    NativeRuntimeLoader.resolveLibraryPath(Paths.get(System.getProperty("user.dir"), "build"))
