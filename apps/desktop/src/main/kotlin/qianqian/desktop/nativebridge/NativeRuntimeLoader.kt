package qianqian.desktop.nativebridge

import qianqian.desktop.player.AbiMismatch
import java.nio.file.Path
import java.nio.file.Paths

/**
 * Runtime loading: resolve the staged runtime, load it, and gate on both
 * frozen ABI versions BEFORE any engine can exist. A version mismatch is a
 * typed failure — never undefined behavior, never a best-effort continue.
 */
object NativeRuntimeLoader {

    /**
     * Validate the ABI gates of an already-loaded api. Split from
     * [load] so tests can prove fail-fast behavior with a fake api.
     */
    fun validateAbi(api: NativeApi) {
        val song = api.songcoreAbiVersion()
        if (song != QianqianAbi.SONGCORE_ABI_VERSION) {
            throw AbiMismatch(
                "SongCore",
                QianqianAbi.SONGCORE_ABI_VERSION,
                song,
            )
        }
        val pe = api.playerEngineAbiVersion()
        if (pe != QianqianAbi.PLAYER_ENGINE_ABI_VERSION) {
            throw AbiMismatch(
                "PlayerEngine",
                QianqianAbi.PLAYER_ENGINE_ABI_VERSION,
                pe,
            )
        }
    }

    /**
     * Load the runtime from an explicit staged path and gate the ABI.
     * Loading uses the absolute file path directly — the OS/JNA search
     * order (current directory, PATH, jna.library.path) never participates.
     */
    fun load(libraryPath: Path): NativeApi {
        val api = JnaNativeApi.load(libraryPath)
        validateAbi(api)
        return api
    }

    /**
     * The app-owned development staging location for the current platform
     * (`apps/desktop/build/native-dev/<os>-<arch>/<runtime>`). Development
     * mode consumes this staged copy, never the repository build output
     * directory itself; a production/package runtime would ship its own
     * location and pass it explicitly.
     *
     * Current campaign scope is Windows x86_64 (product) + Linux x86_64
     * (dev/WSL); any other host fails closed instead of guessing a layout.
     */
    fun devStagedLibraryPath(buildDirectory: Path): Path {
        val os = System.getProperty("os.name").lowercase()
        val arch = System.getProperty("os.arch").lowercase()
        val isX86_64 = arch == "amd64" || arch == "x86_64"
        val (platform, fileName) = when {
            os.contains("win") && isX86_64 -> "windows-x86_64" to "qianqian.dll"
            os.contains("linux") && isX86_64 -> "linux-x86_64" to "libqianqian.so"
            else -> throw IllegalStateException(
                "unsupported desktop staging platform: $os/$arch " +
                    "(campaign scope: windows-x86_64 + linux-x86_64)",
            )
        }
        return Paths.get(buildDirectory.toString(), "native-dev", platform, fileName)
    }
}
