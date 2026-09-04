package qianqian.desktop.player

/**
 * Typed Desktop bridge failures. Machine logic branches on the native
 * numeric status codes carried by these types — never on diagnostic
 * strings ([PlayerSnapshot.lastError] exists for logs only).
 *
 * The adapter reports failures; it never decides recovery (retry, skip,
 * dialogs). Product recovery policy belongs above the bridge.
 */
sealed class PlayerBridgeException(message: String) : RuntimeException(message)

/** The runtime library could not be loaded at the requested path. */
class RuntimeLoadFailure(libraryPath: String, reason: String) :
    PlayerBridgeException("cannot load runtime '$libraryPath': $reason")

/** A loaded runtime reports a different ABI version than the frozen one. */
class AbiMismatch(
    val component: String,
    val expected: Int,
    val actual: Int,
) : PlayerBridgeException(
    "$component ABI mismatch: expected v$expected, runtime reports v$actual",
)

/** `pe_create` returned NULL. */
class EngineCreationFailure :
    PlayerBridgeException("pe_create returned NULL")

/** The local source file could not be opened JVM-side (before `pe_open`). */
class SourceOpenFailure(override val cause: Throwable) :
    PlayerBridgeException("cannot open source: $cause")

/** `pe_open` failed; [songStatus] is the failing SongCore status code. */
class OpenFailure(
    val peStatus: Int,
    val songStatus: Int,
) : PlayerBridgeException("pe_open failed: pe_status=$peStatus song_status=$songStatus")

/** A control call (`play`/`pause`/`seek`/`stop`) failed. */
class ControlFailure(
    val operation: String,
    val peStatus: Int,
    val songStatus: Int,
) : PlayerBridgeException(
    "$operation failed: pe_status=$peStatus song_status=$songStatus",
)

/** `pe_get_snapshot` failed. */
class SnapshotFailure(
    val peStatus: Int,
) : PlayerBridgeException("pe_get_snapshot failed: pe_status=$peStatus")

/** A command was issued after the adapter was closed. */
class BridgeClosedException :
    PlayerBridgeException("native player adapter is closed")
