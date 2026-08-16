package com.jinghumoon.nethop.companion.webui

import kotlin.test.assertEquals
import kotlin.test.assertNull
import org.junit.Test

class WebRootPathValidatorTest {
    @Test
    fun acceptsOnlySingleDecodedRelativeManifestPaths() {
        assertEquals("assets/index-ABC.js", WebRootPathValidator.validate("assets/index-ABC.js"))
        assertEquals("assets/index.js", WebRootPathValidator.validate("assets%2Findex.js"))
        for (path in listOf("", "/index.html", "../index.html", "a/../b", "a//b", "a\\b", "%00", "%252e%252e/index", "%2e%2e/index", "%GG", "中文.js")) {
            assertNull(WebRootPathValidator.validate(path), path)
        }
    }
}
