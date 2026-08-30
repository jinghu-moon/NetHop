import { describe, expect, it } from "vitest";

import type { StatusDto } from "@/model/dto";
import { presentServiceStatus } from "@/model/service-presentation";

function status(
  state: StatusDto["state"],
  configuredEnabled = true,
  effectiveEnabled = configuredEnabled,
  override?: StatusDto["service"]["override"],
): StatusDto {
  return {
    schemaVersion: 2,
    state,
    lastUpdate: "succeeded",
    service: {
      configuredEnabled,
      effectiveEnabled,
      ...(override === undefined ? {} : { override }),
    },
    extension: {},
  };
}

function tunLifecycleStatus(captureState: "enabled" | "disabled"): StatusDto {
  return {
    ...status(captureState === "enabled" ? "running_tun" : "running_tun"),
    extension: { lifecycle: { core_state: "ready", capture_state: captureState, attachment_kind: "tun", detachable: true } },
  };
}

describe("presentServiceStatus", () => {
  it("keeps an absent status in loading instead of reporting a stopped service", () => {
    expect(presentServiceStatus(undefined)).toEqual({
      phase: "loading",
      title: "正在读取状态",
      description: "正在连接 NetHop 服务",
      switchValue: false,
      switchDisabled: true,
      switchLoading: true,
    });
  });

  it("reports an initial status query failure without pretending the service is stopped", () => {
    expect(presentServiceStatus(undefined, true)).toMatchObject({
      phase: "unavailable",
      title: "状态暂不可用",
      switchDisabled: true,
      switchLoading: false,
    });
  });

  it.each(["running_tproxy", "running_tun"] as const)("presents %s as running", (state) => {
    expect(presentServiceStatus(status(state))).toMatchObject({
      phase: "running",
      title: "代理运行中",
      switchValue: true,
      switchDisabled: false,
    });
  });

  it("identifies an enabled TUN attachment instead of presenting it as TPROXY", () => {
    expect(presentServiceStatus(tunLifecycleStatus("enabled"))).toMatchObject({
      phase: "running",
      title: "TUN 代理运行中",
      description: "TUN 路由接管已生效",
    });
  });

  it("identifies a disabled TUN attachment while keeping the core warm", () => {
    expect(presentServiceStatus(tunLifecycleStatus("disabled"))).toMatchObject({
      phase: "stopped",
      title: "TUN 接管已关闭",
      description: "核心和 TUN 接口保持就绪，路由未被接管",
    });
  });

  it("uses configured state as the explicit stopped truth", () => {
    expect(presentServiceStatus(status("init", false, false))).toMatchObject({
      phase: "stopped",
      title: "代理已关闭",
      switchValue: false,
      switchDisabled: false,
    });
  });

  it("preserves an enabled switch while a Wi-Fi scene pauses effective capture", () => {
    expect(presentServiceStatus(status("init", true, false, "wifi_scene"))).toMatchObject({
      phase: "paused",
      title: "场景暂停",
      switchValue: true,
      switchDisabled: false,
    });
  });

  it.each(["init", "probing", "starting_core", "starting_tun", "stopping"] as const)("presents %s as transitioning", (state) => {
    expect(presentServiceStatus(status(state, true, true))).toMatchObject({
      phase: "transitioning",
      title: "状态切换中",
      switchValue: true,
      switchDisabled: true,
      switchLoading: true,
    });
  });

  it.each(["degraded", "fail_open_direct", "backoff", "circuit_open"] as const)("presents %s as unavailable without pretending it is stopped", (state) => {
    expect(presentServiceStatus(status(state, true, true))).toMatchObject({
      phase: "unavailable",
      title: "代理暂不可用",
      switchValue: true,
      switchDisabled: false,
    });
  });
});
