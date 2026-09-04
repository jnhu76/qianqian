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
     */
    fun devStagedLibraryPath(buildDirectory: Path): Path {
        val os = System.getProperty("os.name").lowercase()
        val arch = System.getProperty("os.arch").lowercase()
        val platform = when {
            os.contains("win") -> "windows-x86_64"
            os.contains("linux") && (arch == "amd64" || arch == "x86_64") -> "linux-x86_64"
            os.contains("mac") -> "macos-x86_64"
            else -> error("unsupported desktop platform: $os/$arch")
        }
        val fileName = if (platform.startsWith("windows")) "qianqian.dll" else "libqianqian.so"
        return Paths.get(buildDirectory.toString(), "native-dev", platform, fileName)
    }
}
