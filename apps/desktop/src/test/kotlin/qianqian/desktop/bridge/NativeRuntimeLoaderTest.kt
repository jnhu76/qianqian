package qianqian.desktop.bridge

import java.nio.file.Paths
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import qianqian.desktop.nativebridge.NativeRuntimeLoader

/**
 * The one runtime resolver (§ packaged runtime resolution): a packaged app
 * image consumes its bundled copy via Compose's resources-dir property;
 * development consumes the app-owned staged location. No PATH, working
 * directory, or filesystem search participates in either case.
 */
class NativeRuntimeLoaderTest {

    private val buildDir = Paths.get("build")

    @Test
    fun devStagedPathFollowsThePlatformLayout() {
        val p = NativeRuntimeLoader.devStagedLibraryPath(buildDir)
        if (isWindows) {
            assertEquals("qianqian.dll", p.fileName.toString())
            assertEquals("windows-x86_64", p.parent.fileName.toString())
        } else {
            assertEquals("libqianqian.so", p.fileName.toString())
            assertEquals("linux-x86_64", p.parent.fileName.toString())
        }
        assertEquals("native-dev", p.parent.parent.fileName.toString())
    }

    @Test
    fun packagedPathLivesInsideTheAppResources() {
        val p = NativeRuntimeLoader.packagedLibraryPath(Paths.get("app", "resources"))
        assertTrue(p.toString().startsWith(Paths.get("app", "resources").toString()))
        assertEquals("native", p.parent.parent.fileName.toString())
    }

    @Test
    fun resolverPrefersThePackagedResourcesPropertyWhenSet() {
        System.setProperty("compose.application.resources.dir", "C:\\app\\resources")
        try {
            val p = NativeRuntimeLoader.resolveLibraryPath(buildDir)
            assertEquals(
                NativeRuntimeLoader.packagedLibraryPath(Paths.get("C:\\app\\resources")),
                p,
            )
        } finally {
            System.clearProperty("compose.application.resources.dir")
        }
    }

    @Test
    fun resolverFallsBackToTheDevStagedLocationWithoutTheProperty() {
        System.clearProperty("compose.application.resources.dir")
        assertEquals(
            NativeRuntimeLoader.devStagedLibraryPath(buildDir),
            NativeRuntimeLoader.resolveLibraryPath(buildDir),
        )
    }

    @Test
    fun unsupportedHostFailsClosed() {
        val realOs = System.getProperty("os.name")
        System.setProperty("os.name", "SunOS")
        try {
            assertFailsWith<IllegalStateException> {
                NativeRuntimeLoader.packagedLibraryPath(Paths.get("x"))
            }
        } finally {
            System.setProperty("os.name", realOs)
        }
    }

    private val isWindows: Boolean
        get() = System.getProperty("os.name").lowercase().contains("win")
}
