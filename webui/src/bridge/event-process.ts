import type { HostAdapter, HostChild } from "./host";
import { JsonlDecoder } from "./jsonl";
import { type EventKind, type OperationRequest } from "./operations";

export interface EventProcess {
  readonly child: HostChild;
  readonly stop: () => void;
}

export function createEventSessionId(): string {
  const bytes = new Uint8Array(16);
  globalThis.crypto.getRandomValues(bytes);
  return `evt_${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

export function startEventProcess(host: HostAdapter, kinds: readonly EventKind[], onFrame: (frame: unknown) => void, onError: (error: unknown) => void): EventProcess {
  const request: OperationRequest = { id: "events.subscribe", kinds, sessionId: createEventSessionId() };
  const child = host.spawn(request);
  const decoder = new JsonlDecoder();
  const disposers = [
    child.stdout.onData((chunk) => {
      try { decoder.push(chunk).forEach(onFrame); } catch (error) { onError(error); child.terminate(); }
    }),
    child.stderr.onData(() => undefined),
    child.onError(onError),
    child.onExit((code) => onError(new Error(`event process exited with ${code ?? "unknown"}`))),
  ];
  return {
    child,
    stop() {
      disposers.forEach((dispose) => dispose());
      decoder.dispose();
      child.terminate();
    },
  };
}
