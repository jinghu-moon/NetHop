import { afterEach, describe, expect, it } from "vitest";

import { createAndroidHost } from "@/bridge/android-host";

interface FakeBridge {
  onmessage: ((event: { data: string }) => void) | null;
  postMessage(message: string): void;
}

const bridgeGlobal = globalThis as typeof globalThis & { nethopAndroid?: FakeBridge };

describe("Android HostAdapter", () => {
  afterEach(() => { delete bridgeGlobal.nethopAndroid; });

  it("uses the versioned native wire for commands and package metadata", async () => {
    const sent: Record<string, unknown>[] = [];
    const bridge: FakeBridge = {
      onmessage: null,
      postMessage(encoded) {
        const request = JSON.parse(encoded) as Record<string, unknown>;
        sent.push(request);
        const type = request.kind === "run" ? "result" : "packages";
        const response = type === "result"
          ? { version: 1, request_id: request.request_id, type, errno: 0, stdout: "{}", stderr: "" }
          : { version: 1, request_id: request.request_id, type, packages: ["com.example.app"], info: request.kind === "package_info" ? [{ packageName: "com.example.app", versionName: "1", versionCode: 1, appLabel: "Example", isSystem: false, uid: 10_123 }] : undefined };
        queueMicrotask(() => bridge.onmessage?.({ data: JSON.stringify(response) }));
      },
    };
    bridgeGlobal.nethopAndroid = bridge;
    const host = createAndroidHost();

    await expect(host.run({ id: "status.get" })).resolves.toEqual({ errno: 0, stdout: "{}", stderr: "" });
    await expect(host.listPackages("user")).resolves.toEqual(["com.example.app"]);
    await expect(host.getPackagesInfo(["com.example.app"])).resolves.toMatchObject([{ packageName: "com.example.app", uid: 10_123 }]);
    expect(sent.map((request) => request.kind)).toEqual(["run", "list_packages", "package_info"]);
    expect(sent.every((request) => request.version === 1 && /^a_[a-f0-9]{32}$/.test(String(request.request_id)))).toBe(true);
  });

  it("terminates only the owned event child", () => {
    const sent: Record<string, unknown>[] = [];
    const bridge: FakeBridge = {
      onmessage: null,
      postMessage(encoded) { sent.push(JSON.parse(encoded) as Record<string, unknown>); },
    };
    bridgeGlobal.nethopAndroid = bridge;
    const host = createAndroidHost();
    const child = host.spawn({ id: "events.subscribe", kinds: ["runtime"], sessionId: `evt_${"1".repeat(32)}` });
    const childId = sent[0]?.request_id;
    child.terminate();
    expect(sent[1]).toMatchObject({ kind: "terminate", child_id: childId });
    expect(sent[1]?.request_id).not.toBe(childId);
  });
});
