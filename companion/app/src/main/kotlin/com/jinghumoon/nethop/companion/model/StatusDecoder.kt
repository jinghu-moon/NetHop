package com.jinghumoon.nethop.companion.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement

private const val MAX_STATUS_BYTES = 256 * 1024

@Serializable
data class ControlEnvelope<T>(
    val version: Int,
    @SerialName("request_id") val requestId: String,
    val ok: Boolean,
    val generation: Long? = null,
    val result: T,
)

@Serializable
data class StatusDocument(
    @SerialName("schema_version") val schemaVersion: Int,
    val state: RuntimeState,
    val generation: Long? = null,
    @SerialName("last_update") val lastUpdate: LastUpdate,
    val service: ServiceStatus,
    @SerialName("diagnostic_code") val diagnosticCode: StatusDiagnosticCode? = null,
    @SerialName("watcher_health") val watcherHealth: JsonElement,
    val runtime: JsonElement,
    val subscription: JsonElement,
    @SerialName("core_update") val coreUpdate: JsonElement,
    @SerialName("rule_set") val ruleSet: JsonElement,
    @SerialName("dns_split") val dnsSplit: JsonElement,
    val capture: JsonElement,
    val lifecycle: JsonElement? = null,
    val operational: JsonElement,
)

@Serializable
data class ServiceStatus(
    @SerialName("configured_enabled") val configuredEnabled: Boolean,
    @SerialName("effective_enabled") val effectiveEnabled: Boolean,
    val override: ServiceOverride? = null,
)

@Serializable
enum class ServiceOverride {
    @SerialName("wifi_scene")
    WIFI_SCENE,
}

@Serializable
enum class StatusDiagnosticCode {
    @SerialName("config_unavailable")
    CONFIG_UNAVAILABLE,

    @SerialName("fail_open_direct")
    FAIL_OPEN_DIRECT,

    @SerialName("runtime_degraded")
    RUNTIME_DEGRADED,

    @SerialName("runtime_backoff")
    RUNTIME_BACKOFF,

    @SerialName("runtime_circuit_open")
    RUNTIME_CIRCUIT_OPEN,
}

@Serializable
enum class RuntimeState {
    @SerialName("init") INIT,
    @SerialName("probing") PROBING,
    @SerialName("starting_core") STARTING_CORE,
    @SerialName("running_tproxy") RUNNING_TPROXY,
    @SerialName("starting_tun") STARTING_TUN,
    @SerialName("running_tun") RUNNING_TUN,
    @SerialName("degraded") DEGRADED,
    @SerialName("fail_open_direct") FAIL_OPEN_DIRECT,
    @SerialName("backoff") BACKOFF,
    @SerialName("circuit_open") CIRCUIT_OPEN,
    @SerialName("stopping") STOPPING,
}

@Serializable
enum class LastUpdate {
    @SerialName("never") NEVER,
    @SerialName("succeeded") SUCCEEDED,
    @SerialName("failed") FAILED,
}

sealed interface StatusDecodeResult {
    data class Success(val status: StatusDocument) : StatusDecodeResult
    data class Failure(val code: String) : StatusDecodeResult
}

class StatusDecoder(
    private val json: Json = Json {
        ignoreUnknownKeys = false
        explicitNulls = true
        isLenient = false
    },
) {
    fun decode(bytes: ByteArray): StatusDecodeResult {
        if (bytes.isEmpty() || bytes.size > MAX_STATUS_BYTES) {
            return StatusDecodeResult.Failure("status_size_invalid")
        }
        val envelope = runCatching {
            json.decodeFromString<ControlEnvelope<StatusDocument>>(bytes.decodeToString())
        }.getOrElse { return StatusDecodeResult.Failure("status_json_invalid") }
        if (envelope.version !in setOf(5, 6) || !envelope.ok || envelope.requestId.isBlank()) {
            return StatusDecodeResult.Failure("status_envelope_invalid")
        }
        if (envelope.result.schemaVersion != 2) {
            return StatusDecodeResult.Failure("status_schema_incompatible")
        }
        return StatusDecodeResult.Success(envelope.result)
    }
}
