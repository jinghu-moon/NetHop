# NetHop 订阅选择与节点优选 TDD 开发任务清单

> 测速实施状态：本清单中的多订阅合并、稳定 ID、来源公平池、generation/LKG、manual/auto 用户语义仍是有效输入；其中旧的 sing-box URLTest/group delay、`nethop-auto` 递归解析和 25/30 秒批量测速任务已由 D13/D14 替代。D14 主机阶段的结果不回填为本清单旧 URLTest 证据；真机节点测速/功耗项目必须按 D14 M 阶段重新执行。

> 状态：Implementation in progress（主机阶段完成，真机阶段部分完成）
>
> 上位设计：`docs/10-subscription-selection-and-node-optimization-refactor-design.md`
>
> 未来候选边界：`docs/11-deferred-capabilities-and-future-design.md`
>
> 适用范围：`nethop-subscription`、`nethop-core`、`nethopd`、`nethop-protocol`、`nethopctl`、WebUI、Android arm64 模块
>
> 原则：项目未正式发布，允许破坏性重构；不维护 schema v2、旧 Protocol、旧 WebUI 行为兼容层，但必须以 before/after 测试证明既有有效能力继续正常

## 1. 目的

本文把 `10-subscription-selection-and-node-optimization-refactor-design.md` 拆成可按 TDD 执行的有向无环任务图。每个任务节点只交付一个可验证结果；节点之间要么通过 `depends_on` 形成递进关系，要么通过 `parallel_with` 明确允许并行。

本清单不是功能愿望列表。实现顺序必须从底层模型、纯函数、持久状态和事务开始，再进入 Protocol、CLI、WebUI 和真机消费层。前端不得先模拟尚不存在的后端语义，daemon 不得通过临时字符串或内部 sing-box tag 绕过稳定领域模型。

## 2. 已核验基线

### 2.1 重构前 NetHop 基线（历史快照）

- 本节记录阶段 A 冻结的重构前状态，不代表当前实现。当前用户配置和 Protocol 均为 v3。
- 用户配置当时仍是 `schema_version = 2`；`proxy.selector_mode` 仍存在于 wire model、effective model 和 composer options。
- `ConfigMutation::SelectSource` 会把目标 source 设为 enabled，并关闭其他 source。
- Protocol 当前只有含混的 `ControlMethod::NodeSelect`，参数是通用 `target`。
- `ClashApiClient::nodes()` 直接把内部 tag 当 `id/name`，并返回 `selected: bool`。
- `OperationalControl` 调用 `ClashApiClient::select_node()` 后把 selector target 直接保存到 selector store。
- composer 直接对全部节点执行 `.take(max_candidates)`，尚无来源公平池。
- generation manifest 已保存 `source_ids`，但尚无独立的 generation node registry。
- WebUI `SubscriptionsView` 使用 radio，并在成功后本地拼装“仅一个 source enabled”的乐观快照。
- WebUI `NodeDto` 仍要求 `selected`，尚无独立 selection snapshot。
- WebUI 已有 typed bridge、operation state、event session、shallow store、虚拟列表和 Playwright/Vitest 测试骨架，应复用而不是重建。

### 2.2 sing-box `v1.13.15` 源码约束

- `protocol/group/selector.go` 的 `Now()` 只返回下一层 selected tag，`All()` 只返回直接成员。
- selector 可以选择 urltest group；因此 `selector -> urltest -> terminal` 必须由 NetHop 递归解析。
- `protocol/group/urltest.go` 自己维护当前候选和测速历史，NetHop 不复制 tolerance 选择算法。
- Clash API `/proxies` 与 group delay endpoint 能覆盖当前查询、测速和切换需求。
- `interrupt_exist_connections = false` 只保证节点切换不主动中断已有入站连接；测试必须区分旧连接和新连接。
- `v1.14 services.api` gRPC 不进入本清单实现范围，只作为未来候选登记；当前实现固定为 v1.13.15 独立 sing-box + loopback Clash HTTP API。

### 2.3 本地参考项目取舍

| 项目 | 吸收 | 不吸收 |
|---|---|---|
| sing-box `v1.13.15` | `Now/All/SelectOutbound` 真实语义、group delay、连接保持、固定默认值 | 猜测开发线 API、把内部 tag 暴露给 UI |
| MagicNet | selector canonical fixture、递归解析与 cycle 测试、订阅生命周期失败矩阵、malformed cached config 检查 | shell/JQ 双实现、业务规则散落在生成脚本、隐式修复用户配置 |
| NetProxy-Magisk | 临时文件后原子替换、订阅名称/来源展示、节点来源分组、更新日志分域 | WebUI 直接改文件、目录名充当稳定 ID、无 CAS 的文本替换 |
| NetHop 现有实现 | generation seal/commit/rollback、source registry、LKG、typed IPC、事件流、虚拟列表 | schema v2 兼容层、`selected: bool`、前端乐观伪造 active source |

## 3. 开发期破坏性重构规则

1. 不为 schema v2、旧 `SelectSource`、旧 `NodeSelect` 或旧 node DTO 编写兼容 shim。
2. 删除旧路径前必须先冻结其背后的有效用户行为，而不是冻结错误 wire shape。
3. 每次破坏性节点必须留下 `before fixture + RED + after golden + regression mapping`。
4. 新旧字节不兼容可以接受；单 source 代理、LKG、generation 回滚、TUN/TPROXY、应用范围和路由能力退化不可接受。
5. 新实现通过后删除旧 enum variant、旧字段、旧 helper、旧 fixture 和死 CSS，不保留双路径。
6. 当前 control transport 固定为 loopback Clash API，不引入 protobuf、gRPC、HTTP/2 或异步 runtime。
7. 任务完成不等于提交完成；本文不授权自动执行 `git commit`、`git push` 或删除无关文件。

## 4. TDD 节点规范

每个任务必须按以下顺序执行：

```text
RED       添加一个因目标能力缺失而失败的最小测试
GREEN     只实现让该测试通过的最小生产代码
REFACTOR  消除本节点引入的重复，保持单一职责和窄接口
VERIFY    运行节点测试、直接前驱测试和指定回归门禁
```

任务节点字段：

- `depends_on`：全部完成后才能开始；
- `parallel_with`：前置依赖满足后允许并行的节点；
- `scope`：本节点唯一交付物；
- `RED/GREEN/REFACTOR/VERIFY`：TDD 操作；
- `done`：客观结束条件。

RED 必须因目标行为缺失而失败，不能因路径、拼写、fixture 或环境错误失败。手工真机检查不能替代自动测试，但真机专属能力必须保存受控证据。

## 5. 测试分层与证据

| 层级 | 建议工具 | 只负责 |
|---|---|---|
| Rust 单元 | `cargo test` 相邻模块 | single/merge 不变量、fair pool、递归解析、journal 状态机 |
| Rust 契约 | crate `tests/*_contracts.rs` | config、generation、Protocol、CLI、Clash API、失败恢复 |
| property/fuzz | `proptest`/现有 fuzz 工具 | 公平性、确定性、循环、边界、序列化恢复 |
| 前端 Node | Vitest Node | DTO、planner、operation、event reducer、view-model |
| Vue 浏览器 | Vitest Browser Mode | radio/checkbox、pending、卡片状态、事件更新、虚拟列表 |
| 应用 E2E | Playwright | single/merge、auto/manual、导航、失败保持、截图 |
| 模块契约 | PowerShell + Rust | 默认 TOML、ZIP、manifest、checksums、无 gRPC 依赖 |
| Android 真机 | ADB 脚本 + 人工记录 | TPROXY/TUN、真实 core API、旧连接保持、功耗与回滚 |

建议证据目录：

```text
artifacts/tdd-selection/<task-id>/
  red.txt
  green.txt
  refactor.txt
  verify.txt
  manifest.json
```

`manifest.json` 至少记录 task ID、设计章节、测试路径、命令、退出码、fixture SHA-256、Git worktree revision、Rust/Cargo/Node 版本、feature set 和 Android 设备摘要。禁止记录真实订阅 URL、token、UUID、密码、Reality key、完整 outbound、API secret 或设备完整包列表。

## 6. 需求追踪

| 需求域 | D10 来源 | 任务阶段 |
|---|---|---|
| before baseline 与破坏性护栏 | 17.1、17.6 | A |
| schema v3、single/merge、selection model | 5、6 | B |
| source active set 与来源追踪 | 5.2-5.4、6.1、6.4 | C |
| fair auto pool | 8 | D |
| composer 与 generation node registry | 6.4、8.3、9.1 | E |
| source/mode 原子事务与 journal | 7、16 | F |
| selection intent 与 active terminal | 6.2-6.3、10 | G |
| Clash API 与 operational control | 2.1、9、10 | H |
| Protocol、CLI、事件 | 11、13 | I |
| WebUI DTO、store、bridge | 12.4-12.5 | J |
| 订阅页 | 12.1 | K |
| 节点页与概览 | 12.2-12.3 | L |
| E2E、安全、性能、回归 | 14-17 | M |
| 模块、真机、文档与完成门禁 | 18-20 | N |

## 7. 依赖总图与推荐顺序

```mermaid
flowchart LR
  A[A 基线与护栏] --> B[B schema v3 与领域模型]
  A --> C0[C fixture 与来源基线]
  B --> C[C source active set]
  C0 --> D[D fair pool]
  C --> D
  D --> E[E composer 与 node registry]
  E --> F[F 原子事务与 journal]
  E --> G[G selection 与 active 解析]
  F --> H[H Clash API 与 operational control]
  G --> H
  H --> I[I Protocol/CLI/事件]
  I --> J[J WebUI DTO/store/bridge]
  J --> K[K 订阅页]
  J --> L[L 节点页与概览]
  K --> M[M E2E/性能/安全]
  L --> M
  M --> N[N 模块/真机/完成门禁]
```

- A 中 baseline fixture 可并行采集，但 A gate 串行收口。
- B 的 config model 与 selection model 可并行；C 必须等待 mode model 稳定。
- D 的纯函数、property 和 benchmark fixture 可并行。
- F 的 journal 与 G 的递归解析在 E gate 后可并行，H 同时依赖两者。
- K 与 L 在 J gate 后可并行；共同写操作只能复用同一 operation/CAS 状态机。
- M 的静态安全、性能 fixture 和 E2E 场景可并行；N 真机 gate 必须等待 M 完成。

## 8. 阶段 A：基线、测试骨架与重构护栏

- [x] **A001 - 冻结 B01 单 source 代理闭环**
  - `depends_on`: none；`parallel_with`: A002,A003,A004,A005
  - `scope`: 保存单 source 下载、解析、compose、check、generation publish 和 core start 的脱敏 before fixture。
  - `RED/GREEN`: 完整性测试先因缺少链路步骤失败；补齐现有行为证据，不改生产代码。
  - `REFACTOR/VERIFY`: 复用现有 subscription candidate probe；运行对应 daemon/core 契约。
  - `done`: fixture 能独立证明 B01 当前通过并绑定 digest。

- [x] **A002 - 冻结 B02-B04 parser/LKG/generation 行为**
  - `depends_on`: none；`parallel_with`: A001,A003,A004,A005
  - `scope`: 冻结多 source 去重与 attribution、单源 LKG、check 失败保留旧 runtime。
  - `RED/GREEN`: 映射测试先暴露缺少的 before case；只补 fixture 和断言。
  - `REFACTOR/VERIFY`: 共享 source builder 和 generation snapshot；运行 subscription/core 回归。
  - `done`: B02-B04 每项都有正向和失败 golden。

- [x] **A003 - 冻结 B05-B07 测速、手选和连接保持**
  - `depends_on`: none；`parallel_with`: A001,A002,A004,A005
  - `scope`: 保存 test-all、有界结果、手动选择和旧连接不被中断的 before 行为。
  - `RED/GREEN`: 缺少连接保持样本时测试失败；补 fake Clash API 和连接 probe fixture。
  - `REFACTOR/VERIFY`: 统一 API response builder；运行 clash/operational control tests。
  - `done`: 能区分切换前旧连接与切换后新连接。

- [x] **A004 - 冻结 B08-B09 安全与数据面回归**
  - `depends_on`: none；`parallel_with`: A001,A002,A003,A005
  - `scope`: 冻结 secret canary、服务启停、TUN/TPROXY、应用范围和路由模式行为。
  - `RED/GREEN`: canary 或数据面矩阵缺项时失败；只补测试映射。
  - `REFACTOR/VERIFY`: 复用现有 security scan 和 worker tests。
  - `done`: B08-B09 均有不泄密和不退化证据。

- [x] **A005 - 冻结 B10 WebUI 导航和旧缺陷样本**
  - `depends_on`: none；`parallel_with`: A001,A002,A003,A004
  - `scope`: 保存概览到订阅/节点导航，并单独记录 radio 单选、first-node fallback、无法切 auto 等缺陷。
  - `RED/GREEN`: baseline manifest 对未分类行为失败；把有效行为和待删除缺陷分栏记录。
  - `REFACTOR/VERIFY`: 复用 MockHost 和现有 Playwright fixture。
  - `done`: 后续不会把旧缺陷误当兼容目标。

- [x] **A006 - 冻结 schema v2 与旧 Protocol wire**
  - `depends_on`: A001,A002,A003,A004,A005；`parallel_with`: A007
  - `scope`: 保存 v2 TOML、`SelectSource`、`NodeSelect`、旧 `node.list` 与 node `selected` golden。
  - `RED/GREEN`: wire inventory 缺项时失败；补全 before fixture，不修改枚举。
  - `REFACTOR/VERIFY`: 使用统一 request/envelope builder；运行 protocol/CLI tests。
  - `done`: 旧字节可用于证明 v3 的明确拒绝和功能映射。

- [x] **A007 - 建立本重构 TDD 证据 manifest**
  - `depends_on`: A001,A002,A003,A004,A005；`parallel_with`: A006
  - `scope`: 定义 task、命令、fixture digest、before/after mapping 和脱敏字段。
  - `RED/GREEN`: 非法 manifest golden 先被错误接受；实现测试侧 validator。
  - `REFACTOR/VERIFY`: 复用现有 WebUI/parser 证据字段，不进入 release binary。
  - `done`: 缺 RED/GREEN/VERIFY 或出现 secret 字段均失败。

- [x] **A008 - 建立重构 baseline gate**
  - `depends_on`: A006,A007；`parallel_with`: none
  - `scope`: 聚合 B01-B10、旧 wire、secret canary 和当前构建状态。
  - `RED/GREEN`: 任一 baseline 缺失时 gate 失败；只连接既有测试与 fixture。
  - `REFACTOR/VERIFY`: 输出机器可读 manifest；运行 workspace 与 WebUI baseline。
  - `done`: A gate 全绿，后续任务有可比较起点。

## 9. 阶段 B：Schema v3 与公共领域模型

- [x] **B001 - 将唯一接受的配置版本提升为 v3**
  - `depends_on`: A008；`parallel_with`: B002,B007,B008
  - `scope`: `CONFIG_SCHEMA_VERSION = 3`，v2 和其他版本返回 `UnsupportedSchema`。
  - `RED/GREEN`: v3 被拒绝且 v2 被接受；最小修改版本门禁。
  - `REFACTOR/VERIFY`: 删除 v2 解析分支；运行 config contracts。
  - `done`: 只接受 v3，错误码稳定。

- [x] **B002 - 增加 `SubscriptionMode` wire 与领域枚举**
  - `depends_on`: A008；`parallel_with`: B001,B007,B008
  - `scope`: 只支持 `single|merge`，默认 `single`。
  - `RED/GREEN`: mode 缺失/未知的预期不成立；实现最小 serde 与默认值。
  - `REFACTOR/VERIFY`: wire/domain 转换集中一处；运行 round-trip golden。
  - `done`: canonical TOML 固定输出 mode。

- [x] **B003 - 实现 single 模式配置不变量**
  - `depends_on`: B001,B002；`parallel_with`: B004,B005,B006
  - `scope`: 覆盖零 URL、唯一 enabled、多 enabled、配置 URL 但零 enabled、enabled 空 URL。
  - `RED/GREEN`: table test 暴露错误接受；实现最小 validator。
  - `REFACTOR/VERIFY`: 以数据表消除分支测试重复。
  - `done`: D10 5.2 全部边界有稳定诊断。

- [x] **B004 - 实现 merge 模式配置不变量**
  - `depends_on`: B001,B002；`parallel_with`: B003,B005,B006
  - `scope`: 至少一个有效 enabled source，允许多个 enabled，拒绝全部无效。
  - `RED/GREEN`: merge 边界 table test 失败；实现窄 validator。
  - `REFACTOR/VERIFY`: 复用 source configured 判定，不复用 single 唯一性逻辑。
  - `done`: D10 5.3 配置入口规则通过。

- [x] **B005 - 从 TOML 删除 `proxy.selector_mode`**
  - `depends_on`: B001,B002；`parallel_with`: B003,B004,B006
  - `scope`: wire、effective config、canonical serializer 和 schema metadata 均无该字段。
  - `RED/GREEN`: v3 fixture 含 selector_mode 仍被接受；删除字段和转换。
  - `REFACTOR/VERIFY`: 删除 `SelectorMode` 配置类型及未使用 getter。
  - `done`: 未知字段诊断拒绝 selector_mode，代码无配置路径引用。

- [x] **B006 - 拒绝用户配置 source/node/internal tag ID**
  - `depends_on`: B001,B002；`parallel_with`: B003,B004,B005
  - `scope`: TOML 不接受 source ID、node ID、selection state 或 sing-box tag。
  - `RED/GREEN`: 注入字段被忽略或接受；保持 deny_unknown_fields 并补稳定诊断。
  - `REFACTOR/VERIFY`: 集中 forbidden field fixtures。
  - `done`: 用户文件只含名称、链接和公开配置。

- [x] **B007 - 定义 `NodeSelectionIntent`**
  - `depends_on`: A008；`parallel_with`: B001,B002,B008
  - `scope`: 仅有 `Auto` 与 `Manual { StableNodeId }`。
  - `RED/GREEN`: 无法表达 auto/manual；实现最小领域枚举和 ID 校验。
  - `REFACTOR/VERIFY`: 不携带 internal tag 或 outbound 数据。
  - `done`: unit round-trip 和非法 ID 测试通过。

- [x] **B008 - 定义 `NodeSelectionSnapshot`**
  - `depends_on`: A008；`parallel_with`: B001,B002,B007
  - `scope`: version、intent、active node、changed_at，区分用户意图和 core 事实。
  - `RED/GREEN`: 旧 selector string 无法表达快照；实现 bounded snapshot。
  - `REFACTOR/VERIFY`: 时间与稳定 ID 使用共享值对象。
  - `done`: auto active 可为空，manual intent 不被 active 覆盖。

- [x] **B009 - 定义新 node list/selection DTO**
  - `depends_on`: B007,B008；`parallel_with`: B010
  - `scope`: 删除 `selected`，增加 `is_requested/is_active` 和顶层 selection。
  - `RED/GREEN`: D10 golden 无法反序列化；实现 DTO 类型和 bounds。
  - `REFACTOR/VERIFY`: node 与 selection 类型分离，来源 ID 有界。
  - `done`: auto group 不会被序列化成 node。

- [x] **B010 - 增加新诊断码集合**
  - `depends_on`: B002,B007；`parallel_with`: B009
  - `scope`: 增加 D10 第 13 节的 subscription/node 诊断码。
  - `RED/GREEN`: code mapping 缺项；实现枚举与稳定字符串。
  - `REFACTOR/VERIFY`: message 与 code 分离，CLI/UI 不匹配英文文本。
  - `done`: 所有新增码有唯一含义和 serialization test。

- [x] **B011 - 生成 canonical schema v3 默认配置**
  - `depends_on`: B003,B004,B005,B006；`parallel_with`: B012
  - `scope`: 默认 single、10m/50ms/64/10，不含 daemon-owned ID 和 selector_mode。
  - `RED/GREEN`: golden 与设计不一致；更新 serializer/default fixture。
  - `REFACTOR/VERIFY`: 默认值只由 config model 单一来源提供。
  - `done`: parse→serialize→parse 稳定且 digest 可重放。

- [x] **B012 - 通过 schema/domain gate**
  - `depends_on`: B009,B010,B011；`parallel_with`: none
  - `scope`: 聚合 v3、mode、selection、DTO、诊断和旧字段拒绝。
  - `RED/GREEN`: 任一契约缺失时 gate 失败；只汇总已有测试。
  - `REFACTOR/VERIFY`: 运行 nethopd config 与 protocol model tests。
  - `done`: B gate 全绿，无 v2 接受路径。

## 10. 阶段 C：Source active set 与来源追踪

- [x] **C001 - 为 `SourceConfig` 提供 `configured_sources()`**
  - `depends_on`: B012；`parallel_with`: C002,C005
  - `scope`: 唯一封装“URL 非空且配置有效”的迭代入口。
  - `RED/GREEN`: 调用方各自过滤导致边界不一致；实现窄 iterator。
  - `REFACTOR/VERIFY`: 删除重复 URL 非空过滤。
  - `done`: configured 定义只有一个生产实现。

- [x] **C002 - 为 single 实现 `active_sources()`**
  - `depends_on`: B012；`parallel_with`: C001,C005
  - `scope`: 返回唯一 active source 或稳定错误。
  - `RED/GREEN`: 零/多 enabled 被静默处理；实现 single 分支。
  - `REFACTOR/VERIFY`: 复用 B003 不变量，不二次猜测。
  - `done`: generation 调用方无需自行过滤 enabled。

- [x] **C003 - 为 merge 实现 `active_sources()`**
  - `depends_on`: C001,C002；`parallel_with`: C004,C006
  - `scope`: 按 TOML 顺序返回所有 enabled configured source。
  - `RED/GREEN`: merge 只返回第一个 source；实现有序 iterator。
  - `REFACTOR/VERIFY`: single/merge 共用同一公开入口。
  - `done`: active set 顺序稳定且不赋予隐藏权重。

- [x] **C004 - 保持 daemon-owned source ID 稳定**
  - `depends_on`: C001,C002；`parallel_with`: C003,C006
  - `scope`: mode/enabled/order 变化不改变名称+URL 对应的 registry ID。
  - `RED/GREEN`: 重排或切换导致 ID 重建；修正 registry reconciliation。
  - `REFACTOR/VERIFY`: 使用现有 SourceRegistry checkpoint。
  - `done`: add/update/move/mode change ID fixture 稳定。

- [x] **C005 - 定义节点来源 attribution 值对象**
  - `depends_on`: B012；`parallel_with`: C001,C002
  - `scope`: 每个稳定节点保存有序去重的 `source_ids`，上限 16。
  - `RED/GREEN`: 跨 source duplicate 丢失来源；实现 bounded attribution。
  - `REFACTOR/VERIFY`: 不保存 source URL/name 副本。
  - `done`: 同节点多来源只增加 ID，不增加凭据副本。

- [x] **C006 - 在合并去重中聚合全部来源**
  - `depends_on`: C003,C005；`parallel_with`: C004,C007
  - `scope`: fingerprint 相同的节点只生成一个 terminal candidate，并合并来源。
  - `RED/GREEN`: duplicate 产生多个节点或只保留首来源；修改聚合器。
  - `REFACTOR/VERIFY`: 复用 parser fingerprint，不二次计算连接语义。
  - `done`: B02 attribution after fixture 通过。

- [x] **C007 - 保持 source 顺序只用于确定性 tie-break**
  - `depends_on`: C003,C005；`parallel_with`: C004,C006
  - `scope`: 来源顺序变化只影响明确的排序/轮询，不改变 fingerprint 或 ID。
  - `RED/GREEN`: 重排导致 node ID 变化；移除来源顺序对 fingerprint 的污染。
  - `REFACTOR/VERIFY`: 增加 permutation fixture。
  - `done`: 相同节点集合的 stable node IDs 不变。

- [x] **C008 - 定义非活动 source 更新结果语义**
  - `depends_on`: C003,C004,C006；`parallel_with`: C009
  - `scope`: 非活动 source 更新只更新 LKG/status，不进入 generation 输入。
  - `RED/GREEN`: 更新非活动 source 触发 generation；收口 active_sources 使用点。
  - `REFACTOR/VERIFY`: scheduler/manual update 共享判定。
  - `done`: generation ID、selection 和 runtime 保持不变。

- [x] **C009 - 增加 source active set snapshot**
  - `depends_on`: C003,C004,C006；`parallel_with`: C008
  - `scope`: typed snapshot 包含 mode、active IDs、ordered sources 与 config digest。
  - `RED/GREEN`: UI/transaction 只能重新解析 TOML；实现只读快照。
  - `REFACTOR/VERIFY`: 不包含订阅 URL，名称按现有脱敏策略处理。
  - `done`: Protocol 层可直接消费，无文本解析。

- [x] **C010 - 通过 source domain gate**
  - `depends_on`: C007,C008,C009；`parallel_with`: none
  - `scope`: 聚合 configured/active、ID、attribution、LKG 与 snapshot。
  - `RED/GREEN`: 缺任一 source 语义时失败；只连接测试。
  - `REFACTOR/VERIFY`: 运行 source_config/source_update/manual_source tests。
  - `done`: C gate 全绿，generation 输入集合只有一个入口。

## 11. 阶段 D：Fair auto candidate pool

- [x] **D001 - 实现 single 模式稳定候选选择**
  - `depends_on`: C010；`parallel_with`: D002,D005,D007
  - `scope`: 按稳定 node ID 排序后取前 `max_candidates`。
  - `RED/GREEN`: 输入顺序改变输出；实现纯函数排序与截断。
  - `REFACTOR/VERIFY`: 不读取全局配置或 I/O。
  - `done`: permutation 测试输出一致。

- [x] **D002 - 实现 merge round-robin 候选选择**
  - `depends_on`: C010；`parallel_with`: D001,D005,D007
  - `scope`: 按 source 顺序逐轮取每源下一个稳定节点。
  - `RED/GREEN`: 大 source 垄断 pool；实现确定性轮询。
  - `REFACTOR/VERIFY`: 算法状态局部化，不引入权重抽象。
  - `done`: 容量允许时每个非空 source 至少贡献一个候选。

- [x] **D003 - 处理跨来源 duplicate 不占重复候选位**
  - `depends_on`: D002；`parallel_with`: D004,D006
  - `scope`: shared node 只入池一次，同时推进对应 source 游标。
  - `RED/GREEN`: duplicate 消耗多个位置或造成死循环；实现 visited set。
  - `REFACTOR/VERIFY`: visited 使用 stable node ID，不使用显示名称。
  - `done`: 50% duplicate fixture 达到期望容量。

- [x] **D004 - 处理空 source 与过滤后空 source**
  - `depends_on`: D002；`parallel_with`: D003,D006
  - `scope`: 跳过空队列，不阻塞其他来源，不制造 placeholder。
  - `RED/GREEN`: 空 source 导致提前结束；修正轮询终止条件。
  - `REFACTOR/VERIFY`: 空/耗尽共享同一状态。
  - `done`: 混合空源 fixture 正确终止。

- [x] **D005 - 冻结 `max_candidates` 边界**
  - `depends_on`: C010；`parallel_with`: D001,D002,D007
  - `scope`: 1、16、64、256 及 source 数大于容量的行为。
  - `RED/GREEN`: 边界越界或隐式扩容；实现有界输出。
  - `REFACTOR/VERIFY`: 复用 config limits 常量。
  - `done`: 输出长度永不超过配置上限。

- [x] **D006 - 冻结 source 重排后的确定性顺序**
  - `depends_on`: D002；`parallel_with`: D003,D004
  - `scope`: 同一 source/node 顺序得到同一 pool；重排产生可预测新顺序。
  - `RED/GREEN`: HashMap 遍历导致非确定；使用有序输入和稳定 ID。
  - `REFACTOR/VERIFY`: golden 记录 source contribution。
  - `done`: 重复运行和不同进程 seed 输出一致。

- [x] **D007 - 分离 auto pool 与完整 manual pool**
  - `depends_on`: C010；`parallel_with`: D001,D002,D005
  - `scope`: 未进入 auto 的有效节点仍保留为手选候选。
  - `RED/GREEN`: max_candidates 截断全部节点；返回两个明确集合。
  - `REFACTOR/VERIFY`: 只共享节点记录，不复制 outbound。
  - `done`: manual 集合覆盖全部有效去重节点。

- [x] **D008 - 增加 fair pool property tests**
  - `depends_on`: D003,D004,D005,D006,D007；`parallel_with`: D009
  - `scope`: 确定性、有界、唯一、公平、终止五个性质。
  - `RED/GREEN`: 生成输入找到反例；修正算法而非缩小生成域。
  - `REFACTOR/VERIFY`: 固定失败 seed 进入 regression fixture。
  - `done`: 10,000 组生成样本无反例。

- [x] **D009 - 建立 10,000 节点 fair pool benchmark**
  - `depends_on`: D003,D004,D005,D006,D007；`parallel_with`: D008
  - `scope`: D10 第 15 节五类分布，P95 <= 10ms，额外峰值 <= 4MiB。
  - `RED/GREEN`: baseline 超门槛或缺场景；只优化 profile 证明的热点。
  - `REFACTOR/VERIFY`: 固定 seed、release profile 和 digest。
  - `done`: 三轮独立样本记录 p50/p95/peak。

- [x] **D010 - 通过 fair pool gate**
  - `depends_on`: D008,D009；`parallel_with`: none
  - `scope`: 聚合 single、merge、duplicate、边界、property 和性能。
  - `RED/GREEN`: 任一性质或预算失败时阻断；不添加业务代码。
  - `REFACTOR/VERIFY`: 输出 contribution manifest。
  - `done`: D gate 全绿。

## 12. 阶段 E：Core composer 与 generation node registry

- [x] **E001 - 从 composer 删除 `ManagedSelectorMode`**
  - `depends_on`: D010；`parallel_with`: E002,E005
  - `scope`: composer 不再从 TOML 决定 auto/manual，顶层 selector 默认始终指向 `nethop-auto`。
  - `RED/GREEN`: composer 仍要求 selector mode；删除参数和分支。
  - `REFACTOR/VERIFY`: 更新所有构造器和 fixture，删除 enum/export。
  - `done`: `proxy.selector_mode` 在 core 与 daemon 无生产引用。

- [x] **E002 - 让 `ManagedProfile` 显式接收 auto pool**
  - `depends_on`: D010；`parallel_with`: E001,E005
  - `scope`: terminal outbounds 与 auto candidate stable IDs/tag 集合分离传入。
  - `RED/GREEN`: composer 仍自行 `.take(max_candidates)`；增加受验证参数。
  - `REFACTOR/VERIFY`: auto pool 只由 D 阶段纯函数生成。
  - `done`: composer 不包含来源公平算法。

- [x] **E003 - 冻结 canonical `nethop-auto` urltest**
  - `depends_on`: E001,E002；`parallel_with`: E004,E008,E009
  - `scope`: 受控 204 URL、10m、50ms、30m、`interrupt_exist_connections=false`。
  - `RED/GREEN`: golden 缺字段或值漂移；输出固定 urltest。
  - `REFACTOR/VERIFY`: 默认值来自 typed options，不复制 magic number。
  - `done`: 与 sing-box v1.13.15 check fixture一致。

- [x] **E004 - 冻结 canonical `nethop-select` selector**
  - `depends_on`: E001,E002；`parallel_with`: E003,E008,E009
  - `scope`: 成员为 `nethop-auto` + 全部 manual terminal，默认 auto，连接不中断。
  - `RED/GREEN`: selector 缺 manual 节点或默认首节点；修正输出。
  - `REFACTOR/VERIFY`: 去重成员并校验 default 属于 members。
  - `done`: auto pool 截断不影响 manual candidates。

- [x] **E005 - 定义 `GenerationNodeRecord`**
  - `depends_on`: D010；`parallel_with`: E001,E002
  - `scope`: stable node ID、internal tag、显示名、协议、source IDs、auto flag。
  - `RED/GREEN`: generation 无法完成双向映射；实现最小记录类型。
  - `REFACTOR/VERIFY`: 显示名脱敏/限长，禁止 outbound 凭据字段。
  - `done`: record schema 有 deny-unknown 与 bounds 测试。

- [x] **E006 - 生成 generation node registry**
  - `depends_on`: E005；`parallel_with`: E003,E004
  - `scope`: compose candidate 时同时产生有序 registry。
  - `RED/GREEN`: config 与 registry 节点不一致；由同一 candidate 输入生成。
  - `REFACTOR/VERIFY`: 不从生成后 JSON 反向猜测业务数据。
  - `done`: 每个 terminal outbound 有且仅有一条 record。

- [x] **E007 - 封存并验证 node registry**
  - `depends_on`: E006；`parallel_with`: E008,E009
  - `scope`: registry 与 config/manifest 一起 seal，写入 digest，使用私有权限。
  - `RED/GREEN`: 篡改 registry 仍可 reopen；扩展 generation verification。
  - `REFACTOR/VERIFY`: 复用 generation 文件权限和 digest helper。
  - `done`: 缺失、篡改、symlink 和权限异常均拒绝激活。

- [x] **E008 - 增加 single generation golden**
  - `depends_on`: E003,E004,E006；`parallel_with`: E007,E009
  - `scope`: 单 source、多节点、auto 截断、全部 manual、来源 metadata。
  - `RED/GREEN`: golden 不符合 D10；修正 profile/registry 组合。
  - `REFACTOR/VERIFY`: 使用确定性 stable IDs 和固定 secret canary。
  - `done`: config、manifest、registry 三份 golden 对齐。

- [x] **E009 - 增加 merge generation golden**
  - `depends_on`: E003,E004,E006；`parallel_with`: E007,E008
  - `scope`: 多 source fair pool、duplicate attribution、manual 完整集合。
  - `RED/GREEN`: 前置大 source 挤占或来源丢失；接入 D 阶段结果。
  - `REFACTOR/VERIFY`: single/merge 共享 composer，不复制 JSON 构造。
  - `done`: contribution 与 registry auto flag 一致。

- [x] **E010 - 用真实 `v1.13.15 sing-box check` 验证 golden**
  - `depends_on`: E007,E008,E009；`parallel_with`: none
  - `scope`: single/merge config 均通过真实二进制 check，错误 config 被拒绝。
  - `RED/GREEN`: check 失败；只修正允许字段和 group 结构。
  - `REFACTOR/VERIFY`: 使用现有 `SingBoxCheckRunner`，限制输出和超时。
  - `done`: check 输出脱敏，版本身份记录为 v1.13.15。

- [x] **E011 - 通过 composer/registry gate**
  - `depends_on`: E010；`parallel_with`: none
  - `scope`: 聚合 canonical groups、registry integrity、single/merge golden 和真实 check。
  - `RED/GREEN`: 任一漂移阻断；只连接测试。
  - `REFACTOR/VERIFY`: 运行 nethop-core 与 runner contracts。
  - `done`: E gate 全绿。

## 13. 阶段 F：Source/mode 原子事务与 commit journal

- [x] **F001 - 定义 root-only commit journal schema**
  - `depends_on`: E011；`parallel_with`: F002,F003,F005
  - `scope`: old/new config digest、generation、阶段和 staged path，不含 URL/凭据。
  - `RED/GREEN`: crash 无恢复证据或 journal 泄密；实现版本化 schema。
  - `REFACTOR/VERIFY`: 阶段枚举单向推进，deny unknown fields。
  - `done`: round-trip、权限、size 与 canary tests 通过。

- [x] **F002 - 将配置与 generation mutation 收口到单一写锁**
  - `depends_on`: E011；`parallel_with`: F001,F003,F005
  - `scope`: config apply、source transaction、scheduler publish 共用同一串行入口。
  - `RED/GREEN`: 并发测试出现交错提交；引入或复用唯一 mutation lock。
  - `REFACTOR/VERIFY`: 锁不包围网络下载，只保护 admission/commit。
  - `done`: 同时最多一个 commit transaction。

- [x] **F003 - 实现 single `SubscriptionSelect` candidate planner**
  - `depends_on`: E011；`parallel_with`: F001,F002,F005
  - `scope`: 校验模式、digest、目标 source 和 fresh/LKG，生成但不提交 candidate。
  - `RED/GREEN`: 旧实现先改 enabled；改为纯 plan/stage。
  - `REFACTOR/VERIFY`: planner 不写 TOML、pointer 或 selection state。
  - `done`: 失败时所有持久状态字节不变。

- [x] **F004 - 实现 single `SubscriptionSelect` 原子提交**
  - `depends_on`: F001,F002,F003；`parallel_with`: F006,F007
  - `scope`: journal、generation pointer、canonical TOML、commit marker 的受控顺序。
  - `RED/GREEN`: 中途失败产生 runtime/config 分裂；实现 staged commit。
  - `REFACTOR/VERIFY`: 使用现有 atomic_write/generation store。
  - `done`: 成功后仅目标 source enabled，runtime 与 digest 对齐。

- [x] **F005 - 实现 merge enable candidate planner**
  - `depends_on`: E011；`parallel_with`: F001,F002,F003
  - `scope`: 目标 source 必须预检或有有效 LKG，重新合并 active set。
  - `RED/GREEN`: 无可用目标仍被 enabled；实现 pre-admission。
  - `REFACTOR/VERIFY`: 复用 active_sources/fair pool/composer。
  - `done`: planner 不改变旧 active set。

- [x] **F006 - 实现 merge enable/disable 原子提交**
  - `depends_on`: F001,F002,F005；`parallel_with`: F004,F007
  - `scope`: enable/disable 后新 generation 与 TOML 同一事务提交，拒绝最后有效 source disable。
  - `RED/GREEN`: candidate 失败后 enabled 已改变；接入 journal commit。
  - `REFACTOR/VERIFY`: enable/disable 共用一条 set-enabled transaction。
  - `done`: `NH-SUB-LAST-ACTIVE` 和成功路径均稳定。

- [x] **F007 - 实现 single -> merge 模式事务**
  - `depends_on`: F001,F002；`parallel_with`: F004,F006
  - `scope`: 保留当前 source enabled，不自动启用其他 source。
  - `RED/GREEN`: 切换模式触发额外下载或全启用；实现最小 mode plan。
  - `REFACTOR/VERIFY`: 无 generation 输入变化时仍产生正确 config digest/事件计划。
  - `done`: active generation 可保持，mode 与 TOML 原子更新。

- [x] **F008 - 实现 merge -> single 目标选择事务**
  - `depends_on`: F004,F006,F007；`parallel_with`: F009
  - `scope`: 多 active 时强制 target source，不静默选第一个。
  - `RED/GREEN`: 缺 target 仍成功；返回 `NH-SUB-MODE-TARGET-REQUIRED`。
  - `REFACTOR/VERIFY`: 目标验证复用 single planner。
  - `done`: 新 generation 成功后才关闭其他 source。

- [x] **F009 - 在所有事务入口执行 config digest CAS**
  - `depends_on`: F004,F006,F007；`parallel_with`: F008
  - `scope`: mode/select/set-enabled 都校验 observed digest 与磁盘状态。
  - `RED/GREEN`: stale caller 覆盖新配置；统一 conflict admission。
  - `REFACTOR/VERIFY`: 不在各事务复制 digest 逻辑。
  - `done`: conflict 时不创建 staged generation/journal。

- [x] **F010 - 增加事务阶段失败注入器**
  - `depends_on`: F004,F006,F008,F009；`parallel_with`: F011,F012
  - `scope`: stage、check、seal、journal、pointer、TOML、commit marker 每阶段可注入失败。
  - `RED/GREEN`: 无法验证断电点；实现仅测试可用的窄 hook。
  - `REFACTOR/VERIFY`: release 构建无动态故障开关。
  - `done`: 每个阶段至少一条 deterministic failure test。

- [x] **F011 - 实现 generation 发布前 journal 恢复**
  - `depends_on`: F010；`parallel_with`: F012,F013
  - `scope`: 未发布 pointer 时丢弃 staged config/TOML，保留旧状态。
  - `RED/GREEN`: daemon restart 后残留 candidate 被误激活；实现幂等恢复。
  - `REFACTOR/VERIFY`: 多次恢复结果相同。
  - `done`: staged 文件清理且旧 runtime 可启动。

- [x] **F012 - 实现 generation 已发布但 TOML 未提交的恢复**
  - `depends_on`: F010；`parallel_with`: F011,F013
  - `scope`: 根据 journal 完成 canonical TOML commit，再标记 committed。
  - `RED/GREEN`: pointer/config 分裂后回到错误 generation；实现 forward recovery。
  - `REFACTOR/VERIFY`: 校验 staged TOML digest 后才写入。
  - `done`: 重启后 pointer、TOML、registry、selection 一致。

- [x] **F013 - 统一活动与非活动 source 更新事务**
  - `depends_on`: F010；`parallel_with`: F011,F012
  - `scope`: 活动 source 成功才发布；非活动 source 只更新 LKG/status。
  - `RED/GREEN`: 非活动更新触发 generation或活动失败覆盖 LKG pointer；修正更新编排。
  - `REFACTOR/VERIFY`: scheduler/manual update 共用 transaction service。
  - `done`: D10 7.3 正负路径通过。

- [x] **F014 - 串行化 source update、mode change 与 generation supersede**
  - `depends_on`: F011,F012,F013；`parallel_with`: none
  - `scope`: 并发到达按 mutation lock/CAS 明确成功一方，另一方 conflict/superseded。
  - `RED/GREEN`: 两个事务都报告成功或互相覆盖；实现序列与 guard。
  - `REFACTOR/VERIFY`: 使用 barrier 驱动确定性并发测试。
  - `done`: 最终状态对应唯一 committed journal。

- [x] **F015 - 通过 transaction/recovery gate**
  - `depends_on`: F014；`parallel_with`: none
  - `scope`: 聚合 single/merge/mode/CAS/failure/recovery/concurrency/LKG。
  - `RED/GREEN`: 任一失败矩阵不满足即阻断；不加业务代码。
  - `REFACTOR/VERIFY`: 运行 config_reconciler/source_update/storage/generation tests。
  - `done`: F gate 全绿且无敏感 journal 字段。

## 14. 阶段 G：Selection intent 与 active terminal 解析

- [x] **G001 - 定义私有 selection store 格式**
  - `depends_on`: E011；`parallel_with`: G002,G005
  - `scope`: versioned `NodeSelectionIntent`，只保存 stable node ID 与时间。
  - `RED/GREEN`: 旧 store 保存内部 tag；实现新格式和私有权限。
  - `REFACTOR/VERIFY`: 不迁移开发期旧格式，旧格式明确拒绝/重置 auto。
  - `done`: store 不含 URL、凭据或 outbound。

- [x] **G002 - 缺省 selection state 定义为 auto**
  - `depends_on`: E011；`parallel_with`: G001,G005
  - `scope`: 文件缺失时返回 Auto，而不是从 TOML 或首节点推断。
  - `RED/GREEN`: 缺文件导致 manual/错误；实现默认行为。
  - `REFACTOR/VERIFY`: 默认值只在 store 一处定义。
  - `done`: fresh install intent 为 auto。

- [x] **G003 - 持久化 manual stable node ID**
  - `depends_on`: G001,G002；`parallel_with`: G004,G006
  - `scope`: 手选成功后原子保存 stable ID，不保存 tag。
  - `RED/GREEN`: 保存 tag 或先存后选；调整 commit 顺序。
  - `REFACTOR/VERIFY`: 复用 atomic_write 与 bounds。
  - `done`: core select 失败时 store 不变。

- [x] **G004 - 实现 daemon/core 重启 replay**
  - `depends_on`: G001,G002；`parallel_with`: G003,G006
  - `scope`: Auto replay `nethop-auto`，Manual 通过当前 registry 映射 tag。
  - `RED/GREEN`: restart 丢失 intent或使用旧 tag；实现 registry-aware replay。
  - `REFACTOR/VERIFY`: replay 不修改 TOML。
  - `done`: generation 未变时 auto/manual 均恢复。

- [x] **G005 - 解析 selector 直接指向 terminal node**
  - `depends_on`: E011；`parallel_with`: G001,G002
  - `scope`: 顶层 `now` 是 registry terminal tag 时返回 stable node ID。
  - `RED/GREEN`: 解析器只识别 group；实现最小终止分支。
  - `REFACTOR/VERIFY`: terminal 判定只信任 registry。
  - `done`: unknown terminal tag 不被展示。

- [x] **G006 - 解析 selector -> urltest -> terminal**
  - `depends_on`: G005；`parallel_with`: G003,G004,G007,G008
  - `scope`: 按 group snapshot 逐层解析真实 active child。
  - `RED/GREEN`: auto 模式 active 为空；实现 bounded recursion。
  - `REFACTOR/VERIFY`: 解析器输入为纯 group map，可单测。
  - `done`: D10 标准嵌套 fixture 返回 terminal stable ID。

- [x] **G007 - 检测 group cycle**
  - `depends_on`: G005；`parallel_with`: G006,G008
  - `scope`: visited tag 重复返回 `NH-NODE-GROUP-CYCLE`。
  - `RED/GREEN`: A->B->A 无限递归或消失；增加 visited set。
  - `REFACTOR/VERIFY`: 参考 MagicNet cycle fixture，但使用 Rust typed graph。
  - `done`: self-loop 和 multi-node loop 均有 degraded snapshot。

- [x] **G008 - 限制 group 深度为 8**
  - `depends_on`: G005；`parallel_with`: G006,G007
  - `scope`: 第 9 层返回 `NH-NODE-GROUP-DEPTH`，不停止代理。
  - `RED/GREEN`: 无界递归；增加显式 depth counter。
  - `REFACTOR/VERIFY`: depth 和 cycle 诊断区分。
  - `done`: 8 层成功、9 层失败 fixture 通过。

- [x] **G009 - 处理 unknown/direct/block 与 API 缺失**
  - `depends_on`: G006,G007,G008；`parallel_with`: G010
  - `scope`: unknown/degraded、direct/block non-proxy outcome、API unavailable。
  - `RED/GREEN`: 错误回退第一个节点；返回结构化 outcome。
  - `REFACTOR/VERIFY`: 展示降级与数据面控制分离。
  - `done`: 任何失败均不伪造 active node。

- [x] **G010 - Join selection intent、active 与 registry DTO**
  - `depends_on`: G006,G007,G008；`parallel_with`: G009
  - `scope`: 生成 requested/active/is_requested/is_active/source_ids。
  - `RED/GREEN`: 旧 selected 无法表达；实现明确 join。
  - `REFACTOR/VERIFY`: Clash API 不负责显示名和来源。
  - `done`: auto/manual golden 与 D10 6.3 一致。

- [x] **G011 - 处理 generation 变化后 manual node 消失**
  - `depends_on`: G003,G004,G010；`parallel_with`: G012
  - `scope`: 发布成功后回退 Auto、持久化并产生 warning；发布前不改变 intent。
  - `RED/GREEN`: 悬空 manual ID 或提前回退；实现 post-commit reconciliation。
  - `REFACTOR/VERIFY`: 保留/消失/重新出现三类 fixture。
  - `done`: `NH-NODE-SELECTION-STALE` 行为稳定。

- [x] **G012 - 删除 first-node fallback**
  - `depends_on`: G009,G010；`parallel_with`: G011
  - `scope`: active unresolved 时保持 null/degraded，不取列表第一个节点。
  - `RED/GREEN`: 现有 fallback test 暴露误报；删除 fallback。
  - `REFACTOR/VERIFY`: 搜索所有 `nodes.first`/index 0 展示路径。
  - `done`: backend DTO 不再伪造当前节点。

- [x] **G013 - 通过 selection domain gate**
  - `depends_on`: G011,G012；`parallel_with`: none
  - `scope`: 聚合 store、replay、recursive resolution、DTO、fallback。
  - `RED/GREEN`: 任一 active/intent 语义缺失即失败；只汇总测试。
  - `REFACTOR/VERIFY`: property test 生成有限 group 图。
  - `done`: G gate 全绿。

## 15. 阶段 H：Clash API 与 operational control

- [x] **H001 - 冻结 `/proxies` group snapshot fixture**
  - `depends_on`: F015,G013；`parallel_with`: H002,H003,H008
  - `scope`: selector、urltest、terminal、history、alive、missing field 和超大响应样本。
  - `RED/GREEN`: 当前 parser 只返回扁平 nodes；补 typed group fixture parser。
  - `REFACTOR/VERIFY`: response bytes、field count 和 tag length 统一限额。
  - `done`: fixture 覆盖 D10 递归所需全部关系。

- [x] **H002 - 让 `ClashApiClient` 返回受控 group snapshot**
  - `depends_on`: F015,G013；`parallel_with`: H001,H003,H008
  - `scope`: API 层返回 internal tags、type、now/all 和 delay，不返回 UI DTO。
  - `RED/GREEN`: client 仍把 tag 当 name/id；引入窄 internal snapshot。
  - `REFACTOR/VERIFY`: 显示模型 join 留在 operational layer。
  - `done`: `ClashApiClient::nodes()` 旧混合职责可删除。

- [x] **H003 - 冻结 group delay `NodeTestAll` 契约**
  - `depends_on`: F015,G013；`parallel_with`: H001,H002,H008
  - `scope`: 一次 group API、10s core timeout、12s request timeout、2,000 result 上限。
  - `RED/GREEN`: 非法/过大结果被接受；完善 bounded parser。
  - `REFACTOR/VERIFY`: 稳定排序只用于输出，不改变 core 选择。
  - `done`: partial/invalid/timeout 有明确错误或结果。

- [x] **H004 - 实现 `NodeSelectAuto` core 操作**
  - `depends_on`: H002；`parallel_with`: H005,H006
  - `scope`: 只允许内部固定 `nethop-auto`，前端无参数。
  - `RED/GREEN`: API 仍接受任意 target；增加固定方法。
  - `REFACTOR/VERIFY`: 不把内部 tag写入 Protocol DTO。
  - `done`: selector 不含 auto 时返回受控错误且 intent 不变。

- [x] **H005 - 实现 `NodeSelectManual` registry 映射**
  - `depends_on`: H002；`parallel_with`: H004,H006
  - `scope`: stable node ID -> 当前 generation internal tag -> selector member check。
  - `RED/GREEN`: internal tag 可从 CLI 直接传入；强制 registry lookup。
  - `REFACTOR/VERIFY`: unknown/stale/non-member 使用不同诊断。
  - `done`: 只有当前 generation 手选集合可选。

- [x] **H006 - 在选择成功后提交 intent 并查询 active**
  - `depends_on`: H002；`parallel_with`: H004,H005
  - `scope`: core select 成功后才保存 intent，再解析 active snapshot。
  - `RED/GREEN`: save-before-select 造成虚假状态；调整 operation 顺序。
  - `REFACTOR/VERIFY`: auto/manual 共用 post-select finalize。
  - `done`: 任一失败不留下半持久 selection。

- [x] **H007 - 区分 auto/manual 下 test-all 语义**
  - `depends_on`: H003,H004,H005,H006；`parallel_with`: H009
  - `scope`: manual 只更新 delay；auto 允许 core 自行更新 active child。
  - `RED/GREEN`: daemon 或前端手动挑最低延迟；删除自选逻辑。
  - `REFACTOR/VERIFY`: 测速完成统一重新读取 selection snapshot。
  - `done`: manual target 稳定，auto active 可变化。

- [x] **H008 - 处理 Clash API timeout、invalid response 与 core restart**
  - `depends_on`: F015,G013；`parallel_with`: H001,H002,H003
  - `scope`: API 暂不可用时 intent 保留、active degraded，supervisor 可恢复。
  - `RED/GREEN`: API error 清空 intent或停止代理；映射为 degraded control outcome。
  - `REFACTOR/VERIFY`: transport error 与 semantic error 分离。
  - `done`: restart 后下一次查询恢复 active。

- [x] **H009 - 强化 loopback endpoint 与 API secret 门禁**
  - `depends_on`: H004,H005,H006；`parallel_with`: H007,H010
  - `scope`: 仅 IPv4 loopback、非零端口、16-128 secret、日志始终脱敏。
  - `RED/GREEN`: 非 loopback/短 secret fixture 被接受；收紧构造和 Debug。
  - `REFACTOR/VERIFY`: 复用 ApiSecretStore，不新增第二监听端口。
  - `done`: canary 不出现在 stdout/stderr/event/WebUI。

- [x] **H010 - 建立 gRPC/async runtime 依赖禁入测试**
  - `depends_on`: H004,H005,H006；`parallel_with`: H007,H009
  - `scope`: 本次发布依赖树不得出现 tonic/prost/grpc/tokio 或 `services.api` 配置。
  - `RED/GREEN`: contract 对故意污染 fixture 失败；增加依赖/config scan。
  - `REFACTOR/VERIFY`: 与 `11` F001 候选边界一致。
  - `done`: stable release graph 仅保留同步 Clash HTTP client。

- [x] **H011 - 通过 core control gate**
  - `depends_on`: H007,H008,H009,H010,H012；`parallel_with`: none
  - `scope`: 聚合 group snapshot、测速、选择、错误恢复、安全和依赖边界。
  - `RED/GREEN`: 任一契约失败即阻断；不新增功能。
  - `REFACTOR/VERIFY`: 运行 clash_api/operational_control/api_secret/supervisor tests。
  - `done`: H gate 全绿。

- [x] **H012 - 统一批量测速的 core、HTTP 与 CLI 超时预算**
  - `depends_on`: H003,H008；`parallel_with`: H009,H010
  - `scope`: sing-box group delay 单阶段 10 秒、Clash HTTP 请求单阶段 12 秒；`NodeTestAll` 需要覆盖 selector 批量探测与 urltest 选举刷新两个阶段，因此 CLI 为 25 秒、WebUI bridge 为 30 秒。普通 CLI 请求仍保持 5 秒，显式 `--wait` 与 UDS 上限保持 30 秒。
  - `RED/GREEN`: 真机批量测速在约 3 秒返回 `NH-CORE-CONTROL-UNAVAILABLE`；增加 delayed Clash API 与 CLI command-class timeout 回归测试，再按操作类型分配预算。
  - `REFACTOR/VERIFY`: 只给批量测速放宽端到端预算，不扩大普通控制命令或取消全局资源上限。
  - `done`: `cargo test -p nethopd --test clash_api_contracts`、`cargo test -p nethopctl` 通过；真机 27 候选批量测速在 8.6 秒内完成并返回 26 个本轮成功结果，允许核心省略超时节点，不再被控制层 3/5 秒边界截断。

## 16. 阶段 I：Protocol v3、CLI 与事件

- [x] **I001 - 提升并冻结 Protocol v3**
  - `depends_on`: H011；`parallel_with`: I002,I003,I008
  - `scope`: hello negotiation 和 request/response envelope 使用唯一 v3。
  - `RED/GREEN`: v1 fixture 仍成功协商；提升版本并明确拒绝旧协议。
  - `REFACTOR/VERIFY`: 删除开发期旧协议分支。
  - `done`: Manager min/max 与 daemon v3 一致。

- [x] **I002 - 增加 subscription mode/select/set-enabled 方法**
  - `depends_on`: H011；`parallel_with`: I001,I003,I008
  - `scope`: D10 11.1 的四个 subscription typed 方法和参数 bounds。
  - `RED/GREEN`: wire 无法表达 mode/target/digest；增加专用 params。
  - `REFACTOR/VERIFY`: 不复用含混 `ConfigMutation::SelectSource`。
  - `done`: wrong mode 返回 `NH-SUB-MODE-MISMATCH`。

- [x] **I003 - 增加 node selection/list typed 方法**
  - `depends_on`: H011；`parallel_with`: I001,I002,I008
  - `scope`: `NodeSelectionGet/NodeSelectAuto/NodeSelectManual/NodeTestAll/NodeList`。
  - `RED/GREEN`: auto 仍需 target 或 list 只返回数组；增加专用 envelope。
  - `REFACTOR/VERIFY`: manual 只接受 StableNodeId。
  - `done`: NodeList 同时返回 nodes 与 selection。

- [x] **I004 - 删除旧 Protocol 方法和字段**
  - `depends_on`: I001,I002,I003；`parallel_with`: I005,I006
  - `scope`: 删除 `SelectSource`、含混 `NodeSelect`、旧 `selected` shape。
  - `RED/GREEN`: forbidden-wire test 发现旧 enum/help 仍存在；删除旧路径。
  - `REFACTOR/VERIFY`: 更新所有 match 为穷尽新方法。
  - `done`: 源码和 JSON fixture 无旧方法名称。

- [x] **I005 - 冻结 Protocol v3 golden 与大小边界**
  - `depends_on`: I001,I002,I003；`parallel_with`: I004,I006
  - `scope`: 正向、unknown field、超长 ID、缺 digest、错误 mode、超大列表。
  - `RED/GREEN`: 非法 envelope 被接受；补 validator。
  - `REFACTOR/VERIFY`: 共享 bounded string/list helper。
  - `done`: wire shape 可供 CLI/WebUI 独立生成代码外校验。

- [x] **I006 - 扩展 selection/subscription 事件类型**
  - `depends_on`: I001,I002,I003；`parallel_with`: I004,I005
  - `scope`: mode/active_set/update、selection/active/test/degraded 七类事件。
  - `RED/GREEN`: EventKind 无法订阅或 payload 含混；增加 typed event snapshot。
  - `REFACTOR/VERIFY`: 事件只携稳定 ID、模式、时间和有限状态。
  - `done`: 不含 URL、凭据、internal tag。

- [x] **I007 - 合并高频重复 `node.active_changed` 事件**
  - `depends_on`: I006；`parallel_with`: I009,I010
  - `scope`: 同一 generation/active node 短时间重复通知被 coalesce。
  - `RED/GREEN`: urltest hook 产生事件风暴；增加有界去重状态。
  - `REFACTOR/VERIFY`: 不用轮询模拟事件。
  - `done`: 真变化不丢失，重复变化不放大。

- [x] **I008 - 实现 CLI subscription 命令树**
  - `depends_on`: H011；`parallel_with`: I001,I002,I003
  - `scope`: `mode`、`mode set`、`select`、`enable`、`disable` 及 expected digest。
  - `RED/GREEN`: argv parser 不认识新命令；最小增加 typed request builder。
  - `REFACTOR/VERIFY`: 不拼 shell，不从 human text 取 digest。
  - `done`: help、JSON success/error golden 完整。

- [x] **I009 - 实现 CLI node selection 命令树**
  - `depends_on`: I003,I007；`parallel_with`: I010,I011
  - `scope`: `selection`、`select auto`、`select <node-id>`、`test-all`、`list`。
  - `RED/GREEN`: auto 被解析成普通 node ID；增加明确子命令。
  - `REFACTOR/VERIFY`: auto/manual 请求构造互斥。
  - `done`: help 不暴露 `nethop-auto`。

- [x] **I010 - 将 daemon 请求路由接入新事务与 selection service**
  - `depends_on`: I002,I003,I007；`parallel_with`: I009,I011
  - `scope`: 每个新方法调用唯一领域服务，返回 typed snapshot。
  - `RED/GREEN`: handler 仍直接 mutate config/Clash API；重定向到 F/G/H 服务。
  - `REFACTOR/VERIFY`: handler 只做 admission 和映射。
  - `done`: 无重复事务/选择逻辑。

- [x] **I011 - 增加 CLI/daemon secret canary 与 argv 安全测试**
  - `depends_on`: I002,I003,I007；`parallel_with`: I009,I010
  - `scope`: URL、凭据、API secret、internal tag 不进 stdout/stderr/argv/help。
  - `RED/GREEN`: canary 泄漏测试失败；修正 formatter/Debug/error。
  - `REFACTOR/VERIFY`: 复用统一 redaction helper。
  - `done`: success/error/timeout/diagnostics 均无泄漏。

- [x] **I012 - 建立旧功能到 v3 方法映射测试**
  - `depends_on`: I004,I008,I009,I010,I011；`parallel_with`: I013
  - `scope`: B01-B10 用户能力在新命令/方法上均有对应。
  - `RED/GREEN`: mapping 出现 uncovered；补新测试而非恢复旧方法。
  - `REFACTOR/VERIFY`: 映射使用 capability ID，不依赖文件名。
  - `done`: 旧有效功能覆盖率 100%。

- [x] **I013 - 通过 Protocol/CLI gate**
  - `depends_on`: I007,I012；`parallel_with`: none
  - `scope`: 聚合 v3 wire、CLI、事件、删除旧 API、安全和能力映射。
  - `RED/GREEN`: 任一缺项阻断；不加业务代码。
  - `REFACTOR/VERIFY`: 运行 protocol/nethopctl/uds/event contracts。
  - `done`: I gate 全绿。

## 17. 阶段 J：WebUI DTO、store、bridge 与事件状态

- [x] **J001 - 冻结 WebUI v3 subscription DTO validator**
  - `depends_on`: I013；`parallel_with`: J002,J004,J008
  - `scope`: mode、active set、source contribution、digest 和 status bounds。
  - `RED/GREEN`: 新 fixture 被拒绝或旧含混字段被接受；更新 validator。
  - `REFACTOR/VERIFY`: 不解析 TOML 或 human text。
  - `done`: invalid/unknown/oversize golden 全部拒绝。

- [x] **J002 - 冻结 WebUI v3 node/selection DTO validator**
  - `depends_on`: I013；`parallel_with`: J001,J004,J008
  - `scope`: requested/active、degraded、last_test、node flags/source IDs。
  - `RED/GREEN`: validator 仍要求 `selected`；替换 DTO shape。
  - `REFACTOR/VERIFY`: NodeDto 与 SelectionDto 分离。
  - `done`: old `selected` fixture 明确失败。

- [x] **J003 - 删除前端旧 source/node 兼容解析**
  - `depends_on`: J001,J002；`parallel_with`: J005,J006
  - `scope`: 删除 `selected`、旧 select_source 结果和 first-node fallback parser。
  - `RED/GREEN`: forbidden shape 仍进入 store；删除 fallback/alias。
  - `REFACTOR/VERIFY`: 搜索旧字段和方法字符串。
  - `done`: TypeScript compile 无旧 DTO 引用。

- [x] **J004 - 更新 WebUI operation 到 Protocol v3**
  - `depends_on`: I013；`parallel_with`: J001,J002,J008
  - `scope`: 新 subscription/node command args，auto 无 nodeId，所有配置操作带 digest。
  - `RED/GREEN`: operation golden 仍生成旧 CLI；更新 typed union/planner。
  - `REFACTOR/VERIFY`: 参数逐项传递，不拼 shell。
  - `done`: bridge command contract 与 CLI help 对齐。

- [x] **J005 - 为 runtime store 增加独立 selection state**
  - `depends_on`: J001,J002；`parallel_with`: J003,J006
  - `scope`: nodesById 与 selection 分开保存，active/requested 为稳定 ID。
  - `RED/GREEN`: 只能扫描 node.selected；增加 shallow selection ref。
  - `REFACTOR/VERIFY`: derived flags 由 view-model 计算或服务端 DTO一致。
  - `done`: auto active 变化不重建全部 node 对象。

- [x] **J006 - 为 runtime store 增加 subscription mode/active set**
  - `depends_on`: J001,J002；`parallel_with`: J003,J005
  - `scope`: mode、active IDs、ordered sources 与 digest 成为独立状态。
  - `RED/GREEN`: 页面靠 selectedSourceId 推断模式；增加 typed state。
  - `REFACTOR/VERIFY`: 不把 UI pending 写入业务 snapshot。
  - `done`: single/merge 可由同一 store 表达。

- [x] **J007 - 实现 v3 event reducer**
  - `depends_on`: J005,J006；`parallel_with`: J009,J010
  - `scope`: mode/active set/selection/active/test/degraded/generation 事件原子归并。
  - `RED/GREEN`: 事件后出现 stale node 或 digest；实现 generation-aware reducer。
  - `REFACTOR/VERIFY`: 同一 event ID 幂等。
  - `done`: out-of-order/stale generation 事件被忽略或触发 reload。

- [x] **J008 - 扩展 MockHost v3 fixtures**
  - `depends_on`: I013；`parallel_with`: J001,J002,J004
  - `scope`: single/merge、auto/manual、degraded、transaction conflict 和 partial test。
  - `RED/GREEN`: 页面测试无法复现状态；增加脱敏 fixture。
  - `REFACTOR/VERIFY`: fixture builder 共享 stable IDs/digests。
  - `done`: 不包含真实订阅 URL和凭据。

- [x] **J009 - 统一 subscription/node operation pending 状态机**
  - `depends_on`: J005,J006；`parallel_with`: J007,J010
  - `scope`: start/success/failure/conflict/reload，失败保持旧 snapshot。
  - `RED/GREEN`: 页面各自乐观写 store；扩展共享 operation state。
  - `REFACTOR/VERIFY`: 页面只提交 intent，不直接伪造 daemon state。
  - `done`: 同资源重复操作被阻止，其他资源可并行。

- [x] **J010 - 实现 selection/subscription 查询失效策略**
  - `depends_on`: J005,J006；`parallel_with`: J007,J009
  - `scope`: generation/mode/active change 精确失效 node/subscription/overview query。
  - `RED/GREEN`: 全局 reload 或 stale cache；定义最小 invalidation graph。
  - `REFACTOR/VERIFY`: 不增加高频轮询。
  - `done`: 事件后只刷新受影响领域。

- [x] **J011 - 保持 10,000 节点 store 更新性能**
  - `depends_on`: J007,J008,J009,J010；`parallel_with`: J012
  - `scope`: selection/latency 单点更新不复制无关大数组，stable key 保持。
  - `RED/GREEN`: benchmark 发现全量重建；改用 shallow immutable map 局部替换。
  - `REFACTOR/VERIFY`: 记录 Node/V8 heap 与更新耗时。
  - `done`: 不突破 `08/09` 已冻结前端预算。

- [x] **J012 - 通过 WebUI contract/state gate**
  - `depends_on`: J011；`parallel_with`: none
  - `scope`: 聚合 DTO、bridge、store、events、operation、MockHost 和性能。
  - `RED/GREEN`: 任一 v3 contract 缺失即阻断。
  - `REFACTOR/VERIFY`: 运行 WebUI unit/browser contract tests。
  - `done`: J gate 全绿。

## 18. 阶段 K：WebUI 订阅页

- [x] **K001 - 增加 single/merge Segmented mode control**
  - `depends_on`: J012；`parallel_with`: K002,K003,K008
  - `scope`: 页面显式展示当前 mode，切换只提交 typed operation。
  - `RED/GREEN`: 页面无 mode 控件；实现复用现有 SegmentedControl。
  - `REFACTOR/VERIFY`: pending 时禁止重复切换。
  - `done`: mode 来自 daemon snapshot，不来自 localStorage。

- [x] **K002 - single 模式只渲染圆点 radio**
  - `depends_on`: J012；`parallel_with`: K001,K003,K008
  - `scope`: 唯一 active source，点击调用 `SubscriptionSelect`。
  - `RED/GREEN`: radio 仍直接 config-mutate/select_source；改接新 operation。
  - `REFACTOR/VERIFY`: 删除 `selectedConfigSnapshot()` 乐观伪造。
  - `done`: 失败后旧 radio 保持不变。

- [x] **K003 - merge 模式只渲染 checkbox**
  - `depends_on`: J012；`parallel_with`: K001,K002,K008
  - `scope`: enabled 表示加入合并集合，调用 set-enabled。
  - `RED/GREEN`: merge 仍显示 radio 或关闭其他 source；实现专用控件。
  - `REFACTOR/VERIFY`: radio/checkbox 文案和组件不共用含混逻辑。
  - `done`: 多 source 可同时选中。

- [x] **K004 - 实现 single -> merge 交互**
  - `depends_on`: K001,K002,K003；`parallel_with`: K005,K006
  - `scope`: 保留当前 source，其他 source 不自动启用。
  - `RED/GREEN`: UI 乐观全启用或触发更新；只提交 mode set。
  - `REFACTOR/VERIFY`: 成功后等待 daemon event/snapshot。
  - `done`: mode 与 active set 显示一致。

- [x] **K005 - 实现 merge -> single 目标选择交互**
  - `depends_on`: K001,K002,K003；`parallel_with`: K004,K006
  - `scope`: 多 active 时弹出目标 source 选择，单 active 直接提交。
  - `RED/GREEN`: UI 静默取第一个；实现受控 target dialog。
  - `REFACTOR/VERIFY`: 目标 ID 来自当前 snapshot。
  - `done`: cancel 不改变状态，缺目标不发请求。

- [x] **K006 - 实现 source select/enable/disable pending 与 CAS conflict**
  - `depends_on`: K001,K002,K003；`parallel_with`: K004,K005
  - `scope`: 单卡 pending、页面 mode pending、conflict reload 后提示重试。
  - `RED/GREEN`: 双击或 stale digest 造成 UI 假成功；复用 J009。
  - `REFACTOR/VERIFY`: 不直接写 runtime store。
  - `done`: success/failure/conflict 三态测试通过。

- [x] **K007 - 展示 source 对 auto pool 的贡献**
  - `depends_on`: K004,K005,K006；`parallel_with`: K009,K010
  - `scope`: 卡片展示节点数、auto contribution、active/disabled 状态。
  - `RED/GREEN`: 所有卡片信息相同或无法区分贡献；使用后端 DTO。
  - `REFACTOR/VERIFY`: 不从前端节点数组猜测跨源 duplicate。
  - `done`: single/merge 文案准确。

- [x] **K008 - 保持订阅更新与编辑旧功能**
  - `depends_on`: J012；`parallel_with`: K001,K002,K003
  - `scope`: add/edit/remove/move/update/import 继续工作且不破坏 mode 不变量。
  - `RED/GREEN`: v3 后旧功能回归失败；适配新 digest/snapshot。
  - `REFACTOR/VERIFY`: mutation 与 mode transaction 职责分开。
  - `done`: B01-B04 WebUI 消费能力仍可用。

- [x] **K009 - 处理坏目标 source 切换失败**
  - `depends_on`: K004,K005,K006；`parallel_with`: K007,K010
  - `scope`: single 切坏 Backup 或 merge enable 坏 source 时保持旧选择。
  - `RED/GREEN`: UI 先变更后回滚闪烁；不做乐观业务状态更新。
  - `REFACTOR/VERIFY`: 错误文案按诊断码映射。
  - `done`: before/after DOM 与 store 均保持旧 active set。

- [x] **K010 - 增加订阅页 browser/E2E 截图回归**
  - `depends_on`: K004,K005,K006；`parallel_with`: K007,K009
  - `scope`: single/merge、pending、error、empty、长名称、窄屏/宽屏。
  - `RED/GREEN`: 布局或控件状态不符合设计；修正页面实现。
  - `REFACTOR/VERIFY`: 不新增无障碍发布门槛，保持现有视觉测试范围。
  - `done`: 固定 viewport 截图和交互测试通过。

- [x] **K011 - 通过订阅页 gate**
  - `depends_on`: K007,K008,K009,K010；`parallel_with`: none
  - `scope`: 聚合 mode、radio/checkbox、事务、旧功能和视觉回归。
  - `RED/GREEN`: 任一语义不一致即阻断。
  - `REFACTOR/VERIFY`: 运行 subscriptions unit/browser/E2E tests。
  - `done`: K gate 全绿。

## 19. 阶段 L：WebUI 节点页与概览代理质量

- [x] **L001 - 增加独立“自动优选”控制项**
  - `depends_on`: J012；`parallel_with`: L002,L006,L009
  - `scope`: auto 不是普通 node card，点击调用 `node select auto`。
  - `RED/GREEN`: 无法从 manual 返回 auto；实现固定控制项。
  - `REFACTOR/VERIFY`: UI 不知道 `nethop-auto` tag。
  - `done`: auto/manual 往返测试通过。

- [x] **L002 - 区分 requested 与 active node 卡片状态**
  - `depends_on`: J012；`parallel_with`: L001,L006,L009
  - `scope`: manual requested、实际 active、auto active 使用不同明确状态。
  - `RED/GREEN`: 单一 selected 样式无法表达；接入 v3 DTO。
  - `REFACTOR/VERIFY`: 样式由 view-model 输出，不散落比较。
  - `done`: 同一卡可同时 requested+active，auto 下仅 active。

- [x] **L003 - 实现 manual node 选择操作**
  - `depends_on`: L001,L002；`parallel_with`: L004,L005
  - `scope`: stable node ID、pending、失败保持、成功等待 snapshot。
  - `RED/GREEN`: 仍调用旧 node.select 或乐观 selected；改接 v3 operation。
  - `REFACTOR/VERIFY`: 卡片只发 intent。
  - `done`: stale node 诊断正确展示。

- [x] **L004 - 实现“全部测速”只更新结果**
  - `depends_on`: L001,L002；`parallel_with`: L003,L005
  - `scope`: 闪电按钮调用 test-all，不在前端选最低延迟。
  - `RED/GREEN`: 测速后前端自行 select；删除排序即选择逻辑。
  - `REFACTOR/VERIFY`: latency 更新与 selection 更新分离。
  - `done`: manual target 不变，auto active 只按后端 snapshot 变化。

- [x] **L005 - 处理 generation 变化与 stale node 清理**
  - `depends_on`: L001,L002；`parallel_with`: L003,L004
  - `scope`: 新 generation 替换 node 集合，selection event 决定 fallback。
  - `RED/GREEN`: 陈旧卡片或悬空 selected 保留；使用 J007 reducer。
  - `REFACTOR/VERIFY`: stable key 不复用不同 node。
  - `done`: manual 保留/消失两条路径通过。

- [x] **L006 - 代理质量卡片展示真实 active terminal**
  - `depends_on`: J012；`parallel_with`: L001,L002,L009
  - `scope`: 显示 mode、active node、protocol、latency、source 和 last test。
  - `RED/GREEN`: 概览显示列表第一个节点；只消费 selection snapshot。
  - `REFACTOR/VERIFY`: 复用 node view-model。
  - `done`: auto 模式显示 urltest 当前 child。

- [x] **L007 - 处理 active unresolved/degraded 概览状态**
  - `depends_on`: L006；`parallel_with`: L008,L010
  - `scope`: active null 时显示“状态暂不可用”，不显示伪节点。
  - `RED/GREEN`: first-node fallback test 失败；删除 fallback。
  - `REFACTOR/VERIFY`: degraded reason 有界映射。
  - `done`: API down/cycle/depth/unknown 四类状态通过。

- [x] **L008 - 实现代理质量卡片导航与测速按钮隔离**
  - `depends_on`: L006；`parallel_with`: L007,L010
  - `scope`: 点击主体进入节点页，闪电按钮只测速且不冒泡导航。
  - `RED/GREEN`: 按钮同时触发导航；修正事件边界。
  - `REFACTOR/VERIFY`: 操作反馈复用共享 message。
  - `done`: Playwright 路由与命令断言通过。

- [x] **L009 - 保持两列节点布局与虚拟列表性能**
  - `depends_on`: J012；`parallel_with`: L001,L002,L006
  - `scope`: 10,000 节点 stable key、两列移动布局、selection/latency 更新不跳动。
  - `RED/GREEN`: 更新导致滚动位置或列布局重建；调整 virtualizer/view-model。
  - `REFACTOR/VERIFY`: 不增加搜索/过滤栏等已删除功能。
  - `done`: 长列表滚动、恢复和单点更新预算通过。

- [x] **L010 - 通过 selection/active 事件实时更新页面**
  - `depends_on`: L006；`parallel_with`: L007,L008
  - `scope`: active/test/degraded 事件同步节点页和概览，不轮询。
  - `RED/GREEN`: 事件后一个页面 stale；连接同一 runtime store。
  - `REFACTOR/VERIFY`: 重复事件不重复 toast/reload。
  - `done`: 两页面下一微任务批次内一致。

- [x] **L011 - 增加节点与概览 browser/E2E 截图回归**
  - `depends_on`: L003,L004,L005,L007,L008,L009,L010；`parallel_with`: L012
  - `scope`: auto/manual/requested/active/degraded/testing/large list 与主题视图。
  - `RED/GREEN`: 视觉或交互不匹配；修正组件/CSS。
  - `REFACTOR/VERIFY`: 按现有固定 viewport 生成受控 baseline。
  - `done`: light/dark/mobile/tablet 截图通过。

- [x] **L012 - 通过节点/概览 gate**
  - `depends_on`: L011；`parallel_with`: none
  - `scope`: 聚合 auto/manual、test-all、active、degraded、导航、事件和性能。
  - `RED/GREEN`: 任一用户路径失败即阻断。
  - `REFACTOR/VERIFY`: 运行 nodes/overview unit/browser/E2E tests。
  - `done`: L gate 全绿。

## 20. 阶段 M：端到端、回归、安全与性能

- [x] **M001 - 验证单源 auto → manual → auto 闭环**
  - `depends_on`: I013,J012,K011,L012；`parallel_with`: M002,M004
  - `scope`: Primary single 模式下完成自动优选、手动选择稳定 node ID、返回自动优选。
  - `RED/GREEN`: intent、active terminal 或事件任一断链时测试失败；贯通 daemon、CLI、WebUI。
  - `REFACTOR/VERIFY`: 测试只使用本地 fixture server，不访问真实订阅。
  - `done`: 三次状态变化均由 committed snapshot 驱动且 generation 归因正确。

- [x] **M002 - 验证 single 切换到健康 Backup**
  - `depends_on`: I013,J012,K011,L012；`parallel_with`: M001,M003
  - `scope`: 从 Primary 原子切换至健康 Backup，并验证 source、candidate pool、active terminal。
  - `RED/GREEN`: UI 状态先于 commit 或残留 Primary 节点时失败；修正事务发布顺序。
  - `REFACTOR/VERIFY`: 断言旧 generation 被完整替换。
  - `done`: config、journal、runtime、WebUI 四层一致指向 Backup。

- [x] **M003 - 验证坏 Backup 切换失败保持 Primary**
  - `depends_on`: M002；`parallel_with`: M004,M005
  - `scope`: 覆盖下载、解析、compose、sing-box check、启动健康检查失败。
  - `RED/GREEN`: 任一失败改变 active set、intent 或 runtime 时失败；补齐回滚。
  - `REFACTOR/VERIFY`: 使用表驱动 failure matrix，避免重复测试代码。
  - `done`: 每个失败点均保留旧 generation，并返回稳定诊断码。

- [x] **M004 - 验证 single → merge → enable Backup**
  - `depends_on`: I013,J012,K011,L012；`parallel_with`: M001,M003
  - `scope`: 进入 merge 后启用第二来源，验证 active set 与 fair candidate pool。
  - `RED/GREEN`: merge 退化成全局 `.take()` 或来源状态错误时失败；修正 pool/事务。
  - `REFACTOR/VERIFY`: 用不对称来源规模 fixture 证明公平性。
  - `done`: 两来源均贡献候选，数量与来源追踪符合预算。

- [x] **M005 - 验证禁用 active node 所在来源**
  - `depends_on`: M003,M004；`parallel_with`: M006,M007
  - `scope`: merge/manual 下禁用当前 active terminal 的来源，验证 intent fallback 和新 active。
  - `RED/GREEN`: 出现悬空 manual ID、伪 selected 或空 selector 时失败；执行确定性 fallback。
  - `REFACTOR/VERIFY`: fallback 规则由领域层单点定义。
  - `done`: 提交后 intent 为 auto，active terminal 来自剩余来源。

- [x] **M006 - 验证 merge → single 的显式目标语义**
  - `depends_on`: M004；`parallel_with`: M005,M007
  - `scope`: merge 切 single 必须携带目标 source，不允许依赖列表顺序。
  - `RED/GREEN`: 缺少目标仍被接受或默认首项时失败；加强 admission。
  - `REFACTOR/VERIFY`: CLI、protocol、daemon 共用同一请求模型。
  - `done`: 缺目标稳定拒绝，合法目标原子提交。

- [x] **M007 - 验证订阅更新后保留 manual node**
  - `depends_on`: M001,M004；`parallel_with`: M005,M006,M008
  - `scope`: 节点字段或排序变化但 canonical identity 不变时保留 manual intent。
  - `RED/GREEN`: stable ID 漂移导致回 auto 时失败；修正 fingerprint/registry 映射。
  - `REFACTOR/VERIFY`: 覆盖跨来源同节点去重后的 identity。
  - `done`: 新 generation 保留 requested node，并解析到新 terminal tag。

- [x] **M008 - 验证订阅更新后 manual node 消失回 auto**
  - `depends_on`: M001,M004；`parallel_with`: M007,M009
  - `scope`: requested node 不再存在时提交新 generation，并发出明确 fallback event。
  - `RED/GREEN`: 保留 stale ID、选择首节点或发布失败时测试失败；实现 auto fallback。
  - `REFACTOR/VERIFY`: 不把正常 fallback 误报为事务失败。
  - `done`: intent、snapshot、journal 和 WebUI 同步回 auto。

- [x] **M009 - 验证 commit journal 崩溃与重启恢复矩阵**
  - `depends_on`: F015,G013,H011,I013；`parallel_with`: M008,M010
  - `scope`: prepare、validated、published、intent-applied、committed 各阶段注入崩溃。
  - `RED/GREEN`: 重启后出现半提交、丢失 committed state 或错配 generation 时失败。
  - `REFACTOR/VERIFY`: fault injection 复用同一 journal harness。
  - `done`: 每个崩溃点都恢复到唯一合法状态，重复恢复幂等。

- [x] **M010 - 验证切换不主动中断已有连接**
  - `depends_on`: H011,M001；`parallel_with`: M009,M011
  - `scope`: 保持 `interrupt_exist_connections = false`，旧连接继续，新连接使用新 active terminal。
  - `RED/GREEN`: 切换导致旧连接断开或新连接仍错误归因时失败；修正 composer/API 顺序。
  - `REFACTOR/VERIFY`: 使用可观察的双出口 integration fixture。
  - `done`: 连接生命周期与文档语义一致。

- [x] **M011 - 执行 B01-B10 旧功能回归矩阵**
  - `depends_on`: M003,M005,M006,M007,M008；`parallel_with`: M010,M012
  - `scope`: 订阅增删改排、更新、LKG、去重、部分成功、配置事务、TUN/TPROXY 等冻结行为。
  - `RED/GREEN`: 任一 baseline fixture 退化时阻断；仅修复本次重构引入的回归。
  - `REFACTOR/VERIFY`: baseline 名称与证据链接回阶段 A。
  - `done`: B01-B10 全部维持或提升，不存在静默行为变化。

- [x] **M012 - 执行全链路 secret canary 测试**
  - `depends_on`: M001,M002,M003,M004；`parallel_with`: M011,M013
  - `scope`: URL token、userinfo、header、节点凭据不得进入日志、event、journal、CLI、WebUI、截图和 artifact。
  - `RED/GREEN`: canary 出现在任一输出即失败；在最靠近边界处脱敏。
  - `REFACTOR/VERIFY`: 保留 source ID、digest 和有界诊断上下文。
  - `done`: 仓库扫描与运行产物扫描均无 canary。

- [x] **M013 - 验证后端 10,000 节点性能预算**
  - `depends_on`: M004,M007,M008；`parallel_with`: M012,M014
  - `scope`: registry、fair pool、compose、snapshot、递归 active terminal、event diff。
  - `RED/GREEN`: 超出 D10/01 预算时失败；以 profile 数据修正算法或分配。
  - `REFACTOR/VERIFY`: benchmark fixture 固定来源分布、重复率与 group 深度。
  - `done`: Android arm64 release 目标和主机回归阈值均达标。

- [x] **M014 - 验证 WebUI 10,000 节点交互性能**
  - `depends_on`: L009,L010；`parallel_with`: M013,M015
  - `scope`: 首次渲染、滚动、延迟更新、selection 更新、generation 替换和内存。
  - `RED/GREEN`: 长任务、布局抖动或全列表重渲染超预算时失败；优化 store/view-model/virtualizer。
  - `REFACTOR/VERIFY`: 性能测试使用 production bundle 和固定设备模拟参数。
  - `done`: 满足 D10/08 的交互与资源预算。

- [x] **M015 - 增加架构禁入与依赖门禁**
  - `depends_on`: H011,I013,J012；`parallel_with`: M014
  - `scope`: 禁止新增监听端口、gRPC/protobuf、Tokio/通用 async runtime、libbox AAR 路径。
  - `RED/GREEN`: 依赖树、配置或 socket snapshot 出现禁入项即失败；删除越界实现。
  - `REFACTOR/VERIFY`: 允许的 loopback Clash API 端点显式列入 allowlist。
  - `done`: 当前实现仍是独立 sing-box 子进程 + loopback Clash API。

- [x] **M016 - 通过桌面端完整集成 gate**
  - `depends_on`: M009,M010,M011,M012,M013,M014,M015；`parallel_with`: none
  - `scope`: workspace、WebUI、integration、security、performance 全部聚合。
  - `RED/GREEN`: 任一必需 suite 失败即阻断 Android 打包。
  - `REFACTOR/VERIFY`: 输出机器可读测试报告与证据索引。
  - `done`: M gate 全绿，且没有 ignored/only/focused 测试逃逸。

## 21. 阶段 N：模块、真机、清理与完成门禁

- [x] **N001 - 更新默认 TOML 到 schema v3**
  - `depends_on`: B012,F015；`parallel_with`: N002,N003
  - `scope`: 默认配置表达 source mode，不再包含 `proxy.selector_mode` 或用户 source ID。
  - `RED/GREEN`: 默认配置无法 canonical round-trip 或含旧字段时失败；更新模板与注释。
  - `REFACTOR/VERIFY`: 默认值与 schema metadata 共用定义。
  - `done`: 全新安装得到可验证、可运行的 v3 配置。

- [x] **N002 - 更新模块 ZIP、manifest 与许可证清单**
  - `depends_on`: M016；`parallel_with`: N001,N003
  - `scope`: 打包新 daemon/CLI/WebUI/defaults，保持 checksum target 完整。
  - `RED/GREEN`: ZIP 结构、checksum、SBOM 或许可证测试失败；修正构建脚本。
  - `REFACTOR/VERIFY`: 不打包测试 fixture、真实 URL、refer 或调试产物。
  - `done`: 模块安装结构测试通过。

- [x] **N002A - 覆盖安装必须保留当前 schema v3 用户配置**
  - `depends_on`: N001,N002；`parallel_with`: N003
  - `scope`: 安装器只在配置 schema 不是当前 v3 时备份并重置；同版本覆盖安装必须逐字节保留 TOML、订阅 URL 和用户选项。
  - `RED/GREEN`: 安装器错误接受 v2、导致每次覆盖安装把 v3 配置重置为空默认值；统一 `CONFIG_SCHEMA_VERSION=3`，旧配置备份为 `nethop.toml.pre-v3`。
  - `REFACTOR/VERIFY`: `fake-magisk-smoke.ps1` 使用 `.invalid` 私有标记验证覆盖安装保留，并接入 `build-android-module.ps1` 发布门禁；不得把真实订阅写入 fixture。
  - `done`: 模块契约与 fake Magisk smoke 均通过；真机覆盖安装前后配置 SHA-256 一致（摘要不写入仓库），两个订阅 URL 均保持非空，重启后无需手动 `start` 即恢复 generation 8。

- [ ] **N003 - 构建 Android arm64 release 产物**
  - `depends_on`: M016；`parallel_with`: N001,N002
  - `scope`: 以锁定工具链构建 nethopd、nethopctl 和 WebUI production bundle。
  - `RED/GREEN`: 交叉编译、strip、链接或 bundle budget 失败；修正构建配置。
  - `REFACTOR/VERIFY`: 记录二进制大小、依赖与 build manifest。
  - `done`: 可复现构建校验通过。

  - `2026-08-11 evidence`: 已生成并完成结构、54 项 checksum、许可证、CycloneDX SBOM 和 bundle budget 核验的 arm64 候选；但 manifest 因固定官方预编译 sing-box 与未提交开发工作树记录 `reproducible=false`，故本节点按严格完成定义保持未勾选。

> 真机执行状态（2026-08-12）：已恢复真机测试并完成覆盖安装、配置保留、开机恢复、TPROXY、批量测速和常用站点联网验证。由于 N003 的可复现构建条件仍未满足，且 TUN、merge 公平池、失败回滚、旧连接保持、10 分钟 URLTest、manual 演进和攻击面扫描尚未完整执行，N004-N012 与 N016 仍按严格依赖关系保持未完成。

- [ ] **N004 - 完成真机安装与 capability 基线**
  - `depends_on`: N001,N002,N003；`parallel_with`: none
  - `scope`: 覆盖安装/全新安装、daemon 启动、schema v3、Clash API loopback、网络能力探测。
  - `RED/GREEN`: 安装失败、旧字段残留或 capability 错报时阻断；修正模块边界。
  - `REFACTOR/VERIFY`: 测试前记录设备、Android、root 管理器和内核版本。
  - `done`: 无其他代理模块干扰的基线成立。

  - `2026-08-12 evidence`: Android API 33、arm64-v8a、内核 `4.19.157-perf-g9607d8651312`、Magisk 30.6；覆盖安装后 daemon/sing-box 自动启动，schema v3 配置摘要未变化，generation 8、Clash API、TPROXY 和 DNS guard 均健康。行为条件已通过，但因 N003 尚未完成而不提前勾选。

- [ ] **N005 - 真机验证 TPROXY single 模式**
  - `depends_on`: N004；`parallel_with`: N006
  - `scope`: 单来源 auto/manual/auto、TCP/UDP、DNS 与常用国内外站点。
  - `RED/GREEN`: 访问、切换、归因或回滚失败时阻断；定位到 runner/core/control 层。
  - `REFACTOR/VERIFY`: 不使用用户私人真实订阅作为仓库 fixture。
  - `done`: TPROXY 主路径与选择语义通过。

  - `2026-08-12 partial evidence`: 当前恢复配置为 merge 模式，因此只计入 TPROXY 公共主路径证据，不宣称 single 完成。非 root shell UID 经透明代理访问 Google/YouTube 返回 HTTP 204，Bilibili API 返回 HTTP 200；27 个候选全部得到延迟，范围 117-5860ms，中位数 818ms。single 模式和完整 auto → manual → auto 仍待独立执行。

- [ ] **N006 - 真机验证 TUN single 模式**
  - `depends_on`: N004；`parallel_with`: N005
  - `scope`: 独立 TUN runner、健康检查、回滚以及 auto/manual/auto。
  - `RED/GREEN`: TUN 与 TPROXY 共用错误规则或回滚不完整时失败；修正 runner 边界。
  - `REFACTOR/VERIFY`: 验证切换 capture mode 不残留规则/接口。
  - `done`: TUN 主路径与选择语义通过。

- [ ] **N007 - 真机验证 merge fair candidate pool**
  - `depends_on`: N005,N006；`parallel_with`: N008,N009
  - `scope`: 两个受控订阅来源规模不对称，验证贡献、去重、上限和 active terminal。
  - `RED/GREEN`: 小来源饥饿或展示归因错误时失败；回到 fair pool/registry 修复。
  - `REFACTOR/VERIFY`: 记录 pool 证据但不记录敏感节点字段。
  - `done`: 真机结果与主机 benchmark/fixture 一致。

  - `2026-08-13 partial evidence`: D14 M013 已在 TPROXY 运行态完成真实多源事务与公平池验证。Primary-only generation 33 为 27 candidates；启用 Backup 后 generation 35 为 45 个唯一 candidates，两个稳定 source ID 分别贡献 27/18，auto pool 前 36 个候选按来源严格交替，Primary 的 27 个 stable node ID 在合并前后全部保持。禁用 Backup 后 generation 36 恢复 27 candidates，Backup LKG cache 保留，journal/staged 文件清理，Google/YouTube 均返回 HTTP 204。事务阶段顺序导致的 `PhaseRegression` 已从根因修复，enable/disable ACK 分别为 6397ms/9139ms。该证据不包含 URL、节点名称或凭据；由于本节点按 D12 仍依赖尚未勾选的 N005/N006，故不提前标记完成，最终签核还需把同一公平池行为置于 D12 要求的 TPROXY/TUN 完整 single 基线上复核。

- [ ] **N008 - 真机验证失败切换回滚**
  - `depends_on`: N005,N006；`parallel_with`: N007,N009
  - `scope`: 不可达 source、非法正文、sing-box check 失败和健康超时。
  - `RED/GREEN`: 任一失败导致断网、旧 generation 丢失或 UI 伪成功时阻断。
  - `REFACTOR/VERIFY`: 每次注入后确认网络规则和进程状态。
  - `done`: 失败保持旧代理，诊断可操作且无敏感信息。

- [ ] **N009 - 真机验证旧连接保持与新连接切换**
  - `depends_on`: N005,N006；`parallel_with`: N007,N008
  - `scope`: 长连接存在时切换 manual/auto/source，观察旧连接与新连接出口。
  - `RED/GREEN`: 旧连接被主动中断或新连接不使用新选择时失败。
  - `REFACTOR/VERIFY`: 分别记录 TPROXY/TUN 结果。
  - `done`: 两种 capture mode 均符合 `interrupt_exist_connections = false`。

- [ ] **N010 - 真机验证 10 分钟 URLTest 策略**
  - `depends_on`: N007；`parallel_with`: N011,N012
  - `scope`: 验证 interval 偏离 sing-box 默认 3 分钟的省电/流量策略及 active 更新。
  - `RED/GREEN`: 定时频率错误、空闲唤醒或流量超预算时失败；调整配置或实现。
  - `REFACTOR/VERIFY`: 测试窗口和设备状态固定，结果写入证据报告。
  - `done`: 10 分钟策略满足移动端预算且不影响可用性。

  - `2026-08-12 root cause`: sing-box `v1.13.15` 对外层 selector 的 group delay 走通用分支，只把终端结果写入共享 URLTest history，不调用内层 URLTest group 的 `performUpdateCheck()`；因此旧实现虽完成批量测速，却不会立即刷新 `nethop-auto` 的 active child。
  - `2026-08-12 implementation evidence`: `node test-all` 先通过 `nethop-select` 一次性测试全部终端，再调用 `nethop-auto` group delay。第二步复用刚写入的共享 history、跳过新鲜节点的重复探测，并由 sing-box 自己按 `tolerance` 执行选举；控制层随后重新读取 `/proxies`，不在 NetHop 内复制或推断 URLTest 算法。Clash API 与 operational control 回归测试已覆盖两阶段请求、刷新失败、active core snapshot 和完整 `nethopd` 套件。
  - `2026-08-12 device evidence`: 覆盖安装并重启后的 generation 8 真机上，`node test-all --json` 在 13.99 秒内返回合法 envelope，active 在同次响应中由 `nh1s-5def...` 更新为本轮 152ms 的 `nh1s-8647...`。本轮最低结果为 128ms，两者相差 24ms，小于 `tolerance = 50ms`，因此保留 152ms 节点符合 sing-box 的抗抖动语义；随后 `status` 返回相同 active，Google/YouTube 返回 HTTP 204，Bilibili 返回 HTTP 200，daemon 与 core 保持健康。设备上的 `nethopctl`、`nethopd` SHA-256 与本地最新 Android 产物一致。
  - `timeout evidence`: 旧 CLI 15 秒预算曾在约 15.15 秒截断合法的两阶段操作并表现为 `control protocol response is invalid`；现在仅将 `NodeTestAll` CLI 放宽为 25 秒、WebUI bridge 放宽为 30 秒，两个 Clash API 阶段仍各自受 12 秒约束，其他命令预算不变。真机复测未再发生客户端截断。
  - `remaining`: 即时选举刷新和联网可用性已经通过；仍需完成固定 10 分钟窗口的唤醒、流量和功耗测试，因此 N010 保持未勾选。

- [ ] **N011 - 真机验证更新后 manual 保留/消失**
  - `depends_on`: N007；`parallel_with`: N010,N012
  - `scope`: 受控服务先保留 stable node，再删除该 node，观察两次更新。
  - `RED/GREEN`: 保留时误回 auto 或消失时悬空即失败。
  - `REFACTOR/VERIFY`: 同时核对 CLI、WebUI、journal 和 core group。
  - `done`: 两条 generation 演进路径均符合领域规则。

- [ ] **N012 - 执行真机泄密与攻击面扫描**
  - `depends_on`: N007,N008；`parallel_with`: N010,N011
  - `scope`: logcat、模块日志、journal、process args、socket、WebUI bridge 和临时文件。
  - `RED/GREEN`: 出现 URL token/凭据、非预期监听或宽松权限时失败；在源头修复。
  - `REFACTOR/VERIFY`: canary 输入使用专用本地测试值。
  - `done`: 无敏感泄漏，仅存在 allowlist 内控制端点。

- [x] **N013 - 删除所有旧 schema v2 与含混选择路径**
  - `depends_on`: N010,N011,N012；`parallel_with`: N014
  - `scope`: 删除 selector_mode、SelectSource、旧 NodeSelect、node.selected、乐观 source snapshot 与兼容解析。
  - `RED/GREEN`: 静态搜索或 dead-code 测试仍命中旧路径时失败；彻底删除。
  - `REFACTOR/VERIFY`: 不保留 adapter、feature flag 或迁移层。
  - `done`: 仅存在 schema/protocol v3 语义，B01-B10 仍全绿。

- [x] **N014 - 同步跨文档契约**
  - `depends_on`: N010,N011,N012；`parallel_with`: N013
  - `scope`: 更新 `00`、`06`、`08`、`09` 的 schema、协议、CLI、WebUI 和 sing-box API 描述。
  - `RED/GREEN`: 文档契约检查发现冲突时失败；统一术语和版本。
  - `REFACTOR/VERIFY`: 当前实现与未来候选分开，gRPC 仅链接未来设计文档。
  - `done`: 文档无旧字段、旧命令或相互矛盾的 source/selection 语义。

- [x] **N015 - 生成发布就绪证据报告**
  - `depends_on`: N013,N014；`parallel_with`: none
  - `scope`: 汇总测试、性能、真机、攻击面、依赖、体积、已知限制和回滚证据。
  - `RED/GREEN`: 缺少任一强制证据即不标记 ready。
  - `REFACTOR/VERIFY`: 报告不嵌入真实订阅、节点或设备敏感标识。
  - `done`: 每项 D10 acceptance criterion 可追溯到测试或证据。

  - `2026-08-12 evidence`: 既有 `release-readiness-v1.json` 仍是 2026-08-11 的历史快照；本轮新增真机证据尚未汇入新的 release-readiness artifact。N003 与 N006-N012 仍未完成，因此 `ready=false` 结论不变。

- [ ] **N016 - 通过最终完成 gate**
  - `depends_on`: N015；`parallel_with`: none
  - `scope`: 重新执行桌面、Android、真机和文档全部必需 gate。
  - `RED/GREEN`: 任一失败、跳过或证据缺失都不允许宣布完成。
  - `REFACTOR/VERIFY`: 检查工作树无意外生成物，不自动提交或推送。
  - `done`: 本清单完成定义全部满足，可以进入独立发布评审。

## 22. 推荐执行顺序

按底层到消费层执行；同一阶段内只有显式 `parallel_with` 的节点允许并行。每个节点完成后先达到自身 `done`，再启动依赖它的节点。

1. A001-A008：建立隔离 workspace、测试骨架、fixture 和重构护栏。
2. B001-B012：冻结 schema v3、公共模型、诊断、digest 和资源限制。
3. C001-C010：实现 source active set、来源状态、LKG 与 source transaction。
4. D001-D010：实现 dedupe、来源配额和 fair auto candidate pool。
5. E001-E011：实现 core composer、generation node registry 和 check 边界。
6. F001-F015：实现 source/mode 原子事务、journal、崩溃恢复和回滚。
7. G001-G013：实现 selection intent、递归 active terminal、cycle/depth 防护。
8. H001-H012：实现 loopback Clash API 查询、测速、分级超时、切换和健康控制。
9. I001-I013：冻结 Protocol v3、CLI 命令、事件帧和错误映射。
10. J001-J012：实现 typed DTO、store、bridge、事件 reducer 和版本门禁。
11. K001-K011：实现订阅页 single/merge、source 操作和事务反馈。
12. L001-L012：实现节点页、概览代理质量、测速按钮、导航和虚拟列表。
13. M001-M016：运行主机端端到端、回归、安全、性能和依赖 gate。
14. N001-N016（含 N002A）：构建模块、验证覆盖安装配置保留、安装真机、验证 TUN/TPROXY、清理旧路径并形成完成报告。

gRPC API service、protobuf、Tokio/通用 async runtime 和 libbox AAR 不属于本清单当前实现路径；相关未来候选只记录在独立设计文档，不得以“预留”名义添加实现任务。

## 23. 阶段验证命令

命令必须在对应阶段结束时执行。测试命令按仓库实际脚本调整，但不得降低测试层级或删除失败用例。

```powershell
# Rust workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p nethop-subscription --locked

# WebUI
Set-Location "webui"
npm ci
npm run gate
Set-Location ".."

# 静态契约与任务清单
rg -n "proxy\.selector_mode|ConfigMutation::SelectSource|ControlMethod::NodeSelect|selected:\\s*bool" "crates" "webui"
rg -n "tonic|prost|tokio|libbox" "Cargo.toml" "Cargo.lock" "crates" "webui/package.json"
git diff --check -- "docs/12-subscription-selection-and-node-optimization-tdd-task-list.md"
```

两条静态契约搜索在最终 gate 中都应无输出；若仓库工具把 `rg` 的“无匹配”退出码视为失败，应由 gate 脚本显式反转该断言，而不是忽略搜索结果。

Android arm64 和真机命令必须使用已锁定的交叉编译、模块打包与设备测试脚本；每次结果保存到本地证据目录，并确认其中不含真实订阅 URL、节点凭据或设备隐私信息。

## 24. TDD 完成定义

一个任务只有同时满足以下条件才可以勾选：

- 先存在可复现的 RED 测试，测试失败原因对应任务范围内的一个缺口。
- GREEN 实现只改变完成该任务所需的最小行为，不借机引入未来候选能力。
- REFACTOR 后测试仍通过，且没有遗留旧 schema、旧 wire shape、重复状态源或静默 fallback。
- 单元测试、集成测试、browser/E2E 测试和真机测试（若任务标注）均有明确证据。
- 任务产物只属于一个节点；跨节点共享代码通过已完成的公共模型或测试 helper 提供。
- `depends_on` 的所有节点已经完成；`parallel_with` 的节点之间不得修改同一未冻结契约。

阶段 gate 不是形式性勾选：gate 失败时必须回到第一个失败节点重新 RED→GREEN→REFACTOR，禁止通过跳过、屏蔽、扩大超时或只保留 happy path 来“修绿”。

## 25. 最终完成定义

本次重构仅在以下条件全部满足后才算完成：

- schema v3、Protocol v3、source mode、selection intent 和 active terminal 的语义在代码、CLI、WebUI、文档中一致。
- single、merge、auto、manual、source 禁用、节点消失、更新失败和崩溃恢复均有前后测试。
- 新 generation 发布是事务化的；失败保持 last-known-good，旧连接不因选择变化被主动中断。
- WebUI 只消费 daemon typed snapshot 和事件，不从节点列表、配置顺序或本地乐观状态推断 active。
- 10,000 节点后端和 WebUI 性能预算通过，且移动端 URLTest 周期满足功耗/流量预算。
- 真机 TPROXY、TUN、merge fair pool、手动选择、自动优选、回滚和泄密扫描通过。
- 旧 schema v2 和含混选择路径已删除；没有兼容层、隐式迁移或未实现的假能力。
- 未新增 gRPC、protobuf、Tokio/通用 async runtime、额外监听端口或 libbox AAR 集成。
- 证据报告、构建清单、SBOM/许可证清单和文档交叉引用完整。

达到以上定义后，才能把实现交给独立发布评审；在此之前只能称为开发期重构进行中。
