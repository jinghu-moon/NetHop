import type { ExecResult, HostAdapter } from "./host";
import { buildCommand, MAX_JSON_BYTES, type OperationRequest } from "./operations";
import { parseSingleJsonEnvelope } from "./json";

export class BridgeError extends Error {
  constructor(
    readonly code: "timeout" | "host_error" | "nonzero" | "invalid_json" | "limit" | "daemon_error",
    message: string,
    readonly daemonCode?: string,
  ) { super(message); }
}

export interface BoundedResult {
  readonly response: unknown;
  readonly stderr: string;
  readonly errno: number;
}

const MAX_OUTPUT_BYTES = MAX_JSON_BYTES;

function bounded(value: string): string {
  if (new TextEncoder().encode(value).byteLength > MAX_OUTPUT_BYTES) throw new BridgeError("limit", "command output exceeded bound");
  return value;
}

function daemonFailure(response: unknown): BridgeError | undefined {
  if (!response || typeof response !== "object" || Array.isArray(response) || (response as { ok?: unknown }).ok !== false) return undefined;
  const error = (response as { error?: unknown }).error;
  if (!error || typeof error !== "object" || Array.isArray(error)) return new BridgeError("daemon_error", "daemon operation failed");
  const code = (error as { code?: unknown }).code;
  const message = (error as { message?: unknown }).message;
  return new BridgeError(
    "daemon_error",
    typeof message === "string" && message.length > 0 ? message : "daemon operation failed",
    typeof code === "string" ? code : undefined,
  );
}

export async function runJson(host: HostAdapter, request: OperationRequest): Promise<BoundedResult> {
  const command = buildCommand(request);
  if (command.timeoutMs === 0) throw new BridgeError("host_error", "stream operation requires spawn");
  let timer: ReturnType<typeof setTimeout> | undefined;
  let timedOut = false;
  const timeout = new Promise<never>((_, reject) => { timer = setTimeout(() => { timedOut = true; reject(new BridgeError("timeout", "operation timed out")); }, command.timeoutMs); });
  try {
    const result = await Promise.race([host.run(request), timeout]);
    bounded(result.stdout);
    bounded(result.stderr);
    if (timedOut) throw new BridgeError("timeout", "operation timed out");
    if (result.errno !== 0) throw new BridgeError("nonzero", "operation failed");
    const response = parseSingleJsonEnvelope(result.stdout);
    const failure = daemonFailure(response);
    if (failure) throw failure;
    return { response, stderr: result.stderr, errno: result.errno };
  } catch (error) {
    if (error instanceof BridgeError) throw error;
    throw new BridgeError("host_error", "host operation failed");
  } finally {
    if (timer) clearTimeout(timer);
  }
}

export function validateExecResult(result: ExecResult): void {
  bounded(result.stdout);
  bounded(result.stderr);
  if (!Number.isInteger(result.errno)) throw new BridgeError("host_error", "invalid host errno");
}
