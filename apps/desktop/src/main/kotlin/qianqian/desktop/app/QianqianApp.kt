package qianqian.desktop.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.rememberWindowState

object QianqianWindow {
    const val TITLE = "Qianqian"
    val DEFAULT_SIZE = DpSize(480.dp, 320.dp)
}

@Composable
fun QianqianApp(onExit: () -> Unit) {
    Window(
        onCloseRequest = onExit,
        title = QianqianWindow.TITLE,
        state = rememberWindowState(size = QianqianWindow.DEFAULT_SIZE),
    ) {
        Column(
            modifier = Modifier.fillMaxSize().padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            horizontalAlignment = Alignment.Start,
        ) {
            Text("Qianqian")
            Text("Desktop bootstrap OK")
        }
    }
}
