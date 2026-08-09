import type { HostCapability } from "@/bridge/host";
import type { HelloDto } from "@/model/dto";

export type CompatibilityState = "ready" | "host_unavailable" | "protocol_incompatible";

export function compatibilityState(host: HostCapability | undefined, hello: HelloDto | undefined): CompatibilityState {
  if (!host?.available) return "host_unavailable";
  if (hello && !hello.compatible) return "protocol_incompatible";
  return "ready";
}

export function compatibilityMessage(state: CompatibilityState): string {
  if (state === "host_unavailable") return "当前环境没有可用的 root WebUI 宿主";
  if (state === "protocol_incompatible") return "模块与 WebUI 协议版本不兼容";
  return "控制服务已就绪";
}
