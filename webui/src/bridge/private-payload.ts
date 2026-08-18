import type { HostAdapter } from "./host";
import { runJson } from "./command";
import type { PayloadNamespace, PayloadOperationByNamespace } from "./operations";

const CHUNK_BYTES = 12 * 1024;
const MAX_PAYLOAD_BYTES = 1024 * 1024;

function base64(bytes: Uint8Array): string {
  let binary = "";
  bytes.forEach((byte) => { binary += String.fromCharCode(byte); });
  return btoa(binary);
}

function handleFrom(response: unknown): string {
  if (!response || typeof response !== "object") throw new Error("payload create response is invalid");
  const result = (response as { result?: unknown }).result;
  if (!result || typeof result !== "object" || typeof (result as { handle?: unknown }).handle !== "string") throw new Error("payload handle is missing");
  const handle = (result as { handle: string }).handle;
  if (!/^p_[a-f0-9]{32}$/.test(handle)) throw new Error("payload handle is invalid");
  return handle;
}

export async function uploadPrivatePayload<Namespace extends PayloadNamespace>(
  host: HostAdapter,
  namespace: Namespace,
  operation: PayloadOperationByNamespace[Namespace],
  text: string,
): Promise<unknown> {
  const bytes = new TextEncoder().encode(text);
  if (bytes.byteLength === 0 || bytes.byteLength > MAX_PAYLOAD_BYTES) throw new Error("payload exceeds bound");
  let handle: string | undefined;
  try {
    const created = await runJson(host, { id: "webui.payload.create", namespace });
    handle = handleFrom(created.response);
    for (let offset = 0; offset < bytes.byteLength; offset += CHUNK_BYTES) {
      const chunk = bytes.slice(offset, Math.min(offset + CHUNK_BYTES, bytes.byteLength));
      await runJson(host, { id: "webui.payload.append", namespace, handle, chunk: base64(chunk) });
    }
    const committed = await runJson(host, { id: "webui.payload.commit", namespace, handle, operation });
    return committed.response;
  } catch (error) {
    if (handle) {
      try { await runJson(host, { id: "webui.payload.remove", namespace, handle }); } catch { /* cleanup is best effort */ }
    }
    throw error;
  }
}
