export const MAX_STRING_BYTES = 16 * 1024;
export const MAX_ARRAY_ITEMS = 10_000;
export const MAX_OBJECT_KEYS = 2_048;
export const MAX_DEPTH = 32;

const dangerousKeys = new Set(["__proto__", "prototype", "constructor"]);

export class ValidationError extends Error {
  constructor(readonly path: string, message: string) { super(`${path}: ${message}`); }
}

export function record(value: unknown, path: string, allowedKeys?: readonly string[]): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new ValidationError(path, "expected object");
  const object = value as Record<string, unknown>;
  const keys = Object.keys(object);
  if (keys.length > MAX_OBJECT_KEYS) throw new ValidationError(path, "too many keys");
  if (keys.some((key) => dangerousKeys.has(key))) throw new ValidationError(path, "prototype-shaped key is forbidden");
  if (allowedKeys) {
    const allowed = new Set(allowedKeys);
    const unknown = keys.find((key) => !allowed.has(key));
    if (unknown) throw new ValidationError(`${path}.${unknown}`, "unknown field");
  }
  return object;
}

export function array(value: unknown, path: string, max = MAX_ARRAY_ITEMS): readonly unknown[] {
  if (!Array.isArray(value)) throw new ValidationError(path, "expected array");
  if (value.length > max) throw new ValidationError(path, "too many items");
  return value;
}

export function string(value: unknown, path: string, max = MAX_STRING_BYTES): string {
  if (typeof value !== "string" || value.length === 0 || new TextEncoder().encode(value).byteLength > max) throw new ValidationError(path, "invalid string");
  return value;
}

export function optionalString(value: unknown, path: string, max = MAX_STRING_BYTES): string | undefined {
  return value === undefined || value === null ? undefined : string(value, path, max);
}

export function integer(value: unknown, path: string, min = 0, max = Number.MAX_SAFE_INTEGER): number {
  if (!Number.isSafeInteger(value) || (value as number) < min || (value as number) > max) throw new ValidationError(path, "invalid integer");
  return value as number;
}

export function finiteNumber(value: unknown, path: string, min = 0, max = Number.MAX_SAFE_INTEGER): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value < min || value > max) throw new ValidationError(path, "invalid number");
  return value;
}

export function boolean(value: unknown, path: string): boolean {
  if (typeof value !== "boolean") throw new ValidationError(path, "expected boolean");
  return value;
}

export function enumeration<const T extends string>(value: unknown, path: string, values: readonly T[]): T {
  if (typeof value !== "string" || !values.includes(value as T)) throw new ValidationError(path, "invalid enum");
  return value as T;
}

export function digest(value: unknown, path: string): string {
  const result = string(value, path, 64);
  if (!/^[a-f0-9]{64}$/.test(result)) throw new ValidationError(path, "invalid digest");
  return result;
}

export function safeExtension(value: unknown, path = "$", depth = 0): unknown {
  if (depth > MAX_DEPTH) throw new ValidationError(path, "maximum depth exceeded");
  if (value === null || typeof value === "boolean") return value;
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new ValidationError(path, "non-finite number");
    return value;
  }
  if (typeof value === "string") return string(value, path);
  if (Array.isArray(value)) return array(value, path).map((item, index) => safeExtension(item, `${path}[${index}]`, depth + 1));
  const object = record(value, path);
  return Object.fromEntries(Object.entries(object).map(([key, item]) => [key, safeExtension(item, `${path}.${key}`, depth + 1)]));
}
