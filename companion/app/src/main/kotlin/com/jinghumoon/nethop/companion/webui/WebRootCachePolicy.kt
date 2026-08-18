package com.jinghumoon.nethop.companion.webui

internal object WebRootCachePolicy {
    private const val REVALIDATE = "no-cache"
    private const val IMMUTABLE = "public, max-age=31536000, immutable"
    private val contentHashedAsset = Regex("^assets/(?:[^/]+/)*[^/]+-[A-Za-z0-9_-]{8,}\\.[A-Za-z0-9]+$")

    fun cacheControl(path: String): String = if (contentHashedAsset.matches(path)) IMMUTABLE else REVALIDATE

    fun responseHeaders(asset: WebRootAsset): Map<String, String> = mapOf(
        "Cache-Control" to cacheControl(asset.path),
        "ETag" to "\"${asset.sha256}\"",
        "X-Content-Type-Options" to "nosniff",
    )
}
