# NetHop 系统设计

> 状态：Draft v0.6，作为 Phase 0-A 至首版实现基线
>
> 文档版本：0.6
>
> 目标平台：Android 13+（API 33+）、arm64-v8a、Magisk / KernelSU
>
> 许可证：AGPL-3.0
>
> 参考资料核验日期：2026-08-11

## 1. 文档目的

本文档根据两轮需求确认、`refer/` 内参考工程、`refer/sing-box-testing` 上游源码、`NetGuard/sub-parser` 和 `PathGuard-Next` 的工程结构形成。它定义 NetHop 首版的产品边界、系统架构、运行流程、数据模型、安全约束、性能口径和实施顺序，是后续编码、评审和验收的共同基线。

本文档只冻结已经有需求依据的能力。热点与 USB 共享、KernelSU WebUI、独立 Android App 等后续能力保留明确边界，但不提前实现。

## 2. 已确认需求

### 2.1 平台与交付

| 项目 | 决策 |
|---|---|
| Root 平台 | 架构目标同时兼容 Magisk 与 KernelSU；实际支持状态按已验证设备/版本声明 |
| Android | 设计目标为 Android 13 及以上；未验证系统版本不自动视为稳定支持 |
| ABI | 首版仅 arm64-v8a |
| 数据面核心 | 独立 sing-box CLI 进程，不使用 Android `VpnService` |
| 首版控制面 | `action.sh` + `nethopctl` CLI |
| 后续控制面 | 独立 Kotlin Android App |
| 项目许可证 | AGPL-3.0 |
| sing-box 基线 | 固定稳定版 `1.13.15`；升级需审核、兼容测试和重新发版 |
| 当前控制契约 | 用户配置只接受 schema v3；`nethopctl <-> nethopd` 只接受 Protocol v3，不保留开发期旧版本兼容层 |

### 2.2 网络与策略

| 项目 | 决策 |
|---|---|
| 默认透明代理 | TPROXY，完整支持 TCP、UDP、IPv4、IPv6 |
| 回退模式 | TPROXY 能力探测失败时，本次启动回退到 sing-box TUN |
| TUN 网络栈 | Android 首版默认 `gvisor`；`system` 仅显式启用，恢复自动优先前必须通过有界数据面探测 |
| 异常策略 | 核心崩溃后先撤销接管、恢复直连，再按有限预算重启 |
| IPv6 | 正常情况下完整代理；无法安全接管时阻断公网 IPv6 并告警 |
| 默认路由 | 局域网直连、中国大陆直连、广告阻断、其余代理 |
| DNS | 真实 IP DNS 分流，启用协议 sniff，不使用 FakeIP |
| 应用策略 | 同时支持黑名单和白名单，应用展示分为系统应用与用户应用 |
| Android 用户 | 支持所有已启动用户及工作资料；共享 UID 作为不可拆分整体 |
| 热点/USB 共享 | 不属于首版，进入产品第二阶段 |

### 2.3 订阅、节点与配置

| 项目 | 决策 |
|---|---|
| 输入格式 | Base64 URI 列表、Clash YAML、sing-box outbounds JSON |
| 协议 | VLESS、VMess、Shadowsocks、Trojan、Hysteria2、TUIC、AnyTLS |
| 错误处理 | 逐节点诊断；部分节点合法则部分成功，全部失败才拒绝该订阅 |
| 多订阅 | `single` 显式单源或 `merge` 多源合并；支持公平自动候选池、稳定去重、完整来源追踪和单源 last-known-good 缓存 |
| 节点选择 | 用户意图分为 `auto` 与稳定 node ID 的 `manual`；请求目标与 core 实际 active terminal 分开表达 |
| 更新 | 手动更新 + 默认每 24 小时自动更新；失败保留旧缓存 |
| 配置开放 | 托管配置开始，逐步开放 allowlist override，最终提供受安全约束的专家完整 JSON |
| 版本检查 | 检查 sing-box 最新稳定版，有更新时提示，不在设备上自动替换核心 |

### 2.4 可观察性与性能

| 指标 | 首版验收口径 |
|---|---|
| TPROXY 吞吐 | 受控局域网、同一路径、TCP 四流、稳定温度下达到直连的 85% 以上 |
| TUN 吞吐 | 首版目标为直连的 75% 以上，单独报告，不冒充 TPROXY 指标 |
| 空闲 CPU | `sing-box + nethopd worker + supervisor` 合计不高于 3% |
| 稳态内存 | 加载 500 个 active outbounds、空闲 10 分钟后，模块常驻进程 VmRSS 合计不高于 80 MiB |
| 更新峰值内存 | 订阅转换、候选验证与激活期间模块进程 VmRSS 合计不高于 110 MiB |
| 订阅转换 | 5 MiB、最多 10,000 节点，本地解析至序列化不高于 300 ms |
| 节点/模式/规则热路径 | 通过运行时 API、SRS 原子替换或 netfilter A/B 入口完成，网络不可用窗口不超过 1 秒 |
| 完整配置重载 | 校验、重载、健康检查或回滚不超过 3 秒 |
| 流量 | 实时速率与连接视图；历史保存总量和按节点统计 |

下载时间、`sing-box check` 和远端服务器性能不计入 300 ms 转换指标。真实订阅节点的公网波动不用于证明 85% 吞吐目标。

### 2.5 开发资源与支持声明

NetHop 当前按个人开源项目推进，不假设拥有 Pixel/GKI、Qualcomm、MediaTek、Magisk、KernelSU 的常驻设备实验室。可用真机数量有限，设备型号、Root 管理器和 ROM 以每次测试报告实际记录为准；文档中的目标平台和兼容架构不能自动转换为“已在全部 Android 13+ 设备验证”。

支持状态分为：

| 状态 | 证据 | 对外承诺 |
|---|---|---|
| `reference_verified` | 开发者可用真机完成规定功能、回滚和性能测试 | 对该型号、ROM/build fingerprint、内核、Root 管理器版本提供完整已验证声明 |
| `community_verified` | 社区贡献者提交脱敏 `probe/diagnose`、测试 manifest 和可复现结果 | 只对报告中的精确组合声明已验证，不外推到同 SoC/品牌全部设备 |
| `experimental` | 已有部分实机或集成证据，但尚未通过该能力的全部 feature/release gate | 仅显式启用；必须公开缺失证据、已知限制和回退路径，不宣称稳定支持 |
| `best_effort` | 仅满足 API/ABI 和静态能力要求，尚无真机证据 | 可以尝试安装；能力探测失败必须保持直连或进入已验证回退，不承诺 TPROXY/性能 |
| `unsupported` | 已知缺少安全接管能力、存在不可回滚冲突或测试失败 | 拒绝启用相应能力，并给出明确诊断 |

Alpha 可以只覆盖一个 `reference_verified` 组合。Magisk 或 KernelSU 中未实机验证的一方必须标记 `experimental`，不能因 fake module 测试通过就宣称完整支持。扩大设备覆盖依赖借用设备、社区封测和后续资源，不要求开发者为进入编码阶段购买完整矩阵。

## 3. 参考结论与原方案修正

### 3.1 可复用的设计

- `AndroidTProxyShell`、`box4magisk`、`NetProxy-Magisk`：TPROXY 链、连接标记、策略路由、核心防环路、应用 UID 黑白名单和能力探测。
- `MagicNet-main`：daemon 单写、更新锁、候选配置校验、原子发布、last-known-good、假 Magisk 测试、AVD 与真机分层验证。
- `Proxylink-main`：订阅格式覆盖和转换行为样本，仅作行为参考，不复制 Go 实现。
- `NetGuard/sub-parser`：`格式探测 -> 统一模型 -> sing-box 输出` 的 Rust 分层；NetHop 必须补上严格错误、字段边界和逐节点诊断。
- `PathGuard-Next`：顶层组件分区、模块脚本只做生命周期、daemon 作为唯一发布者、CLI/未来 App 复用版本化本地协议、host/集成/设备/性能测试分层。

### 3.2 必须修正的旧结论

1. sing-box 稳定版没有可替代 Proxylink 的通用订阅 Provider。NetHop 必须拥有自己的 Rust 订阅转换库。
2. `auto_redirect` 不能简单描述为“Android 没有 nftables，所以不可用”。上游当前实现和版本行为更复杂；但官方文档也明确指出 Android 上存在功能限制，因此它只能作为 TUN 内部优化，不能代替首版完整 TPROXY 方案。
3. sing-box 的 SIGHUP 是“先校验，再关闭旧实例并重新创建”，不是原地热更新。只有 selector、Clash mode 等运行时 API 操作可直接承诺低于 1 秒。
4. Clash `/traffic` 只提供进程内实时速率，`/connections` 只提供当前连接快照。历史总量和按节点精确统计不能依赖低频连接快照推算。
5. 截至 2026-08-01，上游正式版为 `v1.13.15`，预发布版为 `v1.14.0-beta.4`。`refer/sing-box-testing` 是包含 1.14 设计的开发线源码，不能将其中的新 gRPC API 当作 1.13.15 已发布契约。
6. 上游 V2Ray Stats 在路由 tracker 中按直接匹配的 outbound 计数，顶层路由为 `selector` 时不会自动归因到最终节点。精确节点历史依赖 18.1 节定义的下游终端 outbound 归因补丁，不能仅靠启用 build tag 获得。

### 3.3 v0.2 审核意见取舍

| 结论 | 意见 | 设计决定 |
|---|---|---|
| 采纳 | daemon watchdog、网络事件监听、netfilter backend/实效探测 | 补入进程模型、状态机、能力报告和运行时重协调 |
| 采纳 | DNS/UDP/QUIC/时延指标、active outbound 上限、资源子预算 | 作为 Phase 0-B/发布完整报告项目；Phase 0-A 只做路线 smoke，已确认的硬门槛不放宽 |
| 采纳 | 二进制 SRS、冲突诊断、SBOM、可复现构建 | 提升为发布设计与测试要求 |
| 条件采纳 | TPROXY 双实例切换 | 从 Phase 0-B 起作为可选原型；只有同时满足 110 MiB 更新峰值、统计隔离和回滚正确性才启用，否则保持受控重启与 3 秒 SLA |
| 条件采纳 | gRPC 改为 JSON stats exporter | 比较客户端体积、RSS、CPU、补丁面积后用 ADR 决定；默认偏向最小下游补丁，而不是先扩大 fork |
| 调整 | 为不同格式放宽转换时限 | 仍保持用户确认的 300 ms 总门槛，但按 URI/JSON/YAML 分项报告，不能用简单 URI 掩盖 YAML 成本 |
| 不采纳 | AnyTLS 在 1.13.15 中可能不可用 | 官方标明自 1.12.0 支持，`v1.13.15` 源码存在正式 outbound；保留首版支持并增加版本 fixture |
| 不采纳 | TUN GSO、首版 BBR/sysctl 调优 | 官方已声明 TUN GSO 对透明代理无收益且不再工作；全局 sysctl 会影响整机，首版不修改 |
| 不采纳 | 首版 native nft/eBPF/XDP | Android/vendor 碎片化和发布成本不符合首版轻量目标；只做能力记录，不作为依赖或自动优化 |
| 不采纳 | inbound 与各 outbound 字节严格守恒 | 代理封装、拒绝路径和统计层次不同，不能用错误等式验收；改用已知 payload fixture 分别校验计数语义 |

## 4. 设计原则

1. **单一发布者**：只有 `nethopd --worker` 可以发布活动配置、网络规则和状态 generation。
2. **先验证后生效**：下载、解析、合并、配置生成、`sing-box check`、安全审计全部通过后才切换。
3. **失败保留旧状态**：失败候选不覆盖 last-known-good；异常接管规则不得遗留。
4. **控制面与数据面分离**：Rust 负责策略和生命周期，sing-box 负责代理，内核 netfilter/TUN 负责流量接管。
5. **同一策略模型生成两种后端**：TPROXY 与 TUN 不各自维护应用黑白名单或 IPv6 语义。
6. **脚本保持薄**：模块脚本只做安装、启动、动作入口和卸载，不解析订阅、不维护定时任务、不拼装复杂规则。
7. **无静默降级**：回退必须记录原因、当前能力和泄漏保护状态。
8. **最小对外接口**：首版不开放 LAN API，不让任意 Android App 直接访问 Clash API。
9. **可验证的极致**：性能必须可测量，混合必须可回退，轻量必须有预算，安全边界不可通过配置关闭。

## 5. 总体架构

```text
                         控制面
 service.sh -------> nethopd --supervise
                              |
                              | spawn/restart
                              v
 action.sh / nethopctl / future Kotlin App(root bridge)
                         |
              Unix socket 0600, protocol v1
                         |
                         v
                  nethopd --worker
       +-----------------+------------------+
       |                 |                  |
 subscription       config composer    core monitor/stats
 parser + IR        + transaction       + capability probe
       |                 |                  |
       +------------ candidate -------------+
                         |
                 sing-box check
                         |
                    atomic publish
                         |
                         v
                     sing-box
          Clash API + chosen stats transport(local-only)
                         |
                         v
                       outbounds

                         数据面
 Android apps -> TPROXY/netfilter --+
                                  +-> sing-box -> direct/proxy/block
 Android apps -> TUN fallback -----+
```

### 5.1 进程职责

`nethopd --supervise` 是常驻 root 父进程，只负责子进程身份校验、信号转发、异常退出退避和重新拉起 `nethopd --worker`。它不读取订阅、不开放业务 IPC、不操作 netfilter，也不直接启动 sing-box；supervisor 路径不得初始化 Tokio 全功能 runtime、SQLite 或 parser，以控制常驻开销。

`nethopd --worker` 是唯一业务 daemon 和状态发布者，负责：

- Root 平台、Android、ABI、netfilter、策略路由、IPv6、TUN 和端口能力探测。
- 订阅下载、解析、合并、缓存和配置事务。
- 生成、安装和撤销 NetHop 自有网络规则。
- 启动、监督、停止和熔断 sing-box。
- 通过 Clash API 控制节点和读取实时视图。
- 通过 20.3 节冻结的统计传输采集精确总量与按 terminal outbound 计数。
- SQLite 历史聚合、状态发布、版本检查和通知状态。

worker 每次启动先执行幂等恢复，清理身份可证明属于旧 worker/core generation 的遗留状态，再决定恢复旧 generation 或保持直连。supervisor 与 worker 使用匿名 pipe/受控信号传递停止原因；不能形成第二套业务协议。

`nethopctl` 是薄客户端，负责参数解析、连接 UDS、输出人类可读或 JSON 结果。它不直接修改配置文件、iptables 或 sing-box。

`sing-box` 以独立 root 子进程运行。其监听、配置目录、PID 和 API secret 由 `nethopd` 管理。

## 6. 仓库结构

参考 PathGuard-Next 的组件边界，NetHop 首版采用 Rust workspace：

```text
NetHop/
|-- Cargo.toml
|-- Cargo.lock
|-- crates/
|   |-- nethop-core/          # 领域模型、状态机、诊断、配置事务
|   |-- nethop-subscription/  # 获取、格式探测、解析、IR、转换
|   |-- nethop-android/       # 包目录、能力探测、netfilter 计划与执行
|   |-- nethop-protocol/      # nethopd/nethopctl/App 的版本化 IPC 类型
|   |-- nethopd/              # 常驻 daemon 二进制
|   `-- nethopctl/            # CLI 二进制
|-- module/                   # Magisk/KernelSU 打包模板
|   |-- module.prop
|   |-- customize.sh
|   |-- service.sh
|   |-- action.sh
|   |-- uninstall.sh
|   |-- bin/arm64-v8a/
|   |   |-- nethopd
|   |   |-- nethopctl
|   |   `-- sing-box
|   |-- defaults/
|   `-- licenses/
|-- scripts/                  # 构建、打包、校验、AVD/真机测试
|-- tests/
|   |-- fixtures/
|   |-- integration/
|   |-- device/
|   `-- performance/
|-- docs/
|-- third_party/              # 许可证清单和受控 vendoring；不放未审核代码
`-- refer/                    # 只读参考，不进入发布包
```

### 6.1 crate 边界

| crate | 允许依赖 | 禁止职责 |
|---|---|---|
| `nethop-core` | serde 类型、纯业务逻辑 | Android shell、网络下载、SQLite |
| `nethop-subscription` | `nethop-core`、HTTP/YAML/JSON/URI 库 | 修改活动配置、启动 sing-box |
| `nethop-android` | `nethop-core`、受控系统调用封装 | 解析订阅、保存用户配置 |
| `nethop-protocol` | 纯类型与有界 framing | 业务执行、文件写入 |
| `nethopd` | 以上库、SQLite、进程与调度 | 复制 parser/规则生成逻辑 |
| `nethopctl` | `nethop-protocol` | 直接执行 root 网络命令 |

不为了“以后可能需要”继续拆 crate。只有形成独立所有权、测试边界或依赖隔离时才增加组件。

## 7. 设备运行目录

发布包位于 Root 管理器模块目录，持久数据独立放置：

```text
/data/adb/modules/nethop/       # 可被模块更新替换
|-- bin/
|-- config/
|   `-- nethop.toml -> /data/adb/nethop/config/nethop.toml
|-- service.sh
|-- action.sh
`-- module.prop

/data/adb/nethop/               # 持久、0700、root:root
|-- config/
|   `-- nethop.toml             # 唯一用户配置 ABI，0600
|-- generations/
|   |-- <generation>/config.json
|   |-- <generation>/manifest.json
|   `-- current
|-- subscriptions/
|   |-- cache/<source-id>/body
|   `-- reports/<generation>.json
|-- rulesets/
|-- stats/                     # 预留历史统计导出目录
|-- state/
|   |-- runtime.json
|   |-- source-registry.v1.json # daemon-owned source ID，0600
|   |-- nethop.db               # 自动更新调度与运行统计
|   `-- api.secret              # 256-bit 随机值，0600
|-- run/
|   |-- supervisor.pid
|   |-- worker.pid
|   |-- sing-box.pid
|   |-- nethopd.sock
|   |-- stats.sock                 # 方案 A，0600；方案 B 时不存在
`-- logs/
```

目录规则：

- secret、订阅 URL、节点凭据、数据库和生成配置一律 `0600`。
- `config/nethop.toml` 必须是 root 所有的绝对路径 `0600` 普通文件；模块只附带不含真实 URL 的 `defaults/nethop.toml`，首装复制，覆盖安装不改用户文件。
- 模块目录中的 `config/nethop.toml` 只是指向持久文件的受控 symlink；daemon 同时监听两个固定目录，并对编辑器写断链接后的普通文件候选执行完整 admission 后再导入。
- UDS 和 PID 目录不得允许普通 App 写入。
- 活动文件名不能由订阅或 IPC 调用方指定。
- 所有临时文件必须在目标目录内创建，`fsync(file) -> rename -> fsync(parent)` 后发布。
- 模块升级保留 `/data/adb/nethop`；显式卸载停止服务、撤销规则并删除该目录。

## 8. 模块生命周期

### 8.1 安装

`customize.sh` 只执行以下工作：

1. 验证 API >= 33、`ARCH=arm64`、安装环境为 Magisk 或 KernelSU。
2. 验证三个二进制的 SHA-256 和版本清单。
3. 将 arm64-v8a 二进制安装到固定路径并设置 `0755`。
4. 创建持久目录并设置 `0700/0600`。
5. 首次安装时复制默认配置；升级时不覆盖用户配置和数据库。
6. 不在安装阶段联网，不下载“最新版”二进制。

NetHop 不修改 `/system`，KernelSU 不需要 metamodule/OverlayFS 才能运行核心能力。

### 8.2 启动

`service.sh` 使用 `MODDIR=${0%/*}` 定位模块，不硬编码 Root 管理器模块路径；随后以 `exec "$MODDIR/bin/nethopd" --supervise --root /data/adb/nethop` 交出进程。Magisk/KernelSU 的 late-start service 只保证执行脚本，不提供通用的 init 式 respawn，因此不能直接 `exec` 业务 worker。supervisor 拉起 worker，worker 自行等待以下条件：

- `/data` 可用；
- `netd` 和 `system_server` 可响应；
- PackageManager 能列出目标用户；
- 至少存在一个默认网络，或已存在可离线启动的缓存配置。

启动不依赖订阅服务器在线。存在有效 generation 时先启动缓存，再在网络稳定后调度更新。

收到正常 stop/uninstall 时，supervisor 先请求 worker 完成 8.4 节清理并等待有界超时，再退出自身。worker 意外退出时，supervisor 在 1、2、4 秒退避后重启；5 分钟内连续崩溃超过 3 次则改为 60 秒低频恢复探测，避免 busy loop，同时保持一个可恢复入口。每个新 worker 都必须先清理旧 generation，supervisor 自身不得越权修改网络。

### 8.3 Action 与 CLI

`action.sh` 依次调用 `nethopctl config reload --wait`、`nethopctl update --if-needed --wait` 和 `nethopctl status --human`。它是 watch 降级和手动重试入口，不承担含糊的 toggle；持久启停只有 TOML 中的 `service.enabled`。CLI 默认 JSON 输出保持机器可读，`status --human` 只渲染受控状态枚举、generation、订阅结果和经过版本解析的核心更新提示，不打印外部 release 文本。

首版 CLI 至少包括：

```text
nethopctl status [--json]
nethopctl start|stop [--wait]
nethopctl probe [--json]
nethopctl update [--if-needed] [--wait]
nethopctl config get|schema
nethopctl config validate|apply|mutate --expected-digest DIGEST  # JSON stdin
nethopctl config reload [--wait]
nethopctl capability get
nethopctl hello --manager-version VERSION --protocol-min 3 --protocol-max 3
nethopctl events [--kinds config,runtime,subscription,generation,network] --jsonl
nethopctl connections close-all
nethopctl logs get [--limit 1..128]
nethopctl logs tail [--kinds ...] --jsonl
nethopctl logs clear
nethopctl subscription list
nethopctl subscription mode
nethopctl subscription mode set single --source <source-id> --expected-digest DIGEST
nethopctl subscription mode set merge --expected-digest DIGEST
nethopctl subscription select <source-id> --expected-digest DIGEST
nethopctl subscription add <name> <https-url> --expected-digest DIGEST
nethopctl subscription remove|move|enable|disable <source-id> --expected-digest DIGEST
nethopctl subscription import preview|apply --file <path>|--text --format auto|... --expected-digest DIGEST [--candidate-digest DIGEST]
nethopctl application list|mode|add-package|remove-package|add-uid|remove-uid ...
nethopctl network set <field-id> <value> --expected-digest DIGEST
nethopctl node selection
nethopctl node select auto
nethopctl node select manual <node-id>
nethopctl node test-all
nethopctl node export <node-id>
nethopctl node remove <node-id> --expected-digest DIGEST
```

### 8.4 停止与卸载

停止顺序固定为：阻止新控制事务、撤销 NetHop 网络规则和 IPv6 guard、停止 sing-box、落盘最终统计、关闭 UDS、清理 PID/socket。任何一步失败都写入结构化诊断，但清理继续执行。

卸载只删除 NetHop 精确拥有的链、规则、路由表、进程和数据目录，不执行全表 flush，不操作其他模块的规则。

## 9. worker 状态机与监督

```text
INIT -> PROBING -> STARTING_CORE -> RUNNING_TPROXY
                     |                  |
                     | probe failed     | runtime failure
                     v                  v
                STARTING_TUN       FAIL_OPEN_DIRECT
                     |                  |
                     v                  v
                RUNNING_TUN        BACKOFF -> retry
                                           |
                                     budget exhausted
                                           v
                                      CIRCUIT_OPEN
```

### 9.1 健康定义

核心仅在以下条件同时成立时进入 `RUNNING_*`：

- 进程存活且 PID/start time 匹配；
- Clash API 使用 secret 可访问；
- 当前 selector 存在至少一个合法 outbound；
- 对应 TPROXY 端口或 TUN 设备已建立；
- IPv4 和 IPv6 实际状态符合 capability report；
- 网络接管 generation 与配置 generation 一致。

### 9.2 崩溃与熔断

- 5 分钟窗口内最多自动重启 3 次。
- 退避为 1、2、4 秒。
- 每次尝试前先撤销接管，恢复直连；这是明确的 availability-first 策略。
- 第 3 次失败后进入 `CIRCUIT_OPEN`，等待用户手动重启或下一次设备启动。
- `runtime.json` 记录失败阶段、退出码、stderr 摘要、配置 generation、网络模式和探测结果。

worker 被 supervisor 重新拉起时先运行幂等清理和状态恢复，不能相信旧 PID 文件，也不能复用身份不一致的进程。sing-box 核心失败由仍存活的 worker 按上述状态机处理；worker 自身崩溃则由最小 supervisor 处理，两套重启预算和诊断码分开记录。

## 10. 能力探测

TPROXY 不能只检查 `/proc/config.gz` 或命令是否存在。每次启动进行无副作用或临时命名空间探测：

1. `iptables`/`ip6tables` 可执行文件真实路径、版本、restore/save 能力和可识别的 legacy/nft-wrapper backend；backend 名称只作诊断，是否可用以实际操作探测为准。
2. mangle、TPROXY、MARK、CONNMARK、owner、socket、addrtype、conntrack target/match。
3. `IP_TRANSPARENT` 的 IPv4/IPv6 socket 设置。
4. `ip rule`、独立路由表、local route 和 fwmark 匹配。
5. IPv4/IPv6 临时链的创建、引用和完整删除。
6. 预定端口、mark、table、rule priority 与现有系统/模块是否冲突。
7. SELinux enforcing 下真实执行结果。

探测输出版本化 `CapabilityReport`，包含每个原语的 `supported/unsupported/denied/conflict`、实际命令版本和脱敏 stderr。不能因为显示为 `iptables-nft` 就假定 native `nft` 可用，也不能因为 `/proc/config.gz` 有选项就跳过真实探测。首版统一通过 `iptables`/`ip6tables` restore 接口工作；native nft sets/maps 只记录能力，不生成活动计划。

失败只影响本次启动模式，不永久修改用户的 `preferred_mode=tproxy`。只有 TUN 也失败时才保持直连并进入 degraded/circuit 状态。

## 11. TPROXY 数据面

### 11.1 规则所有权

NetHop 使用短、带前缀的自有链，例如 `NH_OUT`、`NH_PRE`、`NH_APP_A/B`、`NH_DNS_A/B`。固定入口只跳转到活动 A/B generation；候选链填充成功后通过单条 replace 切换。批量操作优先使用 `iptables-restore --noflush`，不得 flush Android 或其他模块的链。

mark、mask、route table 和 rule priority 由 capability planner 从有限候选集中选择并持久化到 generation manifest。发现冲突时换用下一候选；没有无冲突组合则回退 TUN，不能覆盖未知规则。

### 11.2 流量路径

本机应用流量执行顺序：

```text
OUTPUT
  -> 核心/控制流量 mark bypass
  -> 回包与本地保留地址处理
  -> 应用 UID 黑白名单
  -> DNS 53 接管
  -> 局域网/保留地址 bypass
  -> mark + policy route to loopback
PREROUTING
  -> reply/socket divert
  -> TPROXY to sing-box inbound
```

TPROXY 自有链必须把 `-m conntrack --ctdir REPLY -j ACCEPT` 放在所有 socket、mark、TPROXY 捕获规则之前，并同时安装在 `OUTPUT` 与 `PREROUTING`。这不是可选的性能优化：本地应用首个 SYN 经策略路由进入 loopback 后，sing-box 返回的 SYN-ACK 仍会经过 netfilter；若回包再次被 mark/TPROXY 捕获，客户端会永久停留在 `SYN-SENT`。该规则依赖 `NETFILTER_XT_MATCH_CONNTRACK`，能力探测失败时 TPROXY 必须拒绝发布，不能等待 `iptables-restore` 在运行时失败。

`PREROUTING` 的精确顺序为 `reply bypass -> transparent socket divert -> -i lo -j RETURN -> mark-based TPROXY`。本机 OUTPUT 经策略路由送入 loopback 的包先由透明 socket 接收，其余 loopback 流量不能再次作为普通入站流量递归捕获；热点、USB 和中继流量另走未来独立入口，不得借删除 loopback bypass 实现。

健康检查除了确认自有链、policy rule、local route 和 inbound listener，还必须读取活动族的 netfilter snapshot，验证 `OUTPUT` 与 `PREROUTING` 两条 reply bypass 位于全部捕获规则之前，并验证 PREROUTING 的 loopback bypass 位于 transparent socket divert 之后、mark-based TPROXY 之前。仅凭核心进程存活、7893 监听或策略规则存在，不能声明数据面健康。

sing-box 所有出站 socket 必须设置独立 bypass mark。核心防环路优先依赖 mark，不以“进程恰好运行在 root UID”作为唯一保证。`nethopd` 经代理下载时显式连接 loopback SOCKS/mixed inbound，直连下载则使用 bypass mark。

### 11.3 IPv4 与 IPv6

IPv4/IPv6 使用等价链和独立策略路由。IPv6 代理能力不足时：

- 保留 `::1/128`、`fe80::/10`、必要 multicast 和局域网 `fc00::/7`；
- 对被接管应用阻断公网 GUA/default IPv6；
- 状态显示 `ipv6=blocked_degraded`；
- 不允许 IPv6 绕过代理直接联网。

IPv4 成功、IPv6 失败时仍可启动，但必须先安装 IPv6 guard，再启用 IPv4 接管。

### 11.4 运行时网络重协调

worker 订阅 `NETLINK_ROUTE` 的 link、address、IPv4/IPv6 route 事件，不通过秒级轮询观察网络。事件经过 250 ms debounce，连续震荡最多延后 2 秒，然后在全局 mutation lock 内执行幂等 reconcile：

1. 重新读取默认路由、接口地址、IPv6 GUA、活动 TUN/VPN 和策略规则。
2. 验证 NetHop 固定入口、mark/table/rule 与 IPv6 guard 仍存在且 owner generation 匹配。
3. 仅修复漂移部分；普通 Wi-Fi/蜂窝切换不重编译订阅、不无条件重启核心。
4. 默认网络消失时暂停新下载并保持防泄漏；网络恢复后等待 sing-box 内建 interface monitor 完成更新，并验证数据面。
5. 能力由 supported 变为 denied/conflict 时按 TPROXY -> TUN -> direct 状态机降级；恢复时不自动抢占用户正在使用的稳定回退模式，留到显式重探测或下次启动。

sing-box 1.13.15 已有默认接口 monitor，但它只负责核心自己的拨号与连接更新，不能替代 NetHop 对外部 netfilter 入口、policy route 和 IPv6 guard 的所有权校验。Private DNS 状态在启动、resume/reconcile 和显式诊断时读取，不做高频轮询。

由于 legacy iptables 变更不保证产生可订阅事件，worker 还以默认 60 秒低频核验自有入口、policy rule 和 IPv6 guard 的存在性；只执行 `-C`/精确查询，不扫描或复制全表。周期必须计入 3% 空闲 CPU/wakeup 预算，Phase 0-B 可在保证泄漏窗口与功耗的前提下校准。

### 11.5 不属于首版的路径

PREROUTING 中来自热点、USB 接口的流量通过独立的 `NH_FWD_A/B` 入口受控接管；IPv6 下游流量在 `NH_FWD6_A/B` 中 fail closed，避免在未实现 IPv6 TPROXY 前泄漏。接口必须来自实时 capability probe 的安全命名集合。NetHop 当前不声称兼容 Android tethering eBPF/hardware offload，该路径在完成 offload capability admission 与真机流量证据前保持 experimental。客户端 MAC、IPv6 前缀精细策略仍是后续能力。

## 12. TUN 回退

TUN 配置由同一 `CapturePolicy` 生成：

- `type=tun`、`auto_route=true`、`strict_route=true`；
- `stack=system`；失败后才生成 `stack=gvisor` 候选；
- 首选 MTU 9000，设备探测失败时退到 1500；
- IPv4/IPv6 address 与 route 同时配置；
- 可用时启用 `auto_redirect` 作为优化，但不依赖其承担完整 Android IPv6/UDP 语义；
- 应用范围使用统一解析后的 UID 集合生成 `include_uid` 或 `exclude_uid`。

首版显式不设置已废弃的 `gso`。官方文档说明该选项在透明代理场景无收益且已不再工作。UDP timeout、单连接 buffer 等只允许使用 sing-box 已发布配置项并经 fixture 验证；NetHop 不修改全局 BBR、拥塞控制或 socket sysctl。

TPROXY 与 TUN 的应用、局域网、IPv6 和 DNS策略必须通过共用 golden tests 证明语义一致。

首版不承诺与另一个 Android `VpnService` 或 Root TUN 模块共存。探测到冲突路由/TUN/VPN 时默认保持直连并给出诊断，不强制覆盖。

## 13. 应用目录与黑白名单

### 13.1 统一应用身份

```text
AppIdentity = (android_user_id, uid, package_name)
UidGroup    = (android_user_id, uid, sorted_package_names)
```

包名是持久配置身份，UID 是每次应用策略编译时解析的执行身份。不能把旧 UID 缓存永久当作包身份；应用卸载重装、工作资料变化或共享 UID 集合变化都会生成新的 catalog generation。

### 13.2 系统应用与用户应用

应用列表分为：

- **系统应用**：`ApplicationInfo.FLAG_SYSTEM` 或 `FLAG_UPDATED_SYSTEM_APP`；更新过的预装应用仍归系统应用。
- **用户应用**：其他第三方安装应用。

首版通过 PackageManager shell 接口按 Android user 获取系统/第三方包和 UID，并交叉校验完整包集合。分类只用于展示和筛选，不作为安全信任判断。

共享 UID 无法在 iptables 或 TUN route 层按包拆分。任一共享 UID 包被选择时，整个 `UidGroup` 同时生效，并在 CLI/App 返回 `shared_uid_expanded` 诊断及受影响包列表。

### 13.3 黑白名单语义

| 模式 | 被选应用 | 未选应用 | 空列表 |
|---|---|---|---|
| blacklist | 绕过 NetHop | 进入 NetHop | 所有应用进入 NetHop |
| whitelist | 进入 NetHop | 绕过 NetHop | 没有应用进入 NetHop，并告警 |

应用“绕过”同时绕过透明代理与 NetHop DNS 接管。进入 NetHop 后，应用仍按域名/IP规则决定 direct、proxy 或 block；应用列表不是中国/国外分流规则的替代品。

catalog 在启动、应用策略变更、订阅/配置应用前刷新，并以低频周期校验。未来 Kotlin App 接收到 package/profile 变化广播后主动通知 worker 刷新。

## 14. 路由与 DNS

### 14.1 路由优先级

托管配置的逻辑顺序固定为：

1. 控制面、核心服务器和 loopback 安全规则。
2. sniff 动作与 DNS hijack。
3. 用户显式域名 override，内部固定为 `block -> force proxy -> bypass`。
4. 广告与恶意域名/IP block。
5. 局域网、私有地址和必要系统网络 direct。
6. 中国大陆域名/IP direct。
7. 其余流量进入顶层 `selector`。

规则冲突采用“更明确的用户规则优先，安全不变量不可覆盖”。完整生成配置中每类规则带稳定 tag 和来源，`diagnose/explain` 能说明命中路径。

v1 托管配置提供 `force_proxy_domains`、`bypass_domains` 和 `block_domains` 三组有界域名后缀规则，每组最多 512 项。daemon 将输入规范化为小写 ASCII 域名，拒绝 URL、通配符、端口、路径、IP literal，以及跨动作列表的相同或父子后缀重叠。强制代理与直连域名分别绑定代理 DNS 和直连 DNS；阻断域名当前只在 route 层拒绝连接，不在没有 sing-box 1.13.15 验证证据时承诺 DNS reject。

### 14.2 DNS 设计

首版使用真实 IP DNS：

- 国内上游：DoH，经 `direct`，提供固定 IPv4/IPv6 bootstrap。
- 国外上游：DoH，经当前 proxy selector，提供固定 bootstrap。
- 国内域名规则选择国内 DNS，其余选择国外 DNS。
- NetHop 模块包携带受审核的 `cn-domain.srs` 与 `cn-ip.srs` 基线；安装器在校验
  打包 SHA-256 后，通过同目录临时文件与原子 rename 发布到 daemon-owned
  `/data/adb/nethop/rulesets/`。订阅内容不能替换、追加或远程 include 规则集。
- 使用 sing-box 1.13.15 内建、遵循 TTL 的 DNS cache，显式保持 `disable_cache=false`；`cache_capacity` 从 4096 起在 Phase 0-B 以命中率和 RSS 校准，不再叠加一套 Rust DNS cache。
- DoH transport 复用 sing-box 的持久 HTTP 连接；基准分别记录复用成功与重新建连，不能每次查询新建客户端。
- TCP/UDP 53 仅对进入 NetHop 的应用执行 hijack。
- 广告域名在 DNS 层返回明确拒绝/NXDOMAIN，并在 route 层保留 IP/连接阻断。
- 启用 sniff 获取 HTTP Host、TLS SNI、QUIC 等可识别元数据，但默认不覆盖原始目标地址。
- 不启用 FakeIP，不修改系统 hosts。

默认国内/国外 DNS 端点是可配置的发行默认值，不写死在 parser 中。

DNS 状态至少暴露 `cache_hit/cache_miss/error` 累计数和命中/未命中延迟直方图的有界聚合，不记录查询域名。1.14 的 optimistic cache 不属于 1.13.15 契约，首版配置不得引用。

### 14.3 Android Private DNS

Android Automatic（`opportunistic`）或指定提供商（`hostname`）Private DNS/DoT 可能绕过 NetHop 的 53 端口分流。worker 通过只读的 `/system/bin/settings get global private_dns_mode` 检测状态，不读取或展示 provider hostname：

- 默认不修改系统设置；
- 只有 `off` 才声明 `dns_split=healthy`；`opportunistic` 与 `hostname` 均报告 `degraded_private_dns`；
- 查询工具缺失、权限拒绝或 OEM 返回未知值时报告 `unknown`，但不阻断 daemon 或当前代理；
- 启动前要求配置选择“用户已关闭 Private DNS”或“允许 degraded”；
- degraded 模式下 DoT 流量仍按普通透明流量代理，但不保证国内/国外 DNS 分流语义。

`status --json` 给出结构化 `dns_split.mode` 和 `dns_split.dns_split`；`nethopctl status --human` 对 degraded 状态给出关闭 Private DNS 的固定建议，不能只在日志中写 warning，也不能回显外部 hostname。

自动关闭并恢复系统 Private DNS 不属于首版，避免修改用户全局系统状态。

## 15. 规则集管理

模块包内带一份足以离线启动的审核基线二进制 SRS。托管模式不在运行时加载大型文本域名/IP列表，不把正则作为默认大规则；局域网/保留地址使用内置小规则，用户 override 保持高优先级且有界。在线更新使用 provider manifest 描述：

```text
id, purpose, source_url, license, format, min_sing_box,
max_bytes, expected_content_type, refresh_interval, current_digest
```

更新流程为成对下载、大小/Content-Type/SRS magic 检查、引用候选 SRS 的临时配置 `sing-box check`、持久事务 journal、同路径逐文件原子替换、受控重启当前 generation 和数据面健康检查。只有新核心健康后才删除旧 pair 与 journal；启动失败则恢复旧 pair 并重新启动旧 generation，进程在 commit 前退出时由下次 `RuleSetStore::open` 根据 journal 回滚。sing-box 1.13.15 虽支持本地 rule-set 文件 watcher，但 reload 没有 daemon 可消费的成功 ACK，因此不能把 watcher callback 当作事务提交证据。默认 24 小时更新并加入稳定 jitter，避免每台设备在同一时刻访问上游。

最终采用的中国域名、中国 IP 和广告数据源必须在首次纳入公开发布包前，通过 ADR 冻结 URL、许可证、再分发权限、生成工具版本、更新责任和误杀退出路径。发布 SRS 的 manifest 记录源 digest 与产物 digest。未明确许可证的列表不得进入发布包。

实现状态：当前 `cn-domain.srs` 与 `cn-ip.srs` 是 digest 固定的发布快照，安装器拒绝 symlink/非普通持久目标，并把校验后的包内资产原子发布到 `/data/adb/nethop/rulesets/`；composer 只引用该 daemon-owned 持久路径，不依赖模块升级目录的生命周期。`RuleSetStore` 提供 5 MiB/文件预算、私有 staging、SRS magic 预检、真实 `CandidateChecker` admission、两阶段 `prepare/publish/commit/rollback`、持久 journal 与启动恢复。专用 fetch service 复用 SSRF/redirect/超时/gzip/body 上限控制，强制 Content-Type/SRS/成对成功语义，并在私有目录持久化摘要绑定的 body、ETag 和 Last-Modified；损坏缓存按 miss 处理。worker 已消费独立 `resource:rulesets` 调度，运行态更新执行停止旧核心、发布候选、重启并检查健康、成功提交或失败回滚；`nethopctl ruleset status|update` 提供只读状态和手动触发。首次公开发布仍受 `07-ruleset-provider-supply-chain.md` 的可复现供应链与真机端到端闸门约束。

## 16. Rust 订阅转换库

### 16.1 处理管线

```text
bounded bytes
  -> content classification
  -> format parser
  -> Vec<NodeResult<ProxyNode, NodeDiagnostic>>
  -> semantic validation
  -> canonical fingerprint + merge
  -> sing-box outbound fragment
  -> ConversionReport
```

解析器不得直接生成完整 sing-box 配置。完整配置只由 composer 将可信模板、outbound 片段、规则和运行时端口组合后生成。

### 16.2 输入限制

| 限制 | 默认值 |
|---|---:|
| HTTP 解压后 body | 5 MiB |
| 可解析节点数 | 10,000 |
| 托管模式 active outbounds | 2,000；发布性能基线为 500 |
| 单行 URI | 16 KiB |
| JSON/YAML 嵌套深度 | 64 |
| 单字符串 | 64 KiB；凭据等字段使用更小协议上限 |
| HTTPS 重定向 | 最多 5 次 |
| connect timeout | 5 秒 |
| total timeout | 30 秒 |

YAML parser 必须限制 alias、节点数量、深度和展开后字节数，防止 alias/decompression bomb。未知字段可以作为 warning 保留在报告中；影响连接语义的未知枚举或缺失必填字段必须拒绝该节点。

10,000 是转换与诊断能力，不等于允许把 10,000 个节点全部交给 sing-box。合并后超过 2,000 active outbounds 时，托管模式以 `NH-CONFIG-ACTIVE-LIMIT` 拒绝发布并提示用户按 source/协议/地区过滤，不静默截断。首版 Expert 模式默认关闭；后续即使允许提高上限，也必须标记 `performance_degraded`，且不属于 80 MiB/3% 的受支持范围。

### 16.3 下载策略

- 默认只允许 HTTPS；HTTP 需用户对该 source 显式设置 `allow_insecure_http=true`。
- TLS 证书验证不可关闭；订阅里的 `skip-cert-verify` 只属于代理节点 TLS，不影响下载器。
- 每次重定向重新验证 scheme、目标地址和大小策略。
- loopback、link-local、私网目标默认拒绝，用户显式标记 LAN source 后才允许，降低 root downloader SSRF 风险。
- 核心运行时优先经 loopback proxy inbound 下载；允许配置直连回退并记录是否发生。
- URL、Authorization、Cookie 和 query token 在日志中脱敏。

当前手动更新入口为 `nethopctl update`，通过 root-only UDS 发送 `subscription.update`。worker 按以下顺序执行：

1. 下载并解析全部 source，完成统一去重和托管配置生成；
2. 写入 sealed candidate 并调用固定版本 `sing-box check`，不修改 `current`；
3. 完整撤销旧网络接管并停止旧 core；
4. 直接启动指定 sealed candidate，等待 core、网络规则和数据面健康；
5. 健康后才原子提交 `current`；激活失败则删除候选并从未变化的旧 `current` 恢复。

source 配置缺失时 worker 正常保持 fail-open/缓存 generation 能力，`subscription.update` 返回 `NH-SUB-UPDATE-UNAVAILABLE`。source 配置存在但不是 root-owned `0600` 普通文件，或 schema/URL 不合法时，worker fail-closed 拒绝初始化更新组件。同步 fetch 是当前明确的低频实现；24 小时调度接线完成前只提供手动触发，不把定时更新声明为已实现。

### 16.4 支持格式

| 格式 | 行为 |
|---|---|
| Base64 URI | 支持标准/URL-safe、带/不带 padding；解码后按行解析分享 URI |
| URI 列表 | 逐行处理；空行和注释忽略；错误绑定原始行号 |
| Clash YAML | 只读取 `proxies`；不信任 proxy-groups、rules、script、provider 或外部路径 |
| sing-box JSON | 只提取白名单协议的远端 proxy outbounds；不透传 route、DNS、inbound、service 或本地代理链 |

格式探测使用结构证据和受控回退，不能把任意 JSON/YAML 解析失败后静默当作 Base64。

### 16.5 统一节点模型

`ProxyNode` 至少包含：

```text
source_id, source_item_index, display_name
protocol, server, port, credentials
tls { enabled, server_name, insecure, alpn, utls, reality }
transport { tcp, ws, http, httpupgrade, grpc, quic }
protocol_options
canonical_fingerprint, node_id
warnings
```

凭据采用协议专用 enum，避免把 UUID、密码、密钥混成无约束 map。server 使用规范化域名或 IP，端口必须为 `1..65535`。输出结构通过 serde 类型生成，不用字符串模板拼 JSON。

实现以有界分配和减少完整副本为目标，不把“零拷贝”作为脱离 parser 实际 API 的口号：Base64 按块解码，URI 尽量借用输入 slice，JSON/YAML 不同时长期保留原始 body、完整 AST、IR 和输出四份数据。敏感临时 buffer 在所有权明确且不会产生额外副本时清零；主要安全保证仍是最短生命周期、禁止日志和严格文件权限。

### 16.6 协议范围

| 协议 | 首版重点字段 |
|---|---|
| VLESS | UUID、flow、TLS/uTLS/Reality、WS/HTTP/HTTPUpgrade/gRPC |
| VMess | UUID、security、alterId、TLS、受支持 transport |
| Shadowsocks | method、password、受审核 SIP003 plugin 组合 |
| Trojan | password、TLS、受支持 transport |
| Hysteria2 | password、TLS、obfs、上下行提示 |
| TUIC | UUID/password、TLS、congestion control、UDP relay 参数 |
| AnyTLS | password、TLS、idle session 参数 |

不支持的插件、transport 或未来字段以稳定诊断码拒绝该节点，不擅自降级成看似可连接的默认值。

AnyTLS 官方标明自 sing-box 1.12.0 起支持，固定的 `v1.13.15` 源码也包含正式实现，因此属于首版稳定协议。每种协议仍必须通过 `v1.13.15 sing-box check` 和最小连通 fixture；未来协议不能因为开发线源码存在就进入白名单。

### 16.7 去重与稳定身份

canonical fingerprint 覆盖协议、规范化服务器、端口、凭据、TLS 身份、transport 和影响连接的协议参数，不包含显示名称与来源。fingerprint 算法和 canonical schema 必须共同版本化，具体选型由 `02` 的 Phase 0-B 门禁冻结；SHA-256 是默认基线，BLAKE3 只有在真实 10,000 节点端到端 profile 证明净收益后才可替代。报告和日志只显示带算法/schema 标识的截断 `node_id`，不能反推出凭据。source、fixture、发布资产和供应链 manifest 的摘要仍使用 SHA-256，不随节点 fingerprint 算法变化。

同一节点出现在多个订阅时只生成一个 outbound，同时保留全部 `source_id` 和别名。显示 tag 采用清洗后的名称加短 ID，确保 sing-box tag 全局唯一且订阅重排后稳定。

### 16.8 部分成功与报告

`ConversionReport` 包含：

```text
schema_version, source_id, detected_format
input_bytes, elapsed_ms
accepted, rejected, duplicate
nodes[] { source_index, node_id?, protocol?, severity, code, message }
```

任何诊断不得包含完整密码、UUID、订阅 URL token 或 TLS 私钥。单个 source 全部失败时该 source 更新失败并沿用旧缓存；其他 source 仍可成功更新。

## 17. 多订阅事务

每个 source 有独立身份、缓存和状态：

```text
SourceState = Active(generation) | Stale(last_good, error) | Empty | Disabled
```

一次全局更新流程：

1. 获取更新锁；并发手动/定时更新合并为一个任务。
2. 分别获取各 source 候选；不先覆盖缓存。
3. 解析并生成逐源报告。
4. 成功 source 使用新候选；失败 source 使用 last-known-good。
5. 按 source 配置顺序合并并用 fingerprint 去重。
6. 零节点时拒绝发布。
7. composer 生成候选 sing-box config。
8. 执行结构校验、安全审计和 `sing-box check`。
9. 发布新 generation，重载并做健康检查。
10. 成功后提交 source cache；失败则恢复旧 generation。

删除 source 是显式事务：移除其来源关系后重新合并；只有不再被其他 source 引用的节点才消失。

## 18. sing-box 配置与构建

### 18.1 可复现下游构建

官方默认构建不包含 `with_v2ray_api`。NetHop 从精确上游 tag 进行可复现下游构建，统计方案 A 的候选标签为：

```text
with_quic,with_utls,with_clash_api,with_v2ray_api,with_gvisor,
badlinkname,tfogo_checklinkname0
```

该列表是统计方案 A 的原型基线；仅在启用历史统计时需要验证。若 20.3 节 ADR 选择方案 B，则移除 `with_v2ray_api`，并用 exporter fixture 证明功能等价。不能同时保留无消费者的 gRPC server 和新增 JSON exporter。

`badlinkname` 与 `tfogo_checklinkname0` 以及上游 `release/LDFLAGS` 是 `v1.13.15` 下游构建契约的一部分，不能在“裁剪”时误删。排除首版不需要的 Tailscale、OpenVPN、OpenConnect、WireGuard、ACME、DHCP、Cloudflared、USB/IP、Naive、CCM/OCM 等特性，降低二进制与攻击面。构建标签必须通过协议 fixture、TUN fallback 和内存测试验证，不能仅以“编译成功”为准。

首版包含一个最小、独立且可审核的 NetHop 统计补丁。上游 V2Ray Stats tracker 在路由层接收到的是直接匹配的 outbound；当路由指向顶层 `selector` 时，它默认把字节记到 `selector`，并不知道随后实际委派的终端节点。仅对 `OutboundGroup.Now()` 做一次查询也不充分：`selector -> urltest -> node` 需要递归解析，选择还可能在统计查询与实际委派之间变化。补丁因此必须在连接真正委派时绑定终端 outbound，而不是在路由匹配时猜测：

- 为每个 routed connection/packet connection/flow 携带一次性的终端 outbound attribution hook；
- `selector`、`urltest` 等组在实际选择并委派 child 时继续传递 hook，终端 outbound 在开始传输前提交自身 tag；
- 最多解析 8 层并检测 tag/对象循环；无法得到终端节点时退回最外层 group tag，同时增加 `stats_attribution_degraded_total` 和结构化诊断；
- TCP、UDP 与 TUN flow 三条 tracker 路径共享同一归因语义；切换只影响随后新建的连接，既有连接保持建立时绑定的节点；
- 补丁不得改变选择、失败重试、连接中断或路由行为，并以独立 patch 文件和上游 fixture 维护。

发布 manifest 记录上游 tag、源码 commit、Go 版本、build tags、逐字采用的上游 ldflags、资产 SHA-256 和对应源码地址。

### 18.2 托管配置结构

生成配置至少包含：

- TPROXY 或 TUN inbound；
- 仅 loopback 的 mixed inbound，供 `nethopd` 显式代理下载；
- selector、urltest、节点 outbounds、direct 和 block；
- 真实 IP DNS 与 sniff/hijack 规则；
- 本地/远程 SRS；
- Clash API，随机 secret，loopback only；
- 选定的 stats API，使用 `0600` UDS 或带 secret 的 loopback HTTP，只计需要的 inbound/outbound；
- cache file，用于 selector 选择持久化。

### 18.3 selector 与测速

顶层 `nethop-select` selector 包含固定的 `nethop-auto` urltest 组和全部可手选 terminal 节点。默认：

- `interrupt_exist_connections=false`，节点切换只影响新连接，避免主动打断现有会话；
- urltest 使用可配置的 204 URL、合理 interval 和 tolerance；
- `auto` 候选集与全部可手选节点分离：初始默认最多 64 个，按 source 顺序和稳定 `node_id` 选取，未进入 auto 的节点仍可手选且状态明确；Phase 0-B 在 16..256 范围内校准默认值；
- 手动批量测速限制并发，默认 8；
- 订阅刷新后优先按稳定 `node_id` 恢复原选择；原节点不存在才回退 `auto`。

节点选择意图与实际 active terminal 分开保存和返回。`auto` 下 active terminal 可随 urltest 改变；`manual` 下测速只更新延迟，不改变用户目标。daemon 通过当前 generation registry 将稳定 node ID 映射为内部 tag，并经独立 sing-box 子进程的 loopback-only、随机 secret Clash API 完成测速和切换；WebUI/CLI 不接触内部 tag 或 API secret。节点切换不重写配置、不发送 SIGHUP，因此适用小于 1 秒 SLA。

sing-box `v1.14` 的原生 gRPC API service 不属于当前 `v1.13.15` 发布契约，也不以预留依赖的方式进入实现。评估条件与迁移边界只记录在 [`11-deferred-capabilities-and-future-design.md`](./11-deferred-capabilities-and-future-design.md)。

### 18.4 配置开放阶段

1. **Managed**：用户只编辑 NetHop schema，composer 生成全部 sing-box 配置。
2. **Managed + override**：只允许覆盖日志、DNS 端点、测速、协议调优等 allowlist 字段。
3. **Expert**：用户提供完整 JSON，但仍需通过 schema、`sing-box check` 和 NetHop safety auditor；首版发布默认关闭，后续通过显式 feature flag 开启。

即使 Expert 模式也不能：监听非 loopback 控制 API、移除核心防环路、占用保留端口、关闭必要 IPv6 guard、绕过应用 capture policy 或引用模块目录外不安全路径。专家模式是配置自由度，不是关闭模块安全边界。

## 19. 配置发布与回滚

### 19.1 单写和乐观并发

CLI/App 的配置保存请求包含 `expected_config_digest` 和完整 typed document。该 digest 覆盖最近稳定读取的 TOML exact bytes；worker 在准备和提交阶段检查 CAS，拒绝覆盖并发修改。source/generation 内容身份另用 `source_config_digest`，两者不能混用。客户端不能直接写 `current` generation。

### 19.2 generation 发布

```text
source/config input
  -> parse + semantic validation
  -> platform/capability admission
  -> compose candidate directory
  -> safety audit
  -> sing-box check
  -> fsync candidate files
  -> choose TPROXY overlap OR controlled stop/start plan
  -> transactional candidate activation
  -> API + data-plane health check
  -> commit manifest or rollback
```

候选失败不会改变活动 generation。重载失败时将 `current` 恢复至旧 generation 并重新启动旧配置。TPROXY 规则只在新核心健康后切换入口；旧规则和旧核心不会在候选验证阶段被提前删除。

TPROXY 从 Phase 0-B 起可验证“有界双实例切换”原型：候选实例使用独立 inbound/API 端口和 `core_instance_id` 启动，通过 API 与真实探针后，以单条 A/B 入口 replace 接收新连接；旧实例只保留已建立 TCP/UDP session，在 drain 超时后退出。候选统计不得提交到活动 SQLite generation，回滚必须能把入口切回仍健康的旧实例。该原型不属于 Phase 0-A，也不阻断使用受控 stop/start 的参考设备 Alpha。

双实例不是无条件的首版承诺。只有同时满足更新峰值 RSS <= 110 MiB、入口切换新连接不可用窗口 <= 1 秒、旧连接归因不改名、UDP drain 有界、端口/mark 不冲突时才启用；否则 TPROXY 与 TUN 都使用受控 stop/start，完整配置 SLA 为 3 秒。TUN 无法可靠并行拥有同一路由与设备，始终不做双实例。selector/Clash mode 切换仍走运行时 API，不受此决策影响。

变更按最小必要激活面分类：selector/Clash mode 走 API；规则数据走本地 SRS 原子替换；应用 UID、DNS 入口和 guard 走 netfilter A/B；只有 inbound/outbound 拓扑、监听、DNS server 结构等变化才进入完整 core activation。前三类是 <= 1 秒规则热路径；完整 activation 的控制事务 <= 3 秒，TPROXY overlap 通过时其新连接入口仍 <= 1 秒，TUN stop/start 单独报告实际不可用窗口。

### 19.3 safety auditor

除 `sing-box check` 外还检查：

- 至少一个合法节点或明确 direct-only 模式；
- outbound tag 唯一、引用闭合、无循环 detour；
- Clash API 仅 loopback 且 secret 非空；stats UDS owner/mode 合法，或 stats HTTP 复用认证边界；
- TPROXY/TUN、DNS、mark、端口与 capability report 一致；
- 订阅节点不能新增 inbound/service/rule-set 文件路径；
- IPv6 失败时 guard generation 已准备；
- 配置与当前 sing-box 版本和 build tags 匹配。

## 20. 实时与历史流量

### 20.1 实时

- Clash `/traffic` 提供 1 秒上下行速率流。
- Clash `/connections` 提供当前连接、目标、chain、节点和每连接字节。
- 没有 CLI/App 订阅实时流时，worker 不保持高频 `/traffic` 消费者。

Clash API secret 不直接交给普通 App。`nethopctl traffic live --json` 通过 UDS 转发有界、脱敏事件。

### 20.2 精确历史

NetHop 自定义 sing-box 构建应用 18.1 节的终端 outbound 归因补丁，并通过 20.3 节选定的 API 暴露 counter。worker 每 60 秒读取不重置的累计 counter，与数据库中同一 `core_instance_id + counter_name` 上次值求差：

- 总量按 TPROXY/TUN inbound 计数；
- 节点按连接实际绑定的终端 outbound tag 计数；`selector`/`urltest` 组自身的 counter 不冒充节点；
- direct/block 不计入“代理节点流量”，但单独保留总分类。

核心重启产生新 `core_instance_id`，counter 从零开始，不能把负差当流量。读取成功后在一个 SQLite 事务内提交 bucket 和 last counter，避免 `reset=true` 在数据库写失败时丢失数据。

统计补丁的验收 fixture 至少包含 `selector -> node`、`selector -> urltest -> node`、运行时切换、TCP、UDP、TUN flow、组循环/深度保护和核心重启。测试必须同时断言总量守恒、旧连接不随切换改名、degraded counter 可观测。`/connections` 仅用于实时视图；轮询活跃连接会漏掉采样间隔内结束的短连接，不作为历史按节点统计的数据源。

同一 `v1.13.15`/tags/config 下，以关闭统计 patch 的内部基线对比：TPROXY streaming 吞吐或 TCP_RR request rate 下降不得超过 3%（挑战 <= 1%），500 active outbounds 的额外 RSS 不得超过 5 MiB，空闲时不得新增高频 wakeup。该闸门不取代模块整体的 85%/80 MiB/3% 门槛。

这里的“总量守恒”只指同一统计层、已知 payload fixture 内不重复计数或漏计数，不能断言 inbound application bytes 严格等于 proxy wire bytes：代理封装、DNS/sniff、失败重试和拒绝流量会使两层字节天然不同。

### 20.3 统计传输的可选功能决策

官方 V2Ray API 是 TCP gRPC，`listen` 可配置但没有鉴权字段；旧设计只把它配置到 loopback。从 Phase 0-B 起可并行构建两个原型并以 ADR 冻结一个；在历史统计尚未进入 Alpha 时，这项比较不阻断基础数据面和订阅闭环：

| 方案 | 内容 | 优点 | 代价 |
|---|---|---|---|
| A：gRPC over UDS | 给上游 V2Ray server 增加最小 `unix://` listener，socket mode `0600`；`tonic/prost` 关闭 TLS、reflection 等无关特性，只生成 QueryStats/GetSysStats 所需类型 | 保持上游 service/schema，不依赖共享 loopback 的弱边界 | Rust 二进制和依赖树可能增大，需维护小型 listener patch |
| B：Clash API 下游 JSON exporter | 将独立 counter service 暴露为认证的 `/nethop/stats`，并在可行时移除 gRPC server/client | 客户端简单，可能同时缩小两侧依赖 | 扩大 sing-box fork、HTTP 路由和安全回归面 |

官方 V2Ray API 没有 bearer secret，Android 普通应用又可访问共享 loopback，因此方案 A 禁止直接暴露未认证 TCP listener。UDS patch 必须处理 stale socket、owner/mode、symlink 和关闭清理；如果 UDS 原型失败，A 不得以随机端口替代鉴权。

决策数据必须包含两个发布二进制体积、500 active outbounds 的稳态 RSS、60 秒采集的 CPU/wakeup、端到端采集失败语义、权限绕过测试、补丁行数和上游升级冲突数。只有 B 能实测降低总资源且不扩大安全边界时才选择 B；否则使用 A。无论选择哪种传输，counter 名称、非 reset 累计读取、`core_instance_id` 和 SQLite 事务语义不变。

### 20.4 SQLite

SQLite 使用 Rust 静态 bundled 版本，不依赖 Android 私有 SQLite ABI。文件为 `0600`，WAL 模式：

- 60 秒桶保留 7 天；
- 小时桶保留 90 天；
- 日桶保留 1 年；
- 聚合和删除在低频维护事务中完成；
- WAL 达到阈值或每日维护时 checkpoint；
- wall clock 与 monotonic interval 分开记录，处理用户改时间和时区变化。

首版不保存按应用历史。连接视图中的包/进程只用于实时显示，避免高频进程归因带来的 CPU、隐私和准确性问题。

## 21. 本地控制协议

`nethopd.sock` mode 为 `0600`。worker 使用 `SO_PEERCRED` 验证调用者 UID，只接受 root；未来 App 通过受用户授权的 root shell 启动 `nethopctl`。

协议 v1 使用 `u32 big-endian length + UTF-8 JSON`，单帧最大 1 MiB。请求至少包含：

```json
{
  "version": 1,
  "request_id": "...",
  "method": "status.get",
  "params": {}
}
```

响应统一包含 `ok`、稳定错误码、message、当前 generation 和 method-specific result。流式请求由多个带 sequence 的 frame 组成并有显式 end/error frame。

稳定错误域：

```text
NH-CONFIG-*       配置语法、并发和安全审计
NH-SUB-*          下载、格式、节点诊断和零节点
NH-CAP-*          平台与内核能力
NH-NET-*          netfilter、路由、DNS、IPv6
NH-CORE-*         sing-box 校验、启动、API、熔断
NH-STATS-*        counter 与 SQLite
NH-AUTH-*         IPC 调用者与权限
```

协议类型位于独立 crate，CLI 和未来 Kotlin App 不各自解释 daemon 内部文件。

## 22. 更新与通知

### 22.1 调度

订阅、规则集和 sing-box 版本检查均支持手动触发；默认周期 24 小时。worker 使用持久 `next_run`、失败退避和稳定 jitter，不依赖 Android `crond`。

实现状态：订阅手动触发、持久 schedule、失败退避、稳定 jitter 和到期事件接入订阅 update worker 已完成。sing-box 版本检查同时支持 `nethopctl core version-check` 手动触发和独立的 24 小时持久调度；固定 key `resource:sing-box-version` 与 daemon-owned source ID 分离。规则集使用独立固定 key `resource:rulesets`，同时支持 `nethopctl ruleset update` 手动触发；自动和手动入口共享同一两阶段发布、核心健康确认、journal 恢复与调度结果记录路径。

### 22.2 sing-box 更新

版本检查只查询上游正式 release，忽略 prerelease。发现高于当前 pin 的稳定版时：

- 写入 `runtime.json` 更新状态；
- `action.sh` 和 `nethopctl status` 显示提示；
- 尝试通过 Android `cmd notification post` 发送 best-effort 通知；失败不影响服务；
- 不下载、不覆盖、不执行新核心。

升级由 NetHop 项目重新构建、验证 build tags、跑回归并发布新模块完成。模块自身更新继续使用 Magisk/KernelSU `updateJson`。

当前实现固定查询 `https://api.github.com/repos/SagerNet/sing-box/releases/latest`，复用订阅 fetch 的 HTTPS-only、SSRF、连接时 peer 校验、手动重定向、超时和无环境代理边界，并把响应限制为 256 KiB。`core.version_check` 只接受空参数；结果写入有 64 KiB 上限的 `state/runtime.json`，原子更新时保留其他运行字段。Android 通知使用固定 `cmd notification post` argv，不拼接任何订阅、SSID、节点名或外部文本；同一 latest 版本的已通知状态跨 worker 重启恢复。通知或状态写入失败只产生降级事件，不停止代理。

`nethopctl status` 已返回 `core_update`，自动 24 小时版本检查已接入 worker；`action.sh` 通过 `nethopctl status --human` 显示固定、脱敏的更新提示。

## 23. 安全与隐私

### 23.1 威胁模型

需要防御：恶意或损坏订阅、YAML/JSON 资源消耗、SSRF、配置注入、API 暴露、规则冲突、核心环路、崩溃遗留、日志泄密、过期缓存错误替换。

不声称防御：已获得 root 的其他恶意模块、被篡改内核、能够直接读 `/data/adb/nethop` 的 root 进程。

### 23.2 关键控制

- 所有用户输入先做大小和结构限制，再分配大型对象。
- 系统命令使用固定可执行文件和 argv，不通过 `sh -c` 拼接订阅/配置内容。
- 活动网络变更有全局 mutation lock 和 generation owner。
- Clash API 只监听 loopback 并使用随机 256-bit secret；stats 使用 `0600` UDS 或复用同一认证 HTTP 边界，禁止未认证 V2Ray TCP listener。
- 诊断默认脱敏 URL、Authorization、UUID、password、private key、cookie 和 query。
- support bundle 使用 allowlist 收集，默认不包含生成配置、原始订阅和数据库。
- 日志按大小轮转，生产默认 `warn`，debug 有超时自动恢复。
- 配置和缓存读写拒绝 symlink、异常 owner/mode 和模块根外路径。

### 23.3 冲突诊断

`nethopctl diagnose` 以稳定 schema 输出：

- Root 平台、SELinux 状态及可读取时的相关 AVC 摘要；不可读不能伪报为“无拒绝”；
- netfilter backend、每项原语探测、现有 NetHop/未知链入口；
- TUN/VpnService/Root 网络模块迹象、默认路由和重复 policy rule；
- mark、mask、table、priority、端口和 socket owner 冲突；
- Private DNS、IPv4/IPv6、guard 和最近一次网络 reconcile 结果；
- supervisor/worker/core 身份、API health、stats transport/patch health；
- 最近 rollback/circuit 原因和 generation 对应关系。

诊断只读取或验证 NetHop 自有临时对象，不自动删除其他模块的链、关闭 VPN、修改 Private DNS 或放宽 SELinux。

## 24. 性能设计与验收

### 24.1 基准环境

吞吐测试使用同一局域网中的受控服务端和同一客户端：

1. 设备充电状态、屏幕、温度和 Wi-Fi/有线条件固定。
2. 先测直连 TCP 四并发流 5 分钟，记录稳定区间。
3. 使用同一路径代理服务端测 TPROXY，再测 TUN。
4. 每种模式至少 5 次，报告中位数、P10/P90、CPU、RSS、温度和重传。
5. UDP、DNS、TCP_RR 和 QUIC 单独测试，不混入 TCP streaming 百分比。
6. 报告写明设备型号、SoC、Android build、Root 管理器、内核、netfilter backend、网络接口、sing-box/NetHop commit 和 build manifest，不用“高端/中端”替代可复现 fixture。

数据面 smoke 还必须覆盖普通 shell/app UID 的 IPv4 TCP HTTPS 204（至少一次新建连接和一次复用连接）、UDP DNS/QUIC 可达性、IPv6 captured 或 guard 语义，以及核心出站不回环。抓包或计数器应证明代理回包没有再次命中自有捕获链；只检查 `running_tproxy`、监听端口和规则计数不足以通过验收。

### 24.2 发布闸门

| 指标 | 首版门槛 |
|---|---:|
| TPROXY TCP 四流吞吐 | 同设备同路径直连的 >= 85%；挑战目标 >= 90% |
| TUN system TCP 四流吞吐 | 首版目标 >= 75%，与 TPROXY 分开报告 |
| TUN gvisor | 兼容性回退，只要求功能与泄漏测试通过，不用低吞吐冒充 system 结果 |
| 空闲 CPU | 所有模块常驻进程合计 <= 3%；挑战目标 <= 2% |
| 稳态 RSS | 500 active outbounds 下合计 <= 80 MiB；挑战目标 <= 70 MiB |
| 更新峰值 RSS | 5 MiB/10,000 节点转换与发布 <= 110 MiB；挑战目标 <= 100 MiB |
| 订阅转换 | `detect..serialize` <= 300 ms，URI/JSON/YAML 分项均报告 |
| selector/Clash mode、SRS、netfilter A/B | 热路径网络不可用窗口 <= 1 秒 |
| 完整配置激活/回滚 | <= 3 秒；启用双实例时新连接入口切换 <= 1 秒 |

已确认门槛首先适用于当前 `reference_verified` 设备的精确组合，不因设备被主观归为“中端”而放宽，也不外推为跨设备承诺。社区设备按同一口径独立报告；某组合无法达标或尚未验证时，在支持矩阵中标记 best-effort/experimental/unsupported，而不是修改测量口径。

### 24.3 CPU、内存与体积预算

- 空闲 CPU 在代理已启动、无前台 CLI/App、网络空闲的 10 分钟窗口统计。
- RSS 使用 `/proc/<pid>/status` 的 VmRSS，分别记录 sing-box、worker、supervisor 和总和。
- 配置加载 500 active outbounds，规则基线、DNS cache 和 stats 完整启用。
- 更新峰值使用 5 MiB/10,000 节点 fixture。
- 同时报告 PSS 作为诊断，但硬门槛仍按已确认的 RSS。

用于设计分配的稳态子预算为 sing-box <= 55 MiB、worker + supervisor <= 20 MiB、SQLite page cache/其他余量 <= 5 MiB。子预算不是把共享页重复相加后的独立 release gate，总 VmRSS <= 80 MiB 才是最终判定。双实例、parser 和候选配置必须共同留在 110 MiB 更新峰值内。

发布体积目标（不含单独分发的 Corresponding Source 归档）为：

| 产物 | 目标上限 |
|---|---:|
| Magisk/KernelSU 模块 ZIP | 60 MiB |
| sing-box stripped binary | 30 MiB |
| nethopd stripped binary | 12 MiB |
| nethopctl stripped binary | 4 MiB |
| 默认 SRS、许可和 manifest | 10 MiB |

体积目标在 Phase 0-B 用真实 arm64 产物校准；超出必须提交依赖/符号分析和 ADR，不能直接提高上限。Go 使用固定版本、`-trimpath`、上游 `release/LDFLAGS` 和最小 tags。Rust release 基线使用 `opt-level=3`、Thin LTO、`codegen-units=1`、`strip="symbols"` 和 `incremental=false`，并用 `cargo bloat/tree` 检查重复依赖；不为缩小体积改用 `opt-level="s"/"z"` 而牺牲 300 ms parser 门槛。`panic="abort"` 只有在 supervisor/worker 崩溃恢复、诊断和所有 FFI 边界验证后才能启用，测试、fuzz 和 sanitizer 构建保留 unwind 与符号。禁止 UPX。

### 24.4 时延、DNS 与 UDP/QUIC

这些项目首版必须测量和发布原始结果，但在 Phase 0-B 形成至少三轮稳定基线前不凭空设置跨设备硬阈值：

- `nethopctl node select` 到 API 确认的 p50/p95，工程挑战目标 p95 < 200 ms；另测新连接真正走新节点的时间。
- `netperf TCP_RR` 或等价 fixture 的建连/请求响应 p50/p95，对比同路径直连。
- DNS cache hit/miss/error、cache hit p50/p95（挑战目标 p95 <= 10 ms）、国内 direct DoH 和国外 proxy DoH 的非缓存 p50/p95。
- UDP throughput、PPS、丢包、jitter、长 session timeout，以及 64/512/1200 字节包大小。
- QUIC/HTTP3 首次与恢复握手、Hysteria2/TUIC 实际吞吐、高并发 UDP 下 CPU/RSS。

远端节点公网抖动不能作为回归门槛。发布测试使用受控 QUIC/DoH/代理端点；真实公网结果只作兼容补充。Phase 0-B 基线冻结后，连续版本的回归预算写入独立性能 ADR。

### 24.5 转换基准

在目标 arm64 Android 真机 release build 上预热后测量 `detect + decode + parse + validate + dedupe + serialize`。当前冻结的性能 fixture 覆盖七种 URI-capable 协议、三种输入格式、重复节点和 10% 非法节点。它不构成 HTTP/SOCKS 的 10,000 节点性能证据；后两者进入稳定性能声明前必须补充同规模 YAML/JSON fixture。不得使用只包含简单 SS URI 的单一数据证明 300 ms。

Base64 URI、sing-box JSON、Clash YAML 和多 source 合并分别输出 parse/validate/dedupe/serialize 耗时与 peak RSS。300 ms 是各标准 5 MiB fixture 的硬目标，不因 YAML 实现较慢自动放宽；如果合理限制下仍不能达标，应降低允许的 YAML 复杂度并明确诊断，而不是隐藏分项。

### 24.6 资源控制

- parser 使用有界 collection 并尽量流式处理；不保留原始 YAML AST 和完整 IR 的多份副本。
- worker 定时器休眠时不轮询；应用 catalog 和版本检查低频执行。
- 实时 WebSocket 只按消费者需要建立。
- SQLite 每分钟单事务写入，聚合批量执行。
- sing-box 生产日志为 warn，测速并发默认 8。
- urltest 按用户启用的有界组工作，不对 2,000 个 active outbounds 无差别高频探测；后台 idle timeout 后停止 ticker。
- 首版不提供会修改全局 sysctl 的 performance profile。协议/timeout 等调优只能落在受审核的 sing-box allowlist 字段，并通过相同测试矩阵。

## 25. 测试策略

### 25.1 Host

- URI/Base64/YAML/JSON parser unit tests。
- 九协议字段、默认值和拒绝路径 golden tests；HTTP/SOCKS 仅覆盖经审计的 Clash/Mihomo YAML 与 sing-box JSON 映射。
- canonical fingerprint、tag、合并顺序和 last-known-good tests。
- config composer 与 `sing-box check` fixture。
- worker 状态机、熔断、更新事务、SQLite 聚合 tests。
- supervisor/worker 信号、PID/start-time 身份、退避与 worker 崩溃恢复 tests。
- IPC framing、越界、并发 digest 和鉴权 tests。
- property/fuzz：URI、Base64、YAML、JSON、诊断脱敏。

### 25.2 模块仿真

参考 MagicNet/PathGuard 的分层，提供 fake Magisk/KernelSU 环境，mock `ip`、`iptables`、`ip6tables`、`cmd package`、`getprop` 和 sing-box，验证：

- 安装权限与升级保留；
- service/action/uninstall 生命周期；
- 网络计划仅操作自有链；
- 任意阶段失败都回滚；
- TPROXY 到 TUN 的 capability 决策；
- legacy/nft-wrapper 报告、restore 失败、rtnetlink 事件 debounce 与网络漂移 reconcile；
- 配置/缓存/统计 generation 恢复。

### 25.3 AVD

API 33、34、35、36 AVD 用于验证安装、脚本阶段、PackageManager、多用户、UDS、SQLite 和核心控制。x86_64 AVD 需构建同架构测试二进制，不能证明 arm64 发布资产可用。

### 25.4 参考真机与扩展矩阵

当前没有常驻多设备实验室。Phase 0-A、Phase 0-B 和 Alpha 的真机门槛均可由一台实际可用的 arm64 Android 13+ 参考设备承担，并完整记录 2.5 节所列精确组合。Phase 0-A 只执行 27 章规定的安全最小集；Phase 0-B 在该设备能力允许范围内扩展验证 IPv4/IPv6、Wi-Fi/蜂窝切换、休眠唤醒、TPROXY、失败回滚、IPv6 guard、受控 stop/start、主用户和系统/用户应用。

以下是扩大稳定支持范围的覆盖目标，不是开始实现或发布 reference-device Alpha 的硬前置：

- Pixel/GKI、Qualcomm 厂商设备、MediaTek 厂商设备；
- Android 13 基线和至少一个更新 Android 大版本；
- Magisk 与 KernelSU 各至少一个实机组合；
- IPv4-only、双栈 Wi-Fi、工作资料、共享 UID、TUN system/gvisor 和 TPROXY overlap 的不同能力组合。

覆盖可以来自开发者设备、借用设备或社区贡献的可复核报告。未取得证据时必须缩小 release notes/support matrix 中的支持声明；不能让缺少矩阵永久阻塞实现，也不能把参考项目或相似 SoC 的结果当作本机证据。

真机是 SELinux、vendor netfilter、TPROXY、TUN 和性能的最终证据，AVD 不能替代。

### 25.5 故障注入

必须覆盖：下载截断、YAML bomb、磁盘满、SQLite WAL 损坏、端口冲突、iptables 中途失败、rule priority 冲突、核心启动后立即崩溃、SIGHUP 新配置失败、worker/supervisor 分别被 kill、设备时间跳变、订阅源部分失败、IPv6 规则失败、默认路由反复切换、netd 重写入口和 PackageManager 暂不可用。

### 25.6 性能回归

固定一台当前可用的 `reference_verified` arm64 真机作为可重复性能 runner，不要求它必须是 Pixel/GKI。Host/AVD 承担每次 PR 的自动 smoke；真机 smoke 可以按里程碑或发布候选手动执行，覆盖启动、回滚、空闲 CPU/RSS、短吞吐和 500 节点。reference-device release full 覆盖 24 章全部适用指标、10,000 节点转换、已启用 stats 方案、网络切换与热/冷重载。其他 SoC/Root 组合在设备可获得或社区报告到达后增量执行。设备温度超窗、Wi-Fi 速率变化或服务端不稳定时结果标记 invalid，不用重跑挑选最好值。

## 26. 发布与许可证

NetHop 源码、Rust 二进制、模块脚本和未来 App 使用 AGPL-3.0。sing-box 保持 GPLv3 身份，作为独立可执行文件分发。

每个发布包包含：

- NetHop `LICENSE` 和源码地址；
- sing-box GPLv3 license/notice；
- 精确上游 tag/commit、build tags、Go 版本、补丁列表与补丁 SHA-256；
- 与发布二进制逐字节对应的 patched sing-box 源码归档、构建脚本和获取方式，满足 GPLv3 Corresponding Source 要求；
- 固定的 `Cargo.lock`、`go.sum`、工具链版本和离线可复现构建入口；
- CycloneDX 或 SPDX SBOM，覆盖 Rust crates、Go modules、规则生成工具和发布数据；
- 第三方 Rust/Go/规则数据的许可证与再分发清单；
- 发布资产 SHA-256 manifest、构建 provenance 和性能报告。

CI 对 lockfile 漂移、未知许可证、已知高危依赖、未记录 patch 和不可复现产物失败关闭。SBOM 与 source archive 作为独立 release assets，不为满足模块 ZIP 体积目标而省略。

Proxylink 只作为行为参考，不复制代码。`NetGuard/sub-parser` 当前参考目录未见明确独立 license 文件，因此 NetHop 采用独立实现；在许可证来源明确前不直接迁移源码。

## 27. 分阶段实施

### Phase 0-A：核心安全可行性

- 建立 Rust workspace、模块模板和目标 arm64 构建，构建最小 sing-box 1.13.15 数据面。
- 在一台当前可用参考真机验证 TPROXY 最小 IPv4/IPv6 路径、mark 防环路、自有规则精确清理、核心失败撤销接管和 IPv6 无法接管时的 guard。
- 验证 `sing-box check -> candidate -> activate/rollback` 最小事务，失败候选不得改变活动状态。
- 采集一次 direct/TPROXY 吞吐、RSS、CPU 和切换窗口 smoke，识别路线是否明显不可行；数值先作为诊断，不要求完成跨设备或完整发布认证。
- Host/fake module 覆盖危险网络操作的所有权、幂等清理和失败回滚。

Phase 0-A 不要求多 SoC、Magisk/KernelSU 双实机、24 小时 soak、SBOM、完整 stats 补丁、双实例、九协议全量或兼容方言。安全接管、无泄漏、无环路和可恢复性失败时不得继续堆叠功能。

### Phase 0-B：参考设备 Alpha

- 在参考设备完成 TUN 回退、DNS、应用 UID 策略、supervisor/worker 和基础订阅闭环。
- URI/Base64、Clash YAML、sing-box outbounds JSON 形成稳定核心；兼容方言保持关闭。
- 对已启用能力执行 80/110 MiB、3% CPU、85% TPROXY、300 ms 转换等发布目标；未达标的可选能力关闭或标记 experimental，不伪造跨设备结论。
- 形成精确到设备、ROM、内核和 Root 管理器版本的 Alpha support manifest。
- stats transport、终端归因补丁和 TPROXY overlap 可以并行原型，但只阻断对应功能启用，不阻断不包含这些能力的安全 Alpha。

### Phase 1：控制面与订阅

- `nethop-core`、`nethop-protocol`、`nethopd`、`nethopctl` 最小闭环。
- 九协议 Clash YAML、sing-box outbounds parser；URI/Base64 scheme 保持 VLESS、VMess、Shadowsocks、Trojan、Hysteria2、TUIC、AnyTLS 七种，HTTP/SOCKS 不从 URI carrier 猜测。
- 多订阅、active limit、诊断、last-known-good、配置 composer 和事务发布。
- Host golden/fuzz 与 fake module 测试。

### Phase 2：首版数据面

- TPROXY 能力探测、网络计划、IPv4/IPv6、DNS 和应用 UID 规则。
- TUN 回退及统一语义测试。
- supervisor/worker、rtnetlink reconcile、selector/urltest、崩溃清理、熔断、action.sh。
- 系统/用户应用分类、多用户、工作资料和共享 UID。

### Phase 3：统计与发布闸门

- Clash 实时流、已冻结的 stats transport、SQLite 聚合。
- 验证节点统计总量守恒、切换边界、degraded 诊断与核心重启续算。
- 规则/订阅定时更新、sing-box 版本提示。
- 参考真机的完整性能、故障注入和 AVD 验证；扩展设备矩阵按已获得设备或社区报告增量执行。
- AGPL/GPL/第三方许可证、SBOM、provenance 与 reproducibility audit。

### 产品第二阶段（首版发布后）

- 热点/USB/中继接管和 Android tethering eBPF offload 兼容。
- 在真机矩阵证明收益后评估 opt-in native nft sets/maps；不替代 iptables 基线。
- 独立 Kotlin Android App，通过 root bridge 复用 IPC。
- 按应用历史流量；只有在归因准确性和资源预算通过后启用。
- 评估 KernelSU WebUI；不与独立 App 形成第二套业务协议。

eBPF/XDP fast path 不进入已承诺阶段。只有 native nft 仍无法满足经测量的瓶颈，且 GKI/vendor 交付、回滚和 SELinux 方案有独立 ADR 时才重新立项。

## 28. 不变量与完成标准

实现和评审必须持续满足：

1. 非法/空订阅永不覆盖活动配置。
2. 任一异常退出后不遗留导致断网的 NetHop 规则。
3. IPv6 无法接管时不允许公网 IPv6 静默直连。
4. TPROXY/TUN 应用黑白名单由同一模型生成。
5. 订阅内容不能控制 inbound、控制 API、文件路径或模块脚本。
6. worker 是配置、网络和统计状态的唯一发布者。
7. selector 切换不通过核心重启实现。
8. 日志、状态和诊断不泄露订阅或节点凭据。
9. 稳态 80 MiB、更新峰值 110 MiB、空闲 CPU 3% 是发布门槛，不是优化建议。
10. 未通过当前参考真机验证的能力必须标记 experimental/unsupported；缺少多设备矩阵不阻塞 reference-device Alpha，但不能以参考项目可用代替证据或宣称广泛兼容。
11. supervisor 不得成为第二个业务 daemon；任何时刻最多一个 worker 拥有活动状态写权限。
12. 10,000 节点是转换边界，不是 active outbound 性能承诺；托管模式不得静默截断超限节点。
13. NetHop 不为性能修改全局 sysctl，不自动接管未知 native nft/eBPF 对象。

## 29. 参考资料

### 29.1 本地参考

- `../refer/sing-box-magisk-module-方案.md`
- `../refer/sing-box-testing/`
- `../refer/sing-box-testing/experimental/v2rayapi/server.go`
- `../refer/sing-box-testing/experimental/v2rayapi/stats.go`
- `../refer/sing-box-testing/route/network.go`
- `../refer/sing-box-testing/route/route.go`
- `../refer/sing-box-testing/route/rule/rule_set_local.go`
- `../refer/AndroidTProxyShell/tproxy.sh`
- `../refer/box4magisk/`
- `../refer/NetProxy-Magisk/`
- `../refer/MagicNet-main/docs/next-gen-architecture.md`
- `../refer/MagicNet-main/docs/local-simulation.md`
- `../refer/MagicNet-main/src/MagicNet/lib/magicnet/supervisors.sh`
- `../refer/Proxylink-main/`
- `D:/100_Projects/110_Daily/NetGuard/sub-parser/`
- `D:/100_Projects/110_Daily/PathGuard-Next/WORKSPACE.md`
- `D:/100_Projects/110_Daily/PathGuard-Next/docs/00-architecture-design.md`
- `07-ruleset-provider-supply-chain.md`

### 29.2 官方网页

- sing-box TUN：https://sing-box.sagernet.org/configuration/inbound/tun/
- sing-box TProxy：https://sing-box.sagernet.org/configuration/inbound/tproxy/
- sing-box AnyTLS（自 1.12.0）：https://sing-box.sagernet.org/configuration/outbound/anytls/
- sing-box DNS cache：https://sing-box.sagernet.org/configuration/dns/
- sing-box rule-set：https://sing-box.sagernet.org/configuration/rule-set/
- sing-box Clash API：https://sing-box.sagernet.org/configuration/experimental/clash-api/
- sing-box V2Ray API：https://sing-box.sagernet.org/configuration/experimental/v2ray-api/
- sing-box build tags/ldflags：https://sing-box.sagernet.org/installation/build-from-source/
- sing-box `v1.13.15` 默认 build tags：https://raw.githubusercontent.com/SagerNet/sing-box/v1.13.15/release/DEFAULT_BUILD_TAGS
- sing-box `v1.13.15` V2Ray server/stats source：https://github.com/SagerNet/sing-box/tree/v1.13.15/experimental/v2rayapi
- sing-box `v1.13.15` network manager source：https://github.com/SagerNet/sing-box/blob/v1.13.15/route/network.go
- sing-box releases：https://github.com/SagerNet/sing-box/releases
- Magisk module guide：https://topjohnwu.github.io/Magisk/guides.html
- KernelSU module guide：https://kernelsu.org/guide/module.html
- KernelSU WebUI：https://kernelsu.org/guide/module-webui.html
- Android `ApplicationInfo`：https://developer.android.com/reference/android/content/pm/ApplicationInfo
- Android security checklist（localhost IPC）：https://developer.android.com/privacy-and-security/security-tips
- AOSP network stack configuration tools：https://source.android.com/docs/core/architecture/hidl/network-stack
- AOSP eBPF traffic monitoring：https://source.android.com/docs/core/data/ebpf-traffic-monitor
- GNU AGPLv3/GPLv3 说明：https://www.fsf.org/bulletin/2021/fall/the-fundamentals-of-the-agplv3
