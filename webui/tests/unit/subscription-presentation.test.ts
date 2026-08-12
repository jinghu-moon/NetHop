import { describe, expect, it } from "vitest";

import type { SubscriptionDto } from "@/model/dto";
import { formatRelativeTime, presentSubscription } from "@/model/subscription-presentation";

const now = 2_000_000_000;
const source = (overrides: Partial<SubscriptionDto> = {}): SubscriptionDto => ({ id: "src_primary", name: "Primary", configured: true, active: true, ...overrides });

describe("subscription presentation", () => {
  it("formats bounded relative timestamps", () => {
    expect(formatRelativeTime(now - 30, now)).toBe("刚刚");
    expect(formatRelativeTime(now - 120, now)).toBe("2 分钟前");
    expect(formatRelativeTime(now - 7_200, now)).toBe("2 小时前");
  });

  it("presents healthy, last-known-good and disabled sources from daemon state", () => {
    const gib = 1024 ** 3;
    const status = { sourceId: "src_primary", health: "healthy" as const, lastSuccessWallSeconds: now - 120, accepted: 56, duplicate: 1, rejected: 2, warnings: 3, subscriptionUploadBytes: 20 * gib, subscriptionDownloadBytes: Math.round(108.4 * gib), subscriptionTotalBytes: 200 * gib, subscriptionExpireAt: now + 23 * 86_400, usingLastKnownGood: false };
    expect(presentSubscription(source({ status }), now)).toEqual({ tone: "healthy", summary: "128.4 / 200 GB · 剩余 23 天 · 57 节点", detail: "2 分钟前更新", quotaPercent: 64.2, warning: "3 个警告 · 2 个未导入" });
    expect(presentSubscription(source({ status: { ...status, health: "degraded", usingLastKnownGood: true } }), now).summary).toBe("128.4 / 200 GB · 剩余 23 天 · 57 节点");
    expect(presentSubscription(source({ active: false, status }), now).summary).toBe("128.4 / 200 GB · 剩余 23 天 · 57 节点");
  });

  it("does not claim a node count after a failed or never-attempted update", () => {
    const failed = { sourceId: "src_primary", health: "failed" as const, accepted: 0, duplicate: 0, rejected: 0, warnings: 0, usingLastKnownGood: false, diagnosticCode: "fetch_failed" };
    expect(presentSubscription(source({ status: failed }), now)).toMatchObject({ tone: "failed", summary: "-- / -- GB · 剩余 -- 天 · -- 节点", detail: "订阅不可用 · 尚无成功记录" });
    expect(presentSubscription(source(), now)).toMatchObject({ tone: "never", summary: "-- / -- GB · 剩余 -- 天 · -- 节点", detail: "尚未更新 · 等待首次更新" });
  });

  it("degrades to node count and marks expired plans without inventing quota", () => {
    const status = { sourceId: "src_primary", health: "healthy" as const, accepted: 4, duplicate: 2, rejected: 0, warnings: 0, subscriptionExpireAt: now - 1, usingLastKnownGood: false };
    expect(presentSubscription(source({ status }), now)).toMatchObject({ summary: "-- / -- GB · 已到期 · 6 节点" });
  });
});
