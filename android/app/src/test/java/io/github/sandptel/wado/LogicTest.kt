package io.github.sandptel.wado

import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter
import io.github.sandptel.wado.bridge.BridgeProtocol
import io.github.sandptel.wado.bridge.Computers
import io.github.sandptel.wado.links.DeepLinks
import io.github.sandptel.wado.online.OnlineProbe
import io.github.sandptel.wado.share.ShareParser
import io.github.sandptel.wado.ui.pair.QrDecoder
import io.github.sandptel.wado.ui.theme.Base16
import io.github.sandptel.wado.web.AssetPaths
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.URI

class AssetPathsTest {
    @Test fun root_is_the_page() = assertEquals("client/index.html", AssetPaths.resolve(""))

    @Test fun files_map_under_client() {
        assertEquals("client/assets/wado-client_bg.wasm", AssetPaths.resolve("assets/wado-client_bg.wasm?v=1"))
        assertEquals("client/icons/icon-192.png", AssetPaths.resolve("/icons/icon-192.png"))
    }

    @Test fun nothing_leaves_the_client_directory() {
        assertNull(AssetPaths.resolve("../licenses/AGPL-3.0.txt"))
        assertNull(AssetPaths.resolve("assets/../../x"))
        assertNull(AssetPaths.resolve("a//b"))
    }

    @Test fun routes_fall_back_to_the_page() {
        assertTrue(AssetPaths.isRoute("computer/1"))
        assertFalse(AssetPaths.isRoute("assets/app.js"))
    }

    @Test fun wasm_streams_with_its_own_type() {
        assertEquals("application/wasm", AssetPaths.mimeType("x/y.wasm"))
        assertEquals("text/javascript", AssetPaths.mimeType("a.js"))
        assertEquals("utf-8", AssetPaths.encoding("text/html"))
        assertNull(AssetPaths.encoding("application/wasm"))
    }
}

class DeepLinksTest {
    private val qr = "https://sandptel.github.io/wado/?relay=https%3A%2F%2Fr.example&id=872%20990%20894&pair=ABCD1234&hk=pin%2B1&evil=1"

    @Test fun a_computers_qr_becomes_a_connect_link_on_our_origin() {
        val l = DeepLinks.connect(qr)!!
        assertEquals("872990894", l["id"])
        assertEquals("https://r.example", l["relay"])
        assertTrue(DeepLinks.pairs(l))
        assertFalse("evil" in l)
        assertEquals("?relay=https%3A%2F%2Fr.example&id=872990894&pair=ABCD1234&hk=pin%2B1", DeepLinks.connectQuery(l))
    }

    @Test fun wado_scheme_links_too() {
        assertEquals("1", DeepLinks.connect("wado://pair?id=1")!!["id"])
    }

    @Test fun not_a_connect_link() {
        assertNull(DeepLinks.connect("hello"))
        assertNull(DeepLinks.connect("https://example.com/?q=1"))
        assertNull(DeepLinks.connect("javascript:alert(1)?id=1"))
        assertNull(DeepLinks.connect("https://x/?id=1&relay=javascript:alert(1)"))
    }

    @Test fun doors_match_the_web_shortcuts() {
        assertEquals("session", DeepLinks.door("Desktop"))
        assertEquals("trackpad", DeepLinks.door("trackpad"))
        assertNull(DeepLinks.door("home"))
        val d = DeepLinks.door(URI(DeepLinks.doorLink("files", "https://r|42")))!!
        assertEquals("files", d.door)
        assertEquals("https://r|42", d.key)
        assertNull(DeepLinks.door(URI("wado://go/nope")))
    }
}

class ShareParserTest {
    @Test fun links_are_found_and_tidied() {
        val s = ShareParser.fromText("Look at this: https://example.com/a?b=1. And http://x.y/z)")
        assertEquals(listOf("https://example.com/a?b=1", "http://x.y/z"), s.links)
        assertEquals("2 links", s.describe())
    }

    @Test fun plain_text_is_text() {
        val s = ShareParser.fromText("just words")
        assertTrue(s.links.isEmpty())
        assertEquals("some text", s.describe())
        assertTrue(ShareParser.fromText("  ").isEmpty)
    }
}

class Base16Test {
    @Test fun default_dark_matches_the_web_tokens() {
        val s = Base16.DEFAULT
        assertEquals(0xFF181818.toInt(), s.ground)
        assertEquals(0xFF7CAFC2.toInt(), s.accent)
        assertFalse(s.light)
    }

    @Test fun mixing_is_in_oklab_and_round_trips() {
        val a = 0xFF181818.toInt()
        assertEquals(a, Base16.mix(a, 0xFFD8D8D8.toInt(), 0f))
        assertEquals(0xFFD8D8D8.toInt(), Base16.mix(a, 0xFFD8D8D8.toInt(), 1f))
        // A surface is a little lighter than the ground, never darker.
        val s1 = Base16.DEFAULT.surface1 and 0xFF
        assertTrue(s1 > 0x18)
        assertTrue(Base16.DEFAULT.surface3 and 0xFF > s1)
    }

    @Test fun malformed_schemes_are_refused_whole() {
        assertNull(Base16.parse(listOf("181818")))
        assertNull(Base16.parse(List(16) { "zzzzzz" }))
        assertTrue(Base16.parse(List(16) { "#f9f5d7" })!!.light)
    }
}

class BridgeTest {
    @Test fun computers_parse_from_the_page() {
        val c = Computers.parse(
            JSONObject("""{"selected":1,"profiles":[{"name":"","host":"studio","relay_url":"https://r","remote_id":"42","conn_mode":"relay"},{"key":"k","name":"Den"}]}"""),
        )
        assertEquals(2, c.list.size)
        assertEquals("studio", c.list[0].title)
        assertEquals("https://r|42", c.list[0].key)
        assertTrue(c.list[0].relayed)
        assertEquals("Den", c.current!!.title)
        assertEquals(c, Computers.parse(c.toJson()))
    }

    @Test fun messages_round_trip() {
        val (t, o) = BridgeProtocol.parse(BridgeProtocol.message("go") { put("key", "k") })!!
        assertEquals("go", t)
        assertEquals("k", o.getString("key"))
        assertNull(BridgeProtocol.parse("not json"))
        assertNull(BridgeProtocol.parse("{}"))
    }

    @Test fun online_probe_reads_the_relay() {
        assertEquals(OnlineProbe.Seen.ONLINE, OnlineProbe.parse("""{"daemons":1}"""))
        assertEquals(OnlineProbe.Seen.OFFLINE, OnlineProbe.parse("""{"daemons":0}"""))
        assertEquals(OnlineProbe.Seen.UNREACHABLE, OnlineProbe.parse("<html>"))
    }
}

class QrDecoderTest {
    @Test fun reads_a_code_zxing_drew() {
        val text = "https://sandptel.github.io/wado/?relay=https://r&id=1&pair=ABCD1234&hk=x"
        val m = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, 300, 300)
        val y = ByteArray(300 * 300) { i -> if (m.get(i % 300, i / 300)) 0 else 255.toByte() }
        assertEquals(text, QrDecoder.decode(y, 300, 300))
        assertNull(QrDecoder.decode(ByteArray(300 * 300) { 127 }, 300, 300))
    }
}
