import { parseEventFrame, type EventPayloadDto, type TrafficDto } from "@/model/dto";

export type StreamPhase = "awaiting_snapshot" | "live" | "resync_required" | "closed";
export type ApplyResult = "snapshot" | "applied" | "duplicate" | "resync" | "closed";

export interface EventStateSnapshot {
  readonly phase: StreamPhase;
  readonly lastSequence: number;
  readonly stale: boolean;
  readonly lastConfirmedAt: number | undefined;
  readonly snapshot: EventPayloadDto | undefined;
  readonly latest: Readonly<Record<string, EventPayloadDto>>;
  readonly diagnostics: number;
}

export class EventStateMachine {
  private phase: StreamPhase = "awaiting_snapshot";
  private lastSequence = 0;
  private stale = true;
  private lastConfirmedAt: number | undefined;
  private snapshot: EventPayloadDto | undefined;
  private latest: Record<string, EventPayloadDto> = Object.create(null) as Record<string, EventPayloadDto>;
  private diagnostics = 0;

  apply(raw: unknown, now = Date.now()): ApplyResult {
    if (this.phase === "closed") return "closed";
    const frame = parseEventFrame(raw);
    if (frame.type === "error") {
      this.phase = "resync_required";
      this.stale = true;
      this.diagnostics += 1;
      return "resync";
    }
    if (frame.type === "end") {
      this.phase = "resync_required";
      this.stale = true;
      return "resync";
    }
    if (frame.sequence <= this.lastSequence) return "duplicate";
    const payload = frame.payload;
    if (this.phase === "awaiting_snapshot") {
      if (payload.kind !== "snapshot") {
        this.phase = "resync_required";
        this.stale = true;
        this.diagnostics += 1;
        return "resync";
      }
      this.snapshot = payload;
      this.latest = Object.create(null) as Record<string, EventPayloadDto>;
      this.latest[payload.kind] = payload;
      this.lastSequence = frame.sequence;
      this.lastConfirmedAt = now;
      this.phase = "live";
      this.stale = false;
      return "snapshot";
    }
    if (payload.kind === "resync_required" || frame.sequence > this.lastSequence + 1) {
      this.phase = "resync_required";
      this.stale = true;
      this.diagnostics += 1;
      return "resync";
    }
    this.latest[payload.kind] = payload;
    this.lastSequence = frame.sequence;
    this.lastConfirmedAt = now;
    this.stale = false;
    return "applied";
  }

  markStale(): void { if (this.phase !== "closed") this.stale = true; }
  beginResync(): void { if (this.phase !== "closed") { this.phase = "resync_required"; this.stale = true; } }
  reset(): void { if (this.phase !== "closed") { this.phase = "awaiting_snapshot"; this.lastSequence = 0; this.stale = true; this.snapshot = undefined; this.latest = Object.create(null) as Record<string, EventPayloadDto>; } }
  close(): void { this.phase = "closed"; this.stale = true; }
  value(): EventStateSnapshot { return { phase: this.phase, lastSequence: this.lastSequence, stale: this.stale, lastConfirmedAt: this.lastConfirmedAt, snapshot: this.snapshot, latest: Object.freeze({ ...this.latest }), diagnostics: this.diagnostics }; }
}

export function trafficFromPayload(payload: EventPayloadDto): TrafficDto | undefined { return payload.traffic; }
