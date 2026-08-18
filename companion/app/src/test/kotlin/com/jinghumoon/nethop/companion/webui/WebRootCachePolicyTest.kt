package com.jinghumoon.nethop.companion.webui

import kotlin.test.assertEquals
import org.junit.Test

class WebRootCachePolicyTest {
    @Test
    fun immutableCachingIsLimitedToContentHashedAssets() {
        assertEquals(
            "public, max-age=31536000, immutable",
            WebRootCachePolicy.cacheControl("assets/index-DYU31asV.js"),
        )
        assertEquals(
            "public, max-age=31536000, immutable",
            WebRootCachePolicy.cacheControl("assets/JP-C15v8xO5.svg"),
        )
        assertEquals("no-cache", WebRootCachePolicy.cacheControl("index.html"))
        assertEquals("no-cache", WebRootCachePolicy.cacheControl(".vite/manifest.json"))
        assertEquals("no-cache", WebRootCachePolicy.cacheControl("assets/index.js"))
    }

    @Test
    fun responseHeadersUseTheVerifiedManifestDigestAsEtag() {
        assertEquals(
            mapOf(
                "Cache-Control" to "public, max-age=31536000, immutable",
                "ETag" to "\"${"a".repeat(64)}\"",
                "X-Content-Type-Options" to "nosniff",
            ),
            WebRootCachePolicy.responseHeaders(
                WebRootAsset(
                    path = "assets/client-BiLZH6UQ.css",
                    bytes = 1,
                    sha256 = "a".repeat(64),
                    mime = "text/css",
                ),
            ),
        )
    }
}
