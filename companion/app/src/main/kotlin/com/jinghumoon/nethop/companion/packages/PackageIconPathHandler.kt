package com.jinghumoon.nethop.companion.packages

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.webkit.WebResourceResponse
import androidx.webkit.WebViewAssetLoader
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.util.concurrent.atomic.AtomicBoolean
import android.util.LruCache

/** Serves only revisioned original PackageManager icons. */
class PackageIconPathHandler(private val repository: AndroidPackageRepository) : WebViewAssetLoader.PathHandler, AutoCloseable {
    constructor(context: Context) : this(AndroidPackageRepository(context))
    private val closed = AtomicBoolean(false)
    private val cache = object : LruCache<String, ByteArray>(MAX_CACHE_BYTES) {
        override fun sizeOf(key: String, value: ByteArray): Int = value.size
    }

    override fun handle(path: String): WebResourceResponse {
        if (closed.get()) return notFound()
        val match = PATH.matchEntire(path) ?: return notFound()
        val revision = match.groupValues[1].toLongOrNull() ?: return notFound()
        val packageName = match.groupValues[2]
        val key = "$revision/$packageName"
        val encoded = synchronized(cache) { cache.get(key) } ?: render(packageName, revision)?.also { synchronized(cache) { cache.put(key, it) } } ?: return notFound()
        return WebResourceResponse("image/png", null, 200, "OK", mapOf("Cache-Control" to "private, max-age=300", "X-Content-Type-Options" to "nosniff"), ByteArrayInputStream(encoded))
    }

    private fun render(packageName: String, revision: Long): ByteArray? = runCatching {
        val info = repository.applicationInfo(packageName, revision) ?: return@runCatching null
        val drawable = info.loadIcon(repository.packageManager())
        val bitmap = Bitmap.createBitmap(ICON_SIZE_PX, ICON_SIZE_PX, Bitmap.Config.ARGB_8888)
        try {
            drawable.setBounds(0, 0, ICON_SIZE_PX, ICON_SIZE_PX)
            drawable.draw(Canvas(bitmap))
            ByteArrayOutputStream(16 * 1024).use { output ->
                if (!bitmap.compress(Bitmap.CompressFormat.PNG, 100, output)) return@runCatching null
                output.toByteArray().takeIf { it.isNotEmpty() && it.size <= MAX_ICON_BYTES }
            }
        } finally { bitmap.recycle() }
    }.getOrNull()

    override fun close() { if (closed.compareAndSet(false, true)) synchronized(cache) { cache.evictAll() } }

    private fun notFound() = WebResourceResponse("text/plain", "utf-8", 404, "Not Found", mapOf("Cache-Control" to "no-store"), ByteArrayInputStream(byteArrayOf()))

    companion object {
        private const val ICON_SIZE_PX = 128
        private const val MAX_ICON_BYTES = 256 * 1024
        private const val MAX_CACHE_BYTES = 2 * 1024 * 1024
        private val PATH = Regex("([0-9]{1,20})/([A-Za-z0-9_.-]{1,256})")
    }
}
