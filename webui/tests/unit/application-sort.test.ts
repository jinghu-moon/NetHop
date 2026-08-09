import { describe, expect, it } from "vitest";

import { hasApplicationSortData, parseApplicationSort, prioritizeSelected, serializeApplicationSort, sortApplications } from "@/model/application-sort";
import type { PackageInfo } from "@/bridge/host";

const packages: readonly PackageInfo[] = [
  { packageName: "com.zeta", versionName: "1", versionCode: 1, appLabel: "视频 10", isSystem: false, uid: 10001, lastUpdateTimeMs: 300, storageBytes: 20, lastUsedTimeMs: 30 },
  { packageName: "com.alpha", versionName: "1", versionCode: 1, appLabel: "视频 2", isSystem: false, uid: 10002, lastUpdateTimeMs: 100, storageBytes: 80, lastUsedTimeMs: 10 },
  { packageName: "com.beta", versionName: "1", versionCode: 1, appLabel: "相同", isSystem: false, uid: 10003, lastUpdateTimeMs: 200, storageBytes: 40, lastUsedTimeMs: 20 },
  { packageName: "com.gamma", versionName: "1", versionCode: 1, appLabel: "相同", isSystem: false, uid: 10004 },
];

describe("application sorting", () => {
  it("sorts names naturally and uses package name as a stable tie breaker", () => {
    expect(sortApplications(packages, { field: "name", direction: "asc" }).map((item) => item.packageName)).toEqual(["com.alpha", "com.zeta", "com.beta", "com.gamma"]);
  });

  it("sorts numeric fields in both directions", () => {
    expect(sortApplications(packages, { field: "storage", direction: "desc" }).map((item) => item.packageName)).toEqual(["com.alpha", "com.beta", "com.zeta", "com.gamma"]);
    expect(sortApplications(packages, { field: "updatedAt", direction: "asc" }).map((item) => item.packageName)).toEqual(["com.alpha", "com.beta", "com.zeta", "com.gamma"]);
  });

  it("puts missing numeric values last in either direction and preserves input", () => {
    const original = packages.map((item) => item.packageName);
    expect(sortApplications(packages, { field: "lastUsed", direction: "desc" }).map((item) => item.packageName)).toEqual(["com.zeta", "com.beta", "com.alpha", "com.gamma"]);
    expect(packages.map((item) => item.packageName)).toEqual(original);
  });

  it("reports unavailable metadata instead of treating it as zero", () => {
    expect(hasApplicationSortData(packages, "name")).toBe(true);
    expect(hasApplicationSortData(packages, "storage")).toBe(true);
    expect(hasApplicationSortData(packages.map(({ storageBytes: _storage, ...item }) => item), "storage")).toBe(false);
  });

  it("parses and serializes only supported preference values", () => {
    const sort = parseApplicationSort("lastUsed:desc");
    expect(sort).toEqual({ field: "lastUsed", direction: "desc" });
    expect(serializeApplicationSort(sort)).toBe("lastUsed:desc");
    expect(parseApplicationSort("invalid")).toEqual({ field: "name", direction: "asc" });
  });
});

it("keeps the active sort stable inside selected and unselected partitions", () => {
  const items = [{ name: "A", selected: false }, { name: "B", selected: true }, { name: "C", selected: true }];
  expect(prioritizeSelected(items, (item) => item.selected).map((item) => item.name)).toEqual(["B", "C", "A"]);
});
