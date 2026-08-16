package com.jinghumoon.nethop.companion.webui

import java.io.File
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import org.junit.Test

class WebRootManifestTest {
    @Test
    fun generatedManifestIsStrictAndSelfConsistent() {
        val bytes = File("src/main/assets/webui-asset-manifest.json").readBytes()
        assertNotNull(WebRootManifestIndex.parse(bytes))
        val unknown = bytes.decodeToString().replaceFirst("\"schema\":", "\"unknown\":true,\"schema\":")
        assertNull(WebRootManifestIndex.parse(unknown.encodeToByteArray()))
        val traversal = bytes.decodeToString().replaceFirst("\"index.html\"", "\"../index.html\"")
        assertNull(WebRootManifestIndex.parse(traversal.encodeToByteArray()))
    }
}
