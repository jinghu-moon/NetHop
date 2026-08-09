import type { PackageInfo } from "@/bridge/host";

export type ApplicationSortField = "name" | "updatedAt" | "storage" | "lastUsed";
export type ApplicationSortDirection = "asc" | "desc";

export interface ApplicationSort {
  readonly field: ApplicationSortField;
  readonly direction: ApplicationSortDirection;
}

const nameCollator = new Intl.Collator("zh-CN", { numeric: true, sensitivity: "base" });

function compareNames(left: PackageInfo, right: PackageInfo): number {
  const result = nameCollator.compare(left.appLabel || left.packageName, right.appLabel || right.packageName);
  return result || nameCollator.compare(left.packageName, right.packageName);
}

function numericValue(item: PackageInfo, field: ApplicationSortField): number | undefined {
  const value = field === "updatedAt" ? item.lastUpdateTimeMs : field === "storage" ? item.storageBytes : item.lastUsedTimeMs;
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

export function hasApplicationSortData(items: readonly PackageInfo[], field: ApplicationSortField): boolean {
  return field === "name" || items.some((item) => numericValue(item, field) !== undefined);
}

export function sortApplications(items: readonly PackageInfo[], sort: ApplicationSort): readonly PackageInfo[] {
  const direction = sort.direction === "asc" ? 1 : -1;
  return [...items].sort((left, right) => {
    if (sort.field === "name") return compareNames(left, right) * direction;
    const leftValue = numericValue(left, sort.field);
    const rightValue = numericValue(right, sort.field);
    if (leftValue === undefined && rightValue === undefined) return compareNames(left, right);
    if (leftValue === undefined) return 1;
    if (rightValue === undefined) return -1;
    return (leftValue - rightValue) * direction || compareNames(left, right);
  });
}

export function prioritizeSelected<T>(items: readonly T[], selected: (item: T) => boolean): readonly T[] {
  return [...items.filter(selected), ...items.filter((item) => !selected(item))];
}

export function parseApplicationSort(value: string): ApplicationSort {
  const [field, direction] = value.split(":");
  if ((field === "name" || field === "updatedAt" || field === "storage" || field === "lastUsed") && (direction === "asc" || direction === "desc")) {
    return { field, direction };
  }
  return { field: "name", direction: "asc" };
}

export function serializeApplicationSort(sort: ApplicationSort): string {
  return `${sort.field}:${sort.direction}`;
}
