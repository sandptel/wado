import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
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
    // The one the Nix shell's SDK carries (flake.nix); AGP's default would try to download.
    buildToolsVersion = "37.0.0"

    defaultConfig {
        applicationId = "io.github.sandptel.wado"
        minSdk = 26
        targetSdk = 37
        versionCode = clientVersionCode
        versionName = clientVersion
        // Static shortcuts name their target package; it differs for the debug build.
        resValue("string", "package_name", "io.github.sandptel.wado")
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

// The AGPL text, from the repository root, into the APK's assets for the About screen — copied
// at build time rather than kept twice.
abstract class LicenceAssets : DefaultTask() {
    @get:InputFile abstract val licence: RegularFileProperty
    @get:OutputDirectory abstract val out: DirectoryProperty

    @TaskAction
    fun copy() {
        val dir = out.get().asFile.resolve("licenses").apply { mkdirs() }
        licence.get().asFile.copyTo(dir.resolve("AGPL-3.0.txt"), overwrite = true)
    }
}

val licenceAssets = tasks.register<LicenceAssets>("licenceAssets") {
    licence.set(rootProject.layout.projectDirectory.file("../LICENSE"))
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
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.camerax.camera2)
    implementation(libs.camerax.lifecycle)
    implementation(libs.camerax.view)
    implementation(libs.zxing.core)
    implementation(libs.unifiedpush.connector)

    testImplementation(libs.junit)
    // android.jar's org.json is stubs on the JVM; the real one for unit tests.
    testImplementation(libs.org.json)
}
