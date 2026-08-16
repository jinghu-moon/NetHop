package com.jinghumoon.nethop.companion.packages

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.util.LruCache
import android.webkit.WebResourceResponse
import androidx.webkit.WebViewAssetLoader
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.util.concurrent.atomic.AtomicBoolean

class PackageIconPathHandler(context: Context) : WebViewAssetLoader.PathHandler, AutoCloseable {
    private val packageManager = context.applicationContext.packageManager
    private val closed = AtomicBoolean(false)
    private val cache = object : LruCache<String, ByteArray>(MAX_CACHE_BYTES) {
        override fun sizeOf(key: String, value: ByteArray): Int = value.size
    }

    override fun handle(path: String): WebResourceResponse {
        if (closed.get() || !PACKAGE_NAME.matches(path)) return notFound()
        val encoded = synchronized(cache) { cache.get(path) } ?: render(path)?.also {
            synchronized(cache) { cache.put(path, it) }
        } ?: return notFound()
        return WebResourceResponse(
            "image/png",
            null,
            200,
            "OK",
            mapOf("Cache-Control" to "private, max-age=300", "X-Content-Type-Options" to "nosniff"),
            ByteArrayInputStream(encoded),
        )
    }

    private fun render(packageName: String): ByteArray? = runCatching {
        val drawable = packageManager.getApplicationIcon(packageName)
        val bitmap = Bitmap.createBitmap(ICON_SIZE_PX, ICON_SIZE_PX, Bitmap.Config.ARGB_8888)
        try {
            drawable.setBounds(0, 0, ICON_SIZE_PX, ICON_SIZE_PX)
            drawable.draw(Canvas(bitmap))
            ByteArrayOutputStream(16 * 1024).use { output ->
                if (!bitmap.compress(Bitmap.CompressFormat.PNG, 100, output)) return@runCatching null
                output.toByteArray().takeIf { it.isNotEmpty() && it.size <= MAX_ICON_BYTES }
            }
        } finally {
            bitmap.recycle()
        }
    }.getOrNull()

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        synchronized(cache) { cache.evictAll() }
    }

    private fun notFound() = WebResourceResponse(
        "text/plain",
        "utf-8",
        404,
        "Not Found",
        mapOf("Cache-Control" to "no-store"),
        ByteArrayInputStream(byteArrayOf()),
    )

    companion object {
        private const val ICON_SIZE_PX = 128
        private const val MAX_ICON_BYTES = 256 * 1024
        private const val MAX_CACHE_BYTES = 2 * 1024 * 1024
        private val PACKAGE_NAME = Regex("^[A-Za-z0-9_.-]{1,256}$")
    }
}
