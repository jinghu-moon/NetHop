import { describe, expect, it } from "vitest";
import { buildCommand, redactCommand } from "@/bridge/operations";

const nodeId = "nh1s-0123456789abcdef";
const digest = "a".repeat(64);

describe("consumer operation allowlist", () => {
  it("maps node operations to fixed typed argv", () => {
    expect(buildCommand({ id: "node.test", nodeId }).args).toEqual(["node", "test", nodeId, "--json"]);
    expect(buildCommand({ id: "node.test-all" }).args).toEqual(["node", "test-all", "--json"]);
    expect(buildCommand({ id: "node.select", nodeId }).args).toEqual(["node", "select", nodeId, "--json"]);
    expect(buildCommand({ id: "node.remove", nodeId, expectedDigest: digest }).args).toContain("--expected-digest");
    expect(redactCommand(buildCommand({ id: "node.export", nodeId }))).toEqual(["/data/adb/modules/nethop/bin/nethopctl", "[private-payload]"]);
  });

  it("maps operational commands without accepting user paths or shell syntax", () => {
    expect(buildCommand({ id: "connections.close-all" }).args).toEqual(["connections", "close-all", "--json"]);
    expect(buildCommand({ id: "logs.clear" }).args).toEqual(["logs", "clear", "--json"]);
    expect(buildCommand({ id: "logs.get", channel: "subscription", limit: 32 }).args).toEqual(["logs", "get", "--channel", "subscription", "--json", "--limit", "32"]);
    expect(buildCommand({ id: "metrics.get" }).args).toEqual(["metrics", "--json"]);
    expect(buildCommand({ id: "diagnostics.bundle" }).args).toEqual(["diagnose", "--json"]);
    expect(buildCommand({ id: "ruleset.update", wait: true }).args).toEqual(["ruleset", "update", "--json", "--wait"]);
    expect(() => buildCommand({ id: "connection.close", connectionId: "x;reboot" })).toThrow("invalid connection id");
  });

  it("uses a daemon-owned fixed backup destination", () => {
    const command = buildCommand({ id: "backup.export" });
    expect(command.args).toEqual(["backup", "export", "--file", "/data/adb/nethop/backups/webui-config-backup.json", "--json"]);
    expect(command.sensitive).toBe(true);
  });

  it("rejects the command injection corpus before argv construction", () => {
    const corpus = [
      "x;reboot", "x&&id", "x|id", "x`id`", "x$(id)", "x\n--json", "x\r\n--wait",
      "x>out", "x<input", "x\0tail", "'quoted'", "\"quoted\"", "节点；重启",
    ];
    for (const value of corpus) {
      expect(() => buildCommand({ id: "connection.close", connectionId: value })).toThrow();
      expect(() => buildCommand({ id: "node.list", query: value })).toThrow();
    }
    expect(buildCommand({ id: "connection.close", connectionId: "tcp:stable-01" }).args).toEqual(["connection", "close", "tcp:stable-01", "--json"]);
  });
});
