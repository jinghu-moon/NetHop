# CN rule-set provenance

NetHop bundles immutable sing-box binary rule sets for the managed `rule`
mode. Subscriptions cannot replace or extend these files.

| NetHop asset | Upstream data | Snapshot source | SHA-256 |
|---|---|---|---|
| `cn-domain.srs` | `CHIZI-0618/v2ray-rules-dat` `geosite-cn.srs` | `Fanju6/NetProxy-Magisk` commit `75864788707853aa7b3e206d401f649d65c5c081` | `7017ca7d1a4baa9f00f05852a1fb0599bf2c35a79943dea6401c67a0396b075b` |
| `cn-ip.srs` | `MetaCubeX/meta-rules-dat` `geoip/cn.srs` | `Fanju6/NetProxy-Magisk` commit `75864788707853aa7b3e206d401f649d65c5c081` | `912b19669f1483f2bf911f3244e9012955add97a099e0fbe25935d8db941c6ee` |

The snapshot repository is distributed under GPL-3.0. NetHop is distributed
under AGPL-3.0 and retains the corresponding license and provenance records.
Rule-set updates require a reviewed module release and new frozen digests.
