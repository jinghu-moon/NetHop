import type { OperationRequest } from "./operations";

export type HostKind = "browser" | "kernelsu" | "apatch" | "android";

export interface HostCapability {
  readonly kind: HostKind;
  readonly available: boolean;
  readonly version?: string;
  readonly methods: readonly string[];
  readonly reason?: "missing_api" | "unavailable" | "unsupported";
}

export interface ExecResult {
  readonly errno: number;
  readonly stdout: string;
  readonly stderr: string;
}

export interface HostChild {
  readonly stdout: { onData(listener: (chunk: string) => void): () => void };
  readonly stderr: { onData(listener: (chunk: string) => void): () => void };
  onExit(listener: (code: number | null) => void): () => void;
  onError(listener: (error: unknown) => void): () => void;
  terminate(): void;
}

export interface PackageInfo {
  readonly packageName: string;
  readonly versionName: string;
  readonly versionCode: number;
  readonly appLabel: string;
  readonly isSystem: boolean;
  readonly uid: number;
  /** Optional Android package metadata. Hosts that cannot provide it leave it undefined. */
  readonly lastUpdateTimeMs?: number;
  readonly storageBytes?: number;
  readonly lastUsedTimeMs?: number;
}

export interface HostAdapter {
  readonly capability: HostCapability;
  run(request: OperationRequest): Promise<ExecResult>;
  spawn(request: OperationRequest): HostChild;
  listPackages(type: "user" | "system" | "all"): Promise<readonly string[]>;
  getPackagesInfo(packages: readonly string[]): Promise<readonly PackageInfo[]>;
  toast(message: string): void;
  enableEdgeToEdge(enabled: boolean): void;
  exit(): void;
}

export interface HostBridgeCandidates { readonly ksu?: Record<string, unknown>; readonly apatch?: Record<string, unknown>; readonly nethopAndroid?: Record<string, unknown> }

export function detectHostCapability(
  candidates: HostBridgeCandidates = globalThis as HostBridgeCandidates,
  allowBrowserMock = import.meta.env.DEV,
): HostCapability {
  if (candidates.nethopAndroid) {
    const methods = ["postMessage"].filter((key) => typeof candidates.nethopAndroid?.[key] === "function");
    return methods.length === 1
      ? { kind: "android", available: true, methods: ["run", "spawn", "toast", "listPackages", "getPackagesInfo", "enableEdgeToEdge", "exit"] }
      : { kind: "android", available: false, methods, reason: "missing_api" };
  }
  const required = ["exec", "spawn"] as const;
  for (const [kind, candidate] of [["kernelsu", candidates.ksu], ["apatch", candidates.apatch]] as const) {
    if (!candidate) continue;
    const methods = ["exec", "spawn", "toast", "moduleInfo", "listPackages", "getPackagesInfo", "enableEdgeToEdge", "exit"]
      .filter((key) => typeof candidate[key] === "function");
    if (required.every((method) => methods.includes(method))) return { kind, available: true, methods };
    return { kind, available: false, methods, reason: "missing_api" };
  }
  return allowBrowserMock
    ? { kind: "browser", available: true, methods: ["mock"] }
    : { kind: "browser", available: false, methods: [], reason: "unavailable" };
}
