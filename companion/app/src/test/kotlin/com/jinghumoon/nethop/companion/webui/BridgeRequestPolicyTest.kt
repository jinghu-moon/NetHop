package com.jinghumoon.nethop.companion.webui

import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.junit.Test

class BridgeRequestPolicyTest {
    private val requestId = "a_${"1".repeat(32)}"

    @Test
    fun acceptsOnlyFieldsOwnedByTheMessageKind() {
        assertTrue(
            BridgeRequestPolicy.accepts(
                BridgeRequest(1, requestId, "run", operationId = "status.get", args = listOf("status", "--json")),
            ),
        )
        assertFalse(
            BridgeRequestPolicy.accepts(
                BridgeRequest(
                    1,
                    requestId,
                    "run",
                    operationId = "status.get",
                    args = listOf("status", "--json"),
                    message = "cross-kind",
                ),
            ),
        )
        assertFalse(BridgeRequestPolicy.accepts(BridgeRequest(1, requestId, "list_packages", packageType = "private")))
        assertFalse(
            BridgeRequestPolicy.accepts(
                BridgeRequest(1, requestId, "package_info", packages = List(129) { "com.example.$it" }),
            ),
        )
        assertFalse(BridgeRequestPolicy.accepts(BridgeRequest(1, requestId, "unknown")))
    }
}
