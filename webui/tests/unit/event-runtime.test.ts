import { describe, expect, it, vi } from "vitest";

import { EventStateMachine } from "@/runtime/event-state";
import { createMockHost } from "@/bridge/mock-host";
import { EventSession } from "@/runtime/event-session";
import { ReconnectBackoff } from "@/runtime/reconnect";
import { TrafficCoalescer, TrafficRing, type FrameScheduler } from "@/runtime/traffic-ring";

const frame = (sequence: number, kind: string, extra: Record<string, unknown> = {}): unknown => ({ version: 3, request_id: "events", sequence, kind: "item", payload: { kind, ...extra } });

describe("snapshot-first event state", () => {
  it("rejects item before snapshot and requests resync", () => {
    const state = new EventStateMachine();
    expect(state.apply(frame(1, "runtime"))).toBe("resync");
    expect(state.value()).toMatchObject({ phase: "resync_required", stale: true });
  });

  it("hydrates atomically, ignores duplicates and detects gaps", () => {
    const state = new EventStateMachine();
    expect(state.apply(frame(4, "snapshot"), 100)).toBe("snapshot");
    expect(state.apply(frame(5, "runtime", { state: "running_tproxy" }), 101)).toBe("applied");
    expect(state.apply(frame(5, "runtime", { state: "stopped" }), 102)).toBe("duplicate");
    expect(state.value().latest.runtime?.value.state).toBe("running_tproxy");
    expect(state.apply(frame(7, "network"), 103)).toBe("resync");
    expect(state.value().stale).toBe(true);
  });

  it("clears daemon facts on resync while remaining idempotently disposable", () => {
    const state = new EventStateMachine();
    state.apply(frame(1, "snapshot"));
    state.apply(frame(2, "runtime"));
    state.reset();
    expect(state.value()).toMatchObject({ phase: "awaiting_snapshot", lastSequence: 0, latest: {} });
    state.close();
    state.close();
    expect(state.apply(frame(3, "snapshot"))).toBe("closed");
  });
});

describe("bounded reconnect and traffic", () => {
  it("uses capped exponential backoff with injectable jitter", () => {
    const backoff = new ReconnectBackoff(250, 1000, 0.2);
    expect([backoff.next(0.5), backoff.next(0.5), backoff.next(0.5), backoff.next(0.5)]).toEqual([250, 500, 1000, 1000]);
    backoff.reset();
    expect(backoff.currentAttempt).toBe(0);
  });

  it("keeps only a fixed 60-point monotonically ordered window", () => {
    const ring = new TrafficRing(60);
    for (let index = 0; index < 86_400; index += 1) ring.push({ up: index, down: index * 2, intervalSeconds: 1 }, index === 30 ? 1 : index);
    const values = ring.snapshot();
    expect(values).toHaveLength(60);
    expect(values[0]?.up).toBe(86_340);
    expect(values.every((value, index) => index === 0 || value.timestampMs > values[index - 1]!.timestampMs)).toBe(true);
  });

  it("publishes at most once per animation frame and keeps the newest sample", () => {
    let callback: (() => void) | undefined;
    const scheduler: FrameScheduler = { request(next) { callback = next; return 1; }, cancel: vi.fn() };
    const published: number[] = [];
    const coalescer = new TrafficCoalescer(scheduler, (point) => published.push(point.up));
    for (let index = 0; index < 100; index += 1) coalescer.push({ timestampMs: index, up: index, down: index, intervalSeconds: 1 });
    expect(published).toEqual([]);
    callback?.();
    expect(published).toEqual([99]);
    coalescer.dispose();
  });

  it("performs hello before events and stops the child on visibility changes", async () => {
    vi.useFakeTimers();
    const host = createMockHost({
      responses: {
        hello: { errno: 0, stderr: "", stdout: JSON.stringify({ version: 3, request_id: "hello", ok: true, result: { manager_version: "webui-0.1.0", compatible: true, daemon_protocol_min: 3, daemon_protocol_max: 3, daemon_schema_min: 3, daemon_schema_max: 3, active_schema_version: 3, supported_operations: [], supported_features: [] } }) },
      },
      streams: { "events.subscribe": ['{"version":3,"request_id":"events","sequence":1,"kind":"item","payload":{"kind":"snapshot","runtime":{}}}\n'] },
      closeStreams: false,
    });
    const session = new EventSession({ host, kinds: ["runtime"], managerVersion: "webui-0.1.0" });
    session.start();
    await vi.runAllTimersAsync();
    expect(session.state.value().phase).toBe("live");
    expect(session.sessionStatus()).toBe("live");
    session.setVisible(false);
    expect(session.state.value().stale).toBe(true);
    session.stop();
    vi.useRealTimers();
  });

  it("treats an incompatible hello as terminal and never spawns events", async () => {
    vi.useFakeTimers();
    const host = createMockHost({ responses: { hello: { errno: 0, stderr: "", stdout: JSON.stringify({ version: 3, request_id: "hello", ok: true, result: { manager_version: "webui-0.1.0", compatible: false, daemon_protocol_min: 1, daemon_protocol_max: 1, daemon_schema_min: 1, daemon_schema_max: 1, active_schema_version: 1, supported_operations: [], supported_features: [] } }) } } });
    const session = new EventSession({ host, kinds: ["runtime"], managerVersion: "webui-0.1.0" });
    session.start();
    await vi.runAllTimersAsync();
    expect(session.sessionStatus()).toBe("incompatible");
    expect(session.state.value().phase).toBe("awaiting_snapshot");
    session.stop();
    vi.useRealTimers();
  });
});
