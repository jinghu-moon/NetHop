package com.jinghumoon.nethop.companion.webui

import android.app.Activity
import android.net.Uri
import android.webkit.WebView
import android.widget.Toast
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.WebMessageCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import com.jinghumoon.nethop.companion.control.CommandResult
import com.jinghumoon.nethop.companion.control.RootCommandExecutor
import com.jinghumoon.nethop.companion.packages.AndroidPackageAdapter
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.put

@Serializable
internal data class BridgeRequest(
    val version: Int,
    @SerialName("request_id") val requestId: String,
    val kind: String,
    @SerialName("operation_id") val operationId: String? = null,
    val args: List<String> = emptyList(),
    @SerialName("child_id") val childId: String? = null,
    @SerialName("package_type") val packageType: String? = null,
    val packages: List<String> = emptyList(),
    val message: String? = null,
    val enabled: Boolean? = null,
)

internal object BridgeRequestPolicy {
    private val packageName = Regex("^[A-Za-z0-9_.-]{1,256}$")

    fun accepts(request: BridgeRequest): Boolean = when (request.kind) {
        "run", "spawn" -> request.operationId != null && request.childId == null && request.packageType == null &&
            request.packages.isEmpty() && request.message == null && request.enabled == null
        "terminate" -> request.operationId == null && request.args.isEmpty() && request.childId != null &&
            request.packageType == null && request.packages.isEmpty() && request.message == null && request.enabled == null
        "list_packages" -> request.operationId == null && request.args.isEmpty() && request.childId == null &&
            request.packageType in setOf("user", "system", "all") && request.packages.isEmpty() &&
            request.message == null && request.enabled == null
        "package_info" -> request.operationId == null && request.args.isEmpty() && request.childId == null &&
            request.packageType == null && request.packages.size <= 128 && request.packages.all(packageName::matches) &&
            request.message == null && request.enabled == null
        "toast" -> request.operationId == null && request.args.isEmpty() && request.childId == null &&
            request.packageType == null && request.packages.isEmpty() && request.message != null &&
            request.message.length <= 256 && request.enabled == null
        "edge_to_edge" -> request.operationId == null && request.args.isEmpty() && request.childId == null &&
            request.packageType == null && request.packages.isEmpty() && request.message == null && request.enabled != null
        "exit" -> request.operationId == null && request.args.isEmpty() && request.childId == null &&
            request.packageType == null && request.packages.isEmpty() && request.message == null && request.enabled == null
        else -> false
    }
}

class AndroidWebUiBridge private constructor(
    private val activity: Activity,
    private val webView: WebView,
) : AutoCloseable {
    private val closed = AtomicBoolean(false)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val executor = RootCommandExecutor()
    private val packages = AndroidPackageAdapter(activity)
    private val children = ConcurrentHashMap<String, EventProcess>()
    private val json = Json { ignoreUnknownKeys = false; explicitNulls = false }

    private fun onMessage(
        message: WebMessageCompat,
        sourceOrigin: Uri,
        isMainFrame: Boolean,
        reply: JavaScriptReplyProxy,
    ) {
        if (closed.get() || !TrustedWebOrigin.accepts(sourceOrigin, isMainFrame)) return
        val data = message.data ?: return
        if (data.encodeToByteArray().size > MAX_MESSAGE_BYTES) return
        val request = runCatching { json.decodeFromString<BridgeRequest>(data) }.getOrNull() ?: return
        if (request.version != 1 || !REQUEST_ID.matches(request.requestId)) return
        if (!BridgeRequestPolicy.accepts(request)) return error(request.requestId, "request_invalid", reply)
        when (request.kind) {
            "run" -> run(request, reply)
            "spawn" -> spawn(request, reply)
            "terminate" -> terminate(request, reply)
            "list_packages" -> listPackages(request, reply)
            "package_info" -> packageInfo(request, reply)
            "toast" -> request.message?.takeIf { it.length <= 256 }?.let { Toast.makeText(activity, it, Toast.LENGTH_SHORT).show() }
            "edge_to_edge" -> request.enabled?.let(::setEdgeToEdge)
            "exit" -> activity.finish()
        }
    }

    @Suppress("DEPRECATION")
    private fun setEdgeToEdge(enabled: Boolean) {
        activity.window.setDecorFitsSystemWindows(!enabled)
    }

    private fun run(request: BridgeRequest, reply: JavaScriptReplyProxy) {
        val operationId = request.operationId ?: return error(request.requestId, "operation_invalid", reply)
        val operation = BridgeCommandPolicy.operation(operationId, request.args, spawn = false)
            ?: return error(request.requestId, "operation_rejected", reply)
        scope.launch {
            when (val result = executor.execute(operation)) {
                is CommandResult.Success -> post(reply, buildJsonObject {
                    base(request.requestId, "result")
                    put("errno", 0)
                    put("stdout", result.stdout.decodeToString())
                    put("stderr", result.stderr.decodeToString())
                })
                is CommandResult.Failure -> post(reply, buildJsonObject {
                    base(request.requestId, "result")
                    put("errno", 1)
                    put("stdout", "")
                    put("stderr", result.code)
                })
            }
        }
    }

    private fun spawn(request: BridgeRequest, reply: JavaScriptReplyProxy) {
        val operationId = request.operationId ?: return error(request.requestId, "operation_invalid", reply)
        val operation = BridgeCommandPolicy.operation(operationId, request.args, spawn = true)
            ?: return error(request.requestId, "operation_rejected", reply)
        if (children.containsKey(request.requestId)) return error(request.requestId, "child_exists", reply)
        val child = runCatching {
            EventProcess(operation, scope, emit = { type, data, code ->
                post(reply, buildJsonObject {
                    base(request.requestId, type)
                    if (type == "error") put("code", data ?: "child_failed")
                    else data?.let { put("data", it) }
                    if (type == "exit") put("code", code?.let(::JsonPrimitive) ?: kotlinx.serialization.json.JsonNull)
                })
                if (type == "exit" || type == "error") children.remove(request.requestId)
            }, terminateSession = { termination ->
                CLEANUP_SCOPE.launch { executor.execute(termination) }
            })
        }.getOrElse { return error(request.requestId, "child_start_failed", reply) }
        children[request.requestId] = child
        if (child.isClosed) children.remove(request.requestId, child)
        post(reply, buildJsonObject { base(request.requestId, "ack") })
    }

    private fun terminate(request: BridgeRequest, reply: JavaScriptReplyProxy) {
        val childId = request.childId?.takeIf(REQUEST_ID::matches) ?: return error(request.requestId, "child_invalid", reply)
        children.remove(childId)?.close()
        post(reply, buildJsonObject { base(request.requestId, "ack") })
    }

    private fun listPackages(request: BridgeRequest, reply: JavaScriptReplyProxy) {
        val type = request.packageType ?: return error(request.requestId, "package_type_invalid", reply)
        scope.launch(Dispatchers.IO) {
            val names = packages.listPackages(type)
            post(reply, buildJsonObject {
                base(request.requestId, "packages")
                put("packages", JsonArray(names.map(::JsonPrimitive)))
            })
        }
    }

    private fun packageInfo(request: BridgeRequest, reply: JavaScriptReplyProxy) {
        scope.launch(Dispatchers.IO) {
            val info = packages.packageInfo(request.packages)
            post(reply, buildJsonObject {
                base(request.requestId, "packages")
                put("packages", JsonArray(request.packages.map(::JsonPrimitive)))
                put("info", json.encodeToJsonElement(info))
            })
        }
    }

    private fun error(requestId: String, code: String, reply: JavaScriptReplyProxy) {
        post(reply, buildJsonObject { base(requestId, "error"); put("code", code) })
    }

    private fun kotlinx.serialization.json.JsonObjectBuilder.base(requestId: String, type: String) {
        put("version", 1)
        put("request_id", requestId)
        put("type", type)
    }

    private fun post(reply: JavaScriptReplyProxy, payload: kotlinx.serialization.json.JsonObject) {
        if (closed.get()) return
        val encoded = payload.toString()
        if (encoded.encodeToByteArray().size <= MAX_MESSAGE_BYTES &&
            WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            reply.postMessage(encoded)
        }
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            WebViewCompat.removeWebMessageListener(webView, BRIDGE_NAME)
        }
        children.values.forEach(EventProcess::close)
        children.clear()
        scope.cancel()
    }

    companion object {
        private const val BRIDGE_NAME = "nethopAndroid"
        private const val MAX_MESSAGE_BYTES = 1024 * 1024
        private val REQUEST_ID = Regex("^a_[a-f0-9]{32}$")
        private val CLEANUP_SCOPE = CoroutineScope(SupervisorJob() + Dispatchers.IO)

        fun attach(activity: Activity, webView: WebView): AndroidWebUiBridge? {
            if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) return null
            val bridge = AndroidWebUiBridge(activity, webView)
            WebViewCompat.addWebMessageListener(
                webView,
                BRIDGE_NAME,
                setOf(TrustedWebOrigin.ORIGIN),
            ) { _, message, sourceOrigin, isMainFrame, reply ->
                bridge.onMessage(message, sourceOrigin, isMainFrame, reply)
            }
            return bridge
        }
    }
}
