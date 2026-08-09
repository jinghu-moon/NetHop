import { computed, reactive, type ComputedRef } from "vue";

export type OperationPhase = "idle" | "accepted" | "running" | "success" | "failure" | "conflict" | "timeout";
export type OperationTerminal = Exclude<OperationPhase, "idle" | "accepted" | "running">;

export interface OperationState {
  readonly id: string;
  readonly key: string;
  readonly phase: OperationPhase;
  readonly code?: string;
  readonly message?: string;
  readonly startedAt?: number;
  readonly finishedAt?: number;
}

const transitions: Readonly<Record<OperationPhase, readonly OperationPhase[]>> = {
  idle: ["accepted"], accepted: ["running", "failure", "conflict", "timeout"], running: ["success", "failure", "conflict", "timeout"],
  success: [], failure: [], conflict: [], timeout: [],
};

export function canTransition(from: OperationPhase, to: OperationPhase): boolean {
  return transitions[from].includes(to);
}

export function reduceOperation(previous: OperationState, next: Pick<OperationState, "phase"> & Partial<OperationState>): OperationState {
  if (!canTransition(previous.phase, next.phase) && previous.phase !== next.phase) return previous;
  return { ...previous, ...next };
}

export function createOperationStore(): {
  readonly byId: Readonly<Record<string, OperationState>>;
  readonly active: ComputedRef<readonly OperationState[]>;
  readonly begin: (id: string, key: string) => void;
  readonly update: (id: string, phase: OperationPhase, details?: Pick<OperationState, "code" | "message">) => void;
  readonly clear: (id: string) => void;
} {
  const state = reactive<Record<string, OperationState>>({});
  const active = computed(() => Object.values(state).filter((item) => item.phase === "accepted" || item.phase === "running"));
  return {
    byId: state,
    active,
    begin: (id, key) => { state[id] = { id, key, phase: "accepted", startedAt: Date.now() }; },
    update: (id, phase, details = {}) => {
      const previous = state[id];
      if (!previous) return;
      state[id] = reduceOperation(previous, { phase, ...details, ...(phase === "success" || phase === "failure" || phase === "conflict" || phase === "timeout" ? { finishedAt: Date.now() } : {}) });
    },
    clear: (id) => { delete state[id]; },
  };
}

export function createActionLock(): { readonly run: <T>(key: string, action: () => Promise<T>) => Promise<T>; readonly has: (key: string) => boolean } {
  const active = new Set<string>();
  return {
    has: (key) => active.has(key),
    async run(key, action) {
      if (active.has(key)) throw new Error("operation already running");
      active.add(key);
      try { return await action(); } finally { active.delete(key); }
    },
  };
}
