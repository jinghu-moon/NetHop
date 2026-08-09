import type { SubscriptionDto, SubscriptionHealth } from "./dto";

export type SubscriptionTone = "healthy" | "degraded" | "failed" | "never";

export interface SubscriptionPresentation {
  readonly tone: SubscriptionTone;
  readonly summary: string;
  readonly detail: string;
  readonly quotaPercent?: number;
  readonly warning?: string;
}

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"] as const;

function formatNumber(value: number, fractionDigits: number): string {
  return value.toFixed(fractionDigits).replace(/\.0+$/, "");
}

function formatQuota(usedBytes: number, totalBytes: number): { readonly label: string; readonly percent: number } | undefined {
  if (totalBytes <= 0) return undefined;
  let unitIndex = 0;
  let divisor = 1;
  while (unitIndex < BYTE_UNITS.length - 1 && totalBytes / divisor >= 1024) {
    divisor *= 1024;
    unitIndex += 1;
  }
  const used = usedBytes / divisor;
  const total = totalBytes / divisor;
  return {
    label: `${formatNumber(used, used < 10 ? 2 : 1)} / ${formatNumber(total, total < 10 ? 2 : 1)} ${BYTE_UNITS[unitIndex]}`,
    percent: Math.round(Math.min(100, Math.max(0, usedBytes / totalBytes * 100)) * 100) / 100,
  };
}

function expiryLabel(expireAt: number | undefined, nowSeconds: number): string | undefined {
  if (expireAt === undefined) return undefined;
  if (expireAt <= nowSeconds) return "已到期";
  return `剩余 ${Math.ceil((expireAt - nowSeconds) / 86_400)} 天`;
}

function legacyHealth(state: string | undefined): SubscriptionHealth {
  if (state === "succeeded") return "healthy";
  if (state === "failed") return "failed";
  return "never";
}

export function formatRelativeTime(wallSeconds: number, nowSeconds: number): string {
  const delta = Math.max(0, nowSeconds - wallSeconds);
  if (delta < 60) return "刚刚";
  if (delta < 3_600) return `${Math.floor(delta / 60)} 分钟前`;
  if (delta < 86_400) return `${Math.floor(delta / 3_600)} 小时前`;
  if (delta < 604_800) return `${Math.floor(delta / 86_400)} 天前`;
  const value = new Date(wallSeconds * 1_000);
  const now = new Date(nowSeconds * 1_000);
  return value.getFullYear() === now.getFullYear()
    ? `${value.getMonth() + 1}月${value.getDate()}日`
    : `${value.getFullYear()}年${value.getMonth() + 1}月${value.getDate()}日`;
}

export function presentSubscription(item: SubscriptionDto, nowSeconds: number): SubscriptionPresentation {
  const status = item.status;
  const health = status?.health ?? legacyHealth(item.state);
  const accepted = status?.accepted ?? 0;
  const nodeCount = accepted + (status?.duplicate ?? 0);
  const latestSuccess = status?.lastSuccessWallSeconds;
  const upload = status?.subscriptionUploadBytes;
  const download = status?.subscriptionDownloadBytes;
  const used = upload !== undefined && download !== undefined && Number.isSafeInteger(upload + download) ? upload + download : undefined;
  const quota = used !== undefined && status?.subscriptionTotalBytes !== undefined
    ? formatQuota(used, status.subscriptionTotalBytes)
    : undefined;
  const metrics = [
    quota?.label ?? "-- / -- GB",
    expiryLabel(status?.subscriptionExpireAt, nowSeconds) ?? "剩余 -- 天",
    nodeCount > 0 ? `${nodeCount} 节点` : "-- 节点",
  ];
  const warning = status && (status.warnings > 0 || status.rejected > 0)
    ? [status.warnings > 0 ? `${status.warnings} 个警告` : "", status.rejected > 0 ? `${status.rejected} 个未导入` : ""].filter(Boolean).join(" · ")
    : undefined;

  if (health === "healthy") {
    return { tone: "healthy", summary: metrics.join(" · "), detail: latestSuccess === undefined ? "更新成功" : `${formatRelativeTime(latestSuccess, nowSeconds)}更新`, ...(quota ? { quotaPercent: quota.percent } : {}), ...(warning ? { warning } : {}) };
  }
  if (health === "degraded" || status?.usingLastKnownGood) {
    return { tone: "degraded", summary: metrics.join(" · "), detail: latestSuccess === undefined ? "等待下次更新" : `上次成功：${formatRelativeTime(latestSuccess, nowSeconds)}`, ...(quota ? { quotaPercent: quota.percent } : {}), ...(warning ? { warning } : {}) };
  }
  if (health === "failed") {
    return { tone: "failed", summary: metrics.join(" · "), detail: latestSuccess === undefined ? "订阅不可用 · 尚无成功记录" : `订阅不可用 · 上次成功：${formatRelativeTime(latestSuccess, nowSeconds)}`, ...(quota ? { quotaPercent: quota.percent } : {}), ...(warning ? { warning } : {}) };
  }
  return { tone: "never", summary: metrics.join(" · "), detail: "尚未更新 · 等待首次更新" };
}
