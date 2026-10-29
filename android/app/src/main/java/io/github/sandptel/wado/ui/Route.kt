package io.github.sandptel.wado.ui

/**
 * Which of the app's screens is up. `Web` is the bundled client (the long tail — computer page,
 * session, trackpad, files, shells, its settings — until each is ported); the rest are native.
 */
enum class Route(val depth: Int) {
    Home(0),
    Web(1),
    Pair(1),
    Settings(1),
    About(2),
    ;

    /** Where system Back goes from a native screen (Web asks the page first). */
    fun back(): Route? = when (this) {
        Home -> null
        About -> Settings
        else -> Home
    }
}
