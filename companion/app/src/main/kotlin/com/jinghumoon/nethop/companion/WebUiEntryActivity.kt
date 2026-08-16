package com.jinghumoon.nethop.companion

import android.app.Activity
import android.annotation.SuppressLint
import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.webkit.CookieManager
import android.webkit.WebResourceRequest
import android.webkit.WebResourceError
import android.webkit.WebResourceResponse
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.TextView
import androidx.webkit.WebViewAssetLoader
import com.jinghumoon.nethop.companion.webui.AndroidWebUiBridge
import com.jinghumoon.nethop.companion.webui.RootShellSession
import com.jinghumoon.nethop.companion.webui.RootWebRootPathHandler
import com.jinghumoon.nethop.companion.webui.TrustedWebOrigin
import com.jinghumoon.nethop.companion.packages.PackageIconPathHandler
import java.io.ByteArrayInputStream
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class WebUiEntryActivity : Activity() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val destroyed = AtomicBoolean(false)
    private var bootstrapJob: Job? = null
    private var webView: WebView? = null
    private var bridge: AndroidWebUiBridge? = null
    private var pathHandler: RootWebRootPathHandler? = null
    private var packageIconHandler: PackageIconPathHandler? = null
    private var rootSession: RootShellSession? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        intent.replaceExtras(Bundle())
        intent.data = null
        showLoading()
        bootstrapJob = scope.launch {
            val manifestBytes = runCatching {
                assets.open("webui-asset-manifest.json").use { it.readBytes() }
            }.getOrNull()
            if (manifestBytes == null) {
                showFallback()
                return@launch
            }
            val session = withContext(Dispatchers.IO) {
                RootShellSession.open(this@WebUiEntryActivity, manifestBytes)
            }
            if (session == null || destroyed.get()) {
                session?.close()
                showFallback()
                return@launch
            }
            rootSession = session
            showTrustedWebUi(session)
        }
    }

    private fun showLoading() {
        setContentView(TextView(this).apply {
            setText(R.string.app_name)
            setPadding(32, 48, 32, 32)
        })
    }

    private fun showTrustedWebUi(session: RootShellSession) {
        val handler = RootWebRootPathHandler(session)
        val iconHandler = PackageIconPathHandler(this)
        val loader = WebViewAssetLoader.Builder()
            .setDomain("appassets.androidplatform.net")
            .addPathHandler("/nethop/", handler)
            .addPathHandler("/package-icons/", iconHandler)
            .build()
        val view = hardenedWebView()
        view.webViewClient = localOnlyClient(loader, allowFallback = false, fallbackOnMainFrameError = true)
        val nativeBridge = AndroidWebUiBridge.attach(this, view)
        if (nativeBridge == null) {
            handler.close()
            iconHandler.close()
            view.destroy()
            closeRootSession()
            showFallback()
            return
        }
        pathHandler = handler
        packageIconHandler = iconHandler
        bridge = nativeBridge
        webView = view
        setContentView(view)
        view.loadUrl(TrustedWebOrigin.START_URL)
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun hardenedWebView(): WebView {
        val view = WebView(this)
        view.setBackgroundColor(Color.TRANSPARENT)
        WebView.setWebContentsDebuggingEnabled(BuildConfig.DEBUG)
        view.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            allowFileAccess = false
            allowContentAccess = false
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            cacheMode = WebSettings.LOAD_NO_CACHE
            setSupportMultipleWindows(false)
            javaScriptCanOpenWindowsAutomatically = false
            mediaPlaybackRequiresUserGesture = true
        }
        CookieManager.getInstance().setAcceptCookie(false)
        CookieManager.getInstance().setAcceptThirdPartyCookies(view, false)
        return view
    }

    private fun localOnlyClient(
        loader: WebViewAssetLoader,
        allowFallback: Boolean,
        fallbackOnMainFrameError: Boolean,
    ): WebViewClient = LocalOnlyWebViewClient(loader, allowFallback, fallbackOnMainFrameError)

    @SuppressLint("MissingOnRenderProcessGone")
    private inner class LocalOnlyWebViewClient(
        private val loader: WebViewAssetLoader,
        private val allowFallback: Boolean,
        private val fallbackOnMainFrameError: Boolean,
    ) : WebViewClient() {
        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse =
            if (TrustedWebOrigin.isTrustedResource(request.url) || (allowFallback && TrustedWebOrigin.isFallback(request.url))) {
                loader.shouldInterceptRequest(request.url) ?: forbidden()
            } else {
                forbidden()
            }

        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            val allowed = TrustedWebOrigin.isLocal(request.url) || (allowFallback && TrustedWebOrigin.isFallback(request.url))
            if (!allowed && request.isForMainFrame && fallbackOnMainFrameError) view.post(::showFallback)
            return !allowed
        }

        override fun onReceivedHttpError(
            view: WebView,
            request: WebResourceRequest,
            errorResponse: WebResourceResponse,
        ) {
            if (fallbackOnMainFrameError && request.isForMainFrame && errorResponse.statusCode >= 400) showFallback()
        }

        override fun onReceivedError(
            view: WebView,
            request: WebResourceRequest,
            error: WebResourceError,
        ) {
            if (!request.isForMainFrame) return
            if (fallbackOnMainFrameError) showFallback() else showNativeError()
        }

        override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
            if (fallbackOnMainFrameError) showFallback() else showNativeError()
            return true
        }
    }

    private fun showFallback() {
        if (destroyed.get()) return
        bridge?.close()
        bridge = null
        pathHandler?.close()
        pathHandler = null
        packageIconHandler?.close()
        packageIconHandler = null
        webView?.let { old -> old.stopLoading(); old.destroy() }
        webView = null
        closeRootSession()
        val loader = WebViewAssetLoader.Builder()
            .setDomain("appassets.androidplatform.net")
            .addPathHandler("/fallback/", WebViewAssetLoader.AssetsPathHandler(this))
            .build()
        val fallback = hardenedWebView().apply {
            settings.javaScriptEnabled = false
            settings.domStorageEnabled = false
            webViewClient = localOnlyClient(loader, allowFallback = true, fallbackOnMainFrameError = false)
        }
        webView = fallback
        setContentView(fallback)
        fallback.loadUrl("${TrustedWebOrigin.ORIGIN}/fallback/fallback/error.html")
    }

    private fun showNativeError() {
        if (destroyed.get()) return
        webView?.let { view -> view.stopLoading(); view.destroy() }
        webView = null
        setContentView(TextView(this).apply {
            setText(R.string.webui_unavailable)
            setPadding(32, 48, 32, 32)
        })
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        intent.replaceExtras(Bundle())
        intent.data = null
        setIntent(intent)
    }

    private fun forbidden() = WebResourceResponse(
        "text/plain",
        "utf-8",
        403,
        "Forbidden",
        mapOf("Cache-Control" to "no-store"),
        ByteArrayInputStream(byteArrayOf()),
    )

    private fun closeRootSession() {
        val session = rootSession ?: return
        rootSession = null
        scope.launch(Dispatchers.IO) { session.close() }
    }

    override fun onDestroy() {
        destroyed.set(true)
        bootstrapJob?.cancel()
        bridge?.close()
        bridge = null
        pathHandler?.close()
        pathHandler = null
        packageIconHandler?.close()
        packageIconHandler = null
        webView?.let { view -> view.stopLoading(); view.loadUrl("about:blank"); view.destroy() }
        webView = null
        val session = rootSession
        rootSession = null
        if (session == null) {
            scope.cancel()
        } else {
            scope.launch(Dispatchers.IO) { session.close() }.invokeOnCompletion { scope.cancel() }
        }
        super.onDestroy()
    }
}
