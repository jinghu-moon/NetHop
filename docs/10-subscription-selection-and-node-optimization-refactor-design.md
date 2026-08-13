# NetHop 订阅选择与节点优选前后端重构设计

> 当前状态：本文保留 sing-box URLTest 两阶段方案作为历史分析与取舍记录；实际实施已由 D13/D14 破坏性替代。当前生产链路删除 `nethop-auto` 和 `urltest.concurrency`，composer 只生成单一 `nethop-select` 与 terminal outbounds，Rust benchmark engine 负责批量 delay、共同 cutoff、tolerance 选优和 selector 事务。后续实现或验收不得按本文旧的 group delay 请求序列判定失败。

> 状态：开发期设计冻结候选
> 适用范围：`nethop-subscription`、`nethop-core`、`nethopd`、`nethop-protocol`、`nethopctl`、WebUI、模块默认配置
> 配置 ABI：破坏性升级为 `schema_version = 3`
> 目标平台：Android arm64 Root 模块
> 依赖文档：`00-nethop-system-design.md`、`02-subscription-import-and-parser-design.md`、`06-configuration-toml-refactor-design.md`、`08-webui-design.md`、`09-webui-tdd-task-list.md`
> 未来候选登记：`11-deferred-capabilities-and-future-design.md`

## 1. 文档目的

本文解决当前订阅与节点选择链路中的四个根本问题：

1. 后端能够合并多个订阅，但 WebUI 使用圆点单选并在选择 source 时停用其他 source，产品语义不一致；
2. `proxy.selector_mode` 写在 TOML 中，而运行时又单独持久化手动节点，形成两个互相覆盖的状态源；
3. sing-box 自动模式实际是 `selector -> urltest -> terminal outbound`，当前 API 和 WebUI 不能稳定返回真正承载新连接的终端节点；
4. 自动候选池直接截取合并结果前 `max_candidates` 个节点，前置大订阅可能完全挤占后续订阅。

项目尚未正式发布，因此本次不保留 schema v2、旧 IPC 或旧 WebUI 行为兼容层。实现应通过破坏性重构消除歧义，而不是继续叠加条件分支。重构前必须冻结仍需保留的用户行为测试；重构后必须以新契约证明旧功能仍可用，并证明新增能力生效。

## 2. 当前事实与问题边界

### 2.1 sing-box 能力边界

NetHop 当前冻结的运行时基线是 sing-box `v1.13.15`。集成形态必须明确为：

- `nethopd` 通过 `Command` 启动独立的 `bin/sing-box run -c <sealed-generation>/config.json` 子进程；
- composer 在受控配置中生成 `experimental.clash_api.external_controller`；
- daemon 只连接带随机 secret 的 IPv4 loopback Clash HTTP API；
- NetHop 不链接 libbox，不嵌入 sing-box Go library，也不使用 Android AAR 驱动核心。

在这条已发布且已验证的 `v1.13.15` 边界内，sing-box 支持：

- 在一份配置中定义多个 terminal outbound；
- `selector` 在若干 outbound 之间手动选择；
- `urltest` 周期测试若干 outbound，并按延迟与 tolerance 自动选择；
- 通过 Clash API 查询 group、执行节点或 group 测速、切换 selector 和观察连接。

sing-box `v1.14.0` 开始提供普通配置可启用的 `services.api`：它把图形客户端原有的 `StartedService` protobuf 接口作为 gRPC/gRPC-Web 服务暴露给独立 `sing-box run` 实例，并提供 `SubscribeGroups`、`URLTest`、`SelectOutbound`、状态、日志和连接流。`v1.13.15` 源码已经存在相关 protobuf 与本地图形客户端控制实现；`v1.14.0` 的新增点是可配置、可远程连接的 API service，而不是第一次出现全部 gRPC 控制代码。

因此，“selector 只能通过 Clash API 控制”只能作为 NetHop 当前 `v1.13.15` 独立进程方案的实现约束，不能写成永久的 sing-box 能力边界。截至 2026-08-10，`v1.14.0-beta.13` 仍是预发布版本，当前最新稳定版本为 `v1.13.18`；本文不因开发线已存在新 API 而改变已冻结的 `v1.13.15` 契约。

sing-box 内核不负责：

- 下载订阅 URL；
- 识别 Clash YAML、Surfboard INI、URI 列表或 sing-box JSON 等订阅格式；
- 合并多个订阅、跨来源去重或保存来源关系；
- 为 source 提供 last-known-good、更新事务和诊断报告。

因此，多订阅合并属于 NetHop 控制面能力。sing-box 只消费 NetHop 生成的最终 outbounds 和 group，不知道“订阅”这一业务概念。

### 2.2 当前后端行为

历史链路为：

```text
所有 enabled source
  -> 下载或读取单源缓存
  -> parser 转换
  -> fingerprint 全局去重
  -> TerminalOutbound adapter
  -> 单一 ManagedProfile
  -> 单一 nethop-auto urltest
  -> 单一 nethop-select selector

当前链路为：`all enabled source -> parser/merge/fair pool -> terminal outbounds -> nethop-select selector`；Rust engine 并发调用每个 terminal 的 Clash delay API，daemon auto intent 决定是否切换。
```

当多个 source 同时 enabled 时，自动选择可以跨来源；只有一个 source enabled 时，自动选择自然退化为订阅内选择。后端不是在“切换订阅”，而是在合并后的节点池内切换终端节点。

### 2.3 当前前端行为

订阅页使用圆点单选。调用 `select_source` 后，daemon 把目标 source 设为 enabled，并把其他 source 全部设为 disabled。因此通过 WebUI 的正常路径实际上只有一个活动 source。

节点页支持：

- 列出 terminal nodes；
- 手动测试全部节点；
- 手动选择固定节点；
- 排除、导出节点。

节点页当前不支持：

- 显式切回 `nethop-auto`；
- 展示“用户请求的选择”和“实际承载连接的节点”之间的区别；
- 展示自动模式当前选择的 terminal node；
- 展示节点来源对于单源/合并模式的含义。

### 2.4 必须消除的错误状态

以下状态不得继续存在：

- TOML 写 `selector_mode = "urltest"`，但运行时 replay 又恢复一个手动节点；
- 自动 selector 选中 `nethop-auto` 时，API 因过滤 group tag 而报告没有 selected node；
- 概览页在没有 selected terminal node 时退回显示列表第一个节点，并把它误称为当前节点；
- 用户在订阅页看到单选 UI，却能通过手工 TOML得到完全不同的多源合并行为；
- 第一个 source 提供 64 个以上节点时，后续 source 永远无法进入默认自动候选池；
- “测速全部”与“自动切换”被混为同一个操作。

## 3. 目标与非目标

### 3.1 目标

1. 用户可以明确选择“单订阅”或“合并订阅”，前后端使用同一语义；
2. 默认保持简单：单订阅模式、一个活动 source、自动优选节点；
3. 合并模式能够跨活动 source 全局优选，同时保证自动候选池的来源公平性；
4. 自动/手动节点选择只有一个持久状态源；
5. API 能同时报告选择模式、请求节点、实际活动节点、延迟、来源和最近测试状态；
6. WebUI 可以在自动与手动之间往返切换；
7. 概览页“代理质量”卡片展示真实活动节点，并提供受控的即时测速入口；
8. source 切换、合并集合变化和节点选择都保持事务性，不因失败破坏当前可用代理；
9. 保留 parser、去重、last-known-good、generation、回滚、启停、节点测试和手动选择等既有有效能力；
10. 不增加新的网络监听端口、异步 runtime 或通用状态管理框架。

### 3.2 非目标

- 不让 sing-box 直接下载或解析订阅；
- 不实现 Clash `proxy-providers`；
- 不同时运行多个 sing-box 实例；
- 不为每个订阅创建独立透明代理栈；
- 不实现按价格、流量套餐或地理位置的智能计费调度；
- 不在自动切换时强制中断既有连接；
- 不在本次重构中升级 sing-box pin、引入 gRPC client 或迁移到 `services.api`；
- 不为 schema v2、旧 `select_source`、旧 `node.select` wire shape 编写兼容适配器；
- 不允许用户在 TOML 中填写 source ID、node ID 或内部 sing-box tag。

## 4. 冻结决策

| 编号 | 决策 |
|---|---|
| D1 | 配置 ABI 升级为 schema v3，只接受 v3，不迁移 v2 |
| D2 | `[subscriptions].mode` 明确为 `single` 或 `merge`，默认 `single` |
| D3 | `sources[].enabled` 在 single 模式表示唯一活动 source，在 merge 模式表示是否加入合并集合 |
| D4 | source ID 继续由 daemon 生成并保存在私有 registry，不进入 TOML |
| D5 | 从 TOML 删除 `proxy.selector_mode`，自动/手动选择改为 daemon 私有持久状态 |
| D6 | 自动选择与手动选择使用不同的 typed IPC 方法，不向前端暴露 `nethop-auto` 内部 tag |
| D7 | single 模式只对唯一活动 source 的节点测速和切换；merge 模式对所有活动 source 的合并节点池测速和切换 |
| D8 | merge 自动候选池使用确定性来源轮询算法，不再直接截取全局前 N 个节点 |
| D9 | 手动“测试全部”只更新测试结果；是否切换由当前 auto/manual 状态决定，前端不得自行选择最低延迟节点 |
| D10 | 节点切换保持 `interrupt_exist_connections = false`，只影响后续新连接 |
| D11 | 自动模式的实际节点必须递归解析 group，最多 8 层并检测循环 |
| D12 | WebUI 单源模式使用圆点单选，合并模式使用复选框，两种控件不得共用含混文案 |
| D13 | 当前 core control 固定为 `v1.13.15` loopback Clash API；`v1.14 services.api` 只进入未来候选登记，不进入本次依赖与实现范围 |

## 5. Schema v3

### 5.1 默认配置

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
```

与 v2 相比：

- 新增 `subscriptions.mode`；
- 删除 `proxy.selector_mode`；
- 其他字段继续遵循 `06` 已冻结的类型、范围和安全约束；
- schema v2 文件直接返回 `NH-CONFIG-UNSUPPORTED-SCHEMA`，不猜测用户意图。

`interval_minutes = 10` 是 NetHop 的移动端默认值，不是对 sing-box 上游默认值的转述。sing-box `v1.13.15` 的 URLTest 默认间隔为 `3m`、默认 tolerance 为 `50ms`、默认 idle timeout 为 `30m`。NetHop 保留上游 tolerance 与 idle timeout，但把周期延长到 10 分钟，以减少 Android 常驻 Root 模块的周期唤醒、测试流量和电量消耗；用户触发的即时测速不受该周期限制。

### 5.2 single 模式不变量

当 `subscriptions.mode = "single"`：

1. URL 非空的 configured source 中最多一个 `enabled = true`；
2. 至少一个 source 配置了 URL 但没有 enabled source 时，配置无效；
3. 所有 URL 都为空时允许零个或一个 enabled source，系统进入 unconfigured/direct 状态；
4. 存在非空 URL 时，唯一 enabled source 自身的 URL 必须非空；
5. 自动调度只更新活动 source；
6. 用户可以手动更新非活动 source 以预检，但成功更新不会改变当前 generation；
7. 切换活动 source 必须走专用事务，不能先关闭旧 source 再尝试新 source。

### 5.3 merge 模式不变量

当 `subscriptions.mode = "merge"`：

1. `enabled = true` 表示 source 参与下载、合并、去重、自动候选池和 generation；
2. 至少一个 URL 非空的 source 必须 enabled；
3. source 顺序只用于确定性 tie-break、报告和轮询起点，不赋予隐藏权重；
4. 同一节点出现在多个 source 时只生成一个 terminal outbound，同时保留全部 `source_id`；
5. 未进入 auto 候选池的节点仍可手动选择；
6. 任一 source 更新失败时优先使用该 source 的 last-known-good；没有 LKG 的失败 source 不得阻止其他有效 source 发布；
7. 所有活动 source 最终均无可用节点时才拒绝发布并保留旧 generation。

### 5.4 模式切换

`single -> merge`：

- 保留当前活动 source 为 enabled；
- 其他 source 保持原 disabled 状态；
- 不自动启用备用 source，避免模式切换立即产生额外下载和流量；
- 用户随后显式勾选需要合并的 source。

`merge -> single`：

- 如果恰好一个 source enabled，直接使用它；
- 如果多个 source enabled，调用方必须同时提供目标 `source_id`；
- 不允许 daemon 静默选择第一个 source；
- 目标 source 的 candidate 验证和发布成功后才提交 canonical TOML。

## 6. 后端领域模型

### 6.1 订阅模式

```rust
enum SubscriptionMode {
    Single,
    Merge,
}
```

`SourceConfig` 必须提供窄接口：

```rust
fn mode(&self) -> SubscriptionMode;
fn configured_sources(&self) -> impl Iterator<Item = &Source>;
fn active_sources(&self) -> Result<impl Iterator<Item = &Source>, ConfigError>;
```

`active_sources()` 是唯一决定 generation 输入集合的入口。调用者不得自行过滤 `enabled`，避免 single/merge 规则散落。

### 6.2 节点选择状态

从 TOML 删除 selector mode 后，daemon 私有状态定义为：

```rust
enum NodeSelectionIntent {
    Auto,
    Manual { node_id: StableNodeId },
}

struct NodeSelectionSnapshot {
    version: u8,
    intent: NodeSelectionIntent,
    active_node_id: Option<StableNodeId>,
    changed_at: UnixSeconds,
}
```

不变量：

- `intent` 表示用户意图；
- `active_node_id` 表示当前 core 实际委派的新连接节点，两者不得混用；
- auto 模式下 `active_node_id` 可随 urltest 变化；
- manual 模式下 intent node 正常情况下等于 active node；
- core 未运行、API 不可用或尚未完成测试时，active node 可以为空；
- 私有状态只保存稳定 node ID，不保存订阅凭据、完整 outbound 或用户 URL。

### 6.3 节点列表 DTO

删除每个节点上含混的 `selected: bool`，改为：

```json
{
  "nodes": [
    {
      "id": "nh1s-...",
      "name": "Tokyo",
      "protocol": "vless",
      "source_ids": ["src_..."],
      "latency_ms": 42,
      "alive": true,
      "is_requested": false,
      "is_active": true
    }
  ],
  "selection": {
    "mode": "auto",
    "requested_node_id": null,
    "active_node_id": "nh1s-...",
    "last_test_at": 1786200000
  }
}
```

`is_requested` 仅在 manual 模式目标节点上为 true；`is_active` 表示当前实际 terminal node。auto 模式不得把 urltest group 本身伪装为节点。

### 6.4 Generation 节点注册表

core API 只认识 sing-box 内部 tag，不能作为用户展示和来源追踪模型。每个 sealed generation 必须同时生成受控节点注册表：

```rust
struct GenerationNodeRecord {
    node_id: StableNodeId,
    internal_tag: InternalOutboundTag,
    display_name: SanitizedNodeName,
    protocol: SupportedProtocol,
    source_ids: Vec<SourceId>,
    auto_candidate: bool,
}
```

注册表不包含密码、UUID、私钥、订阅 URL 或完整 outbound。它负责：

- `StableNodeId -> internal_tag`，供 manual select 使用；
- `internal_tag -> StableNodeId`，供 active terminal 解析使用；
- 为 node DTO 恢复显示名称、协议和来源；
- 标记节点是否进入本 generation 的 auto pool；
- 在 generation 切换时验证 manual intent 是否仍有效。

`ClashApiClient` 只负责读取 core 的 tag、alive、history 和 group 状态；`OperationalControl` 必须通过当前 generation registry 进行 join，禁止直接把内部 tag 当作节点名称返回 WebUI。

## 7. Source 激活与合并事务

### 7.1 single source 切换

专用 `SubscriptionSelect` 事务：

```text
校验 expected_config_digest
  -> 定位 daemon-owned source ID
  -> 获取目标 source 新鲜内容或可接受的 LKG
  -> parse / validate / dedupe
  -> 生成目标 candidate
  -> sing-box check
  -> seal candidate 与 canonical TOML 临时文件
  -> 写入受控 commit journal
  -> 发布 generation pointer
  -> 原子替换 canonical TOML（目标 enabled，其余 disabled）
  -> 标记 journal committed
  -> replay node selection intent
  -> 发布 config/subscription/generation/node-selection 事件
```

generation pointer 与 TOML 无法依赖两个独立 `rename` 获得天然跨文件原子性，因此必须复用单一 mutation lock，并通过 root-only commit journal 提供崩溃恢复：

- journal 记录旧/新 config digest、旧/新 generation、事务阶段和 staged path，不记录订阅 URL；
- daemon 启动时先恢复未完成事务，再读取用户配置和启动 core；
- generation 已发布但 TOML 尚未替换时，恢复流程完成 TOML commit；
- generation 尚未发布时，丢弃 staged candidate 和 staged TOML；
- journal committed 后才允许清理旧 generation 与配置备份；
- 任一恢复分支都必须幂等，并有断电点失败注入测试。

任何一步失败：

- TOML 不变；
- 当前 generation 不变；
- 当前 source 不变；
- 当前节点选择意图不变；
- 返回稳定诊断码。

不得继续使用“先修改 enabled，再由后续 reconcile 尝试恢复”的半事务路径。

### 7.2 merge 集合变更

专用 `SubscriptionSetEnabled` 事务用于 merge 模式：

- enable source：目标 source 必须预检成功或已有有效 LKG；
- disable source：重新合并剩余活动 source；
- 禁用最后一个有效 source时拒绝；
- candidate check 失败时 TOML 和 runtime 均不变；
- 当前 manual node 因集合变化消失时，发布成功后受控回退 auto，并产生 warning event。

### 7.3 source 内容更新

更新某个活动 source 时：

1. 只替换该 source 的候选输入；
2. 其他 source 使用各自最新成功缓存；
3. 重新执行全局 fingerprint 去重；
4. 重新构造 fair auto candidate pool；
5. check 成功后原子发布；
6. 失败保留旧 generation 和旧 source cache active pointer。

非活动 source 的手动更新只更新其 LKG 与订阅状态，不发布 generation。

## 8. 自动候选池公平算法

### 8.1 问题

当前算法等价于：

```rust
all_nodes.iter().take(max_candidates)
```

这会让排序靠前且节点数量较大的 source 垄断 auto pool。

### 8.2 新算法

single 模式：

- 对唯一活动 source 的去重节点按稳定 node ID 排序；
- 取前 `max_candidates`；
- 不需要跨来源公平调度。

merge 模式使用确定性 round-robin：

```text
输入：按 TOML 顺序排列的 active sources
每个 source：按稳定 node ID 排列其可用节点

while pool 未达到 max_candidates 且仍有未消费节点：
  for source in active source order:
    取该 source 下一个尚未进入 pool 的 node
    若节点因跨 source 去重已存在，则继续推进该 source 队列
    加入 pool
```

特性：

- 时间复杂度 `O(N + S)`；
- 空间复杂度 `O(N)`，N 为合并后节点数；
- 相同输入、source 顺序和 node ID 得到相同结果；
- 每轮每个非空 source 最多贡献一个节点；
- 共享节点只占一个候选位；
- source 数量大于 max candidates 时，按 source 顺序确定首轮入选者；
- 不增加权重、地区识别或隐式质量评分。

### 8.3 auto 与 manual 集合

- `nethop-auto` 只包含 fair pool；
- `nethop-select` 包含 `nethop-auto` 和所有可手选 terminal node；
- manual node 不受 `max_candidates` 限制；
- active outbound 总量仍受 `00` 的 2,000 managed limit；
- 自动池变化不改变用户 intent；auto 保持 auto，manual 优先按稳定 node ID 恢复。

## 9. 自动测速、即时测速与切换语义

### 9.1 周期自动测速

仅当 intent 为 auto 且 urltest 处于活跃状态时，sing-box 按配置周期执行测试。继续使用：

- 默认 URL：受控固定的 204 endpoint；
- `interval_minutes = 10`；
- `tolerance_ms = 50`；
- `max_candidates = 64`；
- `idle_timeout = 30m`；
- `interrupt_exist_connections = false`。

`10m` 相比 sing-box 默认 `3m` 是有意的移动端功耗取舍，不是兼容性要求。Phase G 真机测试必须记录周期测速造成的唤醒、流量和空闲 CPU；只有证据表明更短周期在预算内且显著改善切换质量时，才调整默认值。

### 9.2 用户即时测速

`NodeTestAll`：

- 由 daemon 先请求外层 selector group delay，一次并发探测全部 terminal 并写入 sing-box 共享 URLTest history；
- 紧接着请求内层 URLTest group delay。新鲜 history 使该请求不重复网络探测，但必须由 sing-box 自身执行 `performUpdateCheck()`，按 `tolerance` 刷新 active child；
- WebUI 不逐节点并发发命令；
- 返回有界结果和每个节点稳定 ID；
- 更新延迟缓存与 `last_test_at`；
- manual 模式绝不因测试结果自动改选节点；
- auto 模式允许 sing-box 根据自身 urltest 规则更新 active child，前端随后重新查询 selection；
- 失败节点清除陈旧的本次结果，但不得删除节点。

该两阶段调用是 sing-box `v1.13.15` Clash API 的必要适配，不是 NetHop 自行实现选优。外层 selector 的通用 group delay 分支只更新共享 history；只有内层 `adapter.URLTestGroup` 分支会在测速返回前调用 URLTest group 的 `performUpdateCheck()`。任一阶段失败时 `NodeTestAll` 均不得报告成功，避免“延迟已更新但自动选举未刷新”的半成功状态。

### 9.3 选择 auto

新增明确方法 `NodeSelectAuto`：

1. selector 选择内部 `nethop-auto`；
2. 保存 `NodeSelectionIntent::Auto`；
3. 查询并解析当前 active terminal node；
4. 发布 node-selection event。

前端永远不传 `nethop-auto` 字符串。

### 9.4 选择 manual

新增明确方法 `NodeSelectManual { node_id }`：

1. 校验稳定 node ID；
2. 通过当前 generation 的 node registry 映射内部 tag；
3. 确认节点仍属于全部手选集合；
4. selector 切换到 terminal node；
5. 保存 manual intent；
6. 发布 node-selection event。

节点消失、generation superseded 或 API 拒绝时不得先乐观持久化。

## 10. 实际活动节点解析

### 10.1 解析算法

从顶层 `nethop-select` 开始：

```text
selector.now
  -> terminal node：返回 node ID
  -> urltest group：读取 group.now，继续解析
  -> 其他 group：按受控 group 接口继续解析
  -> direct/block：返回 non_proxy outcome
```

当前实现从 Clash API 返回的 group `now/all` 关系读取状态。即使未来改用 `services.api.SubscribeGroups`，gRPC 返回的 `Group.selected` 仍只指向下一层 tag，不会替 NetHop 解析到 terminal outbound；因此递归、深度限制和环检测属于 daemon 领域不变量，不依赖具体控制传输。

边界：

- 最大深度 8；
- 记录已访问 tag，检测循环；
- 只接受当前 generation registry 中存在的 tag；
- 失败返回 `active_node_id = null` 和结构化 degraded reason；
- 不因展示失败停止代理。

### 10.2 状态区分

必须区分：

| 字段 | 含义 |
|---|---|
| `mode` | 用户选择 auto 或 manual |
| `requested_node_id` | manual 模式用户指定节点 |
| `active_node_id` | core 当前用于新连接的 terminal node |
| `last_test_at` | 最近一次已完成的 group/node test 时间 |
| `selection_changed_at` | 用户意图最后变更时间 |
| `degraded_reason` | active node 暂不可解析的稳定原因 |

## 11. Protocol、CLI 与事件

### 11.1 破坏性 Protocol v3 方法

删除或替换：

- `ConfigMutation::SelectSource`；
- 含混的 `ControlMethod::NodeSelect`；
- 仅返回 node array 的旧 `node.list`；
- node DTO 的 `selected` 字段。

新增：

```text
SubscriptionModeGet
SubscriptionModeSet { mode, target_source_id?, expected_config_digest }
SubscriptionSelect { source_id, expected_config_digest }
SubscriptionSetEnabled { source_id, enabled, expected_config_digest }
NodeSelectionGet
NodeSelectAuto
NodeSelectManual { node_id }
NodeTestAll
NodeList
```

`SubscriptionSelect` 只允许 single；`SubscriptionSetEnabled` 只允许 merge。错误模式调用返回 `NH-SUB-MODE-MISMATCH`，不得猜测调用意图。

### 11.2 CLI

```text
nethopctl subscription mode
nethopctl subscription mode set single --source <source-id> --expected-digest <digest>
nethopctl subscription mode set merge --expected-digest <digest>
nethopctl subscription select <source-id> --expected-digest <digest>
nethopctl subscription enable <source-id> --expected-digest <digest>
nethopctl subscription disable <source-id> --expected-digest <digest>

nethopctl node selection
nethopctl node select auto
nethopctl node select <node-id>
nethopctl node test-all
nethopctl node list
```

CLI 输出继续使用 JSON envelope；人类可读输出不得包含订阅 URL、节点凭据或内部 tag。

### 11.3 事件

统一事件流增加或扩展：

```text
subscription.mode_changed
subscription.active_set_changed
subscription.update_completed
node.selection_changed
node.active_changed
node.test_completed
node.selection_degraded
```

`node.active_changed` 可由 urltest 自动切换触发，但必须合并高频重复事件。事件只携带稳定 ID、模式、时间和有限状态，不携带完整节点配置。

## 12. WebUI 重构

### 12.1 订阅页

页面顶部增加手写 `SegmentedControl`：

```text
单订阅 | 合并订阅
```

single 模式：

- source 左侧使用圆点单选；
- 选中表示唯一活动 source；
- 点击未选 source 执行完整 `SubscriptionSelect` 事务；
- 操作期间保持旧卡片为 active，目标卡片显示 pending；
- 失败后旧选择不变；
- 文案为“当前使用”，不得使用“已合并”。

merge 模式：

- source 左侧使用复选框；
- 选中表示加入全局节点池；
- 至少保留一个活动 source；
- 文案显示“已加入合并”或“未加入合并”；
- 卡片展示该 source 对 auto pool 的贡献节点数，而不是暗示所有节点都参与测速。

模式切换：

- single -> merge 可直接确认；
- merge -> single 且多个 source active 时，先弹出 source 选择 sheet；
- validate 响应必须说明将触发订阅下载、generation 发布和可能的节点回退；
- 不允许前端通过连续多个普通 config mutation 模拟事务。

### 12.2 节点页

节点列表顶部增加固定的“自动优选”控制项，不把它伪装成普通 terminal node：

```text
自动优选
全局候选 64 · 当前 Tokyo · 42 ms
```

- auto intent 时显示选中背景；
- manual intent 时具体 node card 显示选中背景；
- active node 使用单独的“正在使用”状态，不与“用户请求”混淆；
- merge 模式下 node card 展示来源名称；共享节点可以显示“2 个来源”；
- single 模式不重复显示唯一来源，减少噪音；
- 右上角闪电按钮继续表示“测试全部”；
- 测试结束后刷新 selection，避免自动 active node 仍显示旧值；
- node card 点击执行 manual select；自动优选项点击执行 auto select。

### 12.3 概览代理质量卡片

卡片必须展示：

- auto/manual 状态；
- 真实 active node 名称；
- 协议和来源摘要；
- 最近延迟；
- 最近测试时间或“尚未测速”；
- active node 无法解析时的降级状态。

交互：

- 点击卡片主体进入节点页；
- 右上角闪电图标执行 `NodeTestAll`；
- 测速按钮有独立 pending 状态，不阻塞进入节点页；
- 页面加载不自动触发测速；
- 卡片不得回退显示节点列表第一个元素；
- public IP 属于出口信息，可展示但不能替代 active node。

### 12.4 前端状态模型

WebUI store 分离：

```text
subscriptionMode
activeSourceIds
nodeSelectionIntent
activeNodeId
nodeEntitiesById
nodeTestOperation
```

禁止继续从 `nodes.find(node.selected)` 推导全局选择状态。订阅页和节点页只消费 daemon 返回的 typed snapshot，前端不自行重建来源合并或 selector 规则。

### 12.5 实时更新

- `node.active_changed` 只 patch selection snapshot；
- `node.test_completed` 批量 patch latency；
- `subscription.active_set_changed` 更新 source selection；
- generation 变化后执行有界 resync，避免旧 node ID 残留；
- 页面隐藏时停止图表绘制，但不得丢失协议事件；
- 不使用定时轮询模拟 urltest 状态。

## 13. 诊断码

新增稳定诊断：

| 诊断码 | 含义 |
|---|---|
| `NH-SUB-MODE-MISMATCH` | 当前订阅模式不允许该操作 |
| `NH-SUB-SINGLE-NOT-UNIQUE` | single 模式存在多个 active source |
| `NH-SUB-NO-ACTIVE-SOURCE` | 已配置订阅但没有活动 source |
| `NH-SUB-LAST-ACTIVE` | merge 模式试图禁用最后一个活动 source |
| `NH-SUB-TARGET-NOT-READY` | 目标 source 无新鲜结果或有效 LKG |
| `NH-SUB-MODE-TARGET-REQUIRED` | merge 转 single 时必须指定目标 source |
| `NH-NODE-SELECTION-STALE` | manual node 已不属于当前 generation |
| `NH-NODE-ACTIVE-UNRESOLVED` | 无法解析实际 terminal node |
| `NH-NODE-GROUP-CYCLE` | selector/group 解析检测到循环 |
| `NH-NODE-GROUP-DEPTH` | selector/group 超过最大解析深度 |
| `NH-NODE-TEST-PARTIAL` | 测速部分成功 |

诊断 message 与 code 分离；WebUI 按 code 映射简短文案，详细信息进入受控诊断视图。

## 14. 安全与隐私

1. source ID 和 node ID 可以返回 UI，但订阅 URL 与节点凭据不得进入事件、日志和普通状态 DTO；
2. `NodeSelectManual` 只接受稳定 node ID，daemon 内部映射 tag；
3. `NodeSelectAuto` 是无参数固定操作，不接受任意 group tag；
4. source 模式和 active set 修改继续要求 config digest CAS；
5. WebUI 不直接改 TOML，不通过文本替换 enabled 字段；
6. 测速 URL 由 daemon/core allowlist 控制，订阅内容不能覆盖；
7. source 事务继续执行 HTTPS-only、SSRF、DNS rebinding、大小和超时限制；
8. active node 解析失败只降级展示，不绕过路由或安全审计。
9. Clash API 继续只监听 IPv4 loopback，secret 不进入日志、WebUI 或普通状态 DTO；
10. 未来评估 `services.api` 时必须重新审核监听地址、secret、默认 CORS、gRPC-Web、Dashboard 和新增控制面的攻击面，不能直接沿用本节结论。

## 15. 性能与资源预算

| 项目 | 门槛 |
|---|---:|
| 10,000 节点 fair pool 构建 P95 | <= 10 ms（host release 基线） |
| fair pool 额外峰值内存 | <= 4 MiB |
| 默认自动候选 | 64 |
| 自动候选上限 | 256 |
| source 上限 | 16 |
| selector active 解析深度 | 8 |
| node list 默认上限 | 128，显式分页/虚拟化扩展 |
| node selection API p95 | < 200 ms 工程目标，不含远端连接建立 |
| WebUI selection event 到卡片更新 | <= 1 帧或下一微任务批次 |

fair pool benchmark 必须包含：

- 1 source × 10,000 nodes；
- 16 sources 均匀分布；
- 第一个 source 9,000 nodes、其余 source 少量节点；
- 50% 跨 source 重复节点；
- max candidates 16、64、256。

不得用真实公网延迟证明算法性能。远端网络只作为兼容性补充。

本次实现不增加 protobuf、gRPC、HTTP/2 或异步 runtime 依赖。未来若评估 `services.api`，必须把 `nethopd` 二进制增量、依赖树、RSS、空闲 CPU、连接恢复时间和 Android arm64 构建复杂度与当前同步 Clash HTTP client 做同机对比，不能只以 API 功能更多作为迁移理由。

## 16. 失败与恢复矩阵

| 场景 | 期望行为 |
|---|---|
| single 切换目标下载失败 | 保持原 source、原 generation、原节点 |
| merge enable 新 source 解析失败 | 不修改 active set |
| merge disable source 后 manual node 消失 | candidate 成功发布，intent 回退 auto，产生 warning |
| auto 当前节点失效 | urltest 选择其他候选，只影响新连接 |
| manual 当前节点失效但仍存在 | 报 unhealthy，不擅自切 auto |
| manual 节点从新 generation 消失 | 回退 auto 并持久化，不能指向悬空 ID |
| core API 暂不可用 | intent 保留，active node 显示未知，supervisor 正常恢复 |
| 自动测速部分失败 | 保留成功延迟，标记 partial，不删除节点 |
| config digest 冲突 | 返回 conflict，前端 reload 后重试 |
| generation 发布失败 | canonical TOML、active set、selection state 和 runtime 均保持旧值 |
| daemon 重启 | 恢复 source mode/active set，replay auto/manual intent |

## 17. TDD 与前后对比测试

### 17.1 重构前基线

实现前先增加或冻结以下行为测试，记录当前通过结果：

| ID | 必须保留的行为 |
|---|---|
| B01 | 单 source 下载、解析、生成配置并启动代理 |
| B02 | 多 source parser 合并、fingerprint 去重和来源追踪 |
| B03 | 单源更新失败使用 last-known-good |
| B04 | generation check 失败保留旧 runtime |
| B05 | `node.test-all` 返回有界延迟结果 |
| B06 | 手动选择有效 node 后新连接使用该 node |
| B07 | 旧连接不因节点切换被中断或重新归因 |
| B08 | source/node URL 和凭据不进入日志、事件与 UI DTO |
| B09 | 服务启停、TUN/TPROXY、应用范围和路由模式不受订阅重构影响 |
| B10 | WebUI 可从概览进入订阅页和节点页 |

以下旧行为只记录为待删除缺陷，不作为兼容目标：

- WebUI `select_source` 无条件关闭其他 source；
- TOML `proxy.selector_mode`；
- `node.selected` 无法表达 auto active child；
- 概览退回显示第一个节点；
- auto pool 直接取前 N 个节点；
- WebUI 无法切回 auto。

### 17.2 后端单元测试

配置模型：

- v3 single 零/一/多 enabled source；
- v3 merge 零/一/多 enabled source；
- v2、未知 mode、未知字段干净拒绝；
- TOML 不接受 source ID、node ID 和 selector mode。

fair pool：

- 单源稳定排序；
- 多源 round-robin；
- 大 source 不饿死小 source；
- 跨 source duplicate 只占一个位置；
- source 重排产生确定性新顺序；
- max candidates 边界；
- 空 source 和过滤后空 source。

selection：

- auto、manual 持久化与重启 replay；
- manual node 保留、消失和重新出现；
- selector -> urltest -> node 解析；
- group loop、深度和未知 tag；
- test-all 在 auto/manual 下的不同切换语义。

事务：

- single 切换成功与各阶段失败注入；
- merge enable/disable 成功与失败回滚；
- config CAS conflict；
- source update 与模式切换并发串行化；
- generation supersede；
- TOML、registry、selection state 和 generation 原子一致。

### 17.3 Protocol 与 CLI 合约测试

- 所有新方法参数、JSON shape、大小边界和错误码；
- auto 操作不接受 tag；
- manual 操作只接受稳定 node ID；
- single/merge 错误模式调用拒绝；
- CLI argv 不发生 shell 拼接；
- 旧 protocol 方法从枚举和帮助输出中删除；
- secret canary 不出现在 stdout/stderr。

### 17.4 WebUI 单元与组件测试

订阅页：

- single 渲染 radio，merge 渲染 checkbox；
- 模式切换控件和 pending 状态；
- 失败不改变旧选择；
- merge -> single 多活动 source 必须选择目标；
- source card 正确展示 auto pool contribution。

节点页：

- auto control 与普通 node card 分离；
- requested 与 active 状态分别渲染；
- auto/manual 往返；
- test-all 不在前端自行选最低延迟；
- generation 变化后移除陈旧节点；
- 10,000 节点虚拟列表 stable key 不回归。

概览页：

- 自动模式展示真实 active child；
- manual 模式展示 requested/active node；
- active unresolved 不显示第一个节点；
- 点击主体进入节点页；
- 闪电按钮触发即时测速且不触发导航；
- 测速完成后更新延迟和 active node。

### 17.5 端到端测试

至少覆盖：

```text
single Primary -> 自动优选 -> 手动节点 -> 切回自动
single Primary -> 切换 Backup 成功
single Primary -> 切换坏 Backup 失败并保持 Primary
single -> merge -> 启用 Backup -> 跨来源 auto active node
merge -> 禁用 active node 所在 source -> 保持可用或回退 auto
merge -> single -> 显式选择目标 source
更新订阅 -> manual node 保留
更新订阅 -> manual node 消失 -> auto fallback
```

### 17.6 前后对比验收

建立同一组脱敏 fixtures，在旧实现基线和新实现分别执行：

| 维度 | 新实现要求 |
|---|---|
| parser 接受节点 | 与基线一致或因明确新增能力增加 |
| fingerprint 去重 | 与基线一致 |
| source LKG | 与基线一致 |
| sing-box check/回滚 | 与基线一致 |
| 服务、接管、应用和路由 | 与基线一致 |
| 单 source 自动代理 | 与基线一致 |
| 多 source 公平性 | 新增，所有非空 source 在容量允许时均有候选 |
| auto/manual 往返 | 新增 |
| active terminal 可观测性 | 新增 |
| WebUI 模式语义 | 新增且与后端一致 |
| 性能/RSS | 不超过本文和 `01` 门槛 |

旧测试不得为了“通过”而简单删除。旧 wire shape 测试可以被新契约测试替换，但其背后的有效用户行为必须在映射表中有对应新测试。

## 18. 实施顺序

### 阶段 A：基线与模型

1. 冻结 B01-B10；
2. 新增 schema v3 wire model；
3. 删除 v2 接受路径和 `proxy.selector_mode`；
4. 引入 `SubscriptionMode` 与 `NodeSelectionIntent`。

### 阶段 B：后端 source 事务

1. 将 active source 规则收口到 `SourceConfig::active_sources()`；
2. 实现 single select 原子事务；
3. 实现 merge enable/disable 原子事务；
4. 增加模式切换事务和失败注入测试。

### 阶段 C：候选池与 core composer

1. 在 subscription conversion 结果中保留完整 source attribution；
2. 实现 fair pool 纯函数；
3. composer 分离 auto pool 与 manual pool；
4. 增加性能基准和 golden config。

### 阶段 D：节点选择控制面

1. selection store 升级为单一 intent 状态；
2. 实现 active terminal 递归解析；
3. 拆分 auto/manual typed methods；
4. 更新 CLI、事件和 DTO；
5. 删除旧 `NodeSelect` 和 `selected: bool`。

### 阶段 E：WebUI 订阅页

1. 增加 single/merge segmented control；
2. single 使用 radio；
3. merge 使用 checkbox；
4. 接入专用事务、CAS、pending、冲突和错误恢复；
5. 删除前端组合 mutation。

### 阶段 F：WebUI 节点与概览

1. 增加“自动优选”控制项；
2. 区分 requested/active；
3. 支持 auto/manual 往返；
4. 代理质量卡片接入真实 active terminal；
5. 增加即时测速按钮与事件更新；
6. 删除第一个节点 fallback。

### 阶段 G：集成与真机

1. 运行 workspace、WebUI、module contract 全量回归；
2. 构建 Android arm64 模块；
3. 真机验证 TPROXY/TUN 下 single/merge、auto/manual、更新和回滚；
4. 测量空闲 CPU、RSS、测速网络开销和切换延迟；
5. 更新 `00`、`06`、`08`、`09` 的冲突章节。

每个阶段完成后可以独立提交，但本文不授权自动执行 `git commit` 或 `git push`。

## 19. 完成定义

只有同时满足以下条件，重构才算完成：

1. schema v3 是 daemon、默认模板、CLI、WebUI 和文档的唯一配置 ABI；
2. single/merge 在 TOML、daemon、CLI 和 WebUI 中含义一致；
3. `proxy.selector_mode`、旧 `SelectSource` 和旧含混 `NodeSelect` 已删除；
4. auto/manual intent 可以跨 daemon/core 重启恢复；
5. auto 模式能够报告真实 terminal active node；
6. WebUI 可以从 manual 无损切回 auto；
7. merge fair pool 不饿死后续 source；
8. source 集合和模式变更失败时 runtime、TOML 和 state 均不变；
9. 概览、订阅和节点页面均消费同一 typed snapshot；
10. B01-B10 保留行为全部通过新契约测试；
11. 新增模式、公平性、active 解析和往返切换测试全部通过；
12. Android 真机下代理、测速、节点切换和现有连接保持行为符合设计；
13. 日志、事件、诊断和 WebUI bundle 不泄露真实订阅 URL 或节点凭据；
14. 性能和内存不超过 `01` 与本文预算。

## 20. 需要同步更新的既有文档

实现完成后必须同步：

- `00`：把“按 source 顺序取 auto 候选”改为 single 稳定排序、merge 来源轮询；明确 subscription mode；
- `06`：升级 schema v3，删除 `proxy.selector_mode`，重写 source enabled 与选择事务；
- `08`：订阅页 single/merge 控件、节点 auto control、代理质量卡片和事件模型；
- `09`：增加新的 TDD 节点并把旧 K/L 阶段的含混 selected 契约标记为 superseded；
- `11`：登记 sing-box API service/gRPC 与其他暂缓能力的触发条件、证据门槛和重新评审状态；
- 模块默认 `nethop.toml`：生成 v3 默认 single 配置；
- CLI help 与 JSON schema：只暴露新方法。

## 21. 不采纳的替代方案

### 21.1 永远只支持单订阅

不采纳。它简单，但会放弃 `00` 已明确的多源容灾、合并去重和来源追踪能力。

### 21.2 永远自动合并所有配置 source

不采纳。用户无法控制套餐流量和信任边界，备用订阅也会产生额外下载、测速与出口切换。

### 21.3 用 radio 表示“主订阅”，同时后台仍合并其他订阅

不采纳。视觉选择与真实流量路径不一致，是当前问题的另一种包装。

### 21.4 每个 source 建一个 urltest，再建全局 urltest

暂不采纳。嵌套 group 增加状态解析、测速调度和统计归因复杂度。确定性 round-robin 已能解决首版公平问题。

### 21.5 前端测试全部节点后自行选最低延迟

不采纳。会复制 sing-box tolerance、alive、历史与切换语义，并产生前后端竞态。

### 21.6 保留 `proxy.selector_mode` 作为默认值

不采纳。开发期没有必要维护 TOML 默认值与私有 replay state 两个真相源。默认 auto 可以由缺省 selection state直接定义。

### 21.7 立即迁移到 sing-box `services.api` gRPC 控制面

暂不采纳。`v1.14.0-beta.13` 已证明 API service 能附着到独立 `sing-box run` 实例，并直接提供 group 订阅、测速、selector 切换、状态、日志和连接流；它不是 libbox/AAR 专属能力，也具备未来替代 Clash API 的技术可行性。

当前不迁移的理由是：

1. NetHop 仍冻结在经过真机验证的 `v1.13.15`，不能把 `v1.14` 预发布接口当成现行契约；
2. 当前同步 Clash HTTP client 已覆盖本文需要的最小控制能力；
3. Rust gRPC client 通常会引入 protobuf 生成、HTTP/2、连接管理和异步 runtime，与本文轻量目标及“不新增异步 runtime”约束冲突；
4. API service 同时支持 gRPC-Web、Dashboard 和跨域配置，采用前必须重新收紧监听与认证边界；
5. 改用 gRPC 不会消除嵌套 group 的递归解析、环检测和 terminal node 映射责任。

重新评估必须同时满足：sing-box `1.14` 已发布稳定版并完成 NetHop pin 升级评审；Clash API 出现明确能力缺口或维护风险；Android arm64 原型证明功能、体积、RSS、空闲 CPU 和恢复时间不劣于当前方案。完整候选记录见 `11-deferred-capabilities-and-future-design.md`。

## 22. 源码与官方依据

| 证据 | 结论 |
|---|---|
| `refer/sing-box-v1.13.15/docs/configuration/outbound/urltest.zh.md` | URLTest 默认 `interval=3m`、`tolerance=50ms`、`idle_timeout=30m`，并支持 `interrupt_exist_connections` |
| `refer/sing-box-v1.13.15/docs/configuration/outbound/selector.zh.md` | selector 和连接中断字段的 `v1.13.15` 文档语义 |
| `refer/sing-box-v1.13.15/daemon/started_service.proto` | `v1.13.15` 已包含图形客户端使用的 group、URLTest 和 selector protobuf 方法 |
| `refer/sing-box-1.14.0-beta.13/docs/configuration/service/api.zh.md` | `v1.14.0` 起普通配置可启用 sing-box API gRPC 服务 |
| `refer/sing-box-1.14.0-beta.13/service/api/server.go` | API service 创建 attached service，并在独立核心进程中启动 gRPC/gRPC-Web listener |
| `refer/sing-box-1.14.0-beta.13/daemon/attached_service.go` | `NewAttachedService` 绑定当前运行实例，不要求 libbox/AAR |
| `refer/sing-box-1.14.0-beta.13/daemon/started_service.proto` | `SubscribeGroups`、`URLTest`、`SelectOutbound` 等公开契约 |
| `crates/nethopd/src/process.rs`、`crates/nethop-core/src/composer.rs`、`crates/nethopd/src/clash_api.rs` | NetHop 当前是独立子进程 + loopback Clash API，不链接 libbox |

官方网页：

- [sing-box API service](https://sing-box.sagernet.org/configuration/service/api/)
- [Selector outbound](https://sing-box.sagernet.org/configuration/outbound/selector/)
- [URLTest outbound](https://sing-box.sagernet.org/configuration/outbound/urltest/)
- [sing-box v1.14.0-beta.13 release](https://github.com/SagerNet/sing-box/releases/tag/v1.14.0-beta.13)

## 23. 最终结论

NetHop 应继续保留“在 sing-box 之前解析、合并和去重订阅”的架构，但必须把用户语义显式化：默认 single，按需启用 merge；single 只在一个订阅内优选，merge 才允许跨订阅全局优选。

节点选择不再由 TOML 与 runtime 共同控制。daemon 私有状态只保存 auto/manual intent，sing-box 提供实际 active child，前端同时展示两者。合并模式采用确定性来源轮询构造有界 auto pool，既保持轻量，也避免大订阅垄断候选。

这次重构以 schema v3 和 Protocol v3 直接替换含混旧契约，不实现开发期兼容层；通过前后行为映射、分层 TDD、失败注入和 Android 真机验证，确保新增 single/merge、auto/manual 往返与真实 active node 可观测性，同时保持订阅解析、代理启动、回滚、应用范围和网络接管等既有能力正常工作。
