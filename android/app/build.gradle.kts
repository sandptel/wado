import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    // Navigation 3 keys are @Serializable, so the back stack survives process death.
    alias(libs.plugins.kotlin.serialization)
}

// One version for the whole of wado's client: the web client's crate version is the app's.
val clientVersion: String = run {
    val toml = rootDir.resolve("../crates/client/Cargo.toml").readText()
    Regex("""(?m)^version\s*=\s*"([^"]+)"""").find(toml)?.groupValues?.get(1) ?: "0.0.0"
}
// 1.2.3 → 10203. Monotonic as long as minor and patch stay under 100.
val clientVersionCode: Int = clientVersion.split('.', '-').take(3)
    .map { it.toIntOrNull() ?: 0 }.let { (a, b, c) -> a * 10000 + b * 100 + c }.coerceAtLeast(1)

// Release signing (docs/android-release.md): a keystore the user holds, named by
// android/keystore.properties or WADO_KEYSTORE_* variables. Never committed. Without one, a
// release build is debug-signed and says so in its file name.
val keystore: Properties? = run {
    val f = rootDir.resolve("keystore.properties")
    when {
        f.exists() -> Properties().apply { f.inputStream().use { load(it) } }
        System.getenv("WADO_KEYSTORE_FILE") != null -> Properties().apply {
            setProperty("storeFile", System.getenv("WADO_KEYSTORE_FILE"))
            setProperty("storePassword", System.getenv("WADO_KEYSTORE_PASSWORD") ?: "")
            setProperty("keyAlias", System.getenv("WADO_KEY_ALIAS") ?: "wado")
            setProperty("keyPassword", System.getenv("WADO_KEY_PASSWORD") ?: "")
        }
        else -> null
    }
}

android {
    namespace = "io.github.sandptel.wado"
    compileSdk = 37
    // The ones the Nix shell's SDK carries (flake.nix); AGP's defaults would try to download.
    buildToolsVersion = "37.0.0"
    // Used here only to strip the Rust core's .so (scripts/android-core.sh builds it).
    ndkVersion = "29.0.14206865"

    // The Rust core's Kotlin bindings, generated from its .so (scripts/android-core.sh).
    sourceSets.getByName("main") {
        kotlin.srcDir("build/generated/uniffi")
    }

    defaultConfig {
        applicationId = "io.github.sandptel.wado"
        minSdk = 26
        targetSdk = 37
        versionCode = clientVersionCode
        versionName = clientVersion
        // Static shortcuts name their target package; it differs for the debug build.
        resValue("string", "package_name", "io.github.sandptel.wado")
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        if (keystore != null) {
            create("release") {
                storeFile = file(keystore.getProperty("storeFile"))
                storePassword = keystore.getProperty("storePassword")
                keyAlias = keystore.getProperty("keyAlias")
                keyPassword = keystore.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            // Off for now: nothing here is large enough to need shrinking, and the WebView's
            // message listener and the Rust core's bindings are reached reflectively.
            isMinifyEnabled = false
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
            resValue("string", "package_name", "io.github.sandptel.wado.debug")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
        resValues = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // The bundled web client (scripts/android-build.sh copies `dx build` output here). Its
    // .wasm must not be compressed twice; the asset loader streams it either way.
    androidResources {
        noCompress += listOf("wasm")
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
    }
}

// The AGPL text and the web client's base16 schemes, from the repository, into the APK's assets
// (the About screen; Appearance) — copied at build time rather than kept twice.
abstract class LicenceAssets : DefaultTask() {
    @get:InputFile abstract val licence: RegularFileProperty
    @get:InputFile abstract val schemes: RegularFileProperty
    @get:OutputDirectory abstract val out: DirectoryProperty

    @TaskAction
    fun copy() {
        val dir = out.get().asFile.resolve("licenses").apply { mkdirs() }
        licence.get().asFile.copyTo(dir.resolve("AGPL-3.0.txt"), overwrite = true)
        schemes.get().asFile.copyTo(out.get().asFile.resolve("schemes.json"), overwrite = true)
    }
}

val licenceAssets = tasks.register<LicenceAssets>("licenceAssets") {
    licence.set(rootProject.layout.projectDirectory.file("../LICENSE"))
    schemes.set(rootProject.layout.projectDirectory.file("../crates/client/src/schemes.json"))
}

androidComponents {
    onVariants { variant ->
        variant.sources.assets?.addGeneratedSourceDirectory(licenceAssets, LicenceAssets::out)
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.webkit)
    implementation(libs.androidx.splashscreen)
    implementation(libs.androidx.media)
    implementation(libs.androidx.biometric)
    // biometric 1.1 brings an old Fragment; the activity-result APIs need a current one.
    implementation(libs.androidx.fragment)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.camerax.camera2)
    implementation(libs.camerax.lifecycle)
    implementation(libs.camerax.view)
    implementation(libs.zxing.core)
    implementation(libs.unifiedpush.connector)
    // UniFFI's Kotlin calls the Rust core through JNA (the Android AAR carries its native part).
    implementation(libs.jna) { artifact { type = "aar" } }

    implementation(libs.navigation3.runtime)
    implementation(libs.navigation3.ui)
    implementation(libs.kotlinx.serialization.core)
    implementation(libs.kotlinx.coroutines.android)

    testImplementation(libs.junit)
    // android.jar's org.json is stubs on the JVM; the real one for unit tests.
    testImplementation(libs.org.json)

    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.junit)
    // 3.7: older Espresso reaches for InputManager.getInstance, gone from API 37.
    androidTestImplementation(libs.androidx.test.espresso)
    debugImplementation(libs.compose.ui.test.manifest)
}
