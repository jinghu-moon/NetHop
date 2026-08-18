package com.jinghumoon.nethop.companion.model

import kotlin.test.assertEquals
import kotlin.test.assertIs
import org.junit.Test

class StatusDecoderTest {
    private val decoder = StatusDecoder()

    @Test
    fun decodesStrictStatusV2() {
        val result = decoder.decode(statusEnvelope().encodeToByteArray())
        val success = assertIs<StatusDecodeResult.Success>(result)
        assertEquals(RuntimeState.RUNNING_TPROXY, success.status.state)
        assertEquals(true, success.status.service.configuredEnabled)
    }

    @Test
    fun rejectsOldSchemaUnknownFieldsAndOversizedPayload() {
        assertIs<StatusDecodeResult.Failure>(decoder.decode(statusEnvelope().replace("\"schema_version\":2", "\"schema_version\":1").encodeToByteArray()))
        assertIs<StatusDecodeResult.Failure>(decoder.decode(statusEnvelope().replace("\"state\":", "\"secret\":true,\"state\":").encodeToByteArray()))
        assertIs<StatusDecodeResult.Failure>(decoder.decode(ByteArray(256 * 1024 + 1) { 'x'.code.toByte() }))
    }

    companion object {
        fun statusEnvelope(
            state: String = "running_tproxy",
            configured: Boolean = true,
            effective: Boolean = true,
            override: String = "null",
            diagnostic: String = "null",
        ) = """{"version":6,"request_id":"tile","ok":true,"result":{"schema_version":2,"state":"$state","generation":1,"last_update":"succeeded","service":{"configured_enabled":$configured,"effective_enabled":$effective,"override":$override},"diagnostic_code":$diagnostic,"watcher_health":{},"runtime":{},"subscription":{},"core_update":{},"rule_set":{},"dns_split":{},"capture":{},"operational":{}}}"""
    }
}
