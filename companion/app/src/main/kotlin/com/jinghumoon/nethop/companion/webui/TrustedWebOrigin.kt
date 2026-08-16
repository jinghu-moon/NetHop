package com.jinghumoon.nethop.companion.webui

import android.net.Uri

object TrustedWebOrigin {
    const val ORIGIN = "https://appassets.androidplatform.net"
    const val START_URL = "$ORIGIN/nethop/index.html"

    fun accepts(sourceOrigin: Uri, isMainFrame: Boolean): Boolean =
        isMainFrame && sourceOrigin.toString() == ORIGIN

    fun isLocal(url: Uri): Boolean =
        url.scheme == "https" && url.host == "appassets.androidplatform.net" && url.port == -1 && url.path?.startsWith("/nethop/") == true

    fun isPackageIcon(url: Uri): Boolean =
        url.scheme == "https" && url.host == "appassets.androidplatform.net" && url.port == -1 &&
            url.path?.startsWith("/package-icons/") == true && url.query == null && url.fragment == null

    fun isTrustedResource(url: Uri): Boolean = isLocal(url) || isPackageIcon(url)

    fun isFallback(url: Uri): Boolean =
        url.scheme == "https" && url.host == "appassets.androidplatform.net" && url.port == -1 &&
            url.path == "/fallback/fallback/error.html" && url.query == null && url.fragment == null
}
