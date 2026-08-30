package com.jinghumoon.nethop.companion.webui

import com.jinghumoon.nethop.companion.control.RootOperation

object BridgeCommandPolicy {
    private const val MAX_ARGS = 64
    private const val MAX_ARG_BYTES = 16 * 1024
    private const val MAX_TOTAL_BYTES = 1024 * 1024
    private const val BACKUP_PATH = "/data/adb/nethop/backups/webui-config-backup.json"
    private val managerVersion = Regex("^[A-Za-z0-9][A-Za-z0-9.+_-]{0,63}$")
    private val genericId = Regex("^[A-Za-z0-9_.:-]{1,256}$")
    private val sourceId = Regex("^src_[a-f0-9]{32}$")
    private val nodeId = Regex("^nh1s-[a-f0-9]{16}$")
    private val eventSession = Regex("^evt_[a-f0-9]{32}$")
    private val payloadHandle = Regex("^p_[a-f0-9]{32}$")
    private val digest = Regex("^[a-f0-9]{64}$")
    private val base64 = Regex("^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$")
    private val integer = Regex("^[0-9]+$")
    private val PACKAGE_NAME = Regex("^[A-Za-z0-9_.-]{1,256}$")
    private val eventKinds = setOf(
        "config",
        "runtime",
        "subscription",
        "generation",
        "network",
        "traffic",
        "subscription-mode",
        "subscription-active-set",
        "node-selection",
        "node-active",
        "node-test",
    )
    private val payloadNamespaces = setOf("config", "subscription", "backup", "node")
    private val payloadOperations = setOf(
        "config-validate",
        "config-apply",
        "config-mutate",
        "subscription-import-preview",
        "subscription-import-apply",
        "backup-restore",
        "node-override-apply",
    )

    fun operation(operationId: String, args: List<String>, spawn: Boolean): RootOperation? {
        if (args.size !in 1..MAX_ARGS || !withinBounds(args)) return null
        val valid = when (operationId) {
            "hello" -> args.size == 8 && args[0] == "hello" && args[1] == "--json" &&
                args[2] == "--manager-version" && managerVersion.matches(args[3]) &&
                args.subList(4, 8) == listOf("--protocol-min", "6", "--protocol-max", "6")
            "status.get" -> args == listOf("status", "--json")
            "service.start" -> args == listOf("start", "--json") || args == listOf("start", "--json", "--wait")
            "service.stop" -> args == listOf("stop", "--json") || args == listOf("stop", "--json", "--wait")
            "capture.enable" -> args == listOf("capture", "enable", "--json") || args == listOf("capture", "enable", "--json", "--wait")
            "capture.disable" -> args == listOf("capture", "disable", "--json") || args == listOf("capture", "disable", "--json", "--wait")
            "capture.status" -> args == listOf("capture", "status", "--json")
            "core.start" -> args == listOf("core", "start", "--json") || args == listOf("core", "start", "--json", "--wait")
            "core.stop" -> args == listOf("core", "stop", "--json") || args == listOf("core", "stop", "--json", "--wait")
            "core.status" -> args == listOf("core", "status", "--json")
            "resource.status" -> args == listOf("resource", "status", "--json")
            "capability.get" -> args == listOf("capability", "get", "--json")
            "config.get" -> args == listOf("config", "get", "--json")
            "config.schema" -> args == listOf("config", "schema", "--json")
            "config.reload" -> args == listOf("config", "reload", "--json", "--wait")
            "traffic.get" -> args == listOf("traffic", "--json")
            "metrics.get" -> args == listOf("metrics", "--json")
            "events.subscribe" -> validEventSubscription(args)
            "events.terminate" -> args.size == 5 && args.take(3) == listOf("webui", "events", "terminate") &&
                eventSession.matches(args[3]) && args[4] == "--json"
            "node.list" -> validList(args, listOf("node", "list"), allowQuery = true)
            "node.test" -> args.size == 4 && args.take(2) == listOf("node", "test") && nodeId.matches(args[2]) && args[3] == "--json"
            "node.test-all" -> args == listOf("node", "test-all", "--json")
            "node.selection.get" -> args == listOf("node", "selection", "--json")
            "node.select.auto" -> args == listOf("node", "select", "auto", "--json")
            "node.select.manual" -> args.size == 5 && args.take(3) == listOf("node", "select", "manual") && nodeId.matches(args[3]) && args[4] == "--json"
            "node.export" -> args.size == 4 && args.take(2) == listOf("node", "export") && nodeId.matches(args[2]) && args[3] == "--json"
            "node.override.get" -> validNodeOverride(args, "get")
            "node.override.remove" -> validNodeOverride(args, "remove")
            "node.remove" -> validIdDigestMutation(args, listOf("node", "remove"), nodeId)
            "subscription.list" -> args == listOf("subscription", "list", "--json")
            "subscription.mode.get" -> args == listOf("subscription", "mode", "--json")
            "subscription.mode.set" -> validSubscriptionMode(args)
            "subscription.select" -> validIdDigestMutation(args, listOf("subscription", "select"), sourceId)
            "subscription.set-enabled" -> validSubscriptionEnabled(args)
            "subscription.update" -> validSubscriptionUpdate(args)
            "subscription.enable" -> validIdDigestMutation(args, listOf("subscription", "enable"), sourceId)
            "subscription.disable" -> validIdDigestMutation(args, listOf("subscription", "disable"), sourceId)
            "subscription.move" -> validSubscriptionMove(args)
            "subscription.remove" -> validIdDigestMutation(args, listOf("subscription", "remove"), sourceId)
            "application.list" -> args == listOf("application", "list", "--json")
            "logs.get" -> validLogs(args)
            "logs.clear" -> args == listOf("logs", "clear", "--json")
            "connections.get" -> validList(args, listOf("connections"), allowQuery = true)
            "connection.close" -> args.size == 4 && args.take(2) == listOf("connection", "close") && genericId.matches(args[2]) && args[3] == "--json"
            "connections.close-all" -> args == listOf("connections", "close-all", "--json")
            "diagnostics.bundle" -> args == listOf("diagnose", "--json")
            "topology.get" -> args == listOf("topology", "--json")
            "ruleset.status" -> args == listOf("ruleset", "status", "--json")
            "ruleset.update" -> args == listOf("ruleset", "update", "--json") || args == listOf("ruleset", "update", "--json", "--wait")
            "core.version-check" -> args == listOf("core", "version-check", "--json")
            "backup.export" -> args == listOf("backup", "export", "--file", BACKUP_PATH, "--json")
            "webui.payload.create" -> args.size == 5 && args.take(3) == listOf("webui", "payload", "create") && args[3] in payloadNamespaces && args[4] == "--json"
            "webui.payload.append" -> validPayloadAppend(args)
            "webui.payload.commit" -> validPayloadCommit(args)
            "webui.payload.remove" -> validPayloadRemove(args)
            else -> false
        }
        if (!valid || spawn != (operationId == "events.subscribe")) return null
        val timeout = when {
            spawn -> 0L
            operationId in setOf("service.start", "service.stop", "capture.enable", "capture.disable", "core.start", "core.stop", "config.reload", "subscription.update", "ruleset.update", "webui.payload.commit") -> 30_000L
            operationId == "node.test-all" -> 7_000L
            operationId == "node.test" -> 15_000L
            else -> 5_000L
        }
        return RootOperation.webUi(args, timeout, operationId in mutatingOperations)
    }

    private fun withinBounds(args: List<String>): Boolean {
        var total = 0
        return args.all { arg ->
            val size = arg.encodeToByteArray().size
            total += size
            size in 1..MAX_ARG_BYTES && total <= MAX_TOTAL_BYTES && arg.none { it == '\u0000' || it == '\r' || it == '\n' }
        }
    }

    private fun validEventSubscription(args: List<String>): Boolean {
        if (args.size != 8 || args.take(3) != listOf("events", "--jsonl", "--kinds") ||
            args[4] != "--session-id" || args[6] != "--max-runtime-seconds" || args[7] != "300" ||
            !eventSession.matches(args[5])) return false
        val kinds = args[3].split(',')
        return kinds.isNotEmpty() && kinds.distinct().size == kinds.size && kinds.all(eventKinds::contains)
    }

    private fun validList(args: List<String>, prefix: List<String>, allowQuery: Boolean): Boolean {
        if (args.take(prefix.size) != prefix || args.getOrNull(prefix.size) != "--json") return false
        val tail = args.drop(prefix.size + 1)
        if (tail.isEmpty()) return true
        if (tail.size == 1) return allowQuery && genericId.matches(tail[0])
        if (tail.size == 2) return tail[0] == "--limit" && validLimit(tail[1])
        return tail.size == 3 && allowQuery && genericId.matches(tail[0]) && tail[1] == "--limit" && validLimit(tail[2])
    }

    private fun validLimit(value: String): Boolean = integer.matches(value) && value.toIntOrNull() in 1..128

    private fun validNodeOverride(args: List<String>, action: String): Boolean =
        args.size == 5 && args.take(3) == listOf("node", "override", action) &&
            nodeId.matches(args[3]) && args[4] == "--json"

    private fun validIdDigestMutation(args: List<String>, prefix: List<String>, idPattern: Regex): Boolean =
        args.size == prefix.size + 4 && args.take(prefix.size) == prefix && idPattern.matches(args[prefix.size]) &&
            args.drop(prefix.size + 1).take(2) == listOf("--json", "--expected-digest") && digest.matches(args.last())

    private fun validSubscriptionMode(args: List<String>): Boolean {
        if (args.size !in 7..9 || args.take(3) != listOf("subscription", "mode", "set") ||
            args[3] !in setOf("single", "merge") || args[4] != "--json" || args[5] != "--expected-digest" ||
            !digest.matches(args[6])) return false
        return if (args[3] == "single") {
            args.size == 9 && args[7] == "--source" && sourceId.matches(args[8])
        } else {
            args.size == 7
        }
    }

    private fun validSubscriptionEnabled(args: List<String>): Boolean =
        args.size == 6 && args[0] == "subscription" && args[1] in setOf("enable", "disable") &&
            sourceId.matches(args[2]) && args[3] == "--json" && args[4] == "--expected-digest" && digest.matches(args[5])

    private fun validSubscriptionUpdate(args: List<String>): Boolean =
        args == listOf("subscription", "update", "--json", "--wait") ||
            (args.size == 6 && args.take(4) == listOf("subscription", "update", "--json", "--wait") &&
                args[4] == "--source" && sourceId.matches(args[5]))

    private fun validSubscriptionMove(args: List<String>): Boolean {
        if (args.size !in setOf(6, 8) || args.take(2) != listOf("subscription", "move") ||
            !sourceId.matches(args[2]) || args[3] != "--json" || args[4] != "--expected-digest" ||
            !digest.matches(args[5])) return false
        return args.size == 6 || (args[6] == "--before" && sourceId.matches(args[7]))
    }

    private fun validLogs(args: List<String>): Boolean {
        if (args.take(2) != listOf("logs", "get")) return false
        var index = 2
        if (args.getOrNull(index) == "--channel") {
            if (args.getOrNull(index + 1) !in setOf("service", "subscription", "core")) return false
            index += 2
        }
        if (args.getOrNull(index) != "--json") return false
        index += 1
        if (index == args.size) return true
        return index + 2 == args.size && args[index] == "--limit" && validLimit(args[index + 1])
    }

    private fun validPayloadAppend(args: List<String>): Boolean =
        args.size == 7 && args.take(3) == listOf("webui", "payload", "append") && args[3] in payloadNamespaces &&
            payloadHandle.matches(args[4]) && args[5].length <= MAX_ARG_BYTES && base64.matches(args[5]) && args[6] == "--json"

    private fun validPayloadCommit(args: List<String>): Boolean {
        if (args.size != 7 || args.take(3) != listOf("webui", "payload", "commit") ||
            args[3] !in payloadNamespaces || !payloadHandle.matches(args[4]) ||
            args[5] !in payloadOperations || args[6] != "--json") return false
        return when (args[3]) {
            "config" -> args[5] in setOf("config-validate", "config-apply", "config-mutate")
            "subscription" -> args[5] in setOf("subscription-import-preview", "subscription-import-apply")
            "backup" -> args[5] == "backup-restore"
            "node" -> args[5] == "node-override-apply"
            else -> false
        }
    }

    private fun validPayloadRemove(args: List<String>): Boolean =
        args.size == 6 && args.take(3) == listOf("webui", "payload", "remove") && args[3] in payloadNamespaces &&
            payloadHandle.matches(args[4]) && args[5] == "--json"

    private val mutatingOperations = setOf(
        "service.start",
        "service.stop",
        "capture.enable",
        "capture.disable",
        "core.start",
        "core.stop",
        "config.reload",
        "node.test",
        "node.test-all",
        "node.select.auto",
        "node.select.manual",
        "node.override.remove",
        "node.remove",
        "subscription.mode.set",
        "subscription.select",
        "subscription.set-enabled",
        "subscription.update",
        "subscription.enable",
        "subscription.disable",
        "subscription.move",
        "subscription.remove",
        "logs.clear",
        "connection.close",
        "connections.close-all",
        "ruleset.update",
        "backup.export",
        "webui.payload.create",
        "webui.payload.append",
        "webui.payload.commit",
        "webui.payload.remove",
    )
}
