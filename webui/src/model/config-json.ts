const MAX_CONFIG_JSON_BYTES = 1024 * 1024;

export function serializeConfigDocument(document: Readonly<Record<string, unknown>>): string {
  return JSON.stringify(document, null, 2);
}

export function parseConfigDocument(input: string): Readonly<Record<string, unknown>> {
  if (input.length === 0 || new TextEncoder().encode(input).length > MAX_CONFIG_JSON_BYTES) throw new Error("配置 JSON 大小无效");
  rejectDuplicateObjectKeys(input);
  const value: unknown = JSON.parse(input);
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("配置 JSON 必须是对象");
  return value as Readonly<Record<string, unknown>>;
}

interface ObjectContext {
  readonly kind: "object";
  readonly keys: Set<string>;
  expectingKey: boolean;
}

interface ArrayContext { readonly kind: "array" }
type Context = ObjectContext | ArrayContext;

function rejectDuplicateObjectKeys(input: string): void {
  const stack: Context[] = [];
  for (let index = 0; index < input.length; index += 1) {
    const character = input[index];
    if (character === '"') {
      const start = index;
      index += 1;
      for (; index < input.length; index += 1) {
        if (input[index] === "\\") { index += 1; continue; }
        if (input[index] === '"') break;
      }
      if (index >= input.length) return;
      const context = stack.at(-1);
      if (context?.kind === "object" && context.expectingKey) {
        let cursor = index + 1;
        while (/\s/.test(input[cursor] ?? "")) cursor += 1;
        if (input[cursor] === ":") {
          const key = JSON.parse(input.slice(start, index + 1)) as string;
          if (context.keys.has(key)) throw new Error("配置 JSON 包含重复字段");
          context.keys.add(key);
          context.expectingKey = false;
        }
      }
      continue;
    }
    if (character === "{") stack.push({ kind: "object", keys: new Set(), expectingKey: true });
    else if (character === "[") stack.push({ kind: "array" });
    else if (character === "}" || character === "]") stack.pop();
    else if (character === ",") {
      const context = stack.at(-1);
      if (context?.kind === "object") context.expectingKey = true;
    }
  }
}
