package qianqian.desktop.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.Button
import androidx.compose.material.MaterialTheme
import androidx.compose.material.Slider
import androidx.compose.material.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

/**
 * The one minimal player screen: Open File, current track, native state,
 * snapshot timeline, Play/Pause + Stop, and product error text.
 *
 * Every playback value rendered here comes from the projected
 * [PlayerUiState] (native snapshot underneath); no handler mutates
 * playback state locally. Native commands are issued through launched
 * coroutines — never blocking on the Compose UI thread.
 */
@Composable
fun PlayerScreen(model: PlayerScreenModel) {
    val state by model.uiState.collectAsState()
    val screenScope = rememberCoroutineScope()

    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Button(
            onClick = model::openFileViaPicker,
            enabled = state.canOpenFile,
        ) {
            Text("Open File")
        }

        Text("Current track: ${state.selectedFile?.fileName ?: "No file selected."}")

        Text("State: ${stateLabel(state.snapshot.state)}")

        Timeline(
            state = state,
            onPreview = model::onSeekPreview,
            onCommit = model::onSeekCommit,
        )

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(
                onClick = { screenScope.launch { model.togglePlayback() } },
                enabled = state.canTogglePlayback,
            ) {
                Text(if (state.isPlaying) "Pause" else "Play")
            }
            Button(
                onClick = { screenScope.launch { model.stopPlayback() } },
                enabled = state.canStop,
            ) {
                Text("Stop")
            }
        }

        state.error?.let { error ->
            Text(
                text = error.message,
                color = MaterialTheme.colors.error,
            )
        }
    }
}

/**
 * Snapshot timeline. When the duration is unknown (`durationKnown == false`,
 * native `-1`) it renders truthfully as `--:--` with NO slider — never a
 * fake `0:00`-length track. Microsecond Long values stay the state truth;
 * the Float conversion happens only at this UI boundary (Float precision
 * at multi-hour duration scales is well below a millisecond).
 */
@Composable
private fun Timeline(
    state: PlayerUiState,
    onPreview: (Long) -> Unit,
    onCommit: () -> Unit,
) {
    val durationUs = state.snapshot.durationUs
    if (!state.snapshot.durationKnown || durationUs <= 0) {
        Text("${TimeFormat.formatUs(state.displayedPositionUs)} / ${TimeFormat.UNKNOWN}")
        return
    }
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text("${TimeFormat.formatUs(state.displayedPositionUs)} / ${TimeFormat.formatUs(durationUs)}")
        Slider(
            value = state.displayedPositionUs.coerceIn(0L, durationUs).toFloat(),
            onValueChange = { position -> onPreview(position.toLong()) },
            onValueChangeFinished = onCommit,
            valueRange = 0f..durationUs.toFloat(),
            enabled = state.canSeek,
        )
    }
}
