package qianqian.desktop.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.MaterialTheme
import androidx.compose.material.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.rememberWindowState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import qianqian.desktop.player.PlayerPort

object QianqianWindow {
    const val TITLE = "Qianqian"
    val DEFAULT_SIZE = DpSize(520.dp, 360.dp)
}

/**
 * The application window: one window, one screen. Shows the player screen
 * once the runtime connects, or a truthful unavailable state when the
 * staged runtime cannot be loaded — the window always appears.
 */
@Composable
fun QianqianApp(
    appScope: CoroutineScope,
    port: Deferred<PlayerPort>,
    onExit: () -> Unit,
) {
    val runtime = remember { AppRuntime(appScope, port) }
    val status by runtime.status.collectAsState()

    Window(
        onCloseRequest = { runtime.requestExit(onExit) },
        title = QianqianWindow.TITLE,
        state = rememberWindowState(size = QianqianWindow.DEFAULT_SIZE),
    ) {
        when (val current = status) {
            AppRuntime.Status.Loading -> LoadingContent()
            is AppRuntime.Status.Unavailable -> UnavailableContent(current.error)
            is AppRuntime.Status.Ready -> PlayerScreen(current.model)
        }
    }
}

@Composable
private fun LoadingContent() {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        horizontalAlignment = Alignment.Start,
    ) {
        Text("Qianqian")
        Text("Loading playback runtime…")
    }
}

@Composable
private fun UnavailableContent(error: PlayerUiError) {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        horizontalAlignment = Alignment.Start,
    ) {
        Text("Qianqian")
        Text(error.message, color = MaterialTheme.colors.error)
    }
}
