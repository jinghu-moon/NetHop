package com.jinghumoon.nethop.companion.webui

import android.webkit.WebResourceResponse
import androidx.webkit.WebViewAssetLoader
import java.io.ByteArrayInputStream
import java.io.FilterInputStream
import java.io.InputStream
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong

internal const val DEFAULT_MAX_CONCURRENT_WEBROOT_STREAMS = 64

internal class WebRootResourceBudget(
    private val maxRequests: Int,
    private val maxConcurrentStreams: Int,
    private val maxTotalBytes: Long,
) {
    private val requestCount = AtomicInteger(0)
    private val streamCount = AtomicInteger(0)
    private val totalBytes = AtomicLong(0)

    fun acquire(bytes: Long): Boolean {
        if (bytes <= 0 || requestCount.incrementAndGet() > maxRequests) return false
        if (streamCount.incrementAndGet() > maxConcurrentStreams) {
            streamCount.decrementAndGet()
            return false
        }
        while (true) {
            val observed = totalBytes.get()
            if (bytes > maxTotalBytes - observed) {
                streamCount.decrementAndGet()
                return false
            }
            if (totalBytes.compareAndSet(observed, observed + bytes)) return true
        }
    }

    fun releaseStream() {
        streamCount.decrementAndGet()
    }
}

class RootWebRootPathHandler(
    private val session: RootShellSession,
    private val maxConcurrentStreams: Int = DEFAULT_MAX_CONCURRENT_WEBROOT_STREAMS,
    maxRequests: Int = 2_048,
    maxTotalBytes: Long = 128L * 1024 * 1024,
) : WebViewAssetLoader.PathHandler, AutoCloseable {
    private val closed = AtomicBoolean(false)
    private val budget = WebRootResourceBudget(maxRequests, maxConcurrentStreams, maxTotalBytes)
    private val streams = ConcurrentHashMap.newKeySet<InputStream>()

    override fun handle(path: String): WebResourceResponse {
        if (closed.get()) return notFound()
        val relative = WebRootPathValidator.validate(if (path.isEmpty()) "index.html" else path) ?: return notFound()
        val asset = session.manifest.asset(relative) ?: return notFound()
        if (!budget.acquire(asset.bytes)) return tooManyRequests()
        val raw = session.open(asset)
        if (raw == null) {
            budget.releaseStream()
            return notFound()
        }
        val tracked = TrackedInputStream(raw) {
            streams.remove(it)
            budget.releaseStream()
        }
        streams.add(tracked)
        val charset = if (asset.mime.startsWith("text/") || asset.mime == "application/json" || asset.mime == "image/svg+xml") "utf-8" else null
        return WebResourceResponse(
            asset.mime,
            charset,
            200,
            "OK",
            WebRootCachePolicy.responseHeaders(asset),
            tracked,
        )
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        streams.toList().forEach { runCatching { it.close() } }
    }

    private fun notFound() = WebResourceResponse(
        "text/plain",
        "utf-8",
        404,
        "Not Found",
        mapOf("Cache-Control" to "no-store"),
        ByteArrayInputStream(byteArrayOf()),
    )

    private fun tooManyRequests() = WebResourceResponse(
        "text/plain",
        "utf-8",
        429,
        "Too Many Requests",
        mapOf("Cache-Control" to "no-store"),
        ByteArrayInputStream(byteArrayOf()),
    )

    private class TrackedInputStream(
        source: InputStream,
        private val onClosed: (InputStream) -> Unit,
    ) : FilterInputStream(source) {
        private val closed = AtomicBoolean(false)

        override fun close() {
            if (!closed.compareAndSet(false, true)) return
            try {
                super.close()
            } finally {
                onClosed(this)
            }
        }
    }
}
