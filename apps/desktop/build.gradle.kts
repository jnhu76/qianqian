import org.jetbrains.compose.desktop.application.dsl.TargetFormat

plugins {
    kotlin("jvm") version "2.4.10"
    id("org.jetbrains.compose") version "1.12.0"
    id("org.jetbrains.kotlin.plugin.compose") version "2.4.10"
}

group = "io.github.jnhu76.qianqian"
version = "0.1.0"

repositories {
    mavenCentral()
    google()
}

val jnaVersion = "5.19.1" // current stable release line (Maven Central, 2026-06)
val coroutinesVersion = "1.11.0"

dependencies {
    implementation(compose.desktop.currentOs)
    implementation("net.java.dev.jna:jna:$jnaVersion")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:$coroutinesVersion")
    testImplementation(kotlin("test"))
}

// ---------------------------------------------------------------------
// Native runtime staging.
//
// Gradle owns NO native knowledge here: it only invokes the repository
// Xmake workspace (the sole native build authority) and copies the
// canonical runtime artifact into the app-owned development staging
// location the application consumes. The application never reads the
// repository build output directly.
// ---------------------------------------------------------------------

val repoRoot = rootDir.parentFile.parentFile // apps/desktop -> repository root
val nativePlatform =
    if (System.getProperty("os.name").lowercase().contains("windows")) {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    }
val runtimeArtifact = if (nativePlatform == "windows-x86_64") {
    File(repoRoot, "build/artifacts/windows-mingw-x86_64/runtime/qianqian.dll")
} else {
    File(repoRoot, "build/artifacts/runtime/libqianqian.so")
}

tasks.register<Exec>("buildNativeRuntime") {
    group = "qianqian native bridge"
    description =
        "Build the qianqian runtime through the repository Xmake workspace."
    workingDir = repoRoot
    commandLine("xmake", "build", "-y", "qianqian_runtime")
}

tasks.register<Copy>("stageNativeRuntime") {
    group = "qianqian native bridge"
    description =
        "Stage the canonical runtime artifact into build/native-dev/$nativePlatform/."
    dependsOn("buildNativeRuntime")
    from(runtimeArtifact)
    into(layout.buildDirectory.dir("native-dev/$nativePlatform"))
}

// Packaged-distribution runtime (§ packaged app image): the canonical
// artifact is staged into the Compose app resources; the launcher sets
// `compose.application.resources.dir` at runtime, which the bridge's
// resolver consumes. No repository build path, PATH, or working directory
// participates in the packaged app.
val appResourcesDir = layout.projectDirectory.dir("resources")

tasks.register<Copy>("stagePackagedNativeRuntime") {
    group = "qianqian native bridge"
    description = "Stage the canonical runtime artifact into the app resources."
    dependsOn("buildNativeRuntime")
    from(runtimeArtifact)
    into(appResourcesDir.dir("native/$nativePlatform"))
}

// ---------------------------------------------------------------------
// Tests: `test` covers pure JVM bridge logic (no native artifact needed);
// `nativeBridgeIntegrationTest` is a separate task for the real-runtime
// proof, so staging is an explicit developer action, not a hidden unit
// test dependency.
// ---------------------------------------------------------------------

sourceSets.create("integrationTest") {
    compileClasspath += sourceSets["main"].output
    runtimeClasspath += sourceSets["main"].output
}

configurations["integrationTestImplementation"]
    .extendsFrom(configurations.testImplementation.get())

dependencies {
    "integrationTestImplementation"(kotlin("test"))
}

tasks.register<Test>("nativeBridgeIntegrationTest") {
    group = "verification"
    description =
        "Real-runtime bridge lifecycle proof against the staged native runtime."
    // Staging is cheap and deterministic (incremental Xmake + one copy),
    // so the task graph owns it; the unit `test` task never stages.
    dependsOn("stageNativeRuntime")
    testClassesDirs = sourceSets["integrationTest"].output.classesDirs
    classpath = sourceSets["integrationTest"].runtimeClasspath
    shouldRunAfter(tasks.test)
    // JNA binds through classic JNI; enable-native-access keeps the JDK
    // quiet about restricted native access on JDK 24+.
    jvmArgs("--enable-native-access=ALL-UNNAMED")
}

tasks.test {
    jvmArgs("--enable-native-access=ALL-UNNAMED")
}

compose.desktop {
    application {
        mainClass = "qianqian.desktop.MainKt"
        // Skiko loads native code; required on JDK 25+ where restricted
        // methods are warned (and will be denied in a future release).
        jvmArgs("--enable-native-access=ALL-UNNAMED")

        nativeDistributions {
            targetFormats(TargetFormat.Msi, TargetFormat.Exe)
            packageName = "Qianqian"
            packageVersion = "0.1.0"
            appResourcesRootDir.set(appResourcesDir)
        }
    }
}

// The resources sync must see the staged runtime before it packages.
tasks.matching { it.name == "prepareAppResources" }.configureEach {
    dependsOn("stagePackagedNativeRuntime")
}
