import { MAX_JSON_BYTES } from "./operations";

export function parseSingleJsonEnvelope(text: string): unknown {
  const bytes = new TextEncoder().encode(text).byteLength;
  if (bytes === 0 || bytes > MAX_JSON_BYTES) throw new Error("JSON response exceeds bound");
  try {
    const value: unknown = JSON.parse(text);
    if (value === null || typeof value !== "object" || Array.isArray(value)) throw new Error("JSON envelope must be an object");
    return value;
  } catch {
    throw new Error("invalid JSON response");
  }
}
