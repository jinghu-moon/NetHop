import { describe, expect, it } from "vitest";

import { buildApplicationPolicyDocument, buildApplicationPolicyMutation, readApplicationPolicy } from "@/model/application-policy";

describe("application policy document", () => {
  it("reads package targets while preserving the configured mode", () => {
    const policy = readApplicationPolicy({
      applications: {
        mode: "blacklist",
        targets: [
          { kind: "package", android_user_id: 0, package: "tv.danmaku.bili" },
          { kind: "uid", uid: 10123 },
        ],
      },
    });

    expect(policy.mode).toBe("blacklist");
    expect([...policy.packages]).toEqual(["tv.danmaku.bili"]);
  });

  it("replaces the complete application policy without mutating the source document", () => {
    const source = {
      service: { enabled: true },
      applications: {
        mode: "blacklist",
        targets: [{ kind: "uid", uid: 10123 }],
      },
    } as const;

    const result = buildApplicationPolicyDocument(source, "whitelist", new Set(["com.example.beta", "com.example.alpha"]));

    expect(result).not.toBe(source);
    expect(result.service).toEqual({ enabled: true });
    expect(result.applications).toEqual({
      mode: "whitelist",
      targets: [
        { kind: "package", android_user_id: 0, package: "com.example.alpha" },
        { kind: "package", android_user_id: 0, package: "com.example.beta" },
        { kind: "uid", uid: 10123 },
      ],
    });
    expect(source.applications.targets).toEqual([{ kind: "uid", uid: 10123 }]);
  });

  it("clears targets when all applications are selected", () => {
    const result = buildApplicationPolicyDocument(
      { applications: { mode: "blacklist", targets: [{ kind: "uid", uid: 10123 }] } },
      "all",
      new Set(["tv.danmaku.bili"]),
    );

    expect(result.applications).toEqual({ mode: "all", targets: [] });
  });

  it("builds one atomic mutation for mode and targets", () => {
    const mutation = buildApplicationPolicyMutation(
      { applications: { mode: "blacklist", targets: [{ kind: "uid", uid: 10123 }] } },
      "whitelist",
      new Set(["com.example.beta", "com.example.alpha"]),
    );

    expect(mutation).toEqual({
      type: "set_application_policy",
      mode: "whitelist",
      targets: [
        { kind: "package", android_user_id: 0, package: "com.example.alpha" },
        { kind: "package", android_user_id: 0, package: "com.example.beta" },
        { kind: "uid", uid: 10123 },
      ],
    });
  });
});
