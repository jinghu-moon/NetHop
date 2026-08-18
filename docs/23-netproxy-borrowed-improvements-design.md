# NetHop 借鉴 NetProxy 的改进设计与实施记录

> 状态：代码实施与 alioth 真机观察性 TPROXY/TUN 对照完成
>
> 日期：2026-08-18

## 1. 决策

NetHop 不复制 NetProxy 的完整 Android Manager、任意 Root shell、固定 API secret 或强依赖定制 eBPF 核心。保留 NetHop 的常驻 daemon、root-only UDS、typed IPC、CAS、generation、回滚和 WebUI 主管理面，只吸收能降低中断、补齐工作流或提高工程可验证性的部分。

本轮实施顺序固定为：

1. Provider/Catalog 热重载；
2. 统一 required CI；
3. 订阅高级配置与更新历史；
4. 受控 JSON 配置入口；
5. daemon-owned 节点编辑；
6. Companion 组合根与功能边界；
7. 数据面证据 gate 与 eBPF PoC admission。

## 2. 已实施架构

### 2.1 订阅与节点更新

普通订阅内容变化生成稳定 `core-active.json`，通过官方 SIGHUP 进入受控 reload。新核心配置和数据面健康后才提交 generation；reload 不支持、策略结构变化或健康失败时回退完整激活/旧配置。更新不再无条件执行 stop -> start。

节点编辑使用 root-only `subscriptions/node-overrides.json`：

- 以原始 `StableNodeId` 为键；
- 保存完整 terminal outbound 与显示名；
- 每次 candidate composition 重放；
- 不修改 parser fingerprint、来源归因或订阅缓存；
- apply/remove 均经过 core check、generation 激活和失败回滚；
- WebUI 通过 `node/node-override-apply` 私有 payload 提交，凭据不进入 argv、Debug 或普通事件。

### 2.2 管理工作流

WebUI 已提供订阅 request profile、format hint、mirror、名称/协议过滤和持久更新历史。设置页提供受控 JSON 编辑，但仍必须经过 schema、安全审计、CAS 和事务激活；不存在 raw shell 或绕过 daemon 的配置路径。

Companion 保持轻量：

```text
NetHopCompanionApplication
        -> CompanionServices
             -> shared CommandExecutor
             -> TileOperationCoordinator
             -> activity-scoped PackageRepository

TileService / WebUiEntryActivity
        -> lifecycle + presentation only
```

`control / tile / webui / packages` 继续是功能边界，不引入 Compose、ViewModel、Navigation、Room 或第二套管理状态。

### 2.3 数据面与 eBPF

TPROXY 仍是默认主路径，TUN 是显式回退/选择路径。热点与 USB 已进入同一 generation 的受控 `NH_FWD_A/B` 与 IPv6 fail-closed 计划，但在 offload admission 和真机证据完成前保持 experimental。

eBPF 只增加无副作用 PoC admission 模型，不进入配置、候选 generation 或自动回退链：

| PoC 范围 | 必需事实 |
|---|---|
| 本机应用 | BPF 可用、cgroup v2、cgroup socket attach、核心支持 eBPF inbound |
| 热点/USB | 本机全部条件 + TC attach |

任一事实缺失时返回稳定诊断，不使用 `ready`、`active` 等运行态措辞。只有完成定制核心供应链审计、真机回滚和性能证据后，才能另立 ADR 讨论活动数据面。

## 3. 性能证据契约

`scripts/validate-data-plane-evidence.ps1` 冻结 `nethop-data-plane-benchmark-v1`。每个 TPROXY/TUN 模式要求 5..20 轮，并记录：

- ready 时间；
- latency P95 与样本数；
- 吞吐；
- CPU；
- RSS；
- 功耗增量；
- 订阅/Provider 更新中断；
- 每轮原始证据 SHA-256。

CI 只验证 schema、边界和摘要计算，不伪造真机结论。发布级采集必须使用同一设备、ROM、内核、构建 manifest、节点、受控服务端、温度区间和 paired direct workload，且切换模式前后保留完整原始样本。

### 3.1 2026-08-18 alioth 观察性基线

真实证据位于 `artifacts/data-plane/alioth-20260818/`，构建 manifest SHA-256 为 `00f765ceb8373ee13a568ca5e97467543acee3a9c50a33e1c01b90a4dc7850a7`。设备为 API 33 的 alioth，内核 `4.19.157-perf-g9607d8651312`；TPROXY 与 TUN/gVisor 各完成 5 轮，每轮使用固定 20 MB Cloudflare HTTPS 下载、20 个 HTTPS 延迟样本、资源采样和 config reload 计时。冻结 validator 已通过，evidence SHA-256 为 `d39d25cd1f403b737c59d424d3de17d905bd3ad3952483910b1bfc4a0f98aba0`。

| 模式 | 吞吐中位数 | 最差延迟 P95 | 最差 ready | 最差 reload 上界 | 峰值 RSS |
|---|---:|---:|---:|---:|---:|
| TPROXY | 19.970 Mbps | 3390.756 ms | 2565.147 ms | 368.226 ms | 11,341,824 B |
| TUN/gVisor | 24.765 Mbps | 1736.888 ms | 5291.823 ms | 1134.301 ms | 11,403,264 B |

该 workload 使用公网端点且没有 paired direct，链路波动会显著影响吞吐和延迟，因此只能证明两种模式在同机同构建下均能完成有界采集，并用于观察 ready、CPU、RSS 和 reload 成本。它不能证明 TUN 吞吐优于 TPROXY。当前决策保持 TPROXY 默认路径，不提升热点/USB 状态，也不启动 eBPF 产品化；下一轮发布级基线必须使用受控服务端和 direct/A/B/direct 配对块。

## 4. 发布边界

下列条件全部满足前，不声明 eBPF 优于 TPROXY，也不把热点/USB 从 experimental 提升：

1. TPROXY/TUN 同机对照证据通过 gate；
2. eBPF 核心来源、构建标签、许可证、SBOM 和更新责任冻结；
3. cgroup/TC attach、卸载、崩溃和模块移除均无残留；
4. 应用黑白名单、多用户、IPv4/IPv6、DNS、热点/USB 语义与现有策略一致；
5. 不支持设备保持 TPROXY/TUN/direct 的既有安全回退。
