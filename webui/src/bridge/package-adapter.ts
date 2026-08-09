import type { HostAdapter, PackageInfo } from "./host";

const MAX_PACKAGES = 10_000;
const MAX_PACKAGE_BYTES = 256;

export function readPackages(host: HostAdapter, type: "user" | "system" | "all"): readonly PackageInfo[] {
  const packages = [...host.listPackages(type)];
  if (packages.length > MAX_PACKAGES) throw new Error("package list exceeds bound");
  if (packages.some((name) => name.length === 0 || name.length > MAX_PACKAGE_BYTES || !/^[A-Za-z0-9_.-]+$/.test(name))) throw new Error("package name is invalid");
  const result: PackageInfo[] = [];
  for (let offset = 0; offset < packages.length; offset += 128) {
    result.push(...host.getPackagesInfo(packages.slice(offset, offset + 128)));
  }
  if (result.length > MAX_PACKAGES) throw new Error("package info exceeds bound");
  return result.filter((item) => isPackageInfo(item));
}

function isPackageInfo(item: PackageInfo): boolean {
  return item.packageName.length > 0
    && item.packageName.length <= MAX_PACKAGE_BYTES
    && /^[A-Za-z0-9_.-]+$/.test(item.packageName)
    && item.versionName.length <= 128
    && item.appLabel.length <= 256
    && Number.isSafeInteger(item.versionCode)
    && Number.isSafeInteger(item.uid)
    && item.uid >= 0
    && typeof item.isSystem === "boolean"
    && validOptionalMetric(item.lastUpdateTimeMs)
    && validOptionalMetric(item.storageBytes)
    && validOptionalMetric(item.lastUsedTimeMs);
}

function validOptionalMetric(value: number | undefined): boolean {
  return value === undefined || (Number.isSafeInteger(value) && value >= 0);
}
