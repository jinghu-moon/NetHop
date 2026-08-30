package com.jinghumoon.nethop.companion.webui

import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import org.junit.Test

class BridgeCommandPolicyTest {
    private val source = "src_${"1".repeat(32)}"
    private val digest = "a".repeat(64)
    private val handle = "p_${"2".repeat(32)}"
    private val node = "nh1s-0123456789abcdef"
    private val event = "evt_${"3".repeat(32)}"

    @Test
    fun acceptsExactTypedOperations() {
        val cases = mapOf(
            "hello" to listOf("hello", "--json", "--manager-version", "webui-0.1.0", "--protocol-min", "6", "--protocol-max", "6"),
            "status.get" to listOf("status", "--json"),
            "service.start" to listOf("start", "--json", "--wait"),
            "service.stop" to listOf("stop", "--json", "--wait"),
            "capture.enable" to listOf("capture", "enable", "--json", "--wait"),
            "capture.disable" to listOf("capture", "disable", "--json"),
            "capture.status" to listOf("capture", "status", "--json"),
            "core.start" to listOf("core", "start", "--json", "--wait"),
            "core.stop" to listOf("core", "stop", "--json"),
            "core.status" to listOf("core", "status", "--json"),
            "resource.status" to listOf("resource", "status", "--json"),
            "capability.get" to listOf("capability", "get", "--json"),
            "config.get" to listOf("config", "get", "--json"),
            "config.schema" to listOf("config", "schema", "--json"),
            "config.reload" to listOf("config", "reload", "--json", "--wait"),
            "traffic.get" to listOf("traffic", "--json"),
            "metrics.get" to listOf("metrics", "--json"),
            "events.terminate" to listOf("webui", "events", "terminate", event, "--json"),
            "node.list" to listOf("node", "list", "--json", "jp", "--limit", "64"),
            "node.test" to listOf("node", "test", node, "--json"),
            "node.test-all" to listOf("node", "test-all", "--json"),
            "node.selection.get" to listOf("node", "selection", "--json"),
            "node.select.auto" to listOf("node", "select", "auto", "--json"),
            "node.select.manual" to listOf("node", "select", "manual", node, "--json"),
            "node.export" to listOf("node", "export", node, "--json"),
            "node.override.get" to listOf("node", "override", "get", node, "--json"),
            "node.override.remove" to listOf("node", "override", "remove", node, "--json"),
            "node.remove" to listOf("node", "remove", node, "--json", "--expected-digest", digest),
            "subscription.list" to listOf("subscription", "list", "--json"),
            "subscription.mode.get" to listOf("subscription", "mode", "--json"),
            "subscription.mode.set" to listOf("subscription", "mode", "set", "single", "--json", "--expected-digest", digest, "--source", source),
            "subscription.select" to listOf("subscription", "select", source, "--json", "--expected-digest", digest),
            "subscription.set-enabled" to listOf("subscription", "enable", source, "--json", "--expected-digest", digest),
            "subscription.update" to listOf("subscription", "update", "--json", "--wait", "--source", source),
            "subscription.enable" to listOf("subscription", "enable", source, "--json", "--expected-digest", digest),
            "subscription.disable" to listOf("subscription", "disable", source, "--json", "--expected-digest", digest),
            "subscription.move" to listOf("subscription", "move", source, "--json", "--expected-digest", digest),
            "subscription.remove" to listOf("subscription", "remove", source, "--json", "--expected-digest", digest),
            "application.list" to listOf("application", "list", "--json"),
            "logs.get" to listOf("logs", "get", "--channel", "service", "--json", "--limit", "32"),
            "logs.clear" to listOf("logs", "clear", "--json"),
            "connections.get" to listOf("connections", "--json", "tcp:stable", "--limit", "32"),
            "connection.close" to listOf("connection", "close", "tcp:stable", "--json"),
            "connections.close-all" to listOf("connections", "close-all", "--json"),
            "diagnostics.bundle" to listOf("diagnose", "--json"),
            "topology.get" to listOf("topology", "--json"),
            "ruleset.status" to listOf("ruleset", "status", "--json"),
            "ruleset.update" to listOf("ruleset", "update", "--json", "--wait"),
            "core.version-check" to listOf("core", "version-check", "--json"),
            "webui.payload.append" to listOf("webui", "payload", "append", "config", handle, "YWJjZA==", "--json"),
            "webui.payload.create" to listOf("webui", "payload", "create", "config", "--json"),
            "webui.payload.commit" to listOf("webui", "payload", "commit", "config", handle, "config-apply", "--json"),
            "webui.payload.remove" to listOf("webui", "payload", "remove", "config", handle, "--json"),
            "backup.export" to listOf("backup", "export", "--file", "/data/adb/nethop/backups/webui-config-backup.json", "--json"),
        )
        cases.forEach { (id, args) ->
            val operation = assertNotNull(BridgeCommandPolicy.operation(id, args, spawn = false))
            assertEquals(args, operation.command().args)
        }
        assertNotNull(
            BridgeCommandPolicy.operation(
                "webui.payload.commit",
                listOf("webui", "payload", "commit", "node", handle, "node-override-apply", "--json"),
                spawn = false,
            ),
        )
        assertNotNull(
            BridgeCommandPolicy.operation(
                "webui.payload.commit",
                listOf("webui", "payload", "commit", "config", handle, "config-mutate", "--json"),
                spawn = false,
            ),
        )
    }

    @Test
    fun privatePayloadCommitUsesTheMutationTimeoutBudget() {
        val operation = assertNotNull(
            BridgeCommandPolicy.operation(
                "webui.payload.commit",
                listOf("webui", "payload", "commit", "node", handle, "node-override-apply", "--json"),
                spawn = false,
            ),
        )

        assertEquals(30_000L, operation.command().timeoutMillis)
    }

    @Test
    fun onlyEventsSubscriptionMaySpawn() {
        val args = listOf(
            "events",
            "--jsonl",
            "--kinds",
            "runtime,node-test",
            "--session-id",
            "evt_${"3".repeat(32)}",
            "--max-runtime-seconds",
            "300",
        )
        assertNotNull(BridgeCommandPolicy.operation("events.subscribe", args, spawn = true))
        assertNull(BridgeCommandPolicy.operation("events.subscribe", args, spawn = false))
        assertNull(BridgeCommandPolicy.operation("status.get", listOf("status", "--json"), spawn = true))
    }

    @Test
    fun rejectsPrefixConfusionAndArgumentSmuggling() {
        assertNull(
            BridgeCommandPolicy.operation(
                "subscription.set-enabled",
                listOf("subscription", "remove", source, "--json", "--expected-digest", digest),
                spawn = false,
            ),
        )
        assertNull(
            BridgeCommandPolicy.operation(
                "webui.payload.append",
                listOf("webui", "payload", "append", "config", "YWJjZA==", handle, "--json"),
                spawn = false,
            ),
        )
        assertNull(BridgeCommandPolicy.operation("status.get", listOf("status", "--json", "--socket", "/tmp/x"), false))
        assertNull(BridgeCommandPolicy.operation("backup.export", listOf("backup", "export", "--file", "/sdcard/x", "--json"), false))
        assertNull(BridgeCommandPolicy.operation("node.override.get", listOf("node", "override", "get", "../../etc/passwd", "--json"), false))
        assertNull(BridgeCommandPolicy.operation("webui.payload.commit", listOf("webui", "payload", "commit", "subscription", handle, "config-mutate", "--json"), false))
        assertNull(BridgeCommandPolicy.operation("webui.payload.commit", listOf("webui", "payload", "commit", "config", handle, "node-override-apply", "--json"), false))
        assertNull(BridgeCommandPolicy.operation("unknown", listOf("status", "--json"), false))
    }
}
