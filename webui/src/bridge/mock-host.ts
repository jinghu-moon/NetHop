import type { ExecResult, HostAdapter, HostCapability, HostChild, PackageInfo } from "./host";
import { buildCommand, type OperationRequest, type OperationRequest as Request } from "./operations";

export type MockResponse = ExecResult | ((request: OperationRequest) => ExecResult | Promise<ExecResult>);

export interface MockHostScript {
  readonly responses?: Readonly<Partial<Record<Request["id"], MockResponse>>>;
  readonly streams?: Readonly<Partial<Record<"events.subscribe", readonly string[]>>>;
  readonly packages?: readonly PackageInfo[];
  readonly latencyMs?: number;
  readonly closeStreams?: boolean;
}

class MockChild implements HostChild {
  readonly stdout = { onData: (listener: (chunk: string) => void): (() => void) => this.add(this.stdoutListeners, listener) };
  readonly stderr = { onData: (listener: (chunk: string) => void): (() => void) => this.add(this.stderrListeners, listener) };
  private readonly stdoutListeners = new Set<(chunk: string) => void>();
  private readonly stderrListeners = new Set<(chunk: string) => void>();
  private readonly exitListeners = new Set<(code: number | null) => void>();
  private readonly errorListeners = new Set<(error: unknown) => void>();
  private stopped = false;
  constructor(private readonly chunks: readonly string[], private readonly latencyMs: number, private readonly closeWhenDone: boolean) {
    queueMicrotask(() => this.emit());
  }
  private add<T>(set: Set<T>, listener: T): () => void { set.add(listener); return () => set.delete(listener); }
  private emit(): void {
    if (this.stopped) return;
    this.chunks.forEach((chunk, index) => setTimeout(() => { if (!this.stopped) this.stdoutListeners.forEach((listener) => listener(chunk)); }, this.latencyMs * index));
    if (this.closeWhenDone) setTimeout(() => { if (!this.stopped) this.exitListeners.forEach((listener) => listener(0)); }, this.latencyMs * this.chunks.length + 1);
  }
  onExit(listener: (code: number | null) => void): () => void { return this.add(this.exitListeners, listener); }
  onError(listener: (error: unknown) => void): () => void { return this.add(this.errorListeners, listener); }
  terminate(): void { this.stopped = true; this.exitListeners.forEach((listener) => listener(null)); }
}

export function createMockHost(script: MockHostScript = {}): HostAdapter {
  const capability: HostCapability = { kind: "browser", available: true, methods: ["mock", "spawn", "exec", "listPackages", "getPackagesInfo"] };
  const responses = script.responses ?? {};
  return {
    capability,
    async run(request: OperationRequest) {
      buildCommand(request);
      const response = responses[request.id];
      await new Promise((resolve) => setTimeout(resolve, script.latencyMs ?? 0));
      if (typeof response === "function") return response(request);
      return response ?? { errno: 0, stdout: JSON.stringify({ version: 6, request_id: "mock", ok: true, result: {} }), stderr: "" };
    },
    spawn(request) {
      buildCommand(request);
      return new MockChild(script.streams?.["events.subscribe"] ?? [], script.latencyMs ?? 0, script.closeStreams ?? true);
    },
    listPackages: async (type) => (type === "all" ? script.packages ?? [] : (script.packages ?? []).filter((item) => type === "system" ? item.isSystem : !item.isSystem)).map((item) => item.packageName),
    getPackagesInfo: async (packages) => (script.packages ?? []).filter((item) => packages.includes(item.packageName)),
    toast: () => undefined,
    enableEdgeToEdge: () => undefined,
    exit: () => undefined,
  };
}
