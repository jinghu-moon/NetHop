import type { HostKind } from "./host";

export function originalPackageIconSource(kind: HostKind, packageName: string, lastUpdateTimeMs?: number, origin?: string): string | undefined {
  if (!/^[A-Za-z0-9_.-]{1,256}$/.test(packageName)) return undefined;
  if (kind === "android") {
    const localOrigin = origin ?? globalThis.location?.origin;
    if (!localOrigin || !Number.isSafeInteger(lastUpdateTimeMs) || lastUpdateTimeMs! < 0) return undefined;
    return `${localOrigin}/package-icons/original/${lastUpdateTimeMs}/${encodeURIComponent(packageName)}`;
  }
  if (kind === "kernelsu" || kind === "apatch") return `ksu://icon/${encodeURIComponent(packageName)}`;
  return undefined;
}
