// Versions live in gradle/libs.versions.toml. AGP 9 compiles Kotlin itself (built-in Kotlin);
// the Compose compiler plugin must match the Kotlin it runs with.
plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.kotlin.serialization) apply false
}
