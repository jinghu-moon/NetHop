import type { StatusDto } from "@/model/dto";

export type ServicePhase = "loading" | "running" | "stopped" | "paused" | "transitioning" | "unavailable";

export interface ServicePresentation {
  readonly phase: ServicePhase;
  readonly title: string;
  readonly description: string;
  readonly switchValue: boolean;
  readonly switchDisabled: boolean;
  readonly switchLoading: boolean;
}

const TRANSITIONING_STATES = new Set<StatusDto["state"]>(["init", "probing", "starting_core", "starting_tun", "stopping"]);

export function presentServiceStatus(status: StatusDto | undefined, loadFailed = false): ServicePresentation {
  if (status === undefined) {
    if (loadFailed) {
      return {
        phase: "unavailable",
        title: "状态暂不可用",
        description: "无法连接 NetHop 服务",
        switchValue: false,
        switchDisabled: true,
        switchLoading: false,
      };
    }
    return {
      phase: "loading",
      title: "正在读取状态",
      description: "正在连接 NetHop 服务",
      switchValue: false,
      switchDisabled: true,
      switchLoading: true,
    };
  }

  if (!status.service.configuredEnabled) {
    return {
      phase: "stopped",
      title: "代理已关闭",
      description: "当前网络未经过 NetHop",
      switchValue: false,
      switchDisabled: false,
      switchLoading: false,
    };
  }

  if (status.service.override === "wifi_scene" && !status.service.effectiveEnabled) {
    return {
      phase: "paused",
      title: "场景暂停",
      description: "当前 Wi-Fi 场景暂停流量接管",
      switchValue: true,
      switchDisabled: false,
      switchLoading: false,
    };
  }

  if ((status.state === "running_tproxy" || status.state === "running_tun") && status.service.effectiveEnabled) {
    return {
      phase: "running",
      title: "代理运行中",
      description: "流量接管已生效",
      switchValue: true,
      switchDisabled: false,
      switchLoading: false,
    };
  }

  if (TRANSITIONING_STATES.has(status.state)) {
    return {
      phase: "transitioning",
      title: "状态切换中",
      description: "NetHop 正在应用服务状态",
      switchValue: true,
      switchDisabled: true,
      switchLoading: true,
    };
  }

  return {
    phase: "unavailable",
    title: "代理暂不可用",
    description: "服务已启用，但流量接管尚未生效",
    switchValue: true,
    switchDisabled: false,
    switchLoading: false,
  };
}
