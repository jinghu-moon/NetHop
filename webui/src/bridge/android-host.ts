import type { ExecResult, HostAdapter, HostChild, PackageInfo } from "./host";
import { buildCommand, type OperationRequest } from "./operations";

interface AndroidMessageEvent { readonly data: string }
interface AndroidInjectedBridge {
  postMessage(message: string): void;
  onmessage: ((event: AndroidMessageEvent) => void) | null;
}

type NativeMessage =
  | { readonly version: 1; readonly request_id: string; readonly type: "result"; readonly errno: number; readonly stdout: string; readonly stderr: string }
  | { readonly version: 1; readonly request_id: string; readonly type: "packages"; readonly packages: readonly string[]; readonly info?: readonly PackageInfo[] }
  | { readonly version: 1; readonly request_id: string; readonly type: "ack" }
  | { readonly version: 1; readonly request_id: string; readonly type: "stdout" | "stderr"; readonly data: string }
  | { readonly version: 1; readonly request_id: string; readonly type: "exit"; readonly code: number | null }
  | { readonly version: 1; readonly request_id: string; readonly type: "error"; readonly code: string };

const MAX_MESSAGE_BYTES = 1024 * 1024;

class AndroidChild implements HostChild {
  readonly stdout = { onData: (listener: (chunk: string) => void) => this.add(this.stdoutListeners, listener) };
  readonly stderr = { onData: (listener: (chunk: string) => void) => this.add(this.stderrListeners, listener) };
  private readonly stdoutListeners = new Set<(chunk: string) => void>();
  private readonly stderrListeners = new Set<(chunk: string) => void>();
  private readonly exitListeners = new Set<(code: number | null) => void>();
  private readonly errorListeners = new Set<(error: unknown) => void>();
  private terminated = false;

  constructor(readonly id: string, private readonly transport: AndroidTransport) {}

  private add<T>(listeners: Set<T>, listener: T): () => void { listeners.add(listener); return () => listeners.delete(listener); }
  onExit(listener: (code: number | null) => void): () => void { return this.add(this.exitListeners, listener); }
  onError(listener: (error: unknown) => void): () => void { return this.add(this.errorListeners, listener); }
  accept(message: NativeMessage): void {
    if (this.terminated) return;
    if (message.type === "stdout") this.stdoutListeners.forEach((listener) => listener(message.data));
    else if (message.type === "stderr") this.stderrListeners.forEach((listener) => listener(message.data));
    else if (message.type === "exit") { this.terminated = true; this.exitListeners.forEach((listener) => listener(message.code)); }
    else if (message.type === "error") { this.terminated = true; this.errorListeners.forEach((listener) => listener(new Error(message.code))); }
  }
  terminate(): void {
    if (this.terminated) return;
    this.terminated = true;
    this.transport.terminateChild(this.id);
    this.exitListeners.forEach((listener) => listener(null));
  }
}

class AndroidTransport {
  private readonly pending = new Map<string, { resolve: (message: NativeMessage) => void; reject: (error: Error) => void; timeout: ReturnType<typeof globalThis.setTimeout> }>();
  private readonly children = new Map<string, AndroidChild>();

  constructor(private readonly bridge: AndroidInjectedBridge) {
    bridge.onmessage = (event) => this.accept(event.data);
  }

  request(payload: Record<string, unknown>, timeoutMs = 30_000): Promise<NativeMessage> {
    const id = requestId();
    return new Promise((resolve, reject) => {
      const timeout = globalThis.setTimeout(() => {
        this.pending.delete(id);
        reject(new Error("android_bridge_timeout"));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timeout });
      try {
        this.notify({ ...payload, request_id: id });
      } catch (error) {
        this.pending.delete(id);
        clearTimeout(timeout);
        reject(error instanceof Error ? error : new Error("android_bridge_send_failed"));
      }
    });
  }

  spawn(operation: OperationRequest): AndroidChild {
    const id = requestId();
    const command = buildCommand(operation);
    const child = new AndroidChild(id, this);
    this.children.set(id, child);
    try {
      this.notify({ kind: "spawn", request_id: id, operation_id: operation.id, args: command.args });
    } catch (error) {
      this.children.delete(id);
      throw error;
    }
    return child;
  }

  terminateChild(id: string): void {
    this.children.delete(id);
    this.notify({ kind: "terminate", request_id: requestId(), child_id: id });
  }

  notify(payload: Record<string, unknown>): void {
    const encoded = JSON.stringify({ version: 1, ...payload });
    if (new TextEncoder().encode(encoded).byteLength > MAX_MESSAGE_BYTES) throw new Error("android bridge request exceeds bound");
    this.bridge.postMessage(encoded);
  }

  private accept(encoded: string): void {
    if (new TextEncoder().encode(encoded).byteLength > MAX_MESSAGE_BYTES) return;
    let message: NativeMessage;
    try { message = JSON.parse(encoded) as NativeMessage; } catch { return; }
    if (message.version !== 1 || !/^a_[a-f0-9]{32}$/.test(message.request_id)) return;
    const child = this.children.get(message.request_id);
    if (child && ["stdout", "stderr", "exit", "error"].includes(message.type)) {
      child.accept(message);
      if (message.type === "exit" || message.type === "error") this.children.delete(message.request_id);
      return;
    }
    const pending = this.pending.get(message.request_id);
    if (!pending) return;
    this.pending.delete(message.request_id);
    clearTimeout(pending.timeout);
    if (message.type === "error") pending.reject(new Error(message.code));
    else pending.resolve(message);
  }
}

function requestId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return `a_${[...bytes].map((value) => value.toString(16).padStart(2, "0")).join("")}`;
}

function injectedBridge(): AndroidInjectedBridge {
  const candidate = (globalThis as unknown as { nethopAndroid?: AndroidInjectedBridge }).nethopAndroid;
  if (!candidate || typeof candidate.postMessage !== "function") throw new Error("android bridge unavailable");
  return candidate;
}

function asPackages(message: NativeMessage, info: boolean): readonly PackageInfo[] | readonly string[] {
  if (message.type !== "packages") throw new Error("android package response invalid");
  return info ? message.info ?? [] : message.packages;
}

export function createAndroidHost(): HostAdapter {
  const transport = new AndroidTransport(injectedBridge());
  return {
    capability: { kind: "android", available: true, methods: ["run", "spawn", "toast", "listPackages", "getPackagesInfo", "enableEdgeToEdge", "exit"] },
    async run(request) {
      const command = buildCommand(request);
      const message = await transport.request({ kind: "run", operation_id: request.id, args: command.args }, command.timeoutMs + 1_000);
      if (message.type !== "result") throw new Error("android command response invalid");
      return { errno: message.errno, stdout: message.stdout, stderr: message.stderr } satisfies ExecResult;
    },
    spawn(request) { return transport.spawn(request); },
    async listPackages(type) {
      return asPackages(await transport.request({ kind: "list_packages", package_type: type }), false) as readonly string[];
    },
    async getPackagesInfo(packages) {
      return asPackages(await transport.request({ kind: "package_info", packages }), true) as readonly PackageInfo[];
    },
    toast(message) { transport.notify({ kind: "toast", request_id: requestId(), message: message.slice(0, 256) }); },
    enableEdgeToEdge(enabled) { transport.notify({ kind: "edge_to_edge", request_id: requestId(), enabled }); },
    exit() { transport.notify({ kind: "exit", request_id: requestId() }); },
  };
}
