package io.github.sandptel.wado.ui.pair

import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.ReaderException
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader

/**
 * QR codes from a camera frame's luminance plane, with ZXing's core (Apache-2.0, pure Java —
 * no Google Play services). Pure, so it is unit-tested against codes ZXing itself draws.
 */
object QrDecoder {
    private val hints = mapOf(
        DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE),
        DecodeHintType.TRY_HARDER to true,
    )

    /** `y`: the Y plane, `rowStride` bytes a row. Null when no code is in the frame. */
    fun decode(y: ByteArray, width: Int, height: Int, rowStride: Int = width): String? {
        val source = PlanarYUVLuminanceSource(y, rowStride, height, 0, 0, width, height, false)
        return try {
            QRCodeReader().decode(BinaryBitmap(HybridBinarizer(source)), hints).text
        } catch (_: ReaderException) {
            // A phone screen or a mirrored preview can show the code inverted.
            try {
                QRCodeReader().decode(BinaryBitmap(HybridBinarizer(source.invert())), hints).text
            } catch (_: ReaderException) {
                null
            }
        }
    }
}
