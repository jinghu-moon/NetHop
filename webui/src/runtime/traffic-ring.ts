import type { TrafficDto } from "@/model/dto";

export interface TrafficPoint extends TrafficDto { readonly timestampMs: number }

export class TrafficRing {
  private readonly values: Array<TrafficPoint | undefined>;
  private length = 0;
  private cursor = 0;
  private lastTimestamp = 0;
  constructor(readonly capacity = 60) {
    if (!Number.isInteger(capacity) || capacity < 1 || capacity > 3600) throw new Error("invalid traffic ring capacity");
    this.values = new Array<TrafficPoint | undefined>(capacity);
  }
  push(sample: TrafficDto, timestampMs: number): void {
    if (!Number.isSafeInteger(timestampMs) || timestampMs < 0) throw new Error("invalid traffic timestamp");
    const timestamp = Math.max(timestampMs, this.lastTimestamp + (this.length > 0 ? 1 : 0));
    this.lastTimestamp = timestamp;
    this.values[this.cursor] = Object.freeze({ ...sample, timestampMs: timestamp });
    this.cursor = (this.cursor + 1) % this.capacity;
    this.length = Math.min(this.length + 1, this.capacity);
  }
  snapshot(): readonly TrafficPoint[] {
    const result: TrafficPoint[] = [];
    const start = this.length === this.capacity ? this.cursor : 0;
    for (let index = 0; index < this.length; index += 1) {
      const value = this.values[(start + index) % this.capacity];
      if (value) result.push(value);
    }
    return result;
  }
  clear(): void { this.values.fill(undefined); this.length = 0; this.cursor = 0; this.lastTimestamp = 0; }
}

export interface FrameScheduler { request(callback: () => void): unknown; cancel(handle: unknown): void }

export class TrafficCoalescer {
  private pending: TrafficPoint | undefined;
  private scheduled: unknown;
  constructor(private readonly scheduler: FrameScheduler, private readonly publish: (point: TrafficPoint) => void) {}
  push(point: TrafficPoint): void {
    this.pending = point;
    if (this.scheduled !== undefined) return;
    this.scheduled = this.scheduler.request(() => {
      this.scheduled = undefined;
      const value = this.pending;
      this.pending = undefined;
      if (value) this.publish(value);
    });
  }
  dispose(): void { if (this.scheduled !== undefined) this.scheduler.cancel(this.scheduled); this.scheduled = undefined; this.pending = undefined; }
}

export const browserFrameScheduler: FrameScheduler = {
  request(callback) { return typeof requestAnimationFrame === "function" ? requestAnimationFrame(callback) : setTimeout(callback, 16); },
  cancel(handle) { if (typeof cancelAnimationFrame === "function") cancelAnimationFrame(handle as number); else clearTimeout(handle as ReturnType<typeof setTimeout>); },
};
