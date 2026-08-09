export type ApplicationMode = "all" | "blacklist" | "whitelist";

interface PackageTarget {
  readonly kind: "package";
  readonly android_user_id: number;
  readonly package: string;
}

interface UidTarget {
  readonly kind: "uid";
  readonly uid: number;
}

type ApplicationTarget = PackageTarget | UidTarget;

export interface ApplicationPolicyMutation {
  readonly type: "set_application_policy";
  readonly mode: ApplicationMode;
  readonly targets: readonly ApplicationTarget[];
}

export interface ApplicationPolicy {
  readonly mode: ApplicationMode;
  readonly packages: ReadonlySet<string>;
}

function record(value: unknown): Readonly<Record<string, unknown>> | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Readonly<Record<string, unknown>>
    : undefined;
}

function target(value: unknown): ApplicationTarget | undefined {
  const item = record(value);
  if (!item) return undefined;
  if (item.kind === "package" && Number.isInteger(item.android_user_id) && typeof item.package === "string") {
    return { kind: "package", android_user_id: Number(item.android_user_id), package: item.package };
  }
  if (item.kind === "uid" && Number.isInteger(item.uid)) return { kind: "uid", uid: Number(item.uid) };
  return undefined;
}

function applications(document: Readonly<Record<string, unknown>>): Readonly<Record<string, unknown>> | undefined {
  return record(document.applications);
}

function targets(document: Readonly<Record<string, unknown>>): readonly ApplicationTarget[] {
  const values = applications(document)?.targets;
  return Array.isArray(values) ? values.map(target).filter((item): item is ApplicationTarget => item !== undefined) : [];
}

export function readApplicationPolicy(document: Readonly<Record<string, unknown>>): ApplicationPolicy {
  const rawMode = applications(document)?.mode;
  const mode = rawMode === "blacklist" || rawMode === "whitelist" ? rawMode : "all";
  const packages = targets(document)
    .filter((item): item is PackageTarget => item.kind === "package" && item.android_user_id === 0)
    .map((item) => item.package);
  return { mode, packages: new Set(packages) };
}

export function buildApplicationPolicyDocument(
  document: Readonly<Record<string, unknown>>,
  mode: ApplicationMode,
  packages: ReadonlySet<string>,
): Record<string, unknown> {
  const result = structuredClone(document) as Record<string, unknown>;
  const preservedTargets = mode === "all" ? [] : targets(document).filter((item) => item.kind === "uid" || item.android_user_id !== 0);
  const packageTargets: PackageTarget[] = mode === "all" ? [] : [...packages]
    .sort((left, right) => left.localeCompare(right))
    .map((packageName) => ({ kind: "package", android_user_id: 0, package: packageName }));
  result.applications = { mode, targets: [...packageTargets, ...preservedTargets] };
  return result;
}

export function buildApplicationPolicyMutation(
  document: Readonly<Record<string, unknown>>,
  mode: ApplicationMode,
  packages: ReadonlySet<string>,
): ApplicationPolicyMutation {
  const candidate = buildApplicationPolicyDocument(document, mode, packages);
  return { type: "set_application_policy", mode, targets: targets(candidate) };
}
