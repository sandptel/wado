package io.github.sandptel.wado.ui.theme

import kotlin.math.cbrt
import kotlin.math.pow

/**
 * The user's base16 scheme and the roles DESIGN.md gives its slots. Every colour on the app's
 * own screens comes from here: the sixteen slots, and mixes of them in OKLab — the same
 * `color-mix(in oklab, …)` the web client's tokens are (crates/client/assets/base.css). Pure
 * (ARGB ints), so the maths is unit-tested.
 */
class Base16(val slots: List<Int>) {
    init {
        require(slots.size == 16) { "a base16 scheme has 16 slots" }
    }

    fun base(i: Int) = slots[i]

    val ground get() = slots[0x0]
    val surface1 get() = mix(slots[0x0], slots[0x5], 0.06f)
    val surface2 get() = mix(slots[0x0], slots[0x5], 0.11f)
    val surface3 get() = mix(slots[0x0], slots[0x5], 0.18f)
    val line get() = mix(slots[0x0], slots[0x5], 0.14f)
    val bright get() = slots[0x5]
    val text get() = mix(slots[0x5], slots[0x0], 0.10f)
    /**
     * The web's 42 % mix toward the ground — eased off, two points at a time, until it holds 3:1 on
     * surface-2 (ContrastTest: light schemes such as catppuccin-latte fell to 2.4:1). Dark schemes
     * keep the web's exact value.
     */
    val dim: Int
        get() {
            var t = 0.42f
            var c = mix(slots[0x5], slots[0x0], t)
            while (t > 0.1f && contrast(c, surface2) < 3.0) {
                t -= 0.02f
                c = mix(slots[0x5], slots[0x0], t)
            }
            return c
        }
    val accent get() = slots[0xD]
    val onAccent get() = slots[0x0]
    val good get() = slots[0xB]
    val warn get() = slots[0x9]
    val bad get() = slots[0x8]

    /** A light ground, by base00's luminance — the same test as js/theme.js. */
    val light: Boolean
        get() {
            val n = slots[0]
            val lum = 0.2126 * ((n shr 16) and 255) + 0.7152 * ((n shr 8) and 255) + 0.0722 * (n and 255)
            return lum > 140
        }

    companion object {
        /** default-dark — what the page starts in before any scheme is chosen. */
        val DEFAULT = parse(
            listOf(
                "181818", "282828", "383838", "585858", "b8b8b8", "d8d8d8", "e8e8e8", "f8f8f8",
                "ab4642", "dc9656", "f7ca88", "a1b56c", "86c1b9", "7cafc2", "ba8baf", "a16946",
            ),
        )!!

        /** 16 bare hex triples, base00 first; null for anything else (never half-applied). */
        fun parse(hexes: List<String>?): Base16? {
            if (hexes == null || hexes.size != 16) return null
            val slots = hexes.map { h ->
                val s = h.trim().removePrefix("#")
                if (s.length != 6) return null
                (s.toLongOrNull(16) ?: return null).toInt() or (0xFF shl 24)
            }
            return Base16(slots)
        }

        /** WCAG contrast ratio of two opaque colours. */
        fun contrast(a: Int, b: Int): Double {
            fun ch(v: Int): Double = (v / 255.0).let { if (it <= 0.03928) it / 12.92 else ((it + 0.055) / 1.055).pow(2.4) }
            fun lum(c: Int) = 0.2126 * ch((c shr 16) and 255) + 0.7152 * ch((c shr 8) and 255) + 0.0722 * ch(c and 255)
            val (x, y) = lum(a) to lum(b)
            return (maxOf(x, y) + 0.05) / (minOf(x, y) + 0.05)
        }

        /** `color-mix(in oklab, a, b t)`: t of b into a, opaque. */
        fun mix(a: Int, b: Int, t: Float): Int {
            val x = toOklab(a)
            val y = toOklab(b)
            return fromOklab(
                floatArrayOf(
                    x[0] + (y[0] - x[0]) * t,
                    x[1] + (y[1] - x[1]) * t,
                    x[2] + (y[2] - x[2]) * t,
                ),
            )
        }

        private fun lin(c: Int): Float {
            val v = c / 255f
            return if (v <= 0.04045f) v / 12.92f else ((v + 0.055f) / 1.055f).toDouble().pow(2.4).toFloat()
        }

        private fun gam(v: Float): Int {
            val c = if (v <= 0.0031308f) v * 12.92f else (1.055 * v.toDouble().pow(1 / 2.4) - 0.055).toFloat()
            return (c * 255f + 0.5f).toInt().coerceIn(0, 255)
        }

        fun toOklab(argb: Int): FloatArray {
            val r = lin((argb shr 16) and 255)
            val g = lin((argb shr 8) and 255)
            val b = lin(argb and 255)
            val l = cbrt(0.4122214708f * r + 0.5363325363f * g + 0.0514459929f * b)
            val m = cbrt(0.2119034982f * r + 0.6806995451f * g + 0.1073969566f * b)
            val s = cbrt(0.0883024619f * r + 0.2817188376f * g + 0.6299787005f * b)
            return floatArrayOf(
                0.2104542553f * l + 0.7936177850f * m - 0.0040720468f * s,
                1.9779984951f * l - 2.4285922050f * m + 0.4505937099f * s,
                0.0259040371f * l + 0.7827717662f * m - 0.8086757660f * s,
            )
        }

        fun fromOklab(lab: FloatArray): Int {
            val l = lab[0] + 0.3963377774f * lab[1] + 0.2158037573f * lab[2]
            val m = lab[0] - 0.1055613458f * lab[1] - 0.0638541728f * lab[2]
            val s = lab[0] - 0.0894841775f * lab[1] - 1.2914855480f * lab[2]
            val l3 = l * l * l
            val m3 = m * m * m
            val s3 = s * s * s
            val r = 4.0767416621f * l3 - 3.3077115913f * m3 + 0.2309699292f * s3
            val g = -1.2684380046f * l3 + 2.6097574011f * m3 - 0.3413193965f * s3
            val b = -0.0041960863f * l3 - 0.7034186147f * m3 + 1.7076147010f * s3
            return (0xFF shl 24) or (gam(r) shl 16) or (gam(g) shl 8) or gam(b)
        }
    }
}
