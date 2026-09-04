package qianqian.desktop.app

import kotlin.test.Test
import kotlin.test.assertEquals

class QianqianAppTest {
    @Test
    fun windowTitleIsProductName() {
        assertEquals("Qianqian", QianqianWindow.TITLE)
    }
}
