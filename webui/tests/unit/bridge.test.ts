import { describe, expect, it, vi } from "vitest";

import { BridgeError, runJson } from "@/bridge/command";
import { createAppHost } from "@/bridge/context";
import { detectHostCapability, type ExecResult, type HostAdapter, type HostChild, type PackageInfo } from "@/bridge/host";
import { createMockHost } from "@/bridge/mock-host";
import { JsonlDecoder } from "@/bridge/jsonl";
import { parseSingleJsonEnvelope } from "@/bridge/json";
import { createEventSessionId } from "@/bridge/event-process";
import { buildCommand, EVENT_SESSION_MAX_RUNTIME_SECONDS, NETHOPCTL_PATH, redactCommand, type OperationRequest } from "@/bridge/operations";
import { readPackages } from "@/bridge/package-adapter";
import { uploadPrivatePayload } from "@/bridge/private-payload";
import { validatedQuery } from "@/model/client";
import { parseConfig, parseNodeList, parseStatus, parseTraffic } from "@/model/dto";

function host(run: (request: OperationRequest) => Promise<ExecResult>, packages: readonly PackageInfo[] = []): HostAdapter {
  const child: HostChild = {
    stdout: { onData: () => () => undefined }, stderr: { onData: () => () => undefined },
    onExit: () => () => undefined, onError: () => () => undefined, terminate: () => undefined,
  };
  return {
    capability: { kind: "browser", available: true, methods: ["mock"] }, run, spawn: () => child,
    listPackages: () => packages.map((item) => item.packageName), getPackagesInfo: () => packages,
    toast: () => undefined, enableEdgeToEdge: () => undefined, exit: () => undefined,
  };
}

describe("operation allowlist", () => {
  it("maps only typed operations to the fixed executable", () => {
    const sessionId = `evt_${"a".repeat(32)}`;
    const command = buildCommand({ id: "events.subscribe", kinds: ["runtime", "traffic"], sessionId });
    expect(command.executable).toBe(NETHOPCTL_PATH);
    expect(command.args).toEqual(["events", "--jsonl", "--kinds", "runtime,traffic", "--session-id", sessionId, "--max-runtime-seconds", String(EVENT_SESSION_MAX_RUNTIME_SECONDS)]);
    expect(buildCommand({ id: "events.terminate", sessionId }).args).toEqual(["webui", "events", "terminate", sessionId, "--json"]);
    expect(createEventSessionId()).toMatch(/^evt_[a-f0-9]{32}$/);
    expect(() => buildCommand({ id: "events.terminate", sessionId: "evt_../../proc" })).toThrow("invalid event session");
    expect(() => buildCommand({ id: "node.list", query: "x;id" })).toThrow("invalid query");
    expect(() => buildCommand({ id: "connections.get", query: "$(id)" })).toThrow("invalid query");
    expect(() => buildCommand({ id: "webui.payload.append", namespace: "config", handle: "../../etc/passwd", chunk: "AAAA" })).toThrow("invalid payload handle");
  });

  it("redacts every private payload command preview", () => {
    const command = buildCommand({ id: "webui.payload.append", namespace: "config", handle: `p_${"a".repeat(32)}`, chunk: "c2VjcmV0" });
    expect(redactCommand(command)).toEqual([NETHOPCTL_PATH, "[private-payload]"]);
    expect(JSON.stringify(redactCommand(command))).not.toContain("c2VjcmV0");
  });
});

describe("host capability", () => {
  it("supports stateful typed mock responses without bypassing command validation", async () => {
    let mode: "single" | "merge" = "single";
    const adapter = createMockHost({ responses: {
      "subscription.mode.set": (request) => {
        if (request.id !== "subscription.mode.set") throw new Error("unexpected request");
        mode = request.mode;
        return { errno: 0, stdout: JSON.stringify({ ok: true }), stderr: "" };
      },
      "subscription.mode.get": () => ({ errno: 0, stdout: JSON.stringify({ mode }), stderr: "" }),
    } });
    await adapter.run({ id: "subscription.mode.set", mode: "merge", expectedDigest: "0".repeat(64) });
    expect(JSON.parse((await adapter.run({ id: "subscription.mode.get" })).stdout)).toEqual({ mode: "merge" });
    await expect(adapter.run({ id: "subscription.mode.set", mode: "single", expectedDigest: "0".repeat(64) })).rejects.toThrow("single mode requires source id");
  });

  it("keeps every overview preview payload valid", async () => {
    const adapter = createAppHost();
    const [status, traffic, config, nodes] = await Promise.all([
      validatedQuery(adapter, { id: "status.get" }, parseStatus),
      validatedQuery(adapter, { id: "traffic.get" }, parseTraffic),
      validatedQuery(adapter, { id: "config.get" }, parseConfig),
      validatedQuery(adapter, { id: "node.list", limit: 128 }, parseNodeList),
    ]);
    expect(status.lastUpdate).toBe("never");
    expect(traffic.intervalSeconds).toBe(1);
    expect(config.document).toMatchObject({ proxy: { outbound_mode: "rule" } });
    expect(nodes.nodes.find((node) => node.isActive)?.name).toBe("新加坡 · 高速");
  });

  const complete = { exec: () => undefined, spawn: () => undefined, toast: () => undefined };
  it("classifies KernelSU, declared APatch, browser mock and missing APIs without user-agent guessing", () => {
    expect(detectHostCapability({ ksu: complete }, false).kind).toBe("kernelsu");
    expect(detectHostCapability({ apatch: complete }, false).kind).toBe("apatch");
    expect(detectHostCapability({}, true)).toMatchObject({ kind: "browser", available: true });
    expect(detectHostCapability({ ksu: { exec: () => undefined } }, false)).toMatchObject({ kind: "kernelsu", available: false, reason: "missing_api" });
  });
});

describe("bounded JSON bridge", () => {
  it("accepts exactly one object and rejects text or multiple JSON values", () => {
    expect(parseSingleJsonEnvelope('{"ok":true}')).toEqual({ ok: true });
    expect(() => parseSingleJsonEnvelope("human output")).toThrow("invalid JSON");
    expect(() => parseSingleJsonEnvelope('{}\n{}')).toThrow("invalid JSON");
    expect(() => parseSingleJsonEnvelope("[]")).toThrow("invalid JSON");
  });

  it("maps nonzero and timeout without accepting a late result", async () => {
    await expect(runJson(host(async () => ({ errno: 2, stdout: "{}", stderr: "failed" })), { id: "status.get" })).rejects.toMatchObject({ code: "nonzero" });
    vi.useFakeTimers();
    const late = host(() => new Promise((resolve) => setTimeout(() => resolve({ errno: 0, stdout: "{}", stderr: "" }), 10_000)));
    const promise = runJson(late, { id: "status.get" });
    const expectation = expect(promise).rejects.toBeInstanceOf(BridgeError);
    await vi.advanceTimersByTimeAsync(5_000);
    await expectation;
    await vi.runAllTimersAsync();
    vi.useRealTimers();
  });
});

describe("JSONL decoder", () => {
  it("handles half-lines, CRLF, multiple lines and Unicode chunk boundaries", () => {
    const decoder = new JsonlDecoder();
    expect(decoder.push('{"name":"测')).toEqual([]);
    expect(decoder.push('试"}\r\n{"ok":true}\n')).toEqual([{ name: "测试" }, { ok: true }]);
    decoder.finish();
  });

  it("rejects truncated and overlong lines and clears on dispose", () => {
    const decoder = new JsonlDecoder();
    decoder.push('{"open":');
    expect(() => decoder.finish()).toThrow("truncated");
    const oversized = new JsonlDecoder();
    expect(() => oversized.push(`{"v":"${"x".repeat(33_000)}"}`)).toThrow("buffer exceeds");
    oversized.dispose();
    expect(oversized.push("{}\n")).toEqual([]);
  });
});

describe("private payload and package boundaries", () => {
  it("uploads Unicode in bounded chunks and consumes the handle", async () => {
    const requests: OperationRequest[] = [];
    const adapter = host(async (request) => {
      requests.push(request);
      const result = request.id === "webui.payload.create" ? { handle: `p_${"b".repeat(32)}` } : { accepted: true };
      return { errno: 0, stdout: JSON.stringify({ version: 3, request_id: "mock", ok: true, result }), stderr: "" };
    });
    await uploadPrivatePayload(adapter, "config", "config-apply", "配置".repeat(8_000));
    expect(requests[0]?.id).toBe("webui.payload.create");
    expect(requests.at(-1)?.id).toBe("webui.payload.commit");
    expect(requests.filter((request) => request.id === "webui.payload.append").length).toBeGreaterThan(1);
  });

  it("removes staging state after append failure", async () => {
    const ids: string[] = [];
    const adapter = host(async (request) => {
      ids.push(request.id);
      if (request.id === "webui.payload.append") throw new Error("fail");
      const result = request.id === "webui.payload.create" ? { handle: `p_${"c".repeat(32)}` } : {};
      return { errno: 0, stdout: JSON.stringify({ version: 3, request_id: "mock", ok: true, result }), stderr: "" };
    });
    await expect(uploadPrivatePayload(adapter, "config", "config-apply", "secret")).rejects.toThrow();
    expect(ids.at(-1)).toBe("webui.payload.remove");
  });

  it("uses host package APIs and rejects invalid package names", () => {
    const packages: PackageInfo[] = [{ packageName: "tv.danmaku.bili", versionName: "1", versionCode: 1, appLabel: "Bilibili", isSystem: false, uid: 10123, lastUpdateTimeMs: 2_000_000_000_000, storageBytes: 640_000_000, lastUsedTimeMs: 2_000_000_100_000 }];
    expect(readPackages(host(async () => ({ errno: 0, stdout: "{}", stderr: "" }), packages), "all")).toEqual(packages);
    const invalid = [{ ...packages[0]!, packageName: "bad;package" }];
    expect(() => readPackages(host(async () => ({ errno: 0, stdout: "{}", stderr: "" }), invalid), "all")).toThrow("package name");
    const invalidMetric = [{ ...packages[0]!, storageBytes: -1 }];
    expect(readPackages(host(async () => ({ errno: 0, stdout: "{}", stderr: "" }), invalidMetric), "all")).toEqual([]);
  });
});
