package com.jinghumoon.nethop.companion.webui

import java.security.MessageDigest
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

private const val MAX_ASSET_COUNT = 1024
private const val MAX_ASSET_BYTES = 8L * 1024 * 1024
private const val MAX_TOTAL_BYTES = 32L * 1024 * 1024

@Serializable
data class WebRootAsset(
    val path: String,
    val bytes: Long,
    val sha256: String,
    val mime: String,
)

@Serializable
data class WebRootManifest(
    val schema: String,
    val origin: String,
    @SerialName("root_path") val rootPath: String,
    @SerialName("identity_sha256") val identitySha256: String,
    val assets: List<WebRootAsset>,
)

@Serializable
private data class ManifestIdentity(
    val schema: String,
    val origin: String,
    @SerialName("root_path") val rootPath: String,
    val assets: List<WebRootAsset>,
)

class WebRootManifestIndex private constructor(
    val manifest: WebRootManifest,
    private val byPath: Map<String, WebRootAsset>,
) {
    fun asset(path: String): WebRootAsset? = byPath[path]

    companion object {
        const val EXPECTED_SCHEMA = "nethop.webui.asset-manifest.v1"
        const val EXPECTED_ORIGIN = "https://appassets.androidplatform.net"
        const val EXPECTED_ROOT_PATH = "/nethop/"
        private val allowedMimes = setOf(
            "text/html",
            "text/javascript",
            "text/css",
            "application/json",
            "image/svg+xml",
            "image/png",
            "image/webp",
            "image/x-icon",
            "font/woff2",
        )
        private val digestPattern = Regex("^[a-f0-9]{64}$")
        private val json = Json { ignoreUnknownKeys = false; explicitNulls = true }

        fun parse(bytes: ByteArray): WebRootManifestIndex? {
            if (bytes.isEmpty() || bytes.size > 2 * 1024 * 1024) return null
            val manifest = runCatching { json.decodeFromString<WebRootManifest>(bytes.decodeToString()) }.getOrNull() ?: return null
            if (manifest.schema != EXPECTED_SCHEMA || manifest.origin != EXPECTED_ORIGIN || manifest.rootPath != EXPECTED_ROOT_PATH) return null
            if (!digestPattern.matches(manifest.identitySha256) || manifest.assets.isEmpty() || manifest.assets.size > MAX_ASSET_COUNT) return null
            val byPath = LinkedHashMap<String, WebRootAsset>(manifest.assets.size)
            var total = 0L
            var previous = ""
            for (asset in manifest.assets) {
                if (WebRootPathValidator.validate(asset.path) != asset.path || asset.path <= previous) return null
                if (asset.bytes !in 1..MAX_ASSET_BYTES || !digestPattern.matches(asset.sha256) || asset.mime !in allowedMimes) return null
                if (byPath.put(asset.path, asset) != null) return null
                total += asset.bytes
                if (total > MAX_TOTAL_BYTES) return null
                previous = asset.path
            }
            if (!byPath.containsKey("index.html")) return null
            val identity = ManifestIdentity(manifest.schema, manifest.origin, manifest.rootPath, manifest.assets)
            val calculated = sha256(json.encodeToString(identity).encodeToByteArray())
            if (calculated != manifest.identitySha256) return null
            return WebRootManifestIndex(manifest, byPath)
        }
    }
}

internal fun sha256(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
