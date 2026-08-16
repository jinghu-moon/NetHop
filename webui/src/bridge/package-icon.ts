import type { HostKind } from "./host";

export function packageIconSource(kind: HostKind, packageName: string, origin?: string): string | undefined {
  if (kind === "android") {
    const localOrigin = origin ?? globalThis.location?.origin;
    return localOrigin ? `${localOrigin}/package-icons/${encodeURIComponent(packageName)}` : undefined;
  }
  if (kind === "kernelsu" || kind === "apatch") return `ksu://icon/${encodeURIComponent(packageName)}`;
  return undefined;
}
