import type { HostAdapter } from "@/bridge/host";
import { startEventProcess, type EventProcess } from "@/bridge/event-process";
import { validatedQuery } from "@/model/client";
import { parseHello } from "@/model/dto";
import { EventStateMachine } from "./event-state";
import { browserRetryClock, ReconnectBackoff, type RetryClock } from "./reconnect";
import type { EventKind } from "@/bridge/operations";

export interface EventSessionOptions {
  readonly host: HostAdapter;
  readonly kinds: readonly EventKind[];
  readonly managerVersion: string;
  readonly now?: () => number;
  readonly clock?: RetryClock;
  readonly onState?: (state: ReturnType<EventStateMachine["value"]>) => void;
  readonly onFrame?: (frame: unknown) => void;
  readonly onError?: (error: unknown) => void;
}

export class EventSession {
  readonly state = new EventStateMachine();
  private readonly clock: RetryClock;
  private readonly now: () => number;
  private readonly backoff = new ReconnectBackoff();
  private process: EventProcess | undefined;
  private retryHandle: unknown;
  private active = false;
  private visible = true;
  private manualStop = false;
  private status: "idle" | "connecting" | "live" | "stale" | "incompatible" | "unavailable" | "closed" = "idle";
  private readonly streamKinds: readonly EventKind[];
  constructor(private readonly options: EventSessionOptions) {
    this.clock = options.clock ?? browserRetryClock;
    this.now = options.now ?? (() => Date.now());
    const persistent: readonly EventKind[] = ["config", "runtime", "subscription", "generation", "network", "subscription-mode", "subscription-active-set", "node-selection", "node-active", "node-test"];
    this.streamKinds = [...persistent, ...(options.kinds.includes("traffic") ? ["traffic" as const] : [])];
  }
  start(): void {
    if (this.active) return;
    this.manualStop = false;
    this.active = true;
    this.status = "connecting";
    if (!this.options.host.capability.available) { this.active = false; this.status = "unavailable"; this.emit(); return; }
    this.connect(0);
  }
  stop(): void { this.manualStop = true; this.active = false; this.cancelRetry(); this.disposeProcess(); this.state.close(); this.status = "closed"; this.emit(); }
  setVisible(visible: boolean): void {
    this.visible = visible;
    if (!visible) { this.cancelRetry(); this.disposeProcess(); this.state.markStale(); this.status = "stale"; this.emit(); return; }
    if (this.active) { this.state.reset(); this.status = "connecting"; this.connect(0); }
  }
  private connect(delayMs: number): void {
    this.cancelRetry();
    if (!this.active || !this.visible) return;
    if (delayMs > 0) { this.retryHandle = this.clock.setTimeout(() => { this.retryHandle = undefined; this.connect(0); }, delayMs); return; }
    void this.handshakeAndSubscribe();
  }
  private async handshakeAndSubscribe(): Promise<void> {
    try {
      const hello = await validatedQuery(this.options.host, { id: "hello", managerVersion: this.options.managerVersion }, parseHello);
      if (!hello.compatible || hello.daemonProtocolMin !== 3 || hello.daemonProtocolMax !== 3) {
        this.active = false;
        this.status = "incompatible";
        this.emit();
        return;
      }
      this.backoff.reset();
      this.state.reset();
      this.process = startEventProcess(this.options.host, this.streamKinds, (raw) => this.receive(raw), (error) => this.failed(error));
    } catch (error) { this.failed(error); }
  }
  private receive(raw: unknown): void {
    try {
      this.options.onFrame?.(raw);
      const result = this.state.apply(raw, this.now());
      if (result === "snapshot" || result === "applied") this.status = "live";
      if (result === "resync") { this.state.beginResync(); this.disposeProcess(); this.scheduleRetry(0); }
      this.emit();
    } catch (error) { this.failed(error); }
  }
  private failed(error: unknown): void {
    if (this.manualStop || !this.active || !this.visible) return;
    this.disposeProcess();
    this.state.markStale();
    this.status = "stale";
    this.options.onError?.(error);
    this.emit();
    this.scheduleRetry(this.backoff.next(this.clock.random()));
  }
  private scheduleRetry(delayMs: number): void { this.connect(delayMs); }
  private cancelRetry(): void { if (this.retryHandle !== undefined) { this.clock.clearTimeout(this.retryHandle); this.retryHandle = undefined; } }
  private disposeProcess(): void { this.process?.stop(); this.process = undefined; }
  private emit(): void { this.options.onState?.(this.state.value()); }
  sessionStatus(): typeof this.status { return this.status; }
}
