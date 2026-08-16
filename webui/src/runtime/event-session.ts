import type { HostAdapter } from "@/bridge/host";
import { startEventProcess, type EventProcess } from "@/bridge/event-process";
import { validatedQuery } from "@/model/client";
import { parseHello } from "@/model/dto";
import { EventStateMachine } from "./event-state";
import { browserRetryClock, ReconnectBackoff, type RetryClock } from "./reconnect";
import { PROTOCOL_VERSION, type EventKind } from "@/bridge/operations";

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
  private connectionEpoch = 0;
  private connecting = false;
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
  stop(): void { this.manualStop = true; this.active = false; this.invalidateConnection(); this.state.close(); this.status = "closed"; this.emit(); }
  setVisible(visible: boolean): void {
    if (this.visible === visible) return;
    this.visible = visible;
    if (!visible) { this.invalidateConnection(); this.state.markStale(); this.status = "stale"; this.emit(); return; }
    if (this.active) { this.state.reset(); this.status = "connecting"; this.connect(0); }
  }
  private connect(delayMs: number): void {
    this.cancelRetry();
    if (!this.active || !this.visible || this.connecting || this.process) return;
    if (delayMs > 0) { this.retryHandle = this.clock.setTimeout(() => { this.retryHandle = undefined; this.connect(0); }, delayMs); return; }
    this.connecting = true;
    const epoch = ++this.connectionEpoch;
    void this.handshakeAndSubscribe(epoch);
  }
  private async handshakeAndSubscribe(epoch: number): Promise<void> {
    try {
      const hello = await validatedQuery(this.options.host, { id: "hello", managerVersion: this.options.managerVersion }, parseHello);
      if (epoch !== this.connectionEpoch || !this.active || !this.visible || this.manualStop) return;
      if (!hello.compatible || hello.daemonProtocolMin !== PROTOCOL_VERSION || hello.daemonProtocolMax !== PROTOCOL_VERSION) {
        this.connecting = false;
        this.active = false;
        this.status = "incompatible";
        this.emit();
        return;
      }
      this.backoff.reset();
      this.state.reset();
      this.connecting = false;
      this.process = startEventProcess(this.options.host, this.streamKinds, (raw) => this.receive(raw), (error) => this.failed(error));
    } catch (error) {
      if (epoch === this.connectionEpoch) { this.connecting = false; this.failed(error); }
    }
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
  private invalidateConnection(): void { this.connectionEpoch += 1; this.connecting = false; this.cancelRetry(); this.disposeProcess(); }
  private emit(): void { this.options.onState?.(this.state.value()); }
  sessionStatus(): typeof this.status { return this.status; }
}
