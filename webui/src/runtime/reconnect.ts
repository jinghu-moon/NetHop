export interface RetryClock { setTimeout(callback: () => void, delayMs: number): unknown; clearTimeout(handle: unknown): void; random(): number }

export const browserRetryClock: RetryClock = { setTimeout: (callback, delayMs) => setTimeout(callback, delayMs), clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>), random: () => Math.random() };

export class ReconnectBackoff {
  private attempt = 0;
  constructor(private readonly baseMs = 250, private readonly maxMs = 30_000, private readonly jitterRatio = 0.2) {}
  next(random = Math.random()): number {
    const exponential = Math.min(this.maxMs, this.baseMs * 2 ** this.attempt);
    this.attempt = Math.min(this.attempt + 1, 16);
    const jitter = exponential * this.jitterRatio * (random * 2 - 1);
    return Math.max(0, Math.round(exponential + jitter));
  }
  reset(): void { this.attempt = 0; }
  get currentAttempt(): number { return this.attempt; }
}
