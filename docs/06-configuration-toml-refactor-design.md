# NetHop TOML 配置重构与 Manager 契约设计

> 状态：Implementation Baseline v0.5
>
> 日期：2026-08-11
>
> 适用范围：当前 Alpha 至 Manager APK 接入
>
> 上位文档：[`00-nethop-system-design.md`](./00-nethop-system-design.md)
>
> 订阅约束：[`02-subscription-import-and-parser-design.md`](./02-subscription-import-and-parser-design.md)

## 1. 决策摘要

NetHop 将用户配置统一为 TOML，并废弃用户直接维护 `nethop.json` 与 `sources.json` 的方案。配置体验和能力范围向 NetProxy 看齐，但不复制其 shell 配置加载、多个文件互相引用、WebUI 直接改文件或让用户承担底层网络默认值的实现方式。

重构分为两个阶段：

1. **第一阶段：极简可用闭环。** v2 使用 `[[subscriptions.sources]]`，支持 `1..16` 个有序 source；安装模板只生成一个名为 `Primary` 的 source，单订阅用户仍只需填写名称和 HTTPS URL。NetHop 自动分配内部 ID、下载、识别格式、解析、合并、稳定去重、校验、发布 generation 并启动代理。
2. **第二阶段：完整配置与 Manager 契约。** 开放订阅调度、出站模式、节点选择、应用范围、网络接管和受控高级资源参数；Manager APK 通过 daemon 的 typed IPC 读写配置，不直接执行 shell 文本替换。

配置权威分为三层，不能混为“把 TOML 当数据库”或“内存状态不落盘”：

| 层 | 权威内容 | 生命周期 |
|---|---|---|
| 持久 TOML | 用户期望配置的 durable source of truth；支持手工导入/导出和灾后恢复 | 跨重启 |
| `Arc<EffectiveConfig>` | daemon 已解析、补默认、校验并 admission 后的只读运行快照 | 单次 daemon 生命周期 |
| generation/state | 已编译并通过健康检查的数据面配置与动态选择状态 | 事务发布、跨重启恢复 |

Manager 操作 typed config；daemon 校验后同时更新持久 TOML 和内存快照。daemon 不通过“写 TOML再读 TOML”完成 Manager apply，也不把大量动态状态塞进 TOML。

配置文件采用事件驱动热重载。daemon 同时监听持久配置目录和模块入口目录；用户保存完整 TOML 后，无需重启模块或点击 Action。文件事件只表示“需要重新核对”，不能直接改变运行状态；候选仍须经过稳定读取、摘要去重、typed 校验、capability admission 和统一事务发布。

目标配置入口：

```text
/data/adb/modules/nethop/config/nethop.toml
```

实际持久文件：

```text
/data/adb/nethop/config/nethop.toml
```

模块目录中的入口由安装脚本创建为指向持久文件的受控链接。daemon 始终从持久路径读取普通文件，模块覆盖安装不得覆盖用户配置。

### 1.1 2026-08-05 实现基线

当前代码已经形成以下可验证闭环：

- 完整 v1 TOML wire/effective model、`1..16` 个 source、daemon-owned 128-bit SourceId registry、canonical TOML、exact-byte digest、CAS、原子写、checkpoint 和 rollback；
- 双目录 inotify watcher、稳定读取、digest self-write no-op、入口 symlink 写断后的完整 admission/导入/恢复，以及 watcher degraded/retry health；
- 多 source 下载、格式提示、request profile、最多三个 mirror、有序合并、稳定去重、单源 last-known-good、持久自动更新调度和 `source_config_digest` supersede guard；
- `service.enabled` 持久启停、Stop 对旧 Start/Update 命令的抢占、应用包名到 UID admission、接口 glob 到 probe 已知精确接口名的受控解析、CIDR 和资源候选校验；
- `protocol.hello`、`config.get/validate/apply/reload/schema/mutate`、`capability.get`、`events.subscribe`，以及 root CLI JSON/JSONL 管道；
- capability report schema `3`，包含有界接口证据；有界事件 ring、脱敏持久 JSONL 日志、4 MiB 单文件上限和 `1..30` 天日志清理。
- 显式 TUN activation：独立 `TunRunner` 有界等待 `nethop0` 的 UP/IPv4/IPv6 状态，运行期低频校验，停止或失败后确认接口消失；该路径不创建或回滚 TPROXY `NetworkPlan`。

以下配置值在对应数据面完成前稳定拒绝，不能解释为已经接线：

| 配置能力 | 当前 admission |
|---|---|
| 非 `auto` 的 DNS/IPv6 模式 | 拒绝 |
| TUN 下关闭 TCP/UDP、选择热点/USB 或自定义物理接口 | 拒绝；原生 TUN inbound 当前不能准确兑现这些接管范围 |
| hotspot/USB 接口接管 | 实验性受控启用；仅验证软件 netfilter 路径 |
| `routing.block_quic=true` | 拒绝；Android 首版不阻断 QUIC |

这属于开发期的有意 fail-closed，而不是兼容承诺。`routing.bypass_cn=true` 已完成本地规则集、sing-box `check` 和 Android 侧规则匹配验证；其余被拒绝值必须先补数据面、capability admission、回滚和 Android 真机测试，再移除拒绝分支。

## 2. 设计目标

1. 首次使用只修改一个订阅 URL 即可完成代理闭环。
2. 配置项覆盖 NetProxy 中对 Android 用户真正有价值的能力，并为后续 Manager APK 提供稳定、分区、版本化的数据模型。
3. 手工编辑、CLI 和 Manager 使用同一份配置语义，不维护三套设置模型。
4. daemon 保持配置、generation 和网络接管的单一发布者；配置文件本身不能直接改变活动数据面。
5. 所有配置先完成语法、语义、平台能力和 `sing-box check` 校验，再以事务方式应用。
6. 高级参数可见、可诊断、可恢复，但不能关闭 NetHop 的安全不变量。
7. 配置读取不引入异步 runtime、通用配置框架或脚本解释器。
8. 第一阶段实现 `00` 已承诺的多订阅合并、稳定 ID、来源追踪和单源 last-known-good，不把它降为只有 wire shape 的占位能力。
9. 配置保存后自动、低延迟重载；空闲时不轮询、不产生周期 wakeup，失败时保留当前有效配置和数据面。

## 3. 非目标

- 不兼容尚未发布的 `nethop.json`、`sources.json` schema。
- 项目处于开发期，不为本文件早期草案中的 `sources[].id` 或单 source 形态保留双读、迁移器、deprecated 字段或兼容开关；新 schema 直接替换旧草案。
- 不执行 TOML 中的环境变量、shell、include、模板或本地文件引用。
- 不让 Manager APK 直接修改 sing-box 生成配置、netfilter 规则或 `current` generation。
- 不把所有内核实现常量都变成用户选项；无明确用户价值的参数继续由 capability probe 和安全默认值管理。
- 第一阶段不提供每个 source 独立调度、镜像或请求 profile 高级编辑，但必须执行多订阅有序合并、稳定去重和单源 last-known-good。
- 首版和第二阶段都不实现多 profile/多套命名配置切换、完整规则编辑器或 Expert sing-box JSON；热点/USB 仅走受控 TPROXY 复合计划，硬件/eBPF offload admission 完成前保持 experimental。
- 不因为配置中出现未来字段而静默忽略；未知字段必须拒绝并给出稳定诊断。
- 不在 TOML 中预留 `[plugins]`、`[extensions]` 或用户声明的 `feature_set`。扩展能力通过 IPC 协商和 schema metadata 暴露，避免无行为占位。
- 不承诺 Manager 完全零硬编码。标量设置允许 schema-driven 通用渲染；订阅、节点、应用选择和冲突解决保留领域 UI。

## 4. NetProxy 配置研读

### 4.1 参考范围

本次研读的本地参考包括：

```text
refer/NetProxy-Magisk/src/module/config/module.conf
refer/NetProxy-Magisk/src/module/config/tproxy/tproxy.conf
refer/NetProxy-Magisk/src/module/config/singbox/confdir/
refer/NetProxy-Magisk/src/module/scripts/utils/config.sh
refer/NetProxy-Magisk/src/module/scripts/core/subscription.sh
refer/NetProxy-Magisk/src/module/scripts/core/service.sh
refer/NetProxy-Magisk/src/webui/src/components/SettingsLayout.vue
refer/NetProxy-Magisk/src/webui/src/utils/ksu.ts
refer/NetProxy-Magisk/docs/config/
refer/MagicNet-main/webui/src/utils.ts
refer/MagicNet-main/webui/src/composables/useMagicNet.ts
refer/box4magisk/box/scripts/box.webui
refer/box4magisk/webui/src/lib/bridge.ts
D:/100_Projects/110_Daily/PathGuard-Next/daemon/src/main.cpp
D:/100_Projects/110_Daily/PathGuard-Next/daemon/src/rules_control.cpp
D:/100_Projects/110_Daily/PathGuard-Next/tests/hot_reload_test.ps1
D:/100_Projects/110_Daily/PathGuard-Next/docs/06-rule-file-refactoring-and-desugarer-tdd-implementation-checklist.md
```

MagicNet 的受控 CLI、私有 payload staging、保存前校验和敏感信息脱敏值得吸收；box4magisk 的 WebUI bridge 证明 `get-config/set-config/capabilities` 比直接散落 shell 更易管理，但其逐 key 文本替换仍缺少完整事务和并发摘要。PathGuard-Next 证明了目录 inotify、稳定 source snapshot、digest no-op、单 reconciler 和失败保留活动快照的组合可以工作。NetHop 采用 root CLI 管道作为 APK 桥，并由 daemon 统一执行 typed mutation 和配置 reconcile。

### 4.2 NetProxy 暴露的配置能力

| 配置域 | 代表字段 | 用户价值 | NetHop 取舍 |
|---|---|---|---|
| 服务 | `AUTO_START`、`GMS_FIX` | 开机行为、设备兼容 | `service.enabled` 同时决定当前代理状态和下次开机是否恢复；不增加第二个自启状态。GMS 修复只有形成可验证组件后才加入 |
| 出站 | `OUTBOUND_MODE`、`SELECTOR_MODE`、`CURRENT_CONFIG` | rule/global/direct、自动/手动节点 | 保留模式；当前节点属于运行状态，不接受文件路径 |
| 订阅 | `SUB_AUTO_UPDATE`、`SUB_UPDATE_INTERVAL` | 自动更新 | 保留，默认 24 小时；URL 进入统一 source 模型 |
| 接管 | `PROXY_MODE`、TCP/UDP 开关、IPv6 | TPROXY/回退和协议范围 | 映射为受控 enum；不提供不安全组合 |
| 接口 | 移动、Wi-Fi、热点、USB、额外接口 | 指定代理范围 | 本机与明确识别的 tether 接口；不匹配时 fail closed |
| 应用 | blacklist/whitelist、应用列表 | 分应用代理 | 第二阶段提供包名模型，UID 仅作为高级覆盖 |
| DNS | 劫持方式和端口 | DNS 接管 | 第二阶段开放策略，监听安全边界固定 |
| 路由资源 | mark、mask、table、priority、端口 | 冲突处理与兼容 | 第二阶段高级区开放候选值，仍必须 capability admission |
| 绕过 | 私网/CN IP、强制代理 IP | 路由策略 | 通过受控规则源与 CIDR 类型表达，不接受 shell 字符串 |
| 热点 | tether 接口 | 下游设备 IPv4 代理，IPv6 fail closed | 不处理 MAC/子网精细策略与硬件 offload |
| 调试 | dry-run、日志时间戳 | 排障 | 保留日志级别和 dry-run；敏感信息仍强制脱敏 |

### 4.3 值得吸收的设计

- 配置入口位于模块目录，root 文件管理器容易发现。
- 默认文件带注释，用户不依赖额外 App 也能理解设置。
- 开机自动应用配置，订阅更新失败保留旧节点。
- 服务、订阅、节点、应用和透明代理配置有明确的功能分区。
- Manager 能展示并修改常用开关，而不是要求用户调用底层命令。

### 4.4 不直接复制的设计

| NetProxy 做法 | NetHop 不复制的原因 | NetHop 方案 |
|---|---|---|
| shell `KEY=value` 后直接 `.` 加载 | 配置内容进入 shell 解释环境，边界过宽 | Rust TOML typed deserialize |
| `module.conf`、`tproxy.conf` 和 sing-box 分片共同决定状态 | 写入顺序、重载边界和一致性复杂 | 单一用户 TOML + daemon 生成运行配置 |
| WebUI 用文本替换修改单个键 | 容易产生并发覆盖、半写和未知状态 | `config.get/validate/apply` + digest CAS |
| 用户填写接口名、运行用户和当前节点文件路径 | Android 网络变化后易失效，且暴露实现细节 | capability probe、稳定 node ID 和受控策略 |
| mark/table 直接生效 | 可与 Android 或其他模块冲突 | 用户提供候选值，probe 后才选择并发布 |
| 控制器可监听 `0.0.0.0` 且使用静态 secret | 扩大 root 服务攻击面 | loopback-only + 随机 secret，不可配置关闭 |
| 直接操作节点 JSON 文件 | 绕过 parser、composer 和 generation 事务 | 节点只能来自受控导入或 daemon API |

结论：NetHop 对齐的是 **配置能力和 Manager 体验**，不是 NetProxy 的文件组织与 shell 执行模型。

## 5. 配置模型

### 5.1 版本与命名

- 顶层 `schema_version` 为正整数。当前开发期配置 ABI 固定为 `3`；v3 增加显式 `subscriptions.mode`，删除 `proxy.selector_mode`，并将 source 选择与节点选择收口为 daemon 事务和运行状态。新增可选字段本身不自动升级。
- section 和 key 使用 `snake_case`。
- enum 使用小写 `snake_case` 字符串。
- 时间间隔显式带单位，例如 `update_interval_hours`、`timeout_seconds`。
- 字节限制显式带单位，例如 `max_body_mib`。
- 未指定的可选字段由 daemon 填入当前 schema 的冻结默认值。
- 未知字段、重复 key、错误类型和不支持的 schema 一律拒绝。
- TOML v1 数组表保持出现顺序，`[[subscriptions.sources]]` 的顺序同时定义 source 合并优先级。

配置版本、IPC 协议版本和 Manager 版本独立演进：

```text
schema_version   = durable TOML wire shape
protocol_version = nethopctl <-> nethopd framing and methods
manager_version  = APK release identity，仅用于兼容诊断
```

当前开发期 wire 只接受 `protocol_version = 3`。Protocol v3 提供 typed subscription mode/select/set-enabled、node selection/list/test-all 方法和对应事件；旧 `SelectSource`、通用 `NodeSelect { target }` 与 `selected: bool` wire shape 仅保留为测试中的 before fixture，不进入生产协议。

项目首次正式发布前，daemon 的 schema 支持窗口固定为 `min = max = 3`，且只接受本文冻结后的 v3 shape。开发期旧 JSON、v1/v2 TOML 和包含用户 `sources[].id` 的草案全部干净拒绝，不写迁移器。首次公开发布后如需扩大兼容窗口，必须另立 schema 演进 ADR，不能把开发期兼容负担提前带入实现。

### 5.2 第一阶段默认配置

第一阶段安装包只生成已经实现的字段，避免出现“配置可写但没有行为”的假能力：

```toml
# NetHop user configuration
schema_version = 3

[service]
# Persistent proxy switch. The daemon stays available when disabled.
enabled = true

[subscriptions]
mode = "single"

[[subscriptions.sources]]
name = "Primary"
# Paste one HTTPS subscription URL. Leave empty to keep NetHop in direct mode.
url = ""
```

正常使用只需改为：

```toml
schema_version = 3

[service]
enabled = true

[subscriptions]
mode = "single"

[[subscriptions.sources]]
name = "Primary"
url = "https://example.com/subscription"
```

增加订阅时复制数组表并只填写名称和链接：

```toml
[[subscriptions.sources]]
name = "Backup"
url = "https://example.com/another-subscription"
```

`id` 不属于 TOML schema。用户不填写、维护或迁移 source ID；daemon 在内部 registry 中生成并维护。

URL 中的 `?`、`&`、`#` 在双引号内都是普通内容。配置不得记录真实订阅 URL 的示例、测试 fixture 或日志。

### 5.3 第二阶段目标配置

第二阶段生成完整注释模板。下面是目标 schema，不代表第一阶段可以提前接受尚未实现的字段：

```toml
schema_version = 3

[service]
enabled = true

[subscriptions]
mode = "single"
auto_update = true
update_interval_hours = 24

[[subscriptions.sources]]
name = "Primary"
enabled = true
url = ""
request_profile = "sing_box_android"
format_hint = "auto"
mirrors = []
filter = { include_names = [], exclude_names = [], excluded_node_ids = [], protocols = [] }

[[subscriptions.sources]]
name = "Backup"
enabled = false
url = ""
request_profile = "sing_box_android"
format_hint = "auto"
mirrors = []
filter = { include_names = [], exclude_names = [], excluded_node_ids = [], protocols = [] }

[proxy]
outbound_mode = "rule"

[proxy.urltest]
interval_minutes = 10
tolerance_ms = 50
max_candidates = 64
concurrency = 10

[applications]
mode = "all"
targets = []

[network]
capture_mode = "auto"
proxy_tcp = true
proxy_udp = true
ipv6_mode = "auto"
dns_mode = "auto"
tun_stack = "gvisor"

[network.interfaces]
mobile = true
wifi = true
hotspot = false
usb = false
include = []
exclude = []

[routing]
bypass_private = true
bypass_cn = true
block_quic = false
force_proxy_cidrs = []
bypass_cidrs = []
force_proxy_domains = []
bypass_domains = []
block_domains = []

[logging]
level = "info"
retention_days = 7

[advanced]
inbound_port = 7893
bypass_mark = 131072
ipv6_guard = true
dry_run = false
health_timeout_seconds = 3
reconcile_interval_seconds = 60

[[advanced.resource_candidates]]
mark = 1313407232
mask = 4294967295
route_table = 100
rule_priority = 12000

[[advanced.resource_candidates]]
mark = 1313407488
mask = 4294967295
route_table = 101
rule_priority = 12010

[[advanced.resource_candidates]]
mark = 1313407744
mask = 4294967295
route_table = 102
rule_priority = 12020
```

完整模板用于说明配置能力。Manager 应按 capability report 隐藏或禁用当前设备不可用的选项，不能只依据 TOML schema 判断功能可用。

## 6. 字段契约

### 6.1 服务与订阅

| 字段 | 类型/范围 | 默认值 | 生效方式 | 阶段 |
|---|---|---:|---|---|
| `schema_version` | `3` | 必填 | reload 前校验 | 3 |
| `service.enabled` | bool | `true` | true 启用；false 受控停服并保持 direct | 1 |
| `subscriptions.mode` | `single/merge` | `single` | source/mode generation 事务 | 3 |
| `subscriptions.auto_update` | bool | `true` | 调度器重排 | 2 |
| `subscriptions.update_interval_hours` | `1..168` | `24` | 调度器重排 | 2 |
| `subscriptions.sources` | `1..16` 个有序数组表 | 一个命名为 `Primary` 的 source | 有序合并事务 | 1 |
| `subscriptions.sources[].name` | 必填且配置内唯一；`1..64` Unicode scalar、最多 128 bytes、无控制字符 | `Primary` | source 协调与 UI/status | 1 |
| `subscriptions.sources[].enabled` | bool | `true` | source 合并事务 | 1 |
| `subscriptions.sources[].url` | string，空或有效 HTTPS URL | 空 | 完整订阅事务 | 1 |
| `subscriptions.sources[].request_profile` | `generic/mihomo/clash_standard/surfboard/sing_box/sing_box_android` | `sing_box_android` | 下次 fetch | 2 |
| `subscriptions.sources[].format_hint` | `auto/uri_list/base64_list/clash_yaml/singbox_json/surfboard_ini` | `auto` | 下次 parse | 2 |
| `subscriptions.sources[].mirrors` | 最多 3 个 HTTPS URL | `[]` | 主源失败后有界回退 | 2 |
| `subscriptions.sources[].filter.include_names` | 最多 32 个名称子串规则；ASCII 大小写不敏感 | `[]` | 解析后 source 级节点筛选 | 2 |
| `subscriptions.sources[].filter.exclude_names` | 最多 32 个名称子串规则；ASCII 大小写不敏感 | `[]` | 解析后 source 级节点排除 | 2 |
| `subscriptions.sources[].filter.protocols` | `vless/vmess/shadowsocks/trojan/hysteria2/tuic/anytls/http/socks` 白名单 | `[]` | 解析后 source 级协议筛选 | 2 |

第一阶段行为与内部默认：

```text
source_count = 1..16（模板默认 1）
source_mode = single
source_id = daemon 分配，不属于 UserConfigV1
source_name = Primary（用户填写，配置内唯一）
format_hint = auto
request_profile = sing_box_android
mirrors = []
auto_update = true
update_interval = 24h + stable jitter
```

`name` 是用户配置中的必填人类可读标签，配置内按校验后的 exact UTF-8 值唯一；不引入 Unicode case-fold/normalization 依赖。名称不得有首尾空白、C0/C1 控制字符、换行或终端转义。普通日志只记录 daemon 分配的 source ID，名称仅在授权状态/UI 中显示。

`request_profile` 只决定 `User-Agent` 和 `Accept`，不能强制 parser。自动模式只请求一次，不轮换多个客户端 UA 猜测响应。第一阶段所有 source 共用冻结的 `Auto + SingBoxAndroid` 请求策略；第二阶段才开放逐 source 的 profile、format hint 和 mirrors。

`service.enabled` 是唯一持久代理开关。`nethopctl start/stop` 和 Manager 开关都通过 daemon 更新此字段；不再另设容易与它冲突的 `auto_start`。设置为 false 只停止 sing-box 和网络接管，supervisor/worker 控制面继续运行，以便重新启用和诊断。

### 6.2 daemon-owned SourceId registry

Source ID 是运行时与持久状态身份，不是用户配置项。daemon 在以下私有文件中维护：

```text
/data/adb/nethop/state/source-registry.v1.json
```

该文件是 daemon-owned、root-owned `0600` 的内部状态，不属于 TOML schema，不接受用户编辑，也不由 Manager 作为配置导入/导出。registry 最多保留 active/pending 两个有界 binding；每个 binding 保存：

```text
config_digest
entries[] { source_id, name_digest, url_digest }
```

不得保存第二份明文 URL。`source_id` 为 `src_` 加 128-bit 系统随机值的小写十六进制编码；随机值复用经过测试的 `/dev/urandom` entropy 边界，不新增 UUID/随机数依赖。生成碰撞时有界重试，entropy 不可用或 registry 无法原子持久化时拒绝候选，不能退化为数组下标、名称或 URL 哈希 ID。

每次接受 TOML 候选前，`SourceRegistryReconciler` 按以下确定性顺序协调 identity：

1. 校验 source 名称唯一，非空 URL 的 exact request identity 唯一；
2. 未改变 URL 的 source 通过 `url_digest` 继承原 ID，允许改名和移动顺序；
3. 剩余 source 通过唯一 `name_digest` 继承原 ID，允许订阅链接/token 轮换；
4. 仍未匹配的 source 由 daemon 生成新 ID；
5. 已移除 source 的 ID 立即退出活动 registry，其缓存不再参与合并，后续由有界 GC 清理；
6. 同时修改名称和 URL 的手工编辑无法可靠证明是同一 source，按新 source 分配 ID。Manager 若要同时修改两者，必须使用携带内部 ID 的 `UpdateSource` typed mutation。

registry candidate 与 `observed_config_digest` 绑定，并通过 `write -> fsync -> rename -> fsync(parent)` 原子发布。Manager/CLI 修改 TOML 时使用小型 write-ahead 流程：先持久化 pending binding，再原子替换 TOML，激活成功后把 pending 提升为 active；重启时只选择与磁盘 exact digest 匹配的 binding。这样跨两个文件崩溃也不会把错误 ID 绑定到另一份配置。

registry 和 `EffectiveConfig`/generation 的提交仍受 `MutationCoordinator` 约束；registry 发布失败时配置候选不得生效。registry 丢失或损坏时生成新身份并保持 direct，不能用旧 generation 猜测映射。仅导出/导入 TOML 不承诺跨设备保留内部 ID；需要完整身份恢复时备份的是受控 NetHop state，而不是给 TOML 增加 ID 字段。

`config.get`、订阅状态、诊断报告和 Manager source resource 可以返回只读的 `source_id + name`。`config.schema` 不列出 `subscriptions.sources[].id`，Manager 不渲染 ID 输入框；`AddSource` 只接收名称和 URL，并由 daemon 返回新 ID。

### 6.3 出站与节点选择

| 字段 | 可选值/范围 | 默认值 | 说明 |
|---|---|---:|---|
| `proxy.outbound_mode` | `rule/global/direct` | `rule` | 规则、全局代理、全局直连 |
| `proxy.urltest.interval_minutes` | `5..1440` | `10` | 低于 5 分钟会侵蚀电量和空闲 CPU 预算 |
| `proxy.urltest.tolerance_ms` | `0..1000` | `50` | 防止节点抖动切换 |
| `proxy.urltest.max_candidates` | `1..256` | `64` | 只限制 auto 集，不删除节点 |
| `proxy.urltest.concurrency` | 固定为 `10` | `10` | sing-box 1.13.15 内部固定并发；当前版本拒绝伪可配置值 |

`subscriptions.mode = single` 要求恰好一个已配置 source 启用；`merge` 允许多个已配置 source 同时启用，但不允许关闭最后一个有效 source。模式切换、single source 选择和 merge source enable/disable 都携带 `expected_config_digest`，在同一 mutation lock、generation 与 commit journal 事务中完成；失败时 TOML、active generation 和 source active set 保持不变。

节点的 `auto/manual` 意图不属于 TOML。daemon 私有 selection store 只保存 `Auto` 或稳定 `node_id`，generation node registry 负责稳定 ID 与内部 sing-box tag 的双向映射。状态响应分别返回 requested intent 与 core 实际 active terminal；不得用一个 `selected` 布尔值合并二者，也不得在解析失败时回退列表第一个节点。节点测速和切换由 daemon 通过 loopback-only Clash API 执行，WebUI/Manager 不直连该 API。sing-box 1.14 原生 gRPC API service 仅作为 [`11-deferred-capabilities-and-future-design.md`](./11-deferred-capabilities-and-future-design.md) 的未来候选。

### 6.4 应用代理

| 字段 | 类型/范围 | 默认值 | 说明 |
|---|---|---:|---|
| `applications.mode` | `all/blacklist/whitelist` | `all` | 全部、绕过名单、仅代理名单 |
| `applications.targets` | 最多 2,000 个 typed target：`{ kind = "package", android_user_id = 0, package = "..." }` 或 `{ kind = "uid", uid = 10123 }` | `[]` | 唯一应用范围入口；包名必须绑定 Android user，避免工作资料同名包歧义 |

`mode = "all"` 时 `targets` 必须为空；`blacklist`/`whitelist` 时必须至少有一个 target。包名由 daemon 通过 PackageManager 映射到 UID，shared UID 原子扩展；UID 0 和核心防环路身份不允许由用户写入，root 例外由 daemon 强制排除。wire 输入使用结构化对象，不使用 NetProxy 的空格拼接字符串；`EffectiveConfig` 规范化并稳定排序 target。

用户 target 不能覆盖 NetHop 核心、下载器或必要系统防环路身份；这些排除项只存在于 daemon-owned `CapturePolicy`，不属于用户配置 ABI。

### 6.5 网络接管

| 字段 | 可选值/范围 | 默认值 | 说明 |
|---|---|---:|---|
| `network.capture_mode` | `auto/tproxy/tun` | `auto` | `auto` 当前解析为 TPROXY；`tun` 显式进入独立 TUN activation，自动跨模式回退留给后续候选配置仲裁 |
| `network.proxy_tcp` | bool | `true` | 关闭后不接管 TCP |
| `network.proxy_udp` | bool | `true` | 关闭后不接管 UDP；状态明确 degraded |
| `network.ipv6_mode` | `auto/proxy/block` | `auto` | 无完整 IPv6 接管能力时 guard 阻断 |
| `network.dns_mode` | `auto/proxy/system` | `auto` | strict Private DNS 下按能力降级 |
| `network.tun_stack` | `system/gvisor` | `gvisor` | 显式选择写入受控 sing-box TUN inbound；Android 真机验证表明 `system` 可能出现接口健康但数据面超时，因此默认使用已验证可用的 `gvisor`，`system` 仅作为高级显式选择 |

接口选项表达用户意图，不要求用户知道 Android 实际接口名。`include/exclude` 是高级 glob 列表，必须限制字符集和数量；默认依靠 netlink/capability probe 识别移动数据与 Wi-Fi。

TUN 与 TPROXY 使用不同 attachment 生命周期。TUN 启动顺序固定为 `capability admission -> stage sing-box -> 核心存活 -> nethop0 健康 -> commit generation`；健康或 commit 失败固定为 `停止 staged core -> 有界等待 nethop0 消失 -> 保留旧 generation -> fail-open`。运行期发现 TUN 漂移时不伪造 netfilter 修复，而是停止核心、确认接口消失并进入统一的 `1/2/4s` 重启预算。`capture_mode`、`tun_stack` 或 TUN 下应用 UID 的变化属于 `GenerationActivation`，必须重新 compose 和执行 `sing-box check`，不能只重启旧 generation。

2026-08-09 的 Android `alioth` 真机对照中，`system` 栈虽然通过进程、接口、IPv4/IPv6 地址检查，但 Google、YouTube 与哔哩哔哩均在连接阶段超时；只改变栈为 `gvisor` 后三者恢复。该证据说明接口级健康不能证明数据面可用，也不足以支撑 `system` 作为默认值。首版默认冻结为 `gvisor`；后续若要恢复 `system` 优先，必须先增加有界数据面探针，并在失败时于提交 generation 前回退，而不能依靠表面健康状态。

热点和 USB 已接入单一 generation 的复合 `NetworkPlan`：IPv4 使用独立 `NH_FWD_A/B` 链执行 TPROXY，IPv6 使用同源 `NH_FWD6_A/B` 链 fail closed，所有步骤由同一回执逆序回滚，健康检查同时验证两套链。接口只从 capability probe 的实时链路集合中按 Android 安全命名选择（如 `ap*`、`swlan*`、`wlan1+`、`rndis*`、`usb*`）；请求转发但没有安全匹配时拒绝激活。当前实现尚未识别 Android tethering 硬件/eBPF offload，因此该能力保持 experimental，不作为稳定发布能力；稳定化前必须增加 offload capability admission 和真机流量证据。

Wi-Fi 场景使用 `[network.wifi_scenes]` 和 `[[network.wifi_scenes.rules]]` 表达。规则由有界 `id`、可选 SSID/BSSID 和 `enable_proxy/disable_proxy` 动作组成，最多 64 条；默认 30 秒低频读取 `cmd wifi status`，范围为 15..3600 秒。SSID/BSSID 只进入内存 matcher，Debug、事件和 `config.get` 均脱敏。场景动作是瞬态覆盖，不写回 TOML；`service.enabled=false` 始终是主开关，任何场景都不能重新开启用户明确关闭的代理。无匹配、无线事实不可用或 probe 失败时恢复持久主开关语义。

### 6.6 路由和日志

| 字段 | 类型/范围 | 默认值 | 说明 |
|---|---|---:|---|
| `routing.bypass_private` | bool | `true` | 保留局域网可达性 |
| `routing.bypass_cn` | bool | `true` | `rule` 模式启用 `/data/adb/nethop/rulesets/` 下已校验的持久 `cn-domain.srs`/`cn-ip.srs`；`global` 忽略该分流 |
| `routing.block_quic` | bool | `false` | 显式策略，不作为性能默认值 |
| `routing.force_proxy_cidrs` | CIDR 数组 | `[]` | 强制代理，数量有界 |
| `routing.bypass_cidrs` | CIDR 数组 | `[]` | 自定义绕过，数量有界 |
| `routing.force_proxy_domains` | 域名后缀数组，最多 512 项 | `[]` | 命中域名及其子域强制进入顶层 selector，并使用代理 DNS |
| `routing.bypass_domains` | 域名后缀数组，最多 512 项 | `[]` | 命中域名及其子域强制直连，并使用直连 DNS |
| `routing.block_domains` | 域名后缀数组，最多 512 项 | `[]` | 命中域名及其子域在 route 层拒绝连接 |
| `logging.level` | `error/warn/info/debug/trace` | `info` | trace 仍不得输出 secret |
| `logging.retention_days` | `1..30` | `7` | 有界清理，不按无限大小增长 |

CIDR 使用结构化解析器验证并规范化，不能直接拼入 shell 命令。`force_proxy` 与 `bypass` 冲突时拒绝配置，不采用隐式优先级。

域名规则只接受规范 ASCII 域名后缀，不接受 URL、通配符、端口、路径、IP literal、空 label 或首尾为连字符的 label。输入统一转为小写、排序并去重；每项最长 253 bytes，每个 label 最长 63 bytes。不同动作列表之间出现相同后缀或父子后缀重叠时拒绝整份配置，例如 `force_proxy_domains=["video.example"]` 与 `bypass_domains=["sub.video.example"]` 不允许同时存在，避免同一请求依赖隐式顺序得到不同结果。

托管 route 的用户域名优先级固定为 `block -> force proxy -> bypass -> CIDR/private/CN -> final`。`force_proxy_domains` 和 `bypass_domains` 分别绑定 `dns-proxy` 与 `dns-direct`，保证域名判定与地址解析走同一路径。`block_domains` 当前只生成 route 层连接阻断；在 sing-box 1.13.15 的 DNS reject 行为没有独立 fixture 与 `sing-box check` 证据前，不宣称 DNS 层 NXDOMAIN/reject。

`performance.profile` 不进入 v1。当前没有三组经过真机验证且行为明确的参数集，提前暴露会成为假能力；`00` 也不允许首版任意修改全局 sysctl。未来只有 profile 的具体差异、可逆应用、耗电和性能门槛全部冻结后，才通过 ADR 加入可选字段。

### 6.7 高级资源参数

| 字段 | 类型/范围 | 默认值 | 约束 |
|---|---|---:|---|
| `advanced.inbound_port` | `1..65535` | `7893` | 必须未占用且与 API/DNS 端口不冲突 |
| `advanced.bypass_mark` | 非零 `u32` | `131072` | 不得与 capture mark 相交 |
| `advanced.ipv6_guard` | bool | `true` | 仅在完整 IPv6 代理已验证时允许 false |
| `advanced.dry_run` | bool | `false` | 只生成并校验 plan，不修改 core 或网络 |
| `advanced.health_timeout_seconds` | `1..30` | `3` | 不能取消健康检查 |
| `advanced.reconcile_interval_seconds` | `60..3600` | `60` | 低于 60 秒必须先证明不破坏空闲 CPU/wakeup 预算 |
| `advanced.resource_candidates` | `1..16` 个结构体 | 内置 3 个 | 按顺序 probe，不能直接宣称占用成功 |

每个资源候选包含：

```text
mark: non-zero u32
mask: non-zero u32
route_table: 1..u32::MAX
rule_priority: 1..u32::MAX
```

重复候选、mark 超出 mask、与 bypass mark 相交、表或优先级已被不可兼容规则占用时拒绝或尝试下一候选。Manager 应显示 probe 结果，而不是只显示用户填写值。

## 7. 永远不开放的安全不变量

“高级参数可配置”不等于关闭安全边界。以下项目不进入 TOML：

- 控制 API 监听非 loopback 地址；
- 固定、空白或用户指定的 Clash API secret；
- 关闭订阅 TLS 证书校验；
- 允许订阅执行 script、provider、rewrite、task、include 或本地路径；
- 绕过 SSRF 地址验证、重定向逐跳验证和 response size 上限；
- 让订阅覆盖 inbound、DNS 入口、路由、netfilter 或 Manager IPC；
- 禁用 generation 校验、`sing-box check`、健康探测和失败回滚；
- 任意 shell 命令、环境变量展开或可执行 hook；
- 任意文件路径、PID 路径、socket 路径或活动 generation 名称；
- 全表 flush、清理其他模块规则或关闭防环路身份绕过。

若未来增加 Expert 模式，使用独立受控文件和独立 ADR；不能在本 TOML 中用一个布尔值关闭上述约束。

## 8. TOML 解析与依赖

### 8.1 依赖选择

`toml` 仅加入 `nethopd`，不进入 `nethop-core`、`nethop-android` 或纯 parser 构建：

```toml
[dependencies]
toml = { version = "<audited-current>", default-features = false, features = ["parse", "serde"] }
```

最终 feature 名以选定最新版的实际 manifest 为准。实现时必须：

1. 查询并锁定当时最新稳定版；
2. 只启用反序列化所需 feature；
3. 通过 `cargo tree -e features`、`cargo deny`、Android arm64 编译和二进制体积比较；
4. 将精确版本、源码 digest、许可证和 features 写入 SBOM；
5. 不同时引入 `figment`、`config`、`toml_edit` 或第二套 TOML parser。

配置只有 KiB 级，解析耗时不是数据面热路径。此处优先正确性和可维护性，不自研 TOML 子集 parser。

### 8.2 有界读取

- 最大配置文件：第一阶段 `16 KiB`；第二阶段包含应用/CIDR 列表后 `256 KiB`。
- 只接受 UTF-8；允许 UTF-8 BOM 和 CRLF，读取后只做一次有界规范化。
- 路径必须为固定绝对路径、root-owned、`0600` 普通非 symlink 文件。
- 持久目录必须为 root-owned `0700` 普通目录。
- 空文件、超限文件、非 UTF-8、重复 key 和无效 TOML 全部拒绝。
- `Debug`、错误类型和 telemetry 不得包含 `subscriptions.sources[].url`。

### 8.3 typed model

第一阶段 wire model：

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UserConfigV1 {
    schema_version: u32,
    service: ServiceConfigV1,
    subscriptions: SubscriptionsConfigV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceConfigV1 {
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubscriptionsConfigV1 {
    sources: Vec<SubscriptionSourceV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubscriptionSourceV1 {
    name: SourceName,
    #[serde(default = "default_true")]
    enabled: bool,
    url: SecretString,
}
```

`UserConfigV1` 中没有 `SourceId`。`SourceName` 必须是 validated newtype；内部 registry 分配的 `SourceId` 也是独立 newtype，不能把任意 `String` 直接传入缓存键、诊断或 UI。解析和 registry reconcile 后转换为不含默认歧义的 `EffectiveConfig`。wire model 只表达用户输入，effective model 保存内部 ID、完整默认值和已验证类型；业务组件不得直接读取 TOML value tree。daemon 使用 `Arc<EffectiveConfig>` 原子替换运行快照，读路径不加配置写锁。

### 8.4 Diff 与摘要策略

每份稳定读取的 TOML 字节快照都计算 SHA-256 digest。`observed_config_digest` 表示磁盘上最近一次稳定快照，即使该候选随后校验失败也会更新；`active_config_digest` 表示当前 `EffectiveConfig` 的来源。IPC 中的 `expected_config_digest` 与 observed digest 比较，避免 Manager 覆盖刚发生的手工编辑。

digest 覆盖 exact bytes，因此手工修改注释也会改变 digest。digest 可以公开，原始字节和敏感字段不能进入状态或日志。

是否需要改变运行行为，不根据 exact-byte digest 判断。变更规划比较规范化 typed section 并生成 `ChangeSet`：

```text
service_changed
subscriptions_changed
proxy_changed
applications_changed
network_changed
routing_changed
logging_changed
advanced_changed
```

不在 v1 预计算 section digest。256 KiB 上限内的 typed 比较不在数据面热路径，而每节额外哈希会增加复杂度、碰撞语义和调试成本；只有 Android profile 证明 diff 成为可测瓶颈时才引入。动态应用和规则数据若未来超过 TOML 预算，应进入独立持久 store 和 typed API，而不是不断扩大配置文件。

## 9. 配置状态与语义

配置加载结果不是简单的成功/失败：

```text
Missing        -> installation/config path failure
Disabled       -> valid TOML, service.enabled is false
Unconfigured   -> valid TOML, enabled source URL is empty
Valid          -> valid TOML and all enabled source URLs validated
Invalid        -> syntax, schema, permission or semantic failure
```

行为如下：

| 状态 | 启动行为 | 数据面 | 用户提示 |
|---|---|---|---|
| `Disabled` | daemon 正常启动，不恢复 generation | direct | `service_disabled` |
| `Unconfigured` | daemon 正常启动并等待配置 | direct | `configuration_required` |
| `Valid` + matching generation | 先恢复缓存，再异步更新 | proxy | 显示 generation 和更新状态 |
| `Valid` + no matching generation | 网络就绪后首次更新 | direct，成功后切换 | `initial_update_pending` |
| `Invalid` at boot | 不接管网络 | direct | 稳定错误码和字段路径 |
| watcher/reload invalid | 更新 observed digest 和拒绝诊断；保持当前 effective config | 不变 | `config_reload_rejected` |

`service.enabled=false` 和空 URL 都是显式 direct 状态，不使用伪造的 `example.invalid` 地址。已有代理运行时关闭 service 或清空全部启用 source URL 并成功 apply 后，NetHop 停止接管但不删除历史 generation；重新启用并填写 URL 后可再次更新。

## 10. URL 与 generation 绑定

订阅 URL 可能包含账户 token，不能写入 generation manifest。daemon 计算：

```text
source_config_digest = SHA-256(
  schema_version || ordered(source_id, enabled, url_digest, request_profile,
                             format_hint, mirror_digests)
)
```

显示名称不进入 `source_config_digest`，因此只改名不会使 generation 失配；source 顺序、启用状态、URL/request identity 或 fetch 语义变化会改变 digest。manifest 只保存总 digest 和非敏感 source ID，不保存 URL/name digest。启动时：

- 当前配置 digest 与 generation digest 一致，允许恢复缓存；
- digest 不一致，旧 generation 不得自动接管流量；
- 同一 URL 更新失败，保留并继续使用匹配的 last-known-good generation；
- source 集合、顺序或 URL 发生变化且新订阅事务失败，保持 direct，不回退到旧账户或旧机场节点；未变化 source 的独立 last-known-good 缓存仍可参与 `00` 定义的多源事务。

URL 规范化只用于身份计算，不修改实际请求语义。不得排序或删除可能影响机场鉴权的 query 参数。

## 11. 生命周期

### 11.1 安装与升级

安装阶段：

1. 创建 `/data/adb/nethop/config`，owner `root:root`、mode `0700`。
2. 首次安装复制 `defaults/nethop.toml` 到持久路径并设置 `0600`。
3. 覆盖安装时保留同为 v3 的 TOML；检测到非 v3 开发期配置时，只保存一次 root-only `nethop.toml.pre-v3` 备份，然后直接安装 v3 默认文件，不在安装器中迁移旧字段。
4. 在模块目录创建可发现的 `config/nethop.toml` 受控链接。
5. 不联网、不验证订阅可达性、不生成代理 generation。

项目未发布，因此旧 JSON 不迁移、不双读、不提供兼容开关。旧文件不进入新包，daemon 只读取 TOML。

### 11.2 启动

```text
load bounded TOML
  -> syntax/schema/permission validation
  -> derive EffectiveConfig
  -> service.enabled gate
  -> capability admission
  -> recover matching generation if present
  -> wait for usable network
  -> initial subscription update
  -> parse/compose/sing-box check
  -> transactional activation
```

启动不因订阅服务器离线而删除匹配缓存。没有匹配缓存时保持 direct。

### 11.3 实时文件监听

daemon 启动后持续监听配置变化，用户保存完整 TOML 后自动重载，不要求重启模块或点击 Action。Android/Linux 实现使用 `inotify_init1(IN_CLOEXEC | IN_NONBLOCK)`，并在事件循环中通过 `poll(-1)` 等待；健康且无配置变化时不轮询文件、不产生周期 wakeup。

不能只 watch `nethop.toml` inode。编辑器常用 `temp -> fsync -> rename` 替换文件，旧 inode 上的 watch 会失效。daemon 同时 watch 两个固定目录：

```text
/data/adb/nethop/config
/data/adb/modules/nethop/config
```

只接受文件名精确为 `nethop.toml` 的相关事件。普通文件事件关注：

```text
IN_CLOSE_WRITE | IN_MOVED_TO | IN_CREATE | IN_ATTRIB | IN_DELETE | IN_MOVED_FROM
```

目录 watch 还处理 `IN_DELETE_SELF | IN_MOVE_SELF | IN_IGNORED`；全局处理 `IN_Q_OVERFLOW`。watch 失效或队列溢出时，daemon 重新打开固定目录、重建 watch，并对持久文件和模块入口做一次完整 rescan。不得根据事件携带的任意路径向目录外跟随。

watcher 只把“配置可能变化”写入有界 dirty 通道，由单个 `ConfigReconciler` 串行处理。它先排空当前 burst，再执行一次 reconcile；可使用不超过 100 ms 的合并窗口减少同一次保存产生的重复事件，但正确性不能依赖固定 sleep。配置是否完整由稳定读取和终止事件保证，不以“等得足够久”猜测。

`inotify` 初始化或重建失败时，状态进入 `watch_degraded`，保留显式 `config reload` 能力，并用有界退避重试注册 watch；不能静默退回 1 秒轮询。Android 稳定构建若无法建立 watch，应作为 capability/健康故障报告，而不是假装实时重载仍然可用。

### 11.4 稳定候选与热重载事务

事件不是授权，也不是完整配置证明。每次 reconcile 使用固定目录 FD 和固定文件名执行：

```text
event/rescan
  -> bounded stable read
  -> exact-byte config_digest
  -> typed parse + defaults + semantic validation
  -> capability admission + ChangeSet
  -> prepare required generation/network plan
  -> recheck candidate sequence and digest
  -> commit through MutationCoordinator
  -> atomically publish Arc<EffectiveConfig> and runtime state
```

稳定读取至少满足：

1. 使用 `openat` 和 `O_NOFOLLOW | O_CLOEXEC` 打开持久普通文件；
2. 校验 root owner、`0600`、大小上限和文件类型；
3. 读取前后 `fstat`，inode、size、mtime 或 ctime 变化则丢弃本次快照并有界重试；
4. 只有不可变字节快照进入 UTF-8/TOML parser；
5. exact-byte digest 与最近 observed/rejected digest 相同且设备能力未变化时 no-op；
6. 字节不同但规范化 typed config 相同时，只更新 source/CAS 状态，不触发订阅下载、core reload 或网络发布。

这使 truncate 后分段写入只会得到“unstable/invalid candidate”，不会加载半份 TOML；后续 `IN_CLOSE_WRITE` 会再次触发完整读取。无效候选只更新脱敏诊断，当前 `Arc<EffectiveConfig>`、generation、sing-box 和网络规则保持不变。用户修正并再次保存后自动清除错误。

耗时的下载、解析和 `sing-box check` 可以在提交锁外准备，但必须携带 `candidate_sequence + config_digest`。期间出现更新配置时，旧候选在 commit 前被判定 superseded 并丢弃；`service.enabled=false` 会取消待提交候选并优先执行受控停服。最终状态变更仍通过统一 `MutationCoordinator` 串行提交。

daemon 自身通过 Manager/CLI 原子写 TOML 也会产生 inotify 事件。不得靠临时关闭 watch 规避事件；reconciler 以已提交 digest 识别 self-write 并 no-op，从而避免二次 fetch/apply，同时保留 overflow 后全量恢复能力。

当 source identity 变化时，旧 generation 不再匹配新配置。新 source 准备失败后保持 direct，而不是继续使用旧账户或旧机场；同一 source 的刷新失败则保留 matching last-known-good generation。该语义与第 10 节一致。

### 11.5 模块入口与 Action

模块入口主要用于让 root 文件管理器发现配置。多数 in-place 写入最终修改持久文件，持久目录 watch 会自动重载；若编辑器用 temp+rename 把入口 symlink 替换为普通文件，模块入口目录 watch 必须检测到：

1. 正确 symlink：继续以持久文件为唯一读取目标；
2. root-owned、`0600`、未超限的普通文件：作为入口候选执行完整校验；
3. 候选成功：通过 `MutationCoordinator` 原子写入持久文件，再删除候选并恢复受控 symlink；
4. 候选失败：保留当前 runtime，不执行候选，报告 `NH-CONFIG-ENTRY-DIVERGED`；
5. 错误 symlink、非普通文件或不安全 owner/mode：拒绝且绝不跟随。

`action.sh` 先调用 `nethopctl config reload --wait` 强制执行与 watcher 相同的全量 rescan/reconcile；配置有效时再调用 `nethopctl update --if-needed --wait`，最后通过 `nethopctl status --human` 显示脱敏状态与核心更新提示。CLI 默认 JSON 输出不变；human renderer 只接受封闭状态枚举和数字版本，未知结构直接失败。Action 是 watch 降级、订阅手动重试或排障入口，不是正常保存后的必需步骤，也不承担含糊的 start/stop toggle。服务启停统一由 TOML/Manager 的 `service.enabled` 或明确的 `nethopctl start|stop` 完成。

状态同时包含只读 Android Private DNS 诊断。daemon 只查询 `private_dns_mode`，不读取或回显 provider hostname；`off` 映射为 split DNS healthy，`opportunistic/hostname` 映射为 `degraded_private_dns`，查询失败或未知 OEM 值映射为 `unknown`。诊断失败不阻断代理，NetHop 也不自动修改用户的全局 Private DNS 设置。

### 11.6 自动更新

当前实现覆盖首次启动更新、手动 `nethopctl update`、默认 24 小时持久调度、失败退避和持久 `next_run`。调度记录按 daemon-owned source ID 写入 SQLite；配置变化后重新 reconcile 活动 source。更新事务沿用现有规则：候选失败不改变活动 generation，首次失败保持 direct；prepared candidate 在 commit 前必须同时通过 `service.enabled`、活动 source config digest 和磁盘 config digest 检查，否则发布 `superseded` 并丢弃。

### 11.7 单写与事务串行化

watcher reload、显式 reload、Manager apply/typed mutation、订阅调度、节点切换和网络 activation 共用 daemon 的 `MutationCoordinator`。同一时刻最多一个会改变 effective config、generation 或网络接管的事务；只读 status/schema/capability 不受影响。

不增加第二把可能与全局 mutation lock 反向获取的“配置锁”。并发请求要么有界排队，要么返回 busy/conflict；订阅更新开始后若 observed config digest 已变化，候选必须丢弃并基于新配置重试。

## 12. Manager APK 契约

### 12.1 所有权

```text
Manual editor ---------> persistent TOML --inotify/reload--> nethopd
Manager APK -> su -c nethopctl stdin/stdout pipe -> root-only UDS -> nethopd
                                             |
                                             v
                                  validate + atomic write
                                             |
                                             v
                                     generation apply
```

daemon 是 Manager 写入路径的唯一发布者。Manager 不直接：

- 修改 `/data/adb` 文件；
- 拼 shell 命令替换 TOML key；
- 写 sing-box JSON；
- 调用 iptables/ip rule；
- 读写 `current` generation。

APK 不直接连接 loopback TCP 或放宽权限的 UDS。短请求通过 `su -c nethopctl ... --json` 的 stdin/stdout 有界帧完成；事件流由一个长驻 `nethopctl events --jsonl` 子进程转发。root-only UDS 继续只允许 daemon 与受控 CLI 访问，不为 Manager 新开第二个 socket。

### 12.2 IPC 方法

当前 daemon/CLI 已实现：

```text
config.get
config.export
config.validate
config.apply
config.reload
config.schema
capability.get
config.mutate
events.subscribe
connections.close_all
logs.get
logs.clear
node.export
subscription.import_preview
subscription.import_apply
core.version_check
```

CLI 同时提供：

```text
nethopctl backup export --file <new-path>
nethopctl backup restore --file <path> --expected-digest <sha256>
nethopctl core version-check
```

`config.export` 只通过 root-only UDS 返回完整未脱敏用户配置文档，备份文件使用 `nethop-config-backup-v1` envelope、`0600` 权限和 create-new 语义，不覆盖已有路径。备份不包含 daemon-owned source registry、手动节点源、下载缓存、generation、API secret、日志或 SQLite runtime state。恢复从 envelope 中提取完整 document 后复用 `config.apply` 和 `expected_config_digest` CAS；CLI 不直接覆盖 TOML。

`core.version_check` 是只读、空参数操作：只检查固定 sing-box 官方稳定 release，不下载、不替换、不执行远端资产。结果进入 `status.core_update` 与 `state/runtime.json`；Android 通知是 best-effort，失败不改变代理状态。该能力属于运行控制面，不增加 TOML 字段。

worker 同时维护独立的 sing-box 版本检查 schedule，固定 key 为 `resource:sing-box-version`，不占用或伪造订阅 source ID。首次创建记录时立即到期；成功后按 24 小时加稳定 jitter 重排，失败后从 1 小时开始有界退避。手动 `core.version_check` 也更新同一记录，避免紧接着重复自动请求。schedule 读取或持久化失败只产生运行时降级事件，并进入 1 小时单调时钟冷却以避免空转；它不改变当前 generation、网络接管或核心进程。

`config.apply` 请求包含：

```text
schema_version
expected_config_digest
complete typed document
```

`expected_config_digest` 始终比较磁盘最近稳定快照的 `observed_config_digest`，而不是活动 generation 或 typed section digest。这样即使手工保存了无效 TOML，Manager 也不会在不知情时覆盖；Manager 可读取拒绝诊断后显式选择修复并提交新候选。

daemon 执行：

```text
authorize caller
  -> compare expected digest
  -> validate typed document
  -> compute change plan
  -> prepare candidate if needed
  -> atomic write TOML
  -> activate candidate
  -> commit state or restore previous TOML/runtime
```

并发 digest 不匹配返回 conflict，Manager 必须重新读取，不允许 last-write-wins 覆盖手工编辑。

`config.apply` 是完整文档事务，也是手工导入/导出的基线。对于高频集合操作，第二阶段提供有界的 typed mutation，而不是直接实现 RFC 6902 JSON Patch：

```text
SetServiceEnabled
AddSource { name, url, ... } -> source_id
UpdateSource { source_id, name?, url?, ... }
RemoveSource { source_id }
MoveSource { source_id, before_source_id? }
AddPackage / RemovePackage / ReplacePackages
AddRoutingCidr / RemoveRoutingCidr
SetScalarField（只允许 schema 注册的标量 field_id）
RemoveNode { node_id }（将稳定 fingerprint 写入每个 source 的 `filter.excluded_node_ids`）
```

每个 mutation 都携带 `expected_config_digest`，在 `EffectiveConfig` clone 上应用，随后走完整 validation、SourceId registry reconcile、admission、change plan、持久化和 activation。`AddSource` 不接受调用方指定 ID；daemon 生成并在响应中返回。其他 source mutation 只接受 daemon 已分配的 ID，用户修改名称和链接时无需看到或输入 ID。RFC 6902 使用字符串 JSON Pointer、数组索引和通用 `move/copy`，对版本化配置的类型安全、权限和稳定 source identity 不利；NetHop 只吸收其“有序操作、失败整体不成功、配合 precondition”的事务思想。

### 12.3 版本协商与偏差

每次 Manager 会话先执行 `protocol.hello`：

```text
manager_version
manager_protocol_min/max
daemon_protocol_min/max
daemon_schema_min/max
active_schema_version
supported_operations
supported_features
```

`supported_features` 是 daemon 只读协商结果，不写入用户 TOML。feature 例如 `multi_source`、`typed_mutation`、`event_stream`、`app_scope`；它说明当前构建是否实现能力，不代表当前设备一定可用，设备可用性仍由 capability report 决定。

开发期不实现新旧 Manager/daemon 双向兼容矩阵。`protocol.hello` 仍保留，用于精确发现版本不匹配并干净失败：

- 当前 Alpha 要求 protocol/schema 精确匹配；不匹配时 Manager 只显示升级提示，所有写操作禁用；
- daemon 对 TOML 始终严格，不接受未来字段或旧草案字段；
- Manager 无法无损 round-trip 当前 schema 时整份 `config.apply` 禁用；
- 不实现 opaque 未知 section 回传、旧 DTO fallback 或开发版本迁移分支。

首次正式发布后如果 APK 和模块进入独立更新渠道，再基于真实版本偏差需求增加支持窗口。该工作不得反向污染当前 Alpha 的核心模型。

### 12.4 CAS 冲突与变更预览

`config.validate` 不只返回 valid/invalid，还返回：

```text
observed_config_digest
active_config_digest
candidate_config_digest
changed_field_ids[]
change_set[]
apply_impact = runtime_only | api_hot_change | network_plan | generation_activation | stop_data_plane
estimated_disruption = none | sub_second | up_to_3_seconds | device_measured
warnings[]
```

CAS conflict 返回服务端当前 observed digest，以及在不泄露 sensitive value 前提下可计算的字段差异。Manager 提示用户重新加载、放弃本地编辑或人工合并；daemon 不自动三方合并 secret、数组顺序或网络资源参数。

冲突字段摘要只使用稳定的粗粒度 ID：`service.enabled`、`subscriptions.auto_update`、`subscriptions.update_interval_hours`、`subscriptions.sources`、`proxy`、`applications`、`network`、`routing`、`logging`、`advanced`。只有请求候选能通过完整 typed validation 和 capability admission 时才返回可计算项；否则返回空数组。响应绝不包含字段值、source 名称、URL 或 mirror。

`config.get/status` 同时返回 observed/active digest、`candidate_sequence`、watcher health 和最近一次 reload 结果。observed 与 active 不同时，Manager 必须显示 pending/rejected 状态，不能把旧活动配置误报为磁盘文件已经生效。

### 12.5 Capability report

capability 不是 bool。每个能力返回：

```text
key
status = supported | unsupported | unavailable | conflict | experimental
reason_code
requirements { android_api?, root_backend?, kernel_features[] }
evidence { probe_id, observed_at, digest }
apply_effect
```

`unavailable` 表示设备状态可能变化后可恢复，`conflict` 表示资源被占用，`experimental` 表示有实现但不进入稳定承诺。`NeedKernelSU`、`NeedMagisk` 不膨胀为 status，而作为 requirements/reason 表达。Manager 据此显示状态和解决建议，最终 admission 仍由 daemon 执行。

当前 capability report wire schema 为 `3`。接口证据来自只读 `ip link show`，只接受有界安全接口名并稳定排序；`network.interfaces` 的 include/exclude glob 只在 capability 阶段匹配这些已验证名称，原始用户 glob 绝不直接进入 shell 或 netfilter 命令。

### 12.6 Manager 元数据

`config.schema` 为每个字段提供：

```text
field_id / path
value_type
default
enum/range
title_key / description_key
group / order
advanced / experimental / deprecated
sensitive / read_only / write_only
apply_impact
risk_level / confirmation_key
capability_key
stage
```

`title`、`description`、`default`、`deprecated`、`readOnly`、`writeOnly` 对齐 JSON Schema 标准注解语义；group、order、advanced、risk 和 apply impact 使用版本化的 `x-nethop-*` 扩展。显示文本使用 Manager 自带的 i18n key，不让 daemon 承担多语言资源。

Manager 用元数据生成普通 bool/int/string/enum 控件，并为未知标量提供通用降级渲染；订阅 source、节点选择、应用 picker、CIDR 顺序和冲突解决使用领域页面。业务校验仍以 daemon 为准。订阅 URL 标记 `sensitive=true`，普通状态接口只返回“已配置”和 digest；查看完整 URL 需要明确的 root 授权动作。

`risk_level` 至少为 `normal/disruptive/destructive`。关闭 `service.enabled`、清空所有 source 和导致 IPv6 block 的变更在 apply 前显示 impact；`apply_impact` 区分“立即生效”“下次网络重建”“完整 core activation”，不再用含糊的 `requires_restart`。

### 12.7 TOML 序列化

Manager apply 后由 daemon 输出固定顺序的 canonical TOML，并恢复官方模板注释。首版不承诺保留用户任意位置的自定义注释，因此不为 comment round-trip 引入 `toml_edit`。Manager 在首次写回前必须明确提示“应用后配置文件将规范化，自定义注释可能丢失”；手工 reload 不重写内容时保留注释。未来若真实用户把 comment round-trip 作为硬需求，再对 `toml_edit` 做体积、内存和维护 ADR。

canonical 输出对无优先级集合执行稳定规范化：包名、include/exclude UID、接口 include/exclude glob 和 CIDR 排序，UID 去重并强制保留 root UID `0`，CIDR 写回规范网络地址。source、mirror 和 resource candidate 保留用户顺序，因为顺序分别表达合并、故障转移和 capability 选择优先级。

### 12.8 Event API

不增加五套 `watch.*`。daemon 提供一个有界事件订阅：

```text
events.subscribe { kinds?: [config, runtime, subscription, generation, network] }
```

CLI 通过 JSONL stdout 转发：

```json
{"seq":42,"kind":"generation","active_config_digest":"...","generation":7,"state":"running_tproxy"}
```

事件只携带摘要和稳定 ID，不含 URL、节点凭据或完整配置。订阅者连接后先收到 snapshot，再接收单调 `seq` 的增量；队列有界，慢消费者收到 `resync_required` 后重新取 snapshot，daemon 不为 UI 背压阻塞 worker。

配置事件至少区分 `observed`、`accepted`、`rejected`、`superseded` 和 `watch_degraded`，并携带 observed/active digest。Manager 因而可以在文件保存后实时刷新状态，无需轮询完整配置。

同一份脱敏事件还可以写入固定日志目录的 JSONL 文件。实现限制为最多 4 个订阅者、事件 ring 最多 1,024 项、单行最多 16 KiB、单日文件最多 4 MiB；文件使用 epoch-day 名称、`0600`、`O_NOFOLLOW | O_CLOEXEC`。只记录 NetHop 结构化事件，不持久化 sing-box 原始 stdout/stderr。retention 每 24 小时扫描一次固定目录的最多 1,024 个直接子项，只删除过期普通 `.log` 文件且不跟随 symlink；失败只报告 `retention_degraded`，不终止 worker。

### 12.9 Manager MVP

单人开发阶段的 Manager MVP 只包含：

```text
status
service.enabled 开关
订阅列表：名称、URL、启用、添加、删除和排序
validate/apply/reload
capability 摘要
最近更新与错误
```

应用 picker、CIDR、resource candidates、通用 schema 表单和事件流在契约测试稳定后逐项加入。多 source 是核心能力，不从 Manager MVP 中删除；UI 不显示或要求用户输入内部 ID。

## 13. 变更分类

| 配置变化 | 最小动作 |
|---|---|
| 日志级别、更新调度 | 运行时更新，不重启 core |
| selector 节点、outbound mode | Clash API |
| 应用名单、CIDR、接口范围 | netfilter A/B plan |
| source 名称 | metadata/status 热更新；不 fetch、不使 generation 失配 |
| source URL/顺序/启用状态、协议节点集合 | registry reconcile + fetch + compose + `sing-box check` + generation activation |
| inbound port、capture mode、tun stack | 完整 core/network activation |
| resource candidates | capability re-probe + 完整 activation |
| `service.enabled=false` 或全部 URL 为空 | 受控停止并撤销网络规则；daemon 保持运行 |
| `service.enabled=true` | 恢复 matching generation 或执行首次更新 |

不能为了实现简单而让所有设置都重启 sing-box；也不能在没有事务保护时追求局部热更新。

`config.validate` 必须返回本节计算的 `apply_impact` 和参考设备实测 disruption 档位，使 Manager 在用户确认前说明影响。估计值不是 SLA；设备没有证据时使用 `device_measured`/unknown，不伪造精确毫秒数。

## 14. 诊断与脱敏

稳定诊断码至少包括：

```text
NH-CONFIG-MISSING
NH-CONFIG-NOT-PRIVATE
NH-CONFIG-TOO-LARGE
NH-CONFIG-INVALID-UTF8
NH-CONFIG-INVALID-TOML
NH-CONFIG-UNSUPPORTED-SCHEMA
NH-CONFIG-UNKNOWN-FIELD
NH-CONFIG-INVALID-VALUE
NH-CONFIG-FEATURE-NOT-READY
NH-CONFIG-DUPLICATE-SOURCE-NAME
NH-CONFIG-DUPLICATE-SOURCE-URL
NH-CONFIG-URL-NON-HTTPS
NH-CONFIG-URL-DENIED
NH-CONFIG-CONFLICT
NH-CONFIG-BUSY
NH-CONFIG-ENTRY-DIVERGED
NH-CONFIG-ROUNDTRIP-UNSAFE
NH-CONFIG-UNSTABLE-SNAPSHOT
NH-CONFIG-WATCH-DEGRADED
NH-CONFIG-CANDIDATE-SUPERSEDED
NH-SOURCE-ID-ENTROPY-UNAVAILABLE
NH-SOURCE-REGISTRY-INVALID
NH-SOURCE-REGISTRY-PUBLISH-FAILED
NH-CONFIG-CAPABILITY-REJECTED
NH-CONFIG-RELOAD-REJECTED
NH-CONFIG-APPLY-ROLLED-BACK
```

错误报告可以包含字段路径、约束和建议值，但不得包含：

- 完整订阅 URL；
- query、fragment、Authorization、Cookie；
- 节点 UUID、密码或密钥；
- API secret；
- 完整生成配置。

示例：

```text
NH-CONFIG-URL-NON-HTTPS at subscriptions.sources[0].url: HTTPS is required
```

## 15. 两阶段实施任务

### 15.1 第一阶段：极简多订阅闭环

每项按 TDD 的 RED -> GREEN -> REFACTOR 执行：

1. 增加 TOML 依赖审计、feature 和 Android arm64 构建门禁。
2. 为 `UserConfigV1`、`service.enabled`、`1..16` source、必填唯一名称、空/HTTPS URL、用户提供的未知 `id`、重复 key、权限和大小限制编写失败测试。
3. 实现 bounded TOML loader、`SecretString` 和 `EffectiveConfig`。
4. 实现固定目录 FD、稳定读取、exact-byte `config_digest` 和不安全 owner/mode/symlink 拒绝。
5. 为 `SourceRegistryReconciler` 编写随机 ID、碰撞、entropy 失败、名称/URL 匹配、重排、删除、损坏和原子发布测试，再实现 daemon-owned registry。
6. 删除 `WorkerConfig` JSON 手工 value parser，内部 capture/resource 使用冻结默认值。
7. 删除 `SourceConfig`/`sources.json` 用户路径，从 TOML 的全部 source 构造内部 `SourceId + Auto + SingBoxAndroid` source。
8. 实现有序合并、稳定去重、来源追踪和每 source last-known-good，并扩展 generation manifest 的 `source_config_digest`。
9. 实现匹配 generation 恢复、只改名仍匹配、URL 变化不恢复旧 generation 的测试。
10. 为 watcher 抽象编写 burst、overflow、watch invalidation、delete/recreate 和 shutdown 测试，再实现双目录 inotify watch 与有界 dirty 通道。
11. 实现单 worker `ConfigReconciler`：digest no-op、typed-equal no-op、registry reconcile、candidate sequence supersede、自身写事件去重和失败保留当前 runtime。
12. worker 在 `service.enabled=true`、存在非空 URL、无 matching generation 时自动执行首次更新；false 时取消候选、受控停服并保持 direct。
13. 实现 `config.reload --wait` 全量 rescan、单写协调、超时和脱敏结果。
14. 让 `nethopctl start/stop` 通过 daemon 原子持久更新 `service.enabled`，并验证随后产生的文件事件不重复 apply。
15. 修改 Action 为强制 reload/update 和状态展示入口，保留明确的 start/stop CLI。
16. 安装脚本创建持久 TOML、私有 state 目录和模块目录受控链接；升级不覆盖 v3 配置/registry，非 v3 开发配置只备份后重置。
17. 实现入口 symlink 被编辑器写断后的实时候选校验、导入和恢复测试。
18. 删除模块包中的 JSON 默认文件和 JSON source 示例，不实现旧配置迁移。
19. 更新 module contract、Linux host integration、新旧行为对比和 Android 真机 smoke。
20. 真机验证“安装 -> 填名称/URL -> 保存 -> 自动代理”、多订阅合并、实时修改开关启停、非法配置修正恢复和 Action 强制重试。

第一阶段完成定义：

- 默认 `service.enabled=true`，用户创建 source 时只填写 `name` 和 `url`；
- 配置或 Manager 可用 `service.enabled` 启动/关闭代理，daemon 控制面保持可用；
- 保存完整 TOML 后 watcher 自动重载，Action 不是正常流程的必需步骤；
- source ID 由 daemon 生成并维护，TOML 不存在 `id` 配置项；
- 第一阶段即可合并 `1..16` 个 source 并保留来源追踪和单源 last-known-good；
- 用户无需配置 format、UA、mark、table 或端口；
- URI/Base64、Clash/Mihomo YAML、sing-box JSON 按编译能力自动识别；
- `sing-box check` 和数据面健康通过后才接管；
- 更新失败不破坏匹配的旧 generation；
- 状态和日志不泄露真实 URL。

### 15.2 第二阶段：高级配置与 Manager

1. 实现完整 v1 高级 wire/effective model，并为第二阶段字段增加边界测试。
2. 增加持久 update schedule、逐 source 调度、请求 profile、format hint 和 mirrors。
3. 接入 outbound mode、selector mode 和 urltest 有界参数。
4. 接入包名/Android user 到 UID 的应用范围解析和规范化集合。
5. 接入 capture mode、TCP/UDP、IPv6、DNS 和 TUN stack capability admission。
6. 接入移动/Wi-Fi/热点/USB 意图；热点/USB 的 offload 检测与精细 tether 地址策略仍为后续增强。
7. 接入受控 CIDR、CN ruleset 和 QUIC 策略。
8. 接入日志级别、保留期限和结构化诊断。
9. 接入 inbound、mark/mask、route table、priority 候选参数。
10. 为每类变化实现 `ChangeSet`、impact 预览与事务回滚测试。
11. 实现 `protocol.hello`、`config.get/validate/apply/schema` 和结构化 `capability.get`。
12. 实现 `expected_config_digest`、冲突字段摘要和版本偏差保护。
13. 实现有界 typed mutation，不实现通用 JSON Patch。
14. 实现 canonical TOML 原子写：`write -> fsync -> rename -> fsync(parent)`。
15. 实现单一 `events.subscribe` 与 CLI JSONL 管道、snapshot、序号和背压恢复。
16. 完成 Manager MVP typed DTO、通用标量 fallback 和 sensitive field 展示策略。
17. 完成配置 fuzz、IPC fuzz、断电/半写、reload/apply/update 并发故障注入。

第二阶段完成定义：

- TOML 和 Manager 覆盖同一套高级配置；
- Manager 不直接写 root 文件或执行网络 shell；
- 多 source 本身不要求升级 schema；本次 v2 升级来自 typed application targets、source filter、Wi-Fi scene 和 domain routing 的不兼容 wire shape；
- 所有配置项有类型、默认值、范围、capability 和生效级别；
- 配置 apply 失败时 TOML、generation、core 和网络规则保持一致；
- 高级配置无法关闭第 7 节安全不变量。

## 16. 测试矩阵

### 16.1 Host

- TOML golden、默认值、未知字段、重复 key、enum/range；
- service enable/disable；`1..16` source 次序、名称唯一性、重复 URL 和用户 `id` 字段拒绝；
- SourceId registry：128-bit ID 唯一性、碰撞重试、entropy 失败、rename/reorder/URL 轮换匹配、同时改名换 URL 新建身份、删除和损坏恢复；
- URL 空值、HTTPS、userinfo、缺 host、超长和脱敏；
- config digest 稳定性与 URL 变化；
- change plan 分类；
- atomic apply、CAS conflict、rollback；
- stable read：truncate/write、读取中变化、rename 替换、权限变化和有界重试；
- watcher burst 合并、相同 digest no-op、typed-equal no-op、overflow rescan、watch 丢失重建；
- reload 与 apply 并发、apply 与自动更新并发、watcher 与 Manager self-write、配置变化后丢弃旧候选；
- `service.enabled=false` 抢占待提交 source 更新，旧候选不能复活数据面；
- 开发期 daemon 只接受冻结后的 schema v3；旧 JSON、v1/v2 TOML、旧草案和未知字段均拒绝；Manager 无法无损 round-trip 时只读；
- typed mutation 等价于完整 apply；事件丢队列后要求 resync；
- property test：任意输入不 panic、不输出 secret。

### 16.2 模块契约

- ZIP 只含 `defaults/nethop.toml`，不含 JSON 用户配置；
- 首装创建持久文件；覆盖安装保留 v3 digest，非 v3 配置保存一次私有备份后重置；
- module config 入口精确指向持久文件；
- 编辑器 temp+rename 写断入口后，只在候选校验成功时导入并恢复链接；
- owner/mode 和非 symlink 持久读取约束正确；
- `state/source-registry.v1.json` 由 daemon 原子创建为 root-owned `0600`，ZIP 和用户 TOML 均不预置 ID；
- watcher 只监听两个固定目录和固定文件名，不能跟随入口中的任意 symlink；
- Action 调用受控 CLI，不解析 TOML、不执行订阅 URL。

### 16.3 Android 真机

- 空 URL 启动保持 direct；
- `service.enabled=false` 受控停止，true 恢复或首次更新；
- 填写真实脱敏测试 source 并保存后，无需 Action 即首次自动更新成功；
- 多 source 合并后报告包含不同内部 ID 和用户名称，重排/改名/链接轮换后的 identity 行为符合第 6.2 节；
- in-place save、temp+rename、快速连续保存和删除后重建都能自动 reconcile；
- 半写或非法 TOML 不改变 runtime，修正并保存后自动恢复；
- watcher 健康时空闲无配置轮询，CPU/wakeup 计入 `00` 的空闲预算；
- Clash/Mihomo 与 SFA 内容自动识别；
- URL 拼写错误、网络离线、HTTP 错误和 `sing-box check` 失败不会断网；
- 同 URL 更新失败保留旧代理；URL 变化失败不启动旧 source；
- Action 强制 reload、重启恢复、覆盖安装保留配置；
- TPROXY TCP/UDP 命中、IPv4/IPv6 guard 和防环路继续通过。

### 16.4 新旧行为对比

不做配置格式兼容，不等于放弃回归保护。每次破坏性重构保留当前 Alpha 的外部行为 baseline，并让旧实现与新实现分别使用各自原生配置运行同一组脱敏 fixture：

- URI/Base64、Clash/Mihomo、sing-box JSON 的接受节点和诊断；
- 单 source 下载、解析、compose、`sing-box check` 和 activation；
- 节点 fingerprint、稳定去重和 outbound 结果；
- `service` 启停、失败保留旧 generation、网络规则回滚；
- Android TPROXY TCP/UDP、防环路和 direct fallback。

对比测试要求新实现对仍在规格内的旧功能结果等价或有明确审核过的改进，同时单独验证多 source、内部 SourceId 和实时 TOML reload 等新增能力。测试不得把旧 JSON/TOML 直接喂给新 loader，也不得为了让 baseline 通过保留旧 parser、双写路径或兼容分支。

## 17. 文档与代码影响面

第一阶段预计修改：

```text
Cargo.toml / Cargo.lock
crates/nethopd/Cargo.toml
crates/nethopd/src/worker_config.rs
crates/nethopd/src/config_watch.rs
crates/nethopd/src/config_reconciler.rs
crates/nethopd/src/source_registry.rs
crates/nethopd/src/source_config.rs
crates/nethopd/src/source_update.rs
crates/nethopd/src/application.rs
crates/nethopd/src/worker_application.rs
crates/nethop-protocol/src/lib.rs
crates/nethopctl/src/lib.rs
crates/nethop-core/src/generation.rs
module/defaults/nethop.toml
module/customize.sh
module/action.sh
scripts/module-contracts.ps1
相关 contract/integration/device tests
docs/00-nethop-system-design.md
```

删除或停止发布：

```text
module/defaults/nethop.json
module/defaults/sources.example.json
/data/adb/nethop/config/nethop.json 读取路径
/data/adb/nethop/config/sources.json 读取路径
TOML `subscriptions.sources[].id` 用户字段
```

## 18. 跨文档一致性回写

本设计冻结后必须同步更新上位与任务文档，不能长期保留两套配置契约：

1. `00-nethop-system-design.md` 中的 `nethop.json`、`sources.json`、`expert.json` 和 `source.digest` 路径改为单一持久 `nethop.toml`、`config_digest` 与 generation manifest。
2. `00` 的 `expected_source_digest` 统一为 `expected_config_digest`；source 内容身份继续使用本文件第 10 节的 `source_config_digest`，两者不能混用。
3. `00` 的 `subscription add|remove|list|update` 按 v1 `subscriptions.sources` 重定义；第一阶段即支持多 source，用户只提供名称和 URL，daemon-owned registry 分配内部 ID。
4. `00` 增加 `service.enabled` 唯一持久代理开关，并明确 supervisor/worker 在 disabled 时仍运行。
5. `00` 的 worker 生命周期增加双目录 inotify watcher、单 `ConfigReconciler` 和显式 reload fallback；低频网络规则 drift reconcile 不能兼任配置文件轮询。
6. `01-performance-budget-and-slo.md` 增加 watcher 空闲 wakeup、保存到 reconcile 延迟、稳定读取、typed parse、ChangeSet、prepare 和 commit 分段计时。
7. 后续 TDD 任务文档必须把 SourceId registry、旧/新行为 baseline、watcher、稳定快照、事件风暴、overflow、self-write、supersede 和真机实时保存列为独立节点。

## 19. 调研依据与审核取舍

### 19.1 官方资料

| 资料 | 用于冻结的结论 |
|---|---|
| [TOML v1.0.0](https://toml.io/en/v1.0.0) | UTF-8、重复 key 非法、数组表顺序和基础 wire 语义 |
| [`toml::from_str`](https://docs.rs/toml/latest/toml/fn.from_str.html) | Rust typed deserialize 路径和最小 feature 核验入口 |
| [Linux `inotify(7)`](https://man7.org/linux/man-pages/man7/inotify.7.html) | 目录监听、事件队列、rename、overflow 和 watch 生命周期 |
| [Linux `inotify_init1(2)`](https://man7.org/linux/man-pages/man2/inotify_init.2.html) | `IN_CLOEXEC`、`IN_NONBLOCK` 和 fd 创建语义 |
| [Linux `poll(2)`](https://man7.org/linux/man-pages/man2/poll.2.html) | 空闲阻塞等待，不用周期读取配置文件 |
| [RFC 6902](https://www.rfc-editor.org/rfc/rfc6902.html) | 吸收 precondition 和有序原子操作，不采用字符串路径通用 Patch ABI |
| [JSON Schema annotations](https://json-schema.org/understanding-json-schema/reference/annotations) | Manager schema 的 title、description、default、deprecated、readOnly/writeOnly 语义 |
| [Magisk module guides](https://topjohnwu.github.io/Magisk/guides.html) | 模块安装、`service.sh` 和 Action 的生命周期边界 |

网页资料只用于确认公开契约；crate 精确版本、feature、许可证和 Android 行为仍必须以构建时锁定源码、SBOM 与真机测试为准。

### 19.2 本地参考项目

| 项目/文件 | 吸收内容 | 不直接复制的部分 |
|---|---|---|
| `refer/NetProxy-Magisk` | 配置能力范围、模块内可发现入口、常用开关和 Manager 体验 | shell source、多文件状态、逐 key 文本替换 |
| `refer/MagicNet-main` | 受控 CLI、私有 payload staging、保存前校验、敏感信息脱敏 | 不让 WebUI/Manager 成为第二发布者 |
| `refer/box4magisk` | `get-config/set-config/capabilities` 的桥接体验 | 无 CAS 的文本修改和宽泛 shell 边界 |
| `D:/100_Projects/110_Daily/PathGuard-Next/daemon/src/main.cpp` | inotify 目录监听、`poll(-1)` 空闲等待、burst 排空、overflow 全量 reconcile | 其兼容性 polling fallback 和单目录假设不直接用于 NetHop Android 稳定构建 |
| `D:/100_Projects/110_Daily/PathGuard-Next/daemon/src/rules_control.cpp` | source digest no-op、无效候选保留 active policy、Manager CAS、单 reconciler | NetHop 还需处理远程 fetch、候选 supersede、服务关闭优先级和 generation/source 绑定 |
| `D:/100_Projects/110_Daily/PathGuard-Next/tests/hot_reload_test.ps1` | comment-only 不发布、真实内容变化自动发布的集成验收 | NetHop 追加 rename、半写、overflow、self-write 和订阅失败场景 |
| `D:/100_Projects/110_Daily/PathGuard-Next/docs/06-rule-file-refactoring-and-desugarer-tdd-implementation-checklist.md` | 固定 sleep 不是正确性前提，稳定读取独立保证完整性 | 100 ms 合并窗口只作减负优化，不能替代稳定快照 |

### 19.3 两份审核建议的取舍

| 建议 | 决定 | 理由 |
|---|---|---|
| Typed Config Object 才是运行时权威 | 部分采纳 | `Arc<EffectiveConfig>` 是运行快照，TOML 仍是跨重启的 durable desired config；两者职责不同 |
| 增加 Patch API | 采纳其目标 | 使用受控 typed mutation，不采用 RFC 6902 通用字符串路径 |
| Capability 从 bool 改为结构化状态 | 采纳 | Manager 需要区分 unsupported、unavailable、conflict 和 experimental |
| TOML 增加 `feature_set` | 不采纳 | 功能协商属于 `protocol.hello`，不应成为用户可伪造配置 |
| Schema-driven UI metadata | 部分采纳 | 普通标量可通用渲染，订阅/应用/节点等仍使用领域 UI |
| 为每个 section 预计算 digest | 暂不采纳 | 256 KiB typed compare 不在热路径，先测量再增加复杂度 |
| 应用列表内部使用集合 | 采纳 | 内部规范化集合保证 O(1) membership，canonical TOML 稳定排序 |
| 预留 plugins/extensions section | 不采纳 | 当前无插件需求，违反 YAGNI，未来通过 schema 版本演进 |
| 多个 watch API | 合并采纳 | 使用一个有界 `events.subscribe`，按 kind 过滤 |
| v1 预留多订阅数组形态 | 提升为第一阶段实现 | 已是 `00` 明确承诺，不能只保留无行为的 wire shape |
| 用户 TOML 配置 source ID | 不采纳 | ID 由 daemon 生成并保存在私有 registry；用户只维护唯一名称和 URL |
| 为开发期旧 schema 写兼容/迁移 | 不采纳 | 项目未发布，直接重构；使用新旧行为 baseline 防回归，不保留双读分支 |
| 删除无行为的 `performance.profile` | 采纳 | 未经真机证明的枚举是假能力 |
| 收紧 urltest/reconcile 下限 | 采纳 | 分别为 5 分钟和 60 秒，保护电量与空闲 CPU 预算 |
| 模块入口 symlink 写断检测 | 采纳并扩展 | 双目录 watcher 可实时发现，只有安全且有效的普通文件候选才导入 |
| TOML 自定义注释 round-trip | 暂不实现 | 手工保存保留原文；Manager canonical write 明示可能丢注释，真实需求出现后再评估 `toml_edit` |
| 配置内容实时更新/重载 | 采纳 | 采用 inotify + 单 reconciler + stable snapshot + digest no-op；Action 仅作 fallback |

## 20. 验收结论

配置重构必须同时满足两点：

1. **现在足够简单：** 普通用户只填写订阅名称和 URL 并保存，就能自动分配内部 ID、重载、解析、合并并代理；修改 `service.enabled` 即可启停。
2. **以后足够完整：** 高级用户和 Manager APK 可以操作完整、typed、可验证的配置，而无需绕过 daemon 或直接修改 sing-box/netfilter。

NetHop 对 NetProxy 的正确吸收是：能力完整、入口可发现、默认值清楚、Manager 易操作；NetHop 必须改进的是：单一配置模型、typed 校验、事务发布、并发保护和安全边界。配置项可以丰富，配置发布者只能有一个。
