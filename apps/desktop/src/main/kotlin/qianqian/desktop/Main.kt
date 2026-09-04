package qianqian.desktop

import androidx.compose.ui.window.application
import qianqian.desktop.app.QianqianApp

fun main() = application {
    QianqianApp(onExit = { exitApplication() })
}
