package qianqian.desktop.app

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import qianqian.desktop.player.PlayerPort
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Application-level runtime wiring: turns the startup connection
 * ([Deferred]<[PlayerPort]>) into the screen's live state and owns the
 * exit path that closes the player exactly once before the app exits.
 *
 * This is composition-root glue, NOT a second coordinator above the
 * screen: [PlayerScreenModel] owns the player workflow; [AppRuntime] only
 * decides which screen to show (loading / ready / runtime-unavailable) and
 * guarantees `PlayerPort.close()` completes on window close — without
 * blocking the Compose EDT.
 */
class AppRuntime(
    private val appScope: CoroutineScope,
    private val portDeferred: Deferred<PlayerPort>,
) {
    sealed interface Status {
        data object Loading : Status

        data class Ready(val model: PlayerScreenModel) : Status

        data class Unavailable(val error: PlayerUiError) : Status
    }

    private val _status = MutableStateFlow<Status>(Status.Loading)
    val status: StateFlow<Status> = _status.asStateFlow()

    private val exitRequested = AtomicBoolean(false)

    init {
        appScope.launch {
            _status.value = try {
                Status.Ready(
                    PlayerScreenModel(
                        port = portDeferred.await(),
                        picker = AwtFileDialogPicker,
                        parentScope = appScope,
                    ),
                )
            } catch (e: Exception) {
                // Startup bridge loading failure must be a visible
                // application state, never a crash before the window.
                println("qianqian: playback runtime unavailable: $e")
                Status.Unavailable(PlayerUiError(PlayerUiErrorCategory.RuntimeUnavailable))
            }
        }
    }

    /**
     * Window close request: close the screen model / player port first,
     * THEN exit the application event loop. Idempotent; never blocks the
     * EDT (teardown runs on the app scope).
     */
    fun requestExit(onExit: () -> Unit) {
        if (!exitRequested.compareAndSet(false, true)) return
        appScope.launch {
            try {
                when (val current = _status.value) {
                    is Status.Ready -> current.model.close()
                    // Still connecting or connect failed: await the
                    // connection outcome, then close the port if one was
                    // created. close() itself is idempotent.
                    Status.Loading, is Status.Unavailable -> {
                        try {
                            portDeferred.await().close()
                        } catch (_: Exception) {
                            // connect failed — nothing to close
                        }
                    }
                }
            } finally {
                onExit()
            }
        }
    }
}
