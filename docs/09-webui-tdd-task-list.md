# NetHop WebUI TDD 开发任务清单

> 状态：Implementation Task Baseline v0.1
>
> 上位设计：`docs/08-webui-design.md`
>
> 适用范围：KernelSU Module WebUI、APatch 兼容验证、Magisk Action/CLI 回归
>
> 原则：开发期允许破坏性重构，不维护未发布接口兼容层；必须用变更前后测试证明新能力增加且既有用户行为仍成立

## 1. 目的

本文把 `08-webui-design.md` 拆成可按 TDD 执行的有向无环任务图。每个任务节点只交付一个可验证结果；节点之间要么由 `depends_on` 明确递进，要么由 `parallel_with` 明确允许并行。

本文不是页面愿望清单。任何 UI 能力必须先有稳定的 daemon/CLI JSON 契约，再经过宿主桥、DTO 校验、状态机，最后才能进入组件。页面不得临时解析 human text、拼接任意 shell、直接修改 TOML 或绕过 `nethopctl` 访问 Clash API。

## 2. 已核验事实

### 2.1 当前 NetHop 基线

- 当前协议版本为 `PROTOCOL_VERSION = 1`。
- `nethopctl` 已覆盖 hello、status、start/stop、capability、config、subscription、node、application、connection、logs、diagnostics、topology、traffic、ruleset、backup 和 core version check。
- `events.subscribe` 已存在，但 `EventKind` 当前只有 `config/runtime/subscription/generation/network`，尚无 `traffic`。
- 当前不存在 `nethopctl webui payload create/append/commit/remove` 受控命令。
- `traffic.get` 已存在，因此 traffic 事件应复用现有样本模型，不另造第二套统计语义。

### 2.2 官方资料约束

- KernelSU Module WebUI 必须位于模块 `webroot/`，入口必须是 `webroot/index.html`；JavaScript API 提供 `exec`、`spawn`、包列表、Toast、edge-to-edge 和退出能力。
- APatch 官方声明 WebUI 与 KernelSU 兼容；NetHop 仍以真实设备契约测试作为稳定支持证据。
- Vite 在未知部署基路径时支持 `base: './'`；WebUI 必须构建为完全本地、相对路径资产。
- Vue 官方建议真实测量 production bundle、路由懒加载、稳定 props，并用 `shallowRef` 处理大型不可变结构。
- Vitest Browser Mode 在真实浏览器 DOM、CSS 和浏览器 API 中测试组件；Node 项目与 Browser 项目应分开。
- Playwright 的截图基线必须在固定环境生成；首版验收聚焦布局、交互、安全和性能，无障碍不属于发布范围。
- TanStack Virtual 的 Vue adapter 提供 `useVirtualizer`；稳定业务 key、偏大的动态高度估计、`measureElement`、snapshot 和 offset 是列表身份与滚动恢复的关键。

### 2.3 本地参考项目取舍

| 项目 | 吸收 | 不吸收 |
|---|---|---|
| NetProxy-Magisk | 真机/mock 双宿主、Android 返回、`listPackages/getPackagesInfo`、`ksu://icon/` | 页面任意执行 shell、直接改配置文件、业务状态写 localStorage |
| MagicNet | action lock、后台操作状态、私有 payload、脱敏、纯 planner/view-model、失败清理 | human CLI 文本耦合、高频轮询、页面内命令拼接 |
| Surfing | 仅作为“不采用外部 Clash 面板跳转”的反例 | 外部控制面跳转 |
| TDesign Mobile Vue | 移动交互、主题 token、组件测试思路 | 全量注册、机械复制 Cypress 栈 |
| VueUse | 生命周期、visibility、observer、节流与清理 | WebSocket/fetch 控制面、轮询替代事件流、业务状态持久化 |
| TanStack Virtual | 单列虚拟化、稳定 key、动态测量、滚动恢复 | 多 lane、masonry、无限加载、其他 TanStack 产品 |

## 3. 开发期破坏性重构规则

1. 不为未发布的旧 WebUI/wire contract 写兼容 shim。
2. 新契约需要不兼容 wire 变更时，直接提升 protocol version 并删除旧分支。
3. 删除旧路径前先冻结 `before` fixture 和既有行为清单。
4. 新契约先写 RED 测试，再实现最小 GREEN；不得先写实现后补测试。
5. 变更后的回归套件必须证明 start/stop、配置、订阅、节点、应用、网络与日志等旧功能通过新契约继续工作。
6. “不兼容旧 wire bytes”不等于“允许旧功能退化”。旧字节可被明确拒绝，旧用户能力必须在新接口上继续成立。
7. 重构完成后删除旧 API、旧 fixture、旧依赖和死代码；禁止新旧双路径长期并存。
8. 每次破坏性变更必须留下 `before fixture + new RED + after golden + regression comparison` 四类证据。

## 4. TDD 节点规范

每个任务节点必须执行：

```text
RED       添加一个因目标能力缺失而失败的最小测试
GREEN     只实现让该测试通过的最小生产代码
REFACTOR  消除本节点引入的重复，保持窄接口和单一职责
VERIFY    运行节点测试、直接依赖测试和指定生产验证
```

节点完成条件：

- RED 失败原因与目标一致，不是环境、fixture 或拼写错误。
- GREEN 后新测试通过。
- 直接前驱与既有回归测试通过。
- 不新增未使用依赖、任意 shell、远程资产、敏感日志或无界集合。
- 产物和测试路径已记录；手工验证不能替代自动测试。

任务节点字段：

- `depends_on`：必须全部完成后才能开始。
- `parallel_with`：依赖满足后允许同时进行；`none` 表示应串行。
- `RED/GREEN/REFACTOR/VERIFY`：本节点唯一交付物的 TDD 操作。
- `done`：可客观判断的结束条件。

## 5. 测试分层与固定证据

| 层级 | 工具 | 只负责 |
|---|---|---|
| Rust 单元/契约 | `cargo test` | protocol、daemon、CLI、安全边界、JSON/JSONL wire |
| 前端 Node 单元 | Vitest Node | validator、planner、reducer、parser、状态机、纯函数 |
| Vue 组件 | Vitest Browser Mode + `vitest-browser-vue` | 真实 DOM/CSS、交互、焦点、组件生命周期 |
| 应用 E2E | Playwright Test | 完整路由、MockHost、截图、axe、性能采样 |
| 模块契约 | PowerShell/Rust 测试 | `webroot`、manifest、checksums、license、ZIP 结构 |
| Android 真机 | 固定脚本 + 人工操作记录 | KernelSU/APatch bridge、WebView 性能、root 进程清理、完整闭环 |

固定目录建议：

```text
webui/
  src/
  tests/
    unit/
    browser/
    e2e/
    fixtures/protocol/
    fixtures/host/
    fixtures/performance/
    screenshots/
  scripts/
crates/nethop-protocol/tests/fixtures/webui/
crates/nethopctl/tests/fixtures/webui/
crates/nethopd/tests/fixtures/webui/
artifacts/webui/                 # CI 产物，不提交设备敏感数据
```

fixture 禁止包含真实订阅 URL、token、UUID、密码、节点私钥和设备包列表。所有 secret canary 使用明确的测试前缀。

## 6. 依赖总图与推荐顺序

```text
A 基线与护栏
  -> B daemon/CLI 前置契约
  -> C 工程与依赖门禁
  -> D Host/Bridge
  -> E DTO 与校验
  -> F 事件流
  -> G Store/状态机
  -> H App Shell
  -> I 共享组件
  -> J 概览
  -> K 订阅
  -> L 节点
  -> M 应用
  -> N 设置
  -> O 运维能力
  -> P 性能、安全、真机与发布
```

- B 与 C 在 A gate 后可并行。
- D 依赖 B 的 CLI 契约和 C 的工程骨架。
- E 的各 DTO validator 可按领域并行；F 只依赖 hello/event DTO。
- J、K、L、M、N 在 G/H/I 完成后可按页面并行，但写操作统一复用同一 operation/CAS 状态机。
- O 在稳定日常闭环后进行，避免用次要运维页阻塞 W1。
- P 中静态门禁可持续并行，真机发布 gate 必须等待所有目标页面完成。

## 7. 阶段 A：基线、测试骨架与重构护栏

- [x] **A001 - 冻结当前 CLI JSON 行为清单**
  - `depends_on`: none；`parallel_with`: A002, A003
  - `RED`: 写测试要求所有当前稳定 `CliCommand` 都有 method、JSON 成功和 JSON 失败样本，先暴露缺失样本。
  - `GREEN`: 为缺失命令补脱敏 golden，不改变生产行为。
  - `REFACTOR`: 用共享 fixture builder 去除重复 request ID 和 envelope。
  - `VERIFY`: `cargo test -p nethopctl --test cli_contracts`。
  - `done`: 当前稳定 CLI 能力均有 before golden 和功能名称映射。

- [x] **A002 - 冻结当前 protocol wire 基线**
  - `depends_on`: none；`parallel_with`: A001, A003
  - `RED`: 要求 hello、request、response、stream item/end/error 与五类现有事件都有 golden。
  - `GREEN`: 补齐 protocol v1 fixture。
  - `REFACTOR`: 固定字段排序只用于 golden 可读性，不把排序写入 wire 语义。
  - `VERIFY`: `cargo test -p nethop-protocol`。
  - `done`: protocol v1 before fixture 可独立证明旧 wire 行为。

- [x] **A003 - 冻结当前模块无 WebUI 基线**
  - `depends_on`: none；`parallel_with`: A001, A002
  - `RED`: 增加模块结构测试，断言当前产物尚未包含受管理的 `webroot/index.html`。
  - `GREEN`: 保存当前 ZIP tree 与 manifest fixture，不修改模块。
  - `REFACTOR`: 复用现有 ZIP 检查器读取条目。
  - `VERIFY`: 运行模块 package contract 测试。
  - `done`: 后续加入 WebUI 时可做 before/after 结构比较。

- [x] **A004 - 建立旧功能回归矩阵**
  - `depends_on`: A001, A002；`parallel_with`: A005
  - `RED`: 写矩阵完整性测试，要求 start/stop、config、subscription、node、application、network、logs 至少各有一条正向和失败路径。
  - `GREEN`: 将现有测试映射到矩阵，缺项只补测试。
  - `REFACTOR`: 用稳定 capability ID 代替文件名隐式约定。
  - `VERIFY`: 运行映射脚本并确认无 `uncovered`。
  - `done`: 每次破坏性改动都有同一套旧功能比较基线。

- [x] **A005 - 建立测试 secret canary 集**
  - `depends_on`: A001；`parallel_with`: A004
  - `RED`: 测试扫描 golden、日志、错误和构建产物时发现故意植入的 canary。
  - `GREEN`: 建立集中 canary 列表和扫描脚本。
  - `REFACTOR`: 统一 URL、bearer、UUID、password、private key 模式。
  - `VERIFY`: 正例扫描失败、清除 canary 后通过。
  - `done`: 所有后续 fixture 可复用同一敏感信息门禁。

- [x] **A006 - 建立破坏性变更证据校验器**
  - `depends_on`: A004；`parallel_with`: none
  - `RED`: 构造缺少 before/new/after/regression 任一项的变更清单并断言失败。
  - `GREEN`: 实现轻量 manifest 校验脚本。
  - `REFACTOR`: 只校验证据路径和测试 ID，不引入通用工作流引擎。
  - `VERIFY`: 用本地正反 fixture 运行脚本。
  - `done`: 不兼容协议变更不能在缺证据时通过 CI。

- [x] **A007 - 固定 Node 与 Rust 测试命令入口**
  - `depends_on`: A004；`parallel_with`: A008
  - `RED`: 测试统一入口在任一子套件失败时返回非零。
  - `GREEN`: 增加只编排现有命令的脚本。
  - `REFACTOR`: 禁止脚本吞掉 stderr 或自动更新 golden。
  - `VERIFY`: 故意失败一个 fixture，再恢复并全绿。
  - `done`: 本地与 CI 使用同一测试入口。

- [x] **A008 - 建立 WebUI 决策记录模板**
  - `depends_on`: A004；`parallel_with`: A007
  - `RED`: 校验依赖升级、协议破坏、预算放宽三类决定缺字段时失败。
  - `GREEN`: 新增最小 ADR 模板，包含问题、数据、选择、删除路径与复测。
  - `REFACTOR`: 不为普通页面改动强制 ADR。
  - `VERIFY`: 用 VueUse 选型作为示例验证模板。
  - `done`: 只有高影响决策承担治理成本。

- [x] **A009 - 阶段 A 门禁**
  - `depends_on`: A003, A005, A006, A007, A008；`parallel_with`: none
  - `RED`: gate 在任何 before fixture 或回归映射缺失时失败。
  - `GREEN`: 将 A 阶段证据接入统一入口。
  - `REFACTOR`: gate 仅聚合结果，不重复执行实现逻辑。
  - `VERIFY`: clean checkout 运行 A gate。
  - `done`: B/C 可在有完整回归护栏后启动。

阶段 A 完成证据：

```powershell
pwsh -NoProfile -File "scripts/webui-phase-a-gate.ps1"
```

该入口聚合 workspace Rust 回归、CLI/protocol before golden、模块无 WebUI 基线、七领域正反回归矩阵、secret canary、破坏性变更证据和模块契约。前端 workspace 尚未进入 C 阶段时，统一入口会明确报告跳过 Node 子套件；一旦 `webui/package.json` 存在，则强制执行 `test` 与 `typecheck`。

## 8. 阶段 B：daemon、protocol 与 CLI 前置契约

- [x] **B001 - 定义 traffic 事件 wire RED 契约**
  - `depends_on`: A009；`parallel_with`: C001
  - `RED`: 新增 `EventKind::Traffic` 序列化、过滤与旧 v1 拒绝的新契约测试并确认失败。
  - `GREEN`: 只提交测试和 v2 after golden，不实现 variant。
  - `REFACTOR`: traffic payload 复用 `traffic.get` 的字段命名。
  - `VERIFY`: 确认失败点仅为协议缺少 traffic。
  - `done`: traffic wire 形状和版本策略已冻结。

- [x] **B002 - 提升 WebUI 所需 protocol version**
  - `depends_on`: B001；`parallel_with`: none
  - `RED`: v2 hello 与 v1 peer 的精确不兼容测试失败。
  - `GREEN`: 提升协议版本并加入 `Traffic` variant，明确拒绝 v1，不保留 shim。
  - `REFACTOR`: 删除仅服务 v1 的分支和失效 fixture。
  - `VERIFY`: protocol 新契约通过，before fixture 由差异测试确认被拒绝。
  - `done`: 新 wire 单一路径生效且旧字节失败可诊断。

- [x] **B003 - 发布 traffic ephemeral 样本**
  - `depends_on`: B002；`parallel_with`: B005
  - `RED`: daemon 测试要求显式订阅 traffic 后收到最新 `TrafficSample`。
  - `GREEN`: 从现有采样源发布 traffic event。
  - `REFACTOR`: 复用 `traffic.get` 转换函数，避免两套数值语义。
  - `VERIFY`: `cargo test -p nethopd --test event_contracts`。
  - `done`: 同一 JSONL 流能发送有界 traffic 样本。

- [x] **B004 - 隔离 traffic coalesced lane**
  - `depends_on`: B003；`parallel_with`: none
  - `RED`: 压力测试要求 1,000 个 traffic 样本不占用普通 replay ring，只保留最新值。
  - `GREEN`: 为 traffic 实现 ephemeral latest-value lane。
  - `REFACTOR`: 普通事件持久化路径不感知 traffic payload 细节。
  - `VERIFY`: ring 容量、sequence 和 resync 压力测试。
  - `done`: traffic 不填满持久事件日志且普通事件不丢失。

- [x] **B005 - 支持 traffic event-kind CLI 过滤**
  - `depends_on`: B002；`parallel_with`: B003
  - `RED`: CLI parser 对 `--event-kinds ...traffic` 的测试失败。
  - `GREEN`: 接受 traffic 枚举并传入 `events.subscribe`。
  - `REFACTOR`: 事件名解析只保留一个映射表。
  - `VERIFY`: CLI contract 和 daemon UDS contract。
  - `done`: WebUI 可在单一事件进程中显式请求 traffic。

- [x] **B006 - 定义私有 payload namespace 模型**
  - `depends_on`: A009；`parallel_with`: B001, C001
  - `RED`: Rust 测试拒绝未知 namespace、路径分隔符、空 basename 和超长 basename。
  - `GREEN`: 新增窄 `WebUiPayloadNamespace` 与 basename validator。
  - `REFACTOR`: namespace 不接受用户提供任意目录。
  - `VERIFY`: property/corpus 测试覆盖 traversal 与 Unicode 混淆输入。
  - `done`: payload 目标只能映射到 daemon 拥有的固定私有目录。

- [x] **B007 - 实现 payload create**
  - `depends_on`: B006；`parallel_with`: none
  - `RED`: 测试要求独占创建、0600、随机服务端句柄并拒绝同名覆盖。
  - `GREEN`: 实现 `nethopctl webui payload create`。
  - `REFACTOR`: 文件创建封装在单一 storage service。
  - `VERIFY`: CLI JSON 正反契约与文件权限测试。
  - `done`: create 只返回不含敏感内容的受控 handle。

- [x] **B008 - 实现 payload append**
  - `depends_on`: B007；`parallel_with`: none
  - `RED`: 测试覆盖合法 base64 chunk、非法 base64、单 chunk 上限和累计上限。
  - `GREEN`: 实现有界 append。
  - `REFACTOR`: 流式解码，不为完整 payload 建立第二份无界副本。
  - `VERIFY`: 分块边界、UTF-8 多字节与超限清理测试。
  - `done`: append 不把 chunk 或 decoded secret 写入输出和日志。

- [x] **B009 - 实现 payload commit**
  - `depends_on`: B008；`parallel_with`: none
  - `RED`: 测试要求 commit 只允许 operation allowlist、consume-before-apply 和失败后不可重放。
  - `GREEN`: 实现原子 consume 并调用对应 typed operation。
  - `REFACTOR`: operation adapter 不接受 shell 字符串。
  - `VERIFY`: 成功、业务失败、daemon 崩溃恢复和二次 commit 测试。
  - `done`: payload 最多消费一次且真实操作仍走 daemon 校验/CAS。

- [x] **B010 - 实现 payload remove**
  - `depends_on`: B007；`parallel_with`: B008
  - `RED`: 测试要求 remove 幂等、只删除 owned handle、拒绝 symlink。
  - `GREEN`: 实现受控清理。
  - `REFACTOR`: create/commit/remove 共享 owned-path 验证器。
  - `VERIFY`: traversal、symlink、hardlink 和重复 remove 测试。
  - `done`: 前端失败可显式清理且不能删除目录外文件。

- [x] **B011 - 实现 payload TTL 回收**
  - `depends_on`: B010；`parallel_with`: B009
  - `RED`: fake clock 测试要求过期未提交 payload 被删除、活跃 payload 保留。
  - `GREEN`: 在低频维护路径加入 TTL cleanup。
  - `REFACTOR`: 不新增常驻高频 timer。
  - `VERIFY`: 重启恢复和大量过期文件的有界扫描测试。
  - `done`: WebUI 异常退出不会永久遗留 secret 文件。

- [x] **B012 - 限制 WebUI CLI 输出边界**
  - `depends_on`: B005, B009；`parallel_with`: B013
  - `RED`: 测试要求 stdout/stderr、数组、字符串和诊断 bundle 元数据都有明确上限。
  - `GREEN`: 在 WebUI 可达命令响应加入统一 bounds。
  - `REFACTOR`: 复用 protocol 常量，不在各命令散落数字。
  - `VERIFY`: 超大 logs/nodes/connections 响应测试。
  - `done`: 恶意或异常 daemon 输出不能拖垮 WebView。

- [x] **B013 - 固化 WebUI 错误码**
  - `depends_on`: B002, B006；`parallel_with`: B012
  - `RED`: 测试要求 incompatible、timeout、conflict、invalid-payload、limit 和 unavailable 有稳定 code。
  - `GREEN`: 补齐 typed error code，不依赖 message 文本判断。
  - `REFACTOR`: human message 留在展示层，wire 只传稳定 code/params。
  - `VERIFY`: Rust golden 与未知 code 前端降级 fixture。
  - `done`: WebUI 状态机无需解析自然语言错误。

- [x] **B014 - 更新 CLI/protocol after golden**
  - `depends_on`: B004, B011, B012, B013；`parallel_with`: none
  - `RED`: after-golden 完整性测试因新增命令/事件缺失而失败。
  - `GREEN`: 写入 v2 hello、traffic 和 payload 命令 golden。
  - `REFACTOR`: 删除旧 v1 active fixture，只保留 before 证据。
  - `VERIFY`: golden diff 人工审查 + Rust 全测。
  - `done`: 新 wire 的公开边界有可复核快照。

- [x] **B015 - 执行旧 CLI 功能回归比较**
  - `depends_on`: B014；`parallel_with`: none
  - `RED`: 比较器先报告 v1/v2 envelope 变化及未映射能力。
  - `GREEN`: 将既有能力映射到 v2 并验证语义结果不变。
  - `REFACTOR`: 不忽略字段差异，显式列出允许变化的 version/event 集合。
  - `VERIFY`: 运行 A004 全矩阵。
  - `done`: 新功能增加，原有用户能力在 v2 上全部通过。

- [x] **B016 - 阶段 B 门禁**
  - `depends_on`: B015；`parallel_with`: none
  - `RED`: gate 在 traffic 进入 replay ring、payload 泄密或旧功能缺失时失败。
  - `GREEN`: 聚合 protocol/CLI/daemon/security 回归。
  - `REFACTOR`: gate 不运行真实生产订阅。
  - `VERIFY`: `cargo test --workspace` 与 secret scan。
  - `done`: 前端可依赖的后端契约已稳定。

阶段 B 完成证据：

```powershell
pwsh -NoProfile -File "scripts/webui-phase-b-gate.ps1"
```

该入口聚合 protocol v2、CLI、daemon payload、traffic coalesced lane、旧功能回归矩阵、模块契约和 secret canary 扫描。traffic 只走 latest-value ephemeral lane；payload 仅允许 daemon 拥有的 `config` namespace，采用服务端随机 handle、`0600` 文件、12 KiB chunk、1 MiB 总量、consume-before-apply 和 15 分钟 TTL。

## 9. 阶段 C：WebUI workspace、依赖与构建门禁

- [x] **C001 - 初始化 WebUI workspace**
  - `depends_on`: A009；`parallel_with`: B001, B006
  - `RED`: 模块测试要求存在独立 `webui/package.json`、源码入口和非产物目录时失败。
  - `GREEN`: 建立 Vue 3 + TypeScript + Vite 最小工程。
  - `REFACTOR`: 不引入状态库、HTTP client、图表库或自动 import。
  - `VERIFY`: `npm ci` 和空应用 production build。
  - `done`: workspace 能独立构建且无 root 能力。

- [x] **C002 - 固定 package-lock 依赖图**
  - `depends_on`: C001；`parallel_with`: C003
  - `RED`: lockfile 门禁对浮动直接依赖和缺 license 失败。
  - `GREEN`: 锁定设计指定的最新稳定直接依赖及完整传递依赖。
  - `REFACTOR`: 删除未使用包和重复测试工具。
  - `VERIFY`: clean `npm ci` 可复现安装。
  - `done`: 构建不依赖 `refer/` 或开发机全局包。

- [x] **C003 - 启用 TypeScript strict**
  - `depends_on`: C001；`parallel_with`: C002
  - `RED`: 加入隐式 any、未检查索引和错误 catch fixture 并确认 typecheck 失败。
  - `GREEN`: 配置 strict、noUncheckedIndexedAccess 等必要选项。
  - `REFACTOR`: 不以全局类型断言绕过 bridge DTO。
  - `VERIFY`: `npm run typecheck`。
  - `done`: 生产源码无 `any` 逃生口，测试 mock 例外需局部说明。

- [x] **C004 - 建立 Vitest Node 项目**
  - `depends_on`: C003；`parallel_with`: C005
  - `RED`: 一个纯函数样例在未配置项目时失败。
  - `GREEN`: 配置 `tests/unit` Node project。
  - `REFACTOR`: Node 测试不得依赖 jsdom 偶然行为。
  - `VERIFY`: 单独运行 unit project。
  - `done`: validator/planner/reducer 有快速测试入口。

- [x] **C005 - 建立 Vitest Browser 项目**
  - `depends_on`: C003；`parallel_with`: C004
  - `RED`: Vue 组件真实 CSS/DOM 样例在 Node 环境失败。
  - `GREEN`: 用 Browser Mode、Playwright provider 和 `vitest-browser-vue` 配置 Chromium。
  - `REFACTOR`: unit 与 browser 文件约定互斥。
  - `VERIFY`: 单独运行 browser project。
  - `done`: 组件测试运行在真实浏览器而非手工 DOM 模拟。

- [x] **C006 - 建立 Playwright E2E 项目**
  - `depends_on`: C001；`parallel_with`: C004, C005
  - `RED`: 完整应用 smoke test 因无 preview server 配置失败。
  - `GREEN`: 配置固定 Chromium、preview server、trace 和 artifact 路径。
  - `REFACTOR`: 不与 Vitest Browser 重复低层组件用例。
  - `VERIFY`: 本地 mock 首页 smoke test。
  - `done`: 整应用路由、截图和性能有独立入口。

- [x] **C007 - 配置 hash router 与相对资产**
  - `depends_on`: C001；`parallel_with`: C008
  - `RED`: 从嵌套 `file-like` 基路径打开 build 的测试失败。
  - `GREEN`: 配置 hash history 与 `base: './'`。
  - `REFACTOR`: 禁止运行时拼绝对资产路径。
  - `VERIFY`: production `index.html` 离线加载无 404。
  - `done`: KernelSU WebView 无服务器 rewrite 也能导航。

- [x] **C008 - 固定 Chrome 105 构建目标**
  - `depends_on`: C001；`parallel_with`: C007
  - `RED`: 产物语法扫描对超出目标的语法失败。
  - `GREEN`: 配置 `build.target = 'chrome105'`。
  - `REFACTOR`: 不引入 legacy plugin 或远程 polyfill。
  - `VERIFY`: build + 目标 WebView smoke fixture。
  - `done`: 宿主低于 105 时由静态兼容页明确拒绝。

- [x] **C009 - 建立 TDesign 显式导入门禁**
  - `depends_on`: C002；`parallel_with`: C010
  - `RED`: `app.use(TDesign)` 和默认全量 import fixture 被检测。
  - `GREEN`: 添加静态规则并只允许命名导入。
  - `REFACTOR`: 规则不误伤 TDesign 类型导入。
  - `VERIFY`: 正反源码 fixture + production metafile。
  - `done`: 页面实际使用组件可由源码和 bundle 同时审计。

- [x] **C010 - 建立 Tabler 静态图标门禁**
  - `depends_on`: C002；`parallel_with`: C009
  - `RED`: namespace import 和字符串动态图标表 fixture 被检测。
  - `GREEN`: 添加 lint/AST contract。
  - `REFACTOR`: 允许明确的静态图标映射常量。
  - `VERIFY`: 正反 fixture 与 bundle 模块清单。
  - `done`: 不会把整套 Tabler 图标打入 WebUI。

- [x] **C011 - 完成 VueUse 三方案对照**
  - `depends_on`: C002, C005, C009；`parallel_with`: C012
  - `RED`: 基准脚本因缺三组 bundle/组件结果而失败。
  - `GREEN`: 测量 10.7 去重、14.4 双版本、14.4 override。
  - `REFACTOR`: 删除非获胜 lockfile 与临时 override。
  - `VERIFY`: TDesign 选用组件回归、gzip、metafile、WebView smoke。
  - `done`: 冻结一个无重复 VueUse runtime 的方案并记录 ADR。

- [x] **C012 - 完成虚拟列表候选对照**
  - `depends_on`: C002, C005；`parallel_with`: C011
  - `RED`: 10,000 固定行 fixture 暴露无虚拟化 DOM/时延超限。
  - `GREEN`: 对比 TanStack 与 VueUse 候选，只记录数据。
  - `REFACTOR`: 删除 VueUse virtual list 和临时自研候选。
  - `VERIFY`: 固定/动态、深度跳转、筛选重排与 cleanup 基准。
  - `done`: TanStack 作为唯一引擎且实际 bundle delta 已记录。

- [x] **C013 - 建立 production bundle budget**
  - `depends_on`: C011, C012；`parallel_with`: C014
  - `RED`: 人工超限 chunk、重复 VueUse 和额外 TanStack 包使脚本失败。
  - `GREEN`: 解析 Vite metafile、gzip 与 webroot 总大小。
  - `REFACTOR`: 预算常量只在一个配置文件定义。
  - `VERIFY`: 输出最大 20 模块和各页面 chunk。
  - `done`: `08` 的 JS/CSS/chunk/webroot 硬门槛可自动执行。

- [x] **C014 - 建立本地资产与 CSP 门禁**
  - `depends_on`: C007；`parallel_with`: C013
  - `RED`: CDN、远程字体、connect-src、inline remote loader fixture 被拒绝。
  - `GREEN`: 写严格 CSP 和静态资产扫描器。
  - `REFACTOR`: 开发 server 例外不进入 production HTML。
  - `VERIFY`: 离线 build、CSP console 和网络请求录制。
  - `done`: production WebUI 不发起外部网络连接。

- [x] **C015 - 阶段 C 门禁**
  - `depends_on`: C004, C005, C006, C008, C009, C010, C013, C014；`parallel_with`: none
  - `RED`: 任一测试项目、类型、依赖或 bundle 失败时 gate 非零。
  - `GREEN`: 聚合 npm ci/typecheck/test/build/budget/security。
  - `REFACTOR`: 禁止 gate 自动修复 lockfile 或截图。
  - `VERIFY`: clean workspace 运行 C gate。
  - `done`: 可在稳定工程边界上实现 bridge。

阶段 C 完成证据：

```powershell
pwsh -NoProfile -File "scripts/webui-phase-c-gate.ps1"
```

该入口从 `npm ci --ignore-scripts` 开始，依次执行 TypeScript strict、Vitest Node、Vitest Browser、Playwright E2E、production build、显式导入、依赖图、bundle budget 与 CSP/本地资产门禁。当前基线为 JS gzip 47,441 B、CSS gzip 5,805 B、webroot 164,916 B；VueUse 与虚拟列表选择分别冻结在 `WEBUI-001` 和 `WEBUI-002` ADR。

## 10. 阶段 D：宿主适配器与窄 Bridge

- [x] **D001 - 定义 HostAdapter 接口**
  - `depends_on`: B016, C015；`parallel_with`: E001
  - `RED`: compile-time test 要求 host 只暴露 spawn/exec result、package、toast、edge、exit 等窄能力。
  - `GREEN`: 新增 framework-neutral interface。
  - `REFACTOR`: 页面不可取得原始 KernelSU module 对象。
  - `VERIFY`: TypeScript type tests。
  - `done`: 所有宿主差异只能进入 adapter 层。

- [x] **D002 - 实现 Browser MockHost**
  - `depends_on`: D001；`parallel_with`: D003
  - `RED`: fixture 驱动的 success/fail/timeout/stream 测试失败。
  - `GREEN`: 实现可注入时钟和事件的 MockHost。
  - `REFACTOR`: mock 不复制 daemon 业务逻辑，只回放协议 fixture。
  - `VERIFY`: Node 与 E2E smoke。
  - `done`: 无 root 浏览器可复现所有主要状态和故障。

- [x] **D003 - 实现 KernelSU HostAdapter**
  - `depends_on`: D001；`parallel_with`: D002
  - `RED`: mock `kernelsu` API 测试要求正确映射 errno/stdout/stderr 和 spawn 生命周期。
  - `GREEN`: 封装官方 `kernelsu` npm bridge。
  - `REFACTOR`: 只有此文件可 import `kernelsu`。
  - `VERIFY`: 静态 import gate + Browser test。
  - `done`: 页面代码没有通用 root API 引用。

- [x] **D004 - 实现宿主 capability 探测**
  - `depends_on`: D002, D003；`parallel_with`: D005
  - `RED`: KernelSU、APatch 声明兼容、普通浏览器和缺 API fixture 的分类测试失败。
  - `GREEN`: 返回 typed host capability，不做页面分支。
  - `REFACTOR`: APatch 不用 user-agent 猜测功能。
  - `VERIFY`: 四类 fixture。
  - `done`: API 缺失能显示降级而非运行时报错。

- [x] **D005 - 建立操作 allowlist**
  - `depends_on`: D001；`parallel_with`: D004
  - `RED`: 任意 argv、重定向、管道、分号和未知 operation 测试必须拒绝。
  - `GREEN`: 建立 operation ID 到固定 `nethopctl --json/--jsonl` argv 的映射。
  - `REFACTOR`: 页面只传 typed params，不传命令字符串。
  - `VERIFY`: command injection corpus。
  - `done`: WebUI 无任意 shell 能力。

- [x] **D006 - 实现有界一次性命令执行**
  - `depends_on`: D005；`parallel_with`: D007
  - `RED`: stdout/stderr 超限、非零退出和 host error 测试失败。
  - `GREEN`: 实现 bounded exec result。
  - `REFACTOR`: 错误映射复用 B013 code。
  - `VERIFY`: MockHost 边界测试。
  - `done`: 单次命令有大小、退出码和结构化错误边界。

- [x] **D007 - 实现命令超时**
  - `depends_on`: D005；`parallel_with`: D006
  - `RED`: fake timer 测试要求超时后状态确定且迟到结果被忽略。
  - `GREEN`: 增加 operation-specific timeout。
  - `REFACTOR`: UI toast timeout 与命令 timeout 分离。
  - `VERIFY`: race、abort 与迟到事件测试。
  - `done`: hung root command 不永久锁死界面。

- [x] **D008 - 实现 JSON 响应入口**
  - `depends_on`: D006；`parallel_with`: D009
  - `RED`: 空输出、多 JSON、human text、超限 JSON 和无效 UTF-8 fixture 被拒绝。
  - `GREEN`: 实现单 envelope 读取入口。
  - `REFACTOR`: 解析与 DTO validation 分层。
  - `VERIFY`: protocol fixture corpus。
  - `done`: WebUI 永不降级解析 human CLI 文本。

- [x] **D009 - 实现 JSONL chunk 解码器**
  - `depends_on`: D005；`parallel_with`: D008
  - `RED`: 半行、多行、CRLF、跨 UTF-8 chunk、超长行 fixture 失败。
  - `GREEN`: 实现增量 line decoder。
  - `REFACTOR`: buffer 受 MAX_LINE 限制并在 dispose 清空。
  - `VERIFY`: property/chunk permutation 测试。
  - `done`: spawn 任意分块都得到一致 frame 序列。

- [x] **D010 - 实现事件子进程生命周期**
  - `depends_on`: D003, D007, D009；`parallel_with`: D011
  - `RED`: unmount、visibility stop、host exit 和 error 时仍有 orphan 的测试失败。
  - `GREEN`: 封装 spawn/close/exit/error cleanup。
  - `REFACTOR`: 单一 owner 管理子进程句柄。
  - `VERIFY`: fake process 与真机 `ps` 检查脚本。
  - `done`: WebUI 销毁后无残留 `nethopctl events`。

- [x] **D011 - 实现 package 查询适配**
  - `depends_on`: D003；`parallel_with`: D010
  - `RED`: batch info、无效包、系统/用户分类与 API 缺失 fixture 失败。
  - `GREEN`: 封装 `listPackages/getPackagesInfo`。
  - `REFACTOR`: 包信息不经过 shell `pm` fallback。
  - `VERIFY`: HostAdapter browser tests。
  - `done`: 应用页可获得有界 typed package info。

- [x] **D012 - 实现宿主交互适配**
  - `depends_on`: D003；`parallel_with`: D011
  - `RED`: toast、edge-to-edge、exit 的存在/缺失测试失败。
  - `GREEN`: 实现 capability-aware wrapper。
  - `REFACTOR`: 缺失可选 API 静默降级但可观测。
  - `VERIFY`: KernelSU mock contract。
  - `done`: 页面不直接调用宿主 UI API。

- [x] **D013 - 实现私有 payload 客户端**
  - `depends_on`: B016, D006；`parallel_with`: none
  - `RED`: Unicode 分块、append 失败、commit 失败、remove 清理和 preview 脱敏测试失败。
  - `GREEN`: 严格按 create→append→commit/remove 执行。
  - `REFACTOR`: chunk builder 与操作 adapter 分离，payload 不进入 reactive log。
  - `VERIFY`: MagicNet 同类 corpus + NetHop Rust integration fixture。
  - `done`: URL/TOML/导入内容不出现在命令预览或日志。

- [x] **D014 - 阶段 D 门禁**
  - `depends_on`: D004, D008, D010, D011, D012, D013；`parallel_with`: none
  - `RED`: 任意 shell、human parse、orphan process 或 secret preview 时失败。
  - `GREEN`: 聚合 host/bridge/security tests。
  - `REFACTOR`: bridge 不依赖具体页面。
  - `VERIFY`: unit + Browser + fake process suite。
  - `done`: 所有页面只能通过窄 bridge 消费 typed 协议。

阶段 D 完成证据：

```powershell
pwsh -NoProfile -File "scripts/webui-phase-d-gate.ps1"
```

实现证据位于 `webui/src/bridge/`：KernelSU 只在 `kernelsu-host.ts` 导入；operation allowlist 固定 executable、参数、超时和敏感预览；MockHost 支持响应/JSONL/失败注入；JSON 与 JSONL 均有大小、分帧和 UTF-8 边界；payload 客户端严格执行 create -> append -> commit/remove；package info 按 128 项批量查询。事件流使用 128-bit 随机会话 ID，正常卸载通过受控 `webui events terminate` 命令按当前 executable identity 和完整 argv 精确发送 `SIGTERM`；KernelSU 未提供 `ChildProcess.kill()` 时不使用 `pkill` 或模糊进程名。异常 WebView 退出由 300 秒进程硬上限兜底，避免无限残留。

## 11. 阶段 E：DTO、运行时校验与边界模型

本阶段各 validator 均须 `deny unknown by default`；只有明确为版本扩展点的 map 可以保留未知字段。不得引入把所有输入吞成 `any` 的通用 parser。

- [x] **E001 - 校验 hello DTO**
  - `depends_on`: B016, C015；`parallel_with`: D001, E002
  - `RED`: version range、缺字段、未知字段和超长版本字符串 fixture 失败。
  - `GREEN`: 实现 hello validator。
  - `REFACTOR`: version negotiation 规则只定义一次。
  - `VERIFY`: v2 正反 golden。
  - `done`: 不兼容 daemon 在任何业务命令前被阻断。

- [x] **E002 - 校验 status DTO**
  - `depends_on`: C015；`parallel_with`: E001, E003
  - `RED`: runtime state、generation、degraded reason 非法枚举 fixture 失败。
  - `GREEN`: 实现 status validator。
  - `REFACTOR`: 与 event snapshot 共享子模型。
  - `VERIFY`: current daemon fixtures。
  - `done`: 概览只接收完整有界状态。

- [x] **E003 - 校验 capability DTO**
  - `depends_on`: C015；`parallel_with`: E002, E004
  - `RED`: supported/unsupported/experimental/conflict/unavailable 状态 fixture 失败。
  - `GREEN`: 实现 capability validator。
  - `REFACTOR`: UI label 不进入 wire enum。
  - `VERIFY`: capability.get fixture。
  - `done`: 设置控件可解释禁用原因。

- [x] **E004 - 校验 config DTO**
  - `depends_on`: C015；`parallel_with`: E003, E005
  - `RED`: digest、schema version、document 上限和 unknown field fixture 失败。
  - `GREEN`: 实现 config envelope validator。
  - `REFACTOR`: TOML 文本只作为 private payload，不进入 active model。
  - `VERIFY`: config.get/schema fixtures。
  - `done`: active config 与编辑 draft 有稳定输入边界。

- [x] **E005 - 校验 subscription DTO**
  - `depends_on`: C015；`parallel_with`: E004, E006
  - `RED`: source ID、name、URL redaction marker、result 和 diagnostic 上限 fixture 失败。
  - `GREEN`: 实现 source/subscription validator。
  - `REFACTOR`: source URL 不进入列表摘要模型。
  - `VERIFY`: list/update/import fixtures。
  - `done`: 多 source 页面不泄露凭据。

- [x] **E006 - 校验 node DTO**
  - `depends_on`: C015；`parallel_with`: E005, E007
  - `RED`: stable node ID、protocol、latency、selected、source refs 和 page limit fixture 失败。
  - `GREEN`: 实现 node validator。
  - `REFACTOR`: 凭据字段在 schema 中根本不存在。
  - `VERIFY`: 10,000 节点 fixture 与恶意长名称。
  - `done`: 节点列表模型不包含 secret。

- [x] **E007 - 校验 application DTO**
  - `depends_on`: C015；`parallel_with`: E006, E008
  - `RED`: package name、UID、mode 和 shared UID fixture 失败。
  - `GREEN`: 实现 application config validator。
  - `REFACTOR`: 宿主 PackageInfo 与 daemon selection 分成两个模型。
  - `VERIFY`: application list/config fixtures。
  - `done`: 包信息与代理选择可安全合并。

- [x] **E008 - 校验 traffic DTO**
  - `depends_on`: B016, C015；`parallel_with`: E007, E009
  - `RED`: negative、NaN、超大 counter、时间倒退和未知字段 fixture 失败。
  - `GREEN`: 实现 traffic sample validator。
  - `REFACTOR`: snapshot/event 共用同一 parser。
  - `VERIFY`: traffic.get 与 EventKind::Traffic golden。
  - `done`: 曲线不接收非法数值。

- [x] **E009 - 校验 event frame DTO**
  - `depends_on`: E001, E002, E008；`parallel_with`: E010
  - `RED`: snapshot/item/resync/end/error、seq=0 和 payload-kind 不匹配 fixture 失败。
  - `GREEN`: 实现 discriminated event validator。
  - `REFACTOR`: kind-specific payload 委托对应 validator。
  - `VERIFY`: protocol stream corpus。
  - `done`: event reducer 只处理已验证 frame。

- [x] **E010 - 校验运维 DTO**
  - `depends_on`: C015；`parallel_with`: E009
  - `RED`: connections/logs/topology/ruleset/version/diagnostics 的大小与枚举反例失败。
  - `GREEN`: 分领域实现运维 validator facade。
  - `REFACTOR`: 每个领域保持独立 schema 文件，不做巨型 union。
  - `VERIFY`: 所有现有 CLI golden。
  - `done`: W2 能力不解析 human text。

- [x] **E011 - 建立 DTO fuzz/corpus 入口**
  - `depends_on`: E009, E010；`parallel_with`: none
  - `RED`: truncation、深层对象、超长数组和 prototype-shaped key corpus 暴露未处理异常。
  - `GREEN`: 保证 validator 返回 typed failure 且有深度/数量限制。
  - `REFACTOR`: 共享 bounds helper，不共享业务语义。
  - `VERIFY`: corpus 在固定时间和内存内完成。
  - `done`: 不可信 daemon/host 字节不会使页面崩溃或无界分配。

- [x] **E012 - 阶段 E 门禁**
  - `depends_on`: E003, E004, E005, E006, E007, E011；`parallel_with`: none
  - `RED`: 任一未校验 bridge response 进入 store 时静态/运行测试失败。
  - `GREEN`: 强制 bridge→validator→model 数据流。
  - `REFACTOR`: 删除页面局部 JSON cast。
  - `VERIFY`: typecheck + unit corpus。
  - `done`: 前端状态只来源于已验证 DTO。

阶段 E 完成证据：

```powershell
pwsh -NoProfile -File "scripts/webui-phase-e-gate.ps1"
```

`webui/src/model/bounds.ts` 提供统一深度、字符串、数组、对象、整数、digest 和原型键边界；`dto.ts` 以 discriminated union 解析 v2 envelope、状态、能力、配置、订阅、节点、应用、traffic、event 和运维扩展。payload URL、凭据字段和未知字段不会进入业务模型；27 个 Node 测试覆盖 10,000 节点、深层对象、超大数组、NaN/Infinity、prototype-shaped key 与敏感字段反例。

## 12. 阶段 F：实时事件流与恢复

- [x] **F001 - 实现 hello 握手序列**
  - `depends_on`: D014, E012；`parallel_with`: F002
  - `RED`: 测试要求任何业务命令早于 hello 时失败。
  - `GREEN`: 启动时先协商 protocol/host capability。
  - `REFACTOR`: 握手状态与业务 runtime 状态分离。
  - `VERIFY`: compatible、too-old、too-new、daemon unavailable fixture。
  - `done`: 不兼容宿主只能进入只读错误页。

- [x] **F002 - 实现 snapshot-first 启动**
  - `depends_on`: D014, E012；`parallel_with`: F001
  - `RED`: item 先于 snapshot、重复 snapshot 和缺 snapshot fixture 被拒绝。
  - `GREEN`: 只在验证 snapshot 后开放事件应用。
  - `REFACTOR`: snapshot hydration 使用单一 reducer action。
  - `VERIFY`: JSONL chunk permutation 测试。
  - `done`: 初始 UI 不由零散事件拼出不完整状态。

- [x] **F003 - 实现 sequence 去重**
  - `depends_on`: F002；`parallel_with`: F004
  - `RED`: duplicate 和 out-of-order sequence fixture 造成重复状态变化。
  - `GREEN`: 忽略已应用 sequence 并记录有界诊断计数。
  - `REFACTOR`: sequence 逻辑不散落在各 kind handler。
  - `VERIFY`: property-based sequence 测试。
  - `done`: 重复帧不重复触发操作结果或 UI 提示。

- [x] **F004 - 实现 sequence gap 检测**
  - `depends_on`: F002；`parallel_with`: F003
  - `RED`: gap fixture 被静默应用并形成错误状态。
  - `GREEN`: 进入 resync-required，停止增量应用。
  - `REFACTOR`: gap reason 使用稳定 code。
  - `VERIFY`: gap at first/middle/after reconnect。
  - `done`: 缺帧不会被当作一致状态。

- [x] **F005 - 实现 resync**
  - `depends_on`: F004；`parallel_with`: none
  - `RED`: resync 后旧实体、旧 seq 或旧 operation 仍残留的测试失败。
  - `GREEN`: 请求新 snapshot 并原子替换 event state。
  - `REFACTOR`: 保留本地非敏感 UI 偏好，不保留 daemon 事实缓存。
  - `VERIFY`: snapshot revision/seq 恢复测试。
  - `done`: resync 后状态等价于新打开页面。

- [x] **F006 - 实现有界重连退避**
  - `depends_on`: F001, F005；`parallel_with`: F007
  - `RED`: fake clock 显示固定高频重连或无限退避。
  - `GREEN`: 指数退避 + 上限 + jitter + 前台触发立即重试一次。
  - `REFACTOR`: jitter 可注入，测试不依赖真实时间。
  - `VERIFY`: 断线风暴和 daemon 恢复测试。
  - `done`: 断线不忙循环，恢复不要求用户重启 WebUI。

- [x] **F007 - 实现 stale 状态标识**
  - `depends_on`: F001；`parallel_with`: F006
  - `RED`: 断线后旧数据仍以实时状态展示。
  - `GREEN`: 标记 stale 和最后确认时间，禁止危险提交。
  - `REFACTOR`: stale 是 session metadata，不篡改业务 DTO。
  - `VERIFY`: Browser component fixture。
  - `done`: 用户能区分缓存显示与 daemon 当前事实。

- [x] **F008 - 实现页面 visibility 生命周期**
  - `depends_on`: F006；`parallel_with`: F009
  - `RED`: hidden 后仍存在 traffic/日志绘制或周期 root 命令。
  - `GREEN`: 有序关闭/降级事件子进程，visible 时 snapshot-first 恢复。
  - `REFACTOR`: 使用 VueUse scope cleanup，不实现第二套可见性监听器。
  - `VERIFY`: fake visibility + process count。
  - `done`: hidden 后周期 root 命令为 0。

- [x] **F009 - 实现 traffic ring buffer**
  - `depends_on`: F003, E008；`parallel_with`: F008
  - `RED`: 长时间样本导致数组无界增长或时间倒序。
  - `GREEN`: 固定容量、单调时间、不可变快照 ring。
  - `REFACTOR`: 使用 typed arrays/浅响应式仅在基准证明需要时采用。
  - `VERIFY`: 24h 等价样本压力测试。
  - `done`: 曲线内存与样本频率无关地有界。

- [x] **F010 - 实现 traffic 合帧通知**
  - `depends_on`: F009；`parallel_with`: none
  - `RED`: 每个样本触发多次 Vue render 的计数测试失败。
  - `GREEN`: 每 animation frame 最多发布一次可视更新。
  - `REFACTOR`: 协议帧完整消费，只有绘制通知被合并。
  - `VERIFY`: 100Hz fixture 下 render count 和最终值。
  - `done`: 高频样本不阻塞普通事件和交互。

- [x] **F011 - 实现事件流销毁保证**
  - `depends_on`: F008, F010；`parallel_with`: none
  - `RED`: route unmount、host exit、错误页切换后 listener/process/timer 计数非零。
  - `GREEN`: 集中 dispose 所有资源。
  - `REFACTOR`: teardown 幂等并可重复调用。
  - `VERIFY`: effect-scope lifecycle 测试和真机 `ps` fixture。
  - `done`: 无 orphan 子进程、observer、timer 或 listener。

- [x] **F012 - 阶段 F 门禁**
  - `depends_on`: F007, F011；`parallel_with`: none
  - `RED`: snapshot、gap、reconnect、visibility 任一路径错误时 gate 失败。
  - `GREEN`: 聚合 stream state-machine suite。
  - `REFACTOR`: 不以 sleep 稳定异步测试。
  - `VERIFY`: 重复运行确保无 flaky。
  - `done`: 实时层可在断线和生命周期变化中自恢复。

阶段 F 完成证据：

```powershell
pwsh -NoProfile -File "scripts/webui-phase-f-gate.ps1"
```

`EventSession` 强制 hello -> events.subscribe -> snapshot-first，incompatible 终止于只读状态；`EventStateMachine` 处理重复、跳号、resync 和 stale；`ReconnectBackoff` 使用可注入 jitter 的指数退避；visibility hidden 会撤销重试、精确终止事件子进程并停止高频 lane；`TrafficRing` 固定 60 点，`TrafficCoalescer` 每 animation frame 最多发布一次。每个事件子进程最多运行 300 秒，前台会话在正常退出后按既有恢复状态机重新握手并取得新 snapshot；该低频硬上限只承担 WebView 非正常退出的兜底。daemon 的 wire sequence 已改为每个订阅连续计数，内部 ring sequence 与 traffic coalescing 解耦，避免合法过滤/合帧被误判为 gap。

## 13. 阶段 G：Store、配置草稿与操作状态机

- [x] **G001 - 实现 session store**
  - `depends_on`: F012；`parallel_with`: G002, G003
  - `RED`: 握手、host capability、connected/stale/incompatible 状态转换 fixture 失败。
  - `GREEN`: 实现最小 session store。
  - `REFACTOR`: 不引入 Pinia，先使用 Composition API 单一 store。
  - `VERIFY`: reducer 单测。
  - `done`: 宿主会话状态有唯一事实源。

- [x] **G002 - 实现 normalized runtime store**
  - `depends_on`: F012；`parallel_with`: G001, G003
  - `RED`: 单节点变化导致 10,000 节点对象全部替换。
  - `GREEN`: 按稳定 ID 归一化实体和排序 ID。
  - `REFACTOR`: 大集合用 `shallowRef` + immutable root replacement。
  - `VERIFY`: identity/render count 测试。
  - `done`: 增量事件只更新受影响实体。

- [x] **G003 - 分离 active config 与 draft**
  - `depends_on`: E004, F012；`parallel_with`: G001, G002
  - `RED`: 编辑 draft 污染 active snapshot 或事件覆盖用户输入。
  - `GREEN`: 建立 active/draft/baseDigest 三元模型。
  - `REFACTOR`: dirty 由结构化 field changes 推导。
  - `VERIFY`: reload/event/edit 交错测试。
  - `done`: daemon 事实与未提交表单互不覆盖。

- [x] **G004 - 实现 digest CAS conflict**
  - `depends_on`: G003；`parallel_with`: G005
  - `RED`: stale draft 仍可 apply 的测试失败。
  - `GREEN`: 写操作始终携带 expected digest，conflict 转为明确状态。
  - `REFACTOR`: CAS 逻辑只在 mutation service 一处。
  - `VERIFY`: concurrent external edit fixture。
  - `done`: WebUI 不会覆盖外部或另一客户端修改。

- [x] **G005 - 实现 operation 状态机**
  - `depends_on`: E012, F012；`parallel_with`: G004
  - `RED`: accepted/running/success/failure/conflict/timeout 乱序 fixture 失败。
  - `GREEN`: 以 operation ID 建立单调状态机。
  - `REFACTOR`: 页面文案与状态转换分离。
  - `VERIFY`: property-based transition 测试。
  - `done`: 按钮成功只由 daemon 证据确认。

- [x] **G006 - 实现 action lock**
  - `depends_on`: G005；`parallel_with`: G007
  - `RED`: 同 key 双击重复提交、不同 key 被无谓阻塞。
  - `GREEN`: 实现按 operation key 的锁。
  - `REFACTOR`: finally 清理且不可吞业务错误。
  - `VERIFY`: MagicNet 同类测试 + race fixture。
  - `done`: 相同危险操作不可并发重复发起。

- [x] **G007 - 实现有界 query cache**
  - `depends_on`: G002；`parallel_with`: G006
  - `RED`: 路由往返后 cache 无界增长或保存 sensitive DTO。
  - `GREEN`: 只缓存允许的列表 snapshot/scroll，采用容量与 TTL。
  - `REFACTOR`: cache key 使用业务 ID，不使用原始 URL。
  - `VERIFY`: eviction 和 secret scan。
  - `done`: 页面返回可恢复且内存有界。

- [x] **G008 - 实现 UI storage allowlist**
  - `depends_on`: C015；`parallel_with`: G007
  - `RED`: 保存 config、URL、digest、node、logs 或非 `nethop.ui.*` key 时失败。
  - `GREEN`: 包装 VueUse `useStorage` 只开放主题和非敏感偏好。
  - `REFACTOR`: 默认值与版本迁移保持最小，不保存业务状态。
  - `VERIFY`: localStorage secret canary 扫描。
  - `done`: 卸载宿主只丢 UI 偏好，不影响 NetHop 配置。

- [x] **G009 - 实现搜索索引派生器**
  - `depends_on`: G002；`parallel_with`: G008
  - `RED`: 10,000 节点和 1,000 应用每次输入重复 lowercase/normalize 超预算。
  - `GREEN`: 在实体进入 store 时预计算非敏感搜索字段。
  - `REFACTOR`: 不复制完整实体或凭据。
  - `VERIFY`: 搜索 P95 基准。
  - `done`: 搜索更新满足 `08` 的 50ms 应用门槛。

- [x] **G010 - 阶段 G 门禁**
  - `depends_on`: G001, G004, G006, G007, G008, G009；`parallel_with`: none
  - `RED`: 非规范化大集合、无 CAS 写入或敏感 storage 时失败。
  - `GREEN`: 聚合 store/state/security tests。
  - `REFACTOR`: 页面不得维护平行业务 store。
  - `VERIFY`: Node suite + memory fixture。
  - `done`: 页面可在一致状态模型上开发。

## 14. 阶段 H：应用外壳、导航与基础状态

- [x] **H001 - 实现静态兼容性入口**
  - `depends_on`: C015, G010；`parallel_with`: H002
  - `RED`: WebView <105、无 host、协议不兼容时应用半启动。
  - `GREEN`: 在业务 bundle 初始化前显示明确只读结果。
  - `REFACTOR`: 不兼容页不依赖 TDesign 重组件。
  - `VERIFY`: 三类宿主 fixture 和离线加载。
  - `done`: 失败早、可理解且无 root 写操作。

- [x] **H002 - 实现四区 hash 导航**
  - `depends_on`: C007, G010；`parallel_with`: H001
  - `RED`: 概览/订阅/应用/设置直达和刷新测试失败。
  - `GREEN`: 实现底部四区路由。
  - `REFACTOR`: 二级节点/运维页不占一级 tab。
  - `VERIFY`: E2E direct-link 和 back/forward。
  - `done`: 无服务器路由仍可稳定恢复页面。

- [x] **H003 - 实现路由懒加载**
  - `depends_on`: H002；`parallel_with`: H004
  - `RED`: metafile 测试发现所有页面进入首屏 chunk。
  - `GREEN`: 使用动态 import 分割非首屏页面。
  - `REFACTOR`: 不为小型共享组件制造碎片 chunk。
  - `VERIFY`: 首屏/异步 chunk gzip。
  - `done`: 首屏不加载日志、连接、完整节点和应用页代码。

- [x] **H004 - 实现 Android 返回优先级**
  - `depends_on`: H002, D012；`parallel_with`: H003
  - `RED`: 返回键未按 dialog→sheet→二级页→一级页→exit 顺序处理。
  - `GREEN`: 建立唯一 back dispatcher。
  - `REFACTOR`: 组件通过注册栈参与，不各自监听全局事件。
  - `VERIFY`: NetProxy 同类行为 + Browser interaction。
  - `done`: 返回键不丢草稿或误退出。

- [x] **H005 - 实现离开脏草稿确认**
  - `depends_on`: G003, H004；`parallel_with`: H006
  - `RED`: route/back/host exit 可静默丢弃 dirty draft。
  - `GREEN`: 统一 navigation guard。
  - `REFACTOR`: daemon 已 accepted 的操作不被页面离开取消。
  - `VERIFY`: 三种离开路径组件测试。
  - `done`: 只对未提交本地编辑提示确认。

- [x] **H006 - 实现主题映射**
  - `depends_on`: G008；`parallel_with`: H005, H007
  - `RED`: system/light/dark 未映射到根 `theme-mode`。
  - `GREEN`: 使用 VueUse system preference + allowlisted setting。
  - `REFACTOR`: 不在组件内散布暗色 class。
  - `VERIFY`: light/dark Browser screenshot smoke。
  - `done`: TDesign 和 NetHop token 同步切换。

- [x] **H007 - 实现 Android 字体与 safe area**
  - `depends_on`: C015；`parallel_with`: H006, H008
  - `RED`: 设备字体栈和 inset fixture 出现截断/底栏遮挡。
  - `GREEN`: 添加窄 CSS override 和 safe-area 约束。
  - `REFACTOR`: 不 fork TDesign 样式。
  - `VERIFY`: 4 个视口 screenshot。
  - `done`: 长文本与底部导航在目标 WebView 不重叠。

- [x] **H008 - 限制装饰动画**
  - `depends_on`: C015；`parallel_with`: H007
  - `RED`: 自有持续动画造成额外绘制或掩盖真实状态。
  - `GREEN`: 自有过渡限制为短时、非必要动画不进入运行路径。
  - `REFACTOR`: 状态变化由真实数据驱动。
  - `VERIFY`: computed style 与空闲资源测试。
  - `done`: 无持续装饰动画；系统 reduced-motion 覆盖仅作低成本保留，不是门禁。

- [x] **H009 - 实现全局 loading/error/stale 外壳**
  - `depends_on`: H001, H002, G001；`parallel_with`: none
  - `RED`: handshake/loading/error/stale 状态布局抖动或仍可危险操作。
  - `GREEN`: 实现稳定尺寸的 app shell states。
  - `REFACTOR`: 页面错误与宿主错误分层。
  - `VERIFY`: Browser state fixture + screenshot。
  - `done`: 所有顶层失败状态可恢复且不遮挡导航。

- [x] **H010 - 阶段 H 门禁**
  - `depends_on`: H003, H005, H006, H007, H008, H009；`parallel_with`: none
  - `RED`: 路由、返回、草稿、主题或布局回归时失败。
  - `GREEN`: 聚合 shell tests。
  - `REFACTOR`: shell 不包含业务 mutation。
  - `VERIFY`: Browser + E2E smoke + bundle budget。
  - `done`: 四区静态外壳可在浏览器与宿主中打开。

## 15. 阶段 I：共享组件与组合式基础设施

- [x] **I001 - 实现 StatusLine**
  - `depends_on`: H010；`parallel_with`: I002, I003
  - `RED`: normal/degraded/stale/error 与长文本 component fixture 失败。
  - `GREEN`: 实现图标+文本状态行。
  - `REFACTOR`: 状态不只靠颜色表达。
  - `VERIFY`: Browser + axe。
  - `done`: 所有运行状态使用一致语义。

- [x] **I002 - 实现 MetricValue**
  - `depends_on`: H010；`parallel_with`: I001, I003
  - `RED`: 数值变化导致容器位移或溢出。
  - `GREEN`: 固定数字尺寸与缺失值样式。
  - `REFACTOR`: 格式化函数独立单测。
  - `VERIFY`: 极值/RTL 数字符号/中文单位 screenshot。
  - `done`: traffic 和延迟指标不引发布局跳动。

- [x] **I003 - 实现 OperationBanner**
  - `depends_on`: G005, H010；`parallel_with`: I001, I002
  - `RED`: accepted/running/success/failure/conflict/timeout 展示与关闭行为失败。
  - `GREEN`: 实现 operation 状态横幅。
  - `REFACTOR`: 不从 message 文本推断状态。
  - `VERIFY`: Browser interaction。
  - `done`: 后台操作离开页面后仍可被全局观察。

- [x] **I004 - 实现 SecretField**
  - `depends_on`: H010；`parallel_with`: I005
  - `RED`: 默认明文、复制/显示无显式动作、截图 fixture 泄露 secret。
  - `GREEN`: 实现默认遮蔽和短时显式查看。
  - `REFACTOR`: secret 不写 storage/toast/log。
  - `VERIFY`: secret canary E2E。
  - `done`: URL 等敏感输入不会被被动暴露。

- [x] **I005 - 实现 ConfirmDialog**
  - `depends_on`: H004；`parallel_with`: I004
  - `RED`: destructive/impact 文案、焦点陷阱、返回关闭和焦点恢复失败。
  - `GREEN`: 封装 TDesign Dialog。
  - `REFACTOR`: 不嵌套 card，不复制每页确认逻辑。
  - `VERIFY`: Browser keyboard/back/axe。
  - `done`: 危险操作有一致二次确认。

- [x] **I006 - 实现 TrafficSparkline**
  - `depends_on`: F010, I002；`parallel_with`: I007
  - `RED`: Canvas 空白、resize 错误、NaN 和 hidden 后仍绘制。
  - `GREEN`: 实现轻量 Canvas 曲线。
  - `REFACTOR`: 不引入图表库，绘制数据不进入深响应式。
  - `VERIFY`: canvas pixel check + resize/visibility 性能。
  - `done`: 曲线非空、可缩放且不遮挡可访问文本指标。

- [x] **I007 - 实现 useBoundedVirtualizer**
  - `depends_on`: C012, G007；`parallel_with`: I006
  - `RED`: unstable key、overscan 超界、cleanup、fixed/dynamic 模式 fixture 失败。
  - `GREEN`: 封装唯一 TanStack options 入口。
  - `REFACTOR`: 页面不得直接调用 `useVirtualizer`。
  - `VERIFY`: 10,000 fixed + dynamic measurement tests。
  - `done`: 列表共享 stable key、overscan 2..8 和 cleanup 约束。

- [x] **I008 - 实现 VirtualListViewport**
  - `depends_on`: I007；`parallel_with`: I009
  - `RED`: 空列表、loading、scrollToIndex、resize 与动态 `data-index` fixture 失败。
  - `GREEN`: 实现单列 viewport 组件。
  - `REFACTOR`: 行内容用 slot，组件不理解 node/package 业务。
  - `VERIFY`: Browser virtualization suite。
  - `done`: 节点和应用共享同一视口实现。

- [x] **I009 - 实现通用 Empty/Error/Loading 状态**
  - `depends_on`: H010；`parallel_with`: I008
  - `RED`: 状态高度不稳定、按钮无 label、长错误溢出。
  - `GREEN`: 实现三种 unframed page state。
  - `REFACTOR`: 错误详情按需展开且有上限。
  - `VERIFY`: Browser + screenshot + axe。
  - `done`: 各页无需自制不一致占位状态。

- [x] **I010 - 实现 schema field renderer**
  - `depends_on`: E003, E004, H010；`parallel_with`: I009
  - `RED`: bool/enum/int/string/array、禁用原因和未知 field kind fixture 失败。
  - `GREEN`: 实现受控通用字段渲染器。
  - `REFACTOR`: 高风险字段仍允许领域专用组件覆盖。
  - `VERIFY`: Browser schema matrix。
  - `done`: 高级设置可由 daemon metadata 驱动且未知类型安全拒绝。

- [x] **I011 - 阶段 I 门禁**
  - `depends_on`: I001, I003, I004, I005, I006, I008, I009, I010；`parallel_with`: none
  - `RED`: 组件布局、secret、virtualizer 或 lifecycle 回归时失败。
  - `GREEN`: 聚合 shared component tests。
  - `REFACTOR`: 删除页面级重复 primitive。
  - `VERIFY`: Browser + screenshot smoke。
  - `done`: 页面开发只组合已验证基础组件。

## 16. 阶段 J：概览与代理日常启停

- [x] **J001 - 展示运行状态摘要**
  - `depends_on`: I011；`parallel_with`: J002, K001, L001, M001, N001
  - `RED`: stopped/starting/running/stopping/degraded/error/stale fixture 显示错误。
  - `GREEN`: 用 status snapshot 和 runtime event 渲染摘要。
  - `REFACTOR`: 不从进程名或按钮状态猜测运行事实。
  - `VERIFY`: Browser state matrix。
  - `done`: 用户一眼能看出代理是否真实运行。

- [x] **J002 - 实现代理启停开关**
  - `depends_on`: I011, G006；`parallel_with`: J001
  - `RED`: 双击、失败、timeout、conflict 与离页后结果 fixture 失败。
  - `GREEN`: 连接 typed start/stop operation。
  - `REFACTOR`: 开关视觉值只由 runtime event 决定。
  - `VERIFY`: Component + MockHost E2E。
  - `done`: 提交、accepted、running、final 四阶段可区分。

- [x] **J003 - 展示 generation 状态**
  - `depends_on`: J001；`parallel_with`: J004
  - `RED`: active/pending/rollback/failed generation fixture 混淆。
  - `GREEN`: 展示当前 generation 与最近切换结果。
  - `REFACTOR`: ID 缩写保留可访问完整文本，不显示 secret。
  - `VERIFY`: Browser fixture。
  - `done`: 配置发布与代理进程状态不会混为一谈。

- [x] **J004 - 展示当前节点**
  - `depends_on`: J001, E006；`parallel_with`: J003
  - `RED`: 无节点、自动选择、手动节点和节点消失 fixture 失败。
  - `GREEN`: 展示稳定 node ID 对应的安全摘要。
  - `REFACTOR`: 不把 credential 或完整 outbound 送入组件。
  - `VERIFY`: Browser fixture + secret scan。
  - `done`: 概览可进入节点页且身份稳定。

- [x] **J005 - 展示实时流量**
  - `depends_on`: I006, J001；`parallel_with`: J006
  - `RED`: snapshot、stream、hidden、reconnect 后曲线/数值不一致。
  - `GREEN`: 接入 traffic ring 和 sparkline。
  - `REFACTOR`: 概览不自行启动第二条事件流。
  - `VERIFY`: Canvas pixel + latency instrumentation。
  - `done`: traffic 样本到可见曲线 P95 满足 250ms。

- [x] **J006 - 实现全部订阅更新入口**
  - `depends_on`: G006, J001；`parallel_with`: J005
  - `RED`: 重复点击、partial success、无启用 source、offline fixture 失败。
  - `GREEN`: 连接 update-all operation。
  - `REFACTOR`: 详细结果链接到订阅页，不塞入概览卡片。
  - `VERIFY`: Component + E2E。
  - `done`: 用户可从概览触发更新并看到 daemon 最终证据。

- [x] **J007 - 阶段 J 门禁**
  - `depends_on`: J002, J003, J004, J005, J006；`parallel_with`: none
  - `RED`: 任一状态由本地乐观值冒充最终事实时失败。
  - `GREEN`: 聚合 overview daily-flow tests。
  - `REFACTOR`: 概览仅保留高频命令。
  - `VERIFY`: 打开→启停→流量→更新 E2E。
  - `done`: W1 最小日常控制闭环成立。

## 17. 阶段 K：多订阅管理

- [x] **K001 - 展示订阅源列表**
  - `depends_on`: I011；`parallel_with`: J001, L001, M001, N001
  - `RED`: enabled/disabled/updating/success/failed/partial 与空列表 fixture 失败。
  - `GREEN`: 按 daemon 顺序显示 source 安全摘要。
  - `REFACTOR`: URL 默认不进入行 props。
  - `VERIFY`: Browser state matrix。
  - `done`: 每个 source 的模块生成 ID、名称和状态可区分。

- [x] **K002 - 添加订阅源**
  - `depends_on`: K001, D013, G004；`parallel_with`: K003
  - `RED`: 空名、非法 HTTPS、重名、secret preview 和 conflict fixture 失败。
  - `GREEN`: 通过私有 payload 提交 name+URL，由 daemon 生成 ID。
  - `REFACTOR`: 表单不暴露 ID 字段。
  - `VERIFY`: Component + Rust integration + secret scan。
  - `done`: 用户只填写名称和链接即可新增 source。

- [x] **K003 - 编辑订阅源**
  - `depends_on`: K001, D013, G004；`parallel_with`: K002
  - `RED`: 不改 URL、替换 URL、并发 external edit 和 cancel fixture 失败。
  - `GREEN`: 通过 typed mutation/private payload 更新允许字段。
  - `REFACTOR`: 原 URL 不从 masked value 反推。
  - `VERIFY`: Component + CAS E2E。
  - `done`: 编辑不会覆盖 daemon 新版本配置。

- [x] **K004 - 启用或禁用订阅源**
  - `depends_on`: K001, G004；`parallel_with`: K005
  - `RED`: 最后可用源、更新中切换和 conflict fixture 失败。
  - `GREEN`: 连接 typed enable mutation。
  - `REFACTOR`: switch 最终值由 config event 确认。
  - `VERIFY`: Component state machine。
  - `done`: source 可控启停且安全不变量由 daemon 裁决。

- [x] **K005 - 调整订阅源顺序**
  - `depends_on`: K001, G004；`parallel_with`: K004
  - `RED`: 稳定 ID、边界移动、快速重复和 conflict fixture 失败。
  - `GREEN`: 提供明确上移/下移命令。
  - `REFACTOR`: 首版不用复杂拖拽依赖。
  - `VERIFY`: Component + config event。
  - `done`: 顺序变更可访问、可回滚且不依赖 index 身份。

- [x] **K006 - 删除订阅源**
  - `depends_on`: K001, I005, G004；`parallel_with`: K007
  - `RED`: 删除确认、最后 source、更新中删除和 conflict fixture 失败。
  - `GREEN`: 连接 typed remove mutation。
  - `REFACTOR`: 删除影响文案由 validate/impact 提供。
  - `VERIFY`: Component + E2E。
  - `done`: destructive 操作不会误触或静默清空代理。

- [x] **K007 - 更新单个订阅源**
  - `depends_on`: K001, G006；`parallel_with`: K006
  - `RED`: accepted/running/partial/failure/last-known-good fixture 失败。
  - `GREEN`: 连接 source-specific update。
  - `REFACTOR`: 后台结果复用全局 operation store。
  - `VERIFY`: Component + event stream E2E。
  - `done`: 单源失败保留 last-known-good 并清楚展示。

- [x] **K008 - 展示 source 诊断摘要**
  - `depends_on`: K001；`parallel_with`: K007
  - `RED`: diagnostics 数量超限、长文本、secret 和 proxy-provider warning fixture 失败。
  - `GREEN`: 展示 compact code/count，详情按需加载。
  - `REFACTOR`: message template 在 UI 映射，不保存逐节点长字符串。
  - `VERIFY`: Browser + secret scan。
  - `done`: partial success 可解释且报告有界。

- [x] **K009 - 实现订阅导入预览**
  - `depends_on`: D013, E005, I005；`parallel_with`: none
  - `RED`: URI/text/file payload、超限、unsupported format 和 secret preview fixture 失败。
  - `GREEN`: 通过 private payload 调用 import preview。
  - `REFACTOR`: preview 不创建/修改 source。
  - `VERIFY`: Component + Rust parser integration。
  - `done`: 用户在发布前看到 accepted/skipped/duplicate 摘要。

- [x] **K010 - 实现订阅导入应用**
  - `depends_on`: K009, G004；`parallel_with`: none
  - `RED`: candidate digest 不匹配、preview 过期、conflict 和 apply 失败 fixture 失败。
  - `GREEN`: 只提交 preview 返回的 candidate digest。
  - `REFACTOR`: 不重新解释原 payload。
  - `VERIFY`: preview→confirm→apply E2E。
  - `done`: 导入遵守两阶段确认和 generation 事务。

- [x] **K011 - 阶段 K 门禁**
  - `depends_on`: K002, K003, K004, K005, K006, K007, K008, K010；`parallel_with`: none
  - `RED`: ID 输入、URL 泄露、无 CAS 或 last-known-good 回归时失败。
  - `GREEN`: 聚合 subscription tests。
  - `REFACTOR`: 删除 request_profile 等不应暴露的传输细节控件。
  - `VERIFY`: add→edit→move→update→delete E2E。
  - `done`: 多 source 用户闭环完成。

阶段 G-K 完成证据（2026-08-08）：

```powershell
pwsh -NoProfile -File "scripts/webui-phase-g-gate.ps1"
pwsh -NoProfile -File "scripts/webui-phase-h-gate.ps1"
pwsh -NoProfile -File "scripts/webui-phase-i-gate.ps1"
pwsh -NoProfile -File "scripts/webui-phase-j-gate.ps1"
pwsh -NoProfile -File "scripts/webui-phase-k-gate.ps1"
```

核心证据包括：`tests/unit/store.test.ts` 的 35 项 Node 单测、共享组件 Browser 测试、四区 hash 路由与移动端日常闭环 Playwright 测试、`config-mutate` 私有 payload 的 Rust wire 契约、typecheck、production build、依赖/导入/安全扫描。订阅 URL 仅通过私有 payload 传输，source ID 由 daemon 返回并在页面中只读展示；未实现的运维页仍按任务图留在阶段 O。

## 18. 阶段 L：节点列表与选择

- [x] **L001 - 展示虚拟节点列表**
  - `depends_on`: I011, E006；`parallel_with`: J001, K001, M001, N001
  - `RED`: 10,000 节点导致全量 DOM 或 index key。
  - `GREEN`: 使用 VirtualListViewport 和 stable node ID。
  - `REFACTOR`: 默认固定行高、名称单行省略。
  - `VERIFY`: DOM count、首次范围 P95 和 screenshot。
  - `done`: 列表规模不决定 DOM 规模。

- [x] **L002 - 搜索节点**
  - `depends_on`: L001, G009；`parallel_with`: L003
  - `RED`: 中文/英文/大小写、长查询和快速输入 fixture 失败。
  - `GREEN`: 接入 100-150ms debounce 的预规范化搜索。
  - `REFACTOR`: 搜索不复制完整实体。
  - `VERIFY`: 10,000 节点 P95。
  - `done`: 搜索流畅且结果 identity 稳定。

- [x] **L003 - 筛选节点**
  - `depends_on`: L001；`parallel_with`: L002
  - `RED`: protocol/source/status 筛选后 measurement identity 串用。
  - `GREEN`: 以稳定 ID 生成筛选结果。
  - `REFACTOR`: filter state 只保存非敏感枚举/ID。
  - `VERIFY`: reorder/filter virtualizer test。
  - `done`: 筛选不会错配行高度或选中状态。

- [x] **L004 - 置顶当前节点**
  - `depends_on`: L001；`parallel_with`: L005
  - `RED`: 当前节点置顶后重复、丢失 stable key 或滚动跳动。
  - `GREEN`: 通过派生 ID 顺序置顶。
  - `REFACTOR`: 不克隆节点实体。
  - `VERIFY`: virtualizer identity test。
  - `done`: 当前节点始终可见且仅出现一次。

- [x] **L005 - 测试单个节点**
  - `depends_on`: L001, G006；`parallel_with`: L004
  - `RED`: 双击、timeout、节点消失和迟到结果 fixture 失败。
  - `GREEN`: 连接 node test operation。
  - `REFACTOR`: 延迟显示只由测试结果 DTO 更新。
  - `VERIFY`: Component + event fixture。
  - `done`: 行内操作不重排或拉伸列表。

- [x] **L006 - 测试候选节点集合**
  - `depends_on`: L005；`parallel_with`: L007
  - `RED`: 超过候选上限、并发状态、partial failure fixture 失败。
  - `GREEN`: 连接 daemon 的有界批量测试语义。
  - `REFACTOR`: 前端不自行并发逐节点命令。
  - `VERIFY`: 64 候选 fixture。
  - `done`: 批量测速不会 fork 命令风暴。

- [x] **L007 - 选择节点**
  - `depends_on`: L001, G006；`parallel_with`: L006
  - `RED`: selected 乐观显示、失效节点、operation failure fixture 失败。
  - `GREEN`: 连接 node select，最终状态等 generation/runtime event。
  - `REFACTOR`: 选择与测试状态独立。
  - `VERIFY`: Component + E2E。
  - `done`: 选中只由 daemon 事实确认。

- [x] **L008 - 排除节点**
  - `depends_on`: L001, I005, G004；`parallel_with`: L009
  - `RED`: 当前节点排除、确认、conflict 和撤销提示 fixture 失败。
  - `GREEN`: 连接 typed config mutation。
  - `REFACTOR`: UI 叫“排除”，不误导为删除远端 source 节点。
  - `VERIFY`: Component + CAS。
  - `done`: 节点排除语义清楚且可审计。

- [x] **L009 - 导出节点**
  - `depends_on`: L001, D013；`parallel_with`: L008
  - `RED`: 未确认导出、secret 进入日志/clipboard preview、过大输出 fixture 失败。
  - `GREEN`: 调用受控 node export 并交由宿主明确分享/复制步骤。
  - `REFACTOR`: 普通列表 DTO 不获得 credential。
  - `VERIFY`: secret canary + user gesture test。
  - `done`: 凭据只在显式导出操作的最短生命周期出现。

- [x] **L010 - 恢复节点滚动位置**
  - `depends_on`: L002, L003, I008, G007；`parallel_with`: none
  - `RED`: 页面返回、列表缩短、筛选变化、跳到 9000 项后恢复错误。
  - `GREEN`: 内存保存 TanStack snapshot+offset 并验证 key domain。
  - `REFACTOR`: 不写 localStorage，不平滑滚动动态高度。
  - `VERIFY`: fixed/dynamic deep-scroll E2E。
  - `done`: 恢复准确，无明显锚点跳动。

- [x] **L011 - 阶段 L 门禁**
  - `depends_on`: L004, L006, L007, L008, L009, L010；`parallel_with`: none
  - `RED`: 全量 DOM、index key、secret model 或性能超限时失败。
  - `GREEN`: 聚合 node UX/virtualization/security suite。
  - `REFACTOR`: 页面不直接配置 TanStack options。
  - `VERIFY`: 10,000 节点真机滚动基准。
  - `done`: 节点管理完整且达到固定/动态列表预算。

## 19. 阶段 M：应用范围与黑白名单

- [x] **M001 - 加载应用包列表**
  - `depends_on`: I011, D011, E007；`parallel_with`: J001, K001, L001, N001
  - `RED`: API 缺失、空列表、非法 PackageInfo 和批量上限 fixture 失败。
  - `GREEN`: 分批加载 package info 并合并 daemon selection。
  - `REFACTOR`: 不用 shell `pm list packages`。
  - `VERIFY`: Browser host fixture。
  - `done`: 应用页能在宿主能力不足时明确降级。

- [x] **M002 - 展示虚拟应用列表**
  - `depends_on`: M001, I008；`parallel_with`: M003
  - `RED`: 1,000 应用产生全量 DOM 或图标导致行高跳动。
  - `GREEN`: 固定高度虚拟行与稳定 package key。
  - `REFACTOR`: 图标槽尺寸固定。
  - `VERIFY`: DOM count、scroll FPS、screenshot。
  - `done`: 列表加载与图标回填不改变布局。

- [x] **M003 - 加载应用图标**
  - `depends_on`: M001；`parallel_with`: M002
  - `RED`: `ksu://icon/` 失败、恶意 package path 和快速滚动 fixture 失败。
  - `GREEN`: 使用受控 package name URL 与本地 fallback icon。
  - `REFACTOR`: 不把图标转存 localStorage/base64。
  - `VERIFY`: Browser image error + secret/network scan。
  - `done`: 图标失败不影响选择和滚动。

- [x] **M004 - 搜索应用**
  - `depends_on`: M002, G009；`parallel_with`: M005
  - `RED`: app label/package 中文与快速输入超预算。
  - `GREEN`: 接入预规范化 debounce 搜索。
  - `REFACTOR`: 单一低频 clock，不为行建立 timer。
  - `VERIFY`: 1,000 应用 P95 <=50ms。
  - `done`: 搜索结果 identity 与选择保持稳定。

- [x] **M005 - 筛选用户和系统应用**
  - `depends_on`: M002；`parallel_with`: M004
  - `RED`: unknown/system/user 分类和筛选后选择丢失。
  - `GREEN`: 以 HostAdapter typed category 筛选。
  - `REFACTOR`: unknown 不被猜成 user。
  - `VERIFY`: Browser fixture。
  - `done`: 筛选只影响视图，不改代理配置。

- [x] **M006 - 切换 all/blacklist/whitelist**
  - `depends_on`: M001, G004, I005；`parallel_with`: M007
  - `RED`: 模式含义、空 whitelist、impact、conflict fixture 失败。
  - `GREEN`: 连接 typed application mode mutation。
  - `REFACTOR`: 模式说明短而明确，不在页面写使用教程。
  - `VERIFY`: Component + CAS E2E。
  - `done`: 黑白名单模式已实现并由 daemon admission 校验。

- [x] **M007 - 修改应用选择**
  - `depends_on`: M001, G004；`parallel_with`: M006
  - `RED`: 快速多选、批量 mutation、conflict 与失败回滚 fixture 失败。
  - `GREEN`: 累积 draft 并一次提交 typed package mutation。
  - `REFACTOR`: 不为每次勾选立即 fork root 命令。
  - `VERIFY`: 1,000 app selection test。
  - `done`: 批量选择高效且 CAS 安全。

- [x] **M008 - 处理 shared UID**
  - `depends_on`: M007；`parallel_with`: M009
  - `RED`: 多包共享 UID 时仅显示/选择一包造成误导。
  - `GREEN`: 展示关联说明并按 daemon UID 语义提交。
  - `REFACTOR`: shared UID mapping 只来自 typed host/daemon data。
  - `VERIFY`: shared UID fixture。
  - `done`: 用户能预见选择一个包影响同 UID 应用。

- [x] **M009 - 保护 root UID**
  - `depends_on`: M007；`parallel_with`: M008
  - `RED`: 用户可通过 UI 移除默认 root 排除或代理 UID 0。
  - `GREEN`: 根据 daemon schema/capability 禁用危险选择并解释原因。
  - `REFACTOR`: 前端只是预防，daemon 仍为最终安全边界。
  - `VERIFY`: malicious mutation integration test。
  - `done`: UI 和 daemon 双层保护 root UID。

- [x] **M010 - 阶段 M 门禁**
  - `depends_on`: M003, M004, M005, M006, M008, M009；`parallel_with`: none
  - `RED`: shell 包查询、全量 DOM、逐项 root fork 或 UID 安全回归时失败。
  - `GREEN`: 聚合 application suite。
  - `REFACTOR`: 包信息与配置 mutation 保持分层。
  - `VERIFY`: all→blacklist→whitelist 真机闭环。
  - `done`: 应用范围管理达到 W1 目标。

## 20. 阶段 N：设置、capability 与配置事务

- [x] **N001 - 加载配置 schema metadata**
  - `depends_on`: I011, E003, E004；`parallel_with`: J001, K001, L001, M001
  - `RED`: field 类型、范围、枚举、风险、capability 依赖和 unknown metadata fixture 失败。
  - `GREEN`: 构建只读 schema view model。
  - `REFACTOR`: 页面不硬编码 daemon 默认值。
  - `VERIFY`: schema.get fixture matrix。
  - `done`: 设置控件以服务端 schema 为事实源。

- [x] **N002 - 编辑 proxy 设置**
  - `depends_on`: N001, G003；`parallel_with`: N003, N004, N005
  - `RED`: outbound/selector/urltest 范围和 capability 禁用 fixture 失败。
  - `GREEN`: 实现 proxy draft section。
  - `REFACTOR`: 只渲染已实现字段，不暴露假能力。
  - `VERIFY`: Browser field matrix。
  - `done`: proxy 高级参数有 typed draft 和帮助元数据。

- [x] **N003 - 编辑 network 设置**
  - `depends_on`: N001, G003；`parallel_with`: N002, N004, N005
  - `RED`: capture/TCP/UDP/IPv6/DNS/interface capability fixture 失败。
  - `GREEN`: 实现 network draft section。
  - `REFACTOR`: 不允许关闭 daemon 安全不变量。
  - `VERIFY`: Browser + capability fixture。
  - `done`: 网络选项只显示当前设备可承认能力。

- [x] **N004 - 编辑 routing 设置**
  - `depends_on`: N001, G003；`parallel_with`: N002, N003, N005
  - `RED`: CIDR parse、重复、冲突、bypass/force 重叠 fixture 失败。
  - `GREEN`: 实现 routing draft section。
  - `REFACTOR`: 最终语义校验仍由 daemon 执行。
  - `VERIFY`: CIDR corpus + Browser。
  - `done`: 路由列表输入有即时格式反馈且不自作安全裁决。

- [x] **N005 - 编辑 logging 设置**
  - `depends_on`: N001, G003；`parallel_with`: N002, N003, N004
  - `RED`: level/retention 范围和敏感 debug warning fixture 失败。
  - `GREEN`: 实现 logging draft section。
  - `REFACTOR`: WebUI 不提供关闭脱敏选项。
  - `VERIFY`: Browser field matrix。
  - `done`: 日志高级参数可配但安全边界不可关闭。

- [x] **N006 - 编辑 advanced 设置**
  - `depends_on`: N001, G003；`parallel_with`: none
  - `RED`: port/mark/reconcile/resource candidate 越界与 capability conflict fixture 失败。
  - `GREEN`: 使用 schema renderer + 专用危险字段控件。
  - `REFACTOR`: advanced 默认折叠，不降低校验等级。
  - `VERIFY`: Browser + bounds corpus。
  - `done`: 已实现高级参数完整暴露且风险清楚。

- [x] **N007 - 展示 capability 状态**
  - `depends_on`: N001, E003；`parallel_with`: N006
  - `RED`: unsupported/experimental/unavailable/conflict 仅用颜色或无原因。
  - `GREEN`: 为字段显示文本状态与原因。
  - `REFACTOR`: capability 文案集中映射稳定 code。
  - `VERIFY`: Browser + axe + grayscale screenshot。
  - `done`: 用户能理解为何控件不可用。

- [x] **N008 - 实现 config validate**
  - `depends_on`: N002, N003, N004, N005, N006, N007；`parallel_with`: none
  - `RED`: 本地合法但 daemon admission 失败、impact 缺失和 stale digest fixture 失败。
  - `GREEN`: 通过 private payload/typed document 调用 validate。
  - `REFACTOR`: 本地 validator 只做即时输入反馈。
  - `VERIFY`: config validate integration。
  - `done`: apply 前获得 daemon 诊断和变更动作级别。

- [x] **N009 - 展示 apply impact**
  - `depends_on`: N008, I005；`parallel_with`: none
  - `RED`: hot reload、core restart、network reapply、proxy stop 和 destructive 影响显示错误。
  - `GREEN`: 将 validate 结果映射为确认摘要。
  - `REFACTOR`: 影响不由前端字段名猜测。
  - `VERIFY`: Component impact matrix。
  - `done`: 用户提交前知道不可用窗口和破坏性结果。

- [x] **N010 - 实现 config apply**
  - `depends_on`: N009, G004, G006；`parallel_with`: none
  - `RED`: 未 validate、candidate 过期、conflict、publish fail、rollback fixture 失败。
  - `GREEN`: 提交 candidate digest + expected config digest。
  - `REFACTOR`: apply 结果只由 config/generation event 完成。
  - `VERIFY`: validate→confirm→apply→generation E2E。
  - `done`: 设置变更事务化发布且失败保留旧 runtime。

- [x] **N011 - 实现 config reload**
  - `depends_on`: G003, G006；`parallel_with`: N012
  - `RED`: dirty draft、外部文件 invalid、reload conflict 和成功刷新 fixture 失败。
  - `GREEN`: 提供显式 reload 并保护本地草稿。
  - `REFACTOR`: reload 不等同于 apply。
  - `VERIFY`: manual edit watcher integration fixture。
  - `done`: 手工编辑 TOML 后可实时重载且错误不影响旧配置。

- [x] **N012 - 实现外部修改冲突 UX**
  - `depends_on`: G004, N011；`parallel_with`: none
  - `RED`: observed digest 变化时 stale draft 仍可直接提交。
  - `GREEN`: 提供重新加载或人工保留字段的明确选择。
  - `REFACTOR`: 首版不实现复杂自动三方 merge。
  - `VERIFY`: event→conflict pending component test。
  - `done`: 外部修改不会被静默覆盖。

- [x] **N013 - 阶段 N 门禁**
  - `depends_on`: N010, N012；`parallel_with`: none
  - `RED`: 假能力、无 validate、无 impact、无 CAS 或 rollback 回归时失败。
  - `GREEN`: 聚合 settings transaction suite。
  - `REFACTOR`: 设置页无直接 TOML 写入。
  - `VERIFY`: edit→validate→apply→external reload E2E。
  - `done`: 基础与高级设置完整遵守配置 ABI。

## 21. 阶段 O：稳定 CLI 运维能力覆盖

- [x] **O001 - 展示连接列表**
  - `depends_on`: J007, K011, L011, M010, N013, E010；`parallel_with`: O004, O006, O008, O009, O010
  - `RED`: 分页、空列表、长地址、超限和 stale fixture 失败。
  - `GREEN`: 渲染有界连接摘要。
  - `REFACTOR`: 不持续轮询，使用事件提示后按需刷新。
  - `VERIFY`: Browser + response bound test。
  - `done`: 连接页不泄露凭据且可处理大量连接。

- [x] **O002 - 关闭单个连接**
  - `depends_on`: O001, G006；`parallel_with`: O003
  - `RED`: 连接消失、重复关闭、失败和迟到结果 fixture 失败。
  - `GREEN`: 连接 typed close operation。
  - `REFACTOR`: 行状态由 stable connection ID 管理。
  - `VERIFY`: Component + MockHost。
  - `done`: 单连接关闭结果可证实。

- [x] **O003 - 关闭全部连接**
  - `depends_on`: O001, I005, G006；`parallel_with`: O002
  - `RED`: 无确认、重复提交和 partial failure fixture 失败。
  - `GREEN`: 连接 close-all operation。
  - `REFACTOR`: destructive confirmation 复用共享组件。
  - `VERIFY`: E2E。
  - `done`: 全部关闭不会因误触执行。

- [x] **O004 - 展示有界日志**
  - `depends_on`: E010, I008；`parallel_with`: O001, O006, O008
  - `RED`: 超长行、secret、before cursor、limit 和空日志 fixture 失败。
  - `GREEN`: 渲染结构化事件日志摘要。
  - `REFACTOR`: 不 tail 原始 sing-box 凭据文本。
  - `VERIFY`: secret scan + virtual list。
  - `done`: 日志读取有界、分页且脱敏。

- [x] **O005 - 清除日志**
  - `depends_on`: O004, I005, G006；`parallel_with`: none
  - `RED`: 无确认、失败和事件后列表未刷新 fixture 失败。
  - `GREEN`: 连接 logs.clear。
  - `REFACTOR`: 不本地乐观清空事实。
  - `VERIFY`: Component + event fixture。
  - `done`: 清理结果由 daemon 确认。

- [x] **O006 - 生成诊断包**
  - `depends_on`: D013, E010, G006；`parallel_with`: O001, O004, O008
  - `RED`: secret、路径输入、超时、文件不存在和重复生成 fixture 失败。
  - `GREEN`: 连接 diagnostics.bundle 并显示安全元数据。
  - `REFACTOR`: WebUI 不浏览任意文件系统路径。
  - `VERIFY`: secret canary + module integration。
  - `done`: 用户可生成脱敏诊断证据而不暴露 root 文件浏览器。

- [x] **O007 - 展示网络拓扑**
  - `depends_on`: E010；`parallel_with`: O006
  - `RED`: capability 缺失、IPv4/IPv6、接口变化和长名称 fixture 失败。
  - `GREEN`: 用紧凑 unframed 布局展示 topology.get。
  - `REFACTOR`: 不引入图布局库。
  - `VERIFY`: Browser screenshot + axe。
  - `done`: 用户能理解当前接管路径而不阅读原始规则。

- [x] **O008 - 管理规则集状态**
  - `depends_on`: E010, G006；`parallel_with`: O001, O004, O006
  - `RED`: missing/current/stale/updating/failed 和 update 结果 fixture 失败。
  - `GREEN`: 连接 ruleset status/update。
  - `REFACTOR`: 不让 WebUI 直接下载规则集。
  - `VERIFY`: Component + operation event。
  - `done`: 规则供应链仍由 daemon 控制。

- [x] **O009 - 检查核心版本**
  - `depends_on`: E010；`parallel_with`: O001, O006, O008
  - `RED`: current/newer/unavailable/invalid signature metadata fixture 失败。
  - `GREEN`: 展示 core.version-check 结果。
  - `REFACTOR`: 首版不从 WebUI 自动替换 sing-box。
  - `VERIFY`: Browser fixture。
  - `done`: 版本信息只读且供应链边界不扩大。

- [x] **O010 - 导出配置备份**
  - `depends_on`: D013, E004, I005；`parallel_with`: O001, O008, O009
  - `RED`: 未确认、secret preview、输出超限和 host 分享失败 fixture 失败。
  - `GREEN`: 连接 config export 并显式交付文件。
  - `REFACTOR`: 备份内容不进入 store/localStorage。
  - `VERIFY`: secret lifecycle test。
  - `done`: 敏感备份只在用户明确操作中短暂存在。

- [x] **O011 - 恢复配置备份**
  - `depends_on`: O010, D013, N008, N009；`parallel_with`: none
  - `RED`: invalid schema、超限、validate failure、conflict 和 rollback fixture 失败。
  - `GREEN`: 私有 payload→validate→impact→apply。
  - `REFACTOR`: restore 复用配置事务，不另写发布路径。
  - `VERIFY`: export→modify fixture→restore E2E。
  - `done`: 备份恢复具有与设置 apply 相同安全性。

- [x] **O012 - 阶段 O 门禁**
  - `depends_on`: O002, O003, O005, O006, O007, O008, O009, O011；`parallel_with`: none
  - `RED`: 任一 W2 页面绕过 typed CLI、直接轮询或泄密时失败。
  - `GREEN`: 聚合 stable CLI coverage matrix。
  - `REFACTOR`: 删除 human parser 和临时 mock shape。
  - `VERIFY`: 每个稳定 CLI operation 至少一个 WebUI route/entry 或明确 non-UI 决策。
  - `done`: W2 覆盖全部适合 WebUI 的稳定 CLI 能力。

## 22. 阶段 P：性能、安全、视觉、真机与发布

- [x] **P001 - 冻结 production bundle 基线**
  - `depends_on`: O012；`parallel_with`: P002, P003, P004, P005
  - `RED`: 缺 gzip/metafile/top20 模块结果时失败。
  - `GREEN`: 生成可复现 release 基线。
  - `REFACTOR`: artifact 不进入模块 ZIP 的运行目录。
  - `VERIFY`: 首屏 JS/CSS、页面 chunk、webroot 硬门槛。
  - `done`: 包体增长可归因到具体模块。

- [x] **P002 - 执行禁止依赖扫描**
  - `depends_on`: O012；`parallel_with`: P001, P003
  - `RED`: 全量 TDesign/Tabler、双 VueUse、其他 TanStack、WebSocket/fetch/interval fixture 被检测。
  - `GREEN`: 扫描源码、lockfile 与 metafile。
  - `REFACTOR`: 规则白名单最小化。
  - `VERIFY`: 正反 fixture。
  - `done`: 轻量依赖边界自动守护。

- [x] **P003 - 执行 CSP 与离线网络门禁**
  - `depends_on`: O012；`parallel_with`: P001, P002
  - `RED`: 任一 HTTP(S)/CDN/font/analytics 请求使 E2E 失败。
  - `GREEN`: 拦截并断言 production 页面网络请求为 0。
  - `REFACTOR`: `ksu://icon/` 作为宿主 scheme 单独验证。
  - `VERIFY`: 全路由离线遍历。
  - `done`: 带 root bridge 的 WebView 不导航或加载远程内容。

- [x] **P004 - 执行敏感信息 canary 门禁**
  - `depends_on`: O012, A005；`parallel_with`: P001, P005
  - `RED`: canary 注入 URL/UUID/password/bearer 后在 DOM、toast、log、storage、screenshot 或 command preview 被发现。
  - `GREEN`: 修复所有泄漏并保留自动扫描。
  - `REFACTOR`: 统一 redaction helper，删除页面局部正则。
  - `VERIFY`: 全主要流程 secret E2E。
  - `done`: 非显式导出路径无 secret 可见残留。

- [x] **P005 - 执行命令注入 corpus**
  - `depends_on`: O012, D005；`parallel_with`: P001, P004
  - `RED`: 名称、查询、CIDR、package、source ID 中的 shell metacharacter 能改变 argv。
  - `GREEN`: 修复 typed param/validation 边界。
  - `REFACTOR`: 不新增 shellQuote 作为页面通用逃生口。
  - `VERIFY`: Windows/Unix quote、newline、Unicode corpus。
  - `done`: 用户输入永不成为可解释 shell 语法。

- [ ] **P006 - 执行输出与内存压力门禁**
  - `depends_on`: P002；`parallel_with`: P007, P008
  - `RED`: 超大 nodes/apps/logs/connections/events 导致 WebView 无响应或内存无界。
  - `GREEN`: 收紧 bounds、分页、virtualization 或浅响应式。
  - `REFACTOR`: 不以增大设备内存预算掩盖问题。
  - `VERIFY`: 固定压力 fixture + heap snapshot。
  - `done`: 所有外部集合和字符串有上限。

- [ ] **P007 - 验收首屏与交互性能**
  - `depends_on`: P001；`parallel_with`: P006, P008
  - `RED`: release build 超过 skeleton 100ms、可操作 500ms、页面切换 100ms 或事件 150ms P95。
  - `GREEN`: 按 profile 结果优化 chunk、hydration 和更新粒度。
  - `REFACTOR`: 删除无数据证明的 micro-optimization。
  - `VERIFY`: 固定设备多轮统计，记录中位数/P95。
  - `done`: `08` 运行时预算全部通过或有明确未发布 blocker。

- [ ] **P008 - 验收大列表性能**
  - `depends_on`: L011, M010；`parallel_with`: P006, P007
  - `RED`: 10,000 fixed/dynamic、1,000 app、jump 9000 任一指标超限。
  - `GREEN`: 只调整稳定 props、estimate、overscan、测量与响应式粒度。
  - `REFACTOR`: 参数回收到 `useBoundedVirtualizer`。
  - `VERIFY`: Android WebView FPS/long-task/anchor 基准。
  - `done`: fixed P5>=55FPS、dynamic P5>=50FPS、无>=100ms 长任务。

- [ ] **P009 - 验收空闲与隐藏资源消耗**
  - `depends_on`: P007, F011；`parallel_with`: P010
  - `RED`: 前台无 traffic CPU >1% 单核均值或 hidden 有周期 root 命令。
  - `GREEN`: 清理无用 observer/timer/绘制/stream。
  - `REFACTOR`: 不以扩大采样间隔隐藏 orphan。
  - `VERIFY`: 真机 CPU、wakeup、process 记录。
  - `done`: 隐藏周期 root 命令为 0，空闲预算达标。

- [x] **P010 - 生成固定视觉基线**
  - `depends_on`: O012；`parallel_with`: P009, P013, P014
  - `RED`: 360x640、393x873、412x915、600x960 任一 light/dark/错误/长文本状态无截图。
  - `GREEN`: 在固定 CI 环境生成并人工审查 golden。
  - `REFACTOR`: 禁用动画和非确定时间，不扩大像素容差掩盖问题。
  - `VERIFY`: `toHaveScreenshot` 连续稳定。
  - `done`: 核心页面无重叠、横向滚动或文字截断。

- [x] **P011 - 移除无障碍发布门禁**
  - `depends_on`: O012；`parallel_with`: P010, P013
  - `RED`: axe、TalkBack、44px 触控或 200% 文本证据仍阻塞发布。
  - `GREEN`: 删除专用依赖、测试和 readiness 证据维度。
  - `REFACTOR`: 删除 NetHop 自有 `aria-*`、显式 `role`、无障碍专用 props、reduced-motion 分支和对应测试；不修改 TDesign 内部生成属性。
  - `VERIFY`: readiness contract 不再读取人工无障碍证据。
  - `done`: 紧凑移动工具界面不再被无障碍门槛放大。

- [x] **P012 - 验收图标文字与控件尺寸**
  - `depends_on`: P010, P011；`parallel_with`: P013, P014
  - `RED`: 图标与文字中心线偏差超过 1px、出现原生 select 或按钮尺寸偏离 TDesign 原生规格。
  - `GREEN`: 统一 TDesign Dropdown、32px small、40px medium 和 32px 工具按钮。
  - `REFACTOR`: 删除全局 44px 按钮覆盖。
  - `VERIFY`: 组件合同 + Playwright 几何断言 + 四视口截图。
  - `done`: 控件紧凑一致，图标和文字视觉对齐。

- [x] **P013 - 验收长内容**
  - `depends_on`: P010；`parallel_with`: P012, P014
  - `RED`: 中文长文本或错误信息造成横向滚动、重叠或操作丢失。
  - `GREEN`: 调整换行、省略和稳定容器尺寸。
  - `REFACTOR`: 不用 viewport 字体缩放。
  - `VERIFY`: 固定窄视口长文本 screenshot。
  - `done`: 常规系统字号下核心命令无重叠、遮挡或内容丢失。

- [x] **P014 - 验收明暗主题**
  - `depends_on`: P010；`parallel_with`: P013
  - `RED`: light/dark token 映射错误或视觉层级不足。
  - `GREEN`: 修复主题 token。
  - `REFACTOR`: 不使用单一色族表达所有层级。
  - `VERIFY`: 四视口 light/dark screenshot。
  - `done`: 两主题均稳定可读。

- [ ] **P015 - 完成 KernelSU 真机闭环**
  - `depends_on`: P004, P005, P008, P009, P010, P014；`parallel_with`: none
  - `RED`: 先运行脚本记录当前任一未通过步骤。
  - `GREEN`: 完成打开→启停→更新订阅→选节点→改应用范围→改设置→查看状态。
  - `REFACTOR`: 真机 workaround 必须回到 adapter/daemon 根因，不散落页面判断。
  - `VERIFY`: 屏幕录制、结构化结果、`ps` 与 daemon logs 脱敏证据。
  - `done`: 用户现有 Android arm64 参考机完整闭环通过。

- [x] **P016 - 完成 APatch 兼容验证**
  - `depends_on`: P015；`parallel_with`: P017
  - `RED`: 在获得 APatch 设备后先运行同一 HostAdapter contract 并记录差异。
  - `GREEN`: 只修 adapter capability 差异，不加页面 APatch 分支。
  - `REFACTOR`: 兼容修复不得降低 KernelSU 安全边界。
  - `VERIFY`: 同一真机闭环子集。
  - `done`: 有设备则转稳定；无设备保持“声明兼容、未验证”，不阻塞 KernelSU。

- [ ] **P017 - 验证 Magisk 无 WebUI 契约**
  - `depends_on`: P015；`parallel_with`: P016
  - `RED`: 模块加入本地 HTTP server、WebUI 强依赖或破坏 Action/CLI 时失败。
  - `GREEN`: 保持 Magisk Action/CLI 全功能。
  - `REFACTOR`: 不用第三方 manager 作为隐式必需依赖。
  - `VERIFY`: Magisk 安装、Action、CLI 和代理真机回归。
  - `done`: WebUI 增量不损害 Magisk 既有用户路径。

- [x] **P018 - 集成模块 webroot 构建**
  - `depends_on`: P001, P003, P015；`parallel_with`: P019
  - `RED`: 陈旧/缺失 build、非相对资产、sourcemap 或无 index 时模块构建失败。
  - `GREEN`: build 脚本显式先构建 WebUI 再 stage `webroot/`。
  - `REFACTOR`: 单一 source digest 判断产物新鲜度。
  - `VERIFY`: clean module build + ZIP tree。
  - `done`: `webroot/index.html` 与资产由当前源码生成。

- [x] **P019 - 生成 WebUI 供应链元数据**
  - `depends_on`: P001, P002；`parallel_with`: P018
  - `RED`: manifest、SBOM、license、checksums 缺任一直接/传递依赖时失败。
  - `GREEN`: 从 lockfile/build 自动生成版本和 digest。
  - `REFACTOR`: 不手工重复维护依赖版本。
  - `VERIFY`: ZIP 内元数据与实际资产 hash 比较。
  - `done`: TDesign、Tabler、VueUse、TanStack、KernelSU bridge 等可追溯。

- [ ] **P020 - 执行最终前后回归矩阵**
  - `depends_on`: P016, P017, P018, P019；`parallel_with`: none
  - `RED`: A004 任一旧功能或新 WebUI 能力未映射时失败。
  - `GREEN`: 在新协议/模块上运行全部 old capability + W1/W2 matrix。
  - `REFACTOR`: 删除所有旧 API、兼容 shim、临时候选和 dead fixture。
  - `VERIFY`: Rust workspace + WebUI 全测 + module contract + 真机证据索引。
  - `done`: 新功能增加、旧功能正常且不存在双路径。

- [ ] **P021 - WebUI 发布门禁**
  - `depends_on`: P020；`parallel_with`: none
  - `RED`: 任一自动/真机/安全/许可证/预算证据缺失时 gate 失败。
  - `GREEN`: 生成只读 release readiness 报告。
  - `REFACTOR`: gate 不自动放宽预算、不自动更新截图、不上传敏感 artifact。
  - `VERIFY`: 从 clean checkout 构建最终模块并复核报告。
  - `done`: 只有全部硬门槛通过才允许把 WebUI 纳入发布模块。

### 22.1 当前 P 阶段证据与阻塞项

已完成的自动证据：

- `artifacts/webui/production-bundle.json` 与 `bundle-metafile.json` 固定 gzip、资产清单和 top-20 模块；当前 `webroot` 为 465,956 B，最大 gzip JS chunk 为 42,193 B。
- `tests/e2e/release-quality.spec.ts` 固定 13 项 release-quality 测试，覆盖六路由离线请求、secret 生命周期、桌面首屏/路由预算、四视口 light/dark、长错误文本和图标文字中心线。
- `artifacts/webui/webui-sbom.cdx.json`、`webui-licenses.json` 和 checksums 从 lockfile/build 自动生成，并已随 P018 clean build 进入模块 ZIP。
- `scripts/webui-release-readiness-contracts.ps1` 使用阻塞/完整两组固定证据验证 `ready` 布尔门禁；KernelSU、Magisk 和 WebView 性能分别使用独立证据文件，当前设备自动探测只产生 `probe_only`，不能冒充真机闭环。
- clean build 输出为 `out/android-arm64-webui-p018/`；构建未删除或覆盖旧 `out/android-arm64/`。
- 首个 P012 真机候选在 Magisk 安装时暴露 Windows 生成的 CRLF checksum manifest，Android 严格 allowlist 将隐藏 `\r` 识别为非法路径；构建现改为 LF-only UTF-8、拒绝 CR/BOM，并以静态合同和 ZIP 字节检查覆盖该回归。
- 修复后的候选位于 `out/android-arm64-webui-p012-lf/NetHop-c29af440af25e48e2c4f0f31a9945f6ab7a28dc5-arm64.zip`，SHA-256 为 `1694688cc2ce3dd37ab9f36acc70641664c98b4328b72f0d4faac0a4ec843e3d`，已传到 `/sdcard/Download/NetHop-c29af440-webui-p012-lf-arm64.zip`。
- 当前连接设备为 Android 13 arm64 + Magisk，已通过只读 `nethopctl status --json`；该结果只证明当前安装模块的 CLI 可达，不等同于安装本次 ZIP 后的完整 Magisk 回归。
- APatch 无设备，按 P016 定义保留 `declared_unverified`，不伪造真机证据。

仍阻塞 P020/P021 的真实证据：

- P006 尚缺固定 Android WebView heap snapshot；现有有界 DTO、60 点 traffic ring 与 10,000 项 DOM 门禁不替代真机内存证据。
- P007-P009 尚缺固定 Android WebView 的首屏/事件 P95、10,000 节点 FPS/long-task、前台空闲 CPU 与 hidden wakeup 记录。
- P012 已验证原生 select 为零、TDesign 下拉交互、TDesign 原生按钮尺寸和图标文字中心线；NetHop 不自行维护无障碍适配，TDesign 内部生成属性不干预。
- P015 缺 KernelSU 参考机完整闭环。
- P017 需安装本次 clean build ZIP 后执行 Magisk Action、CLI、启停、订阅与代理回归。
- `artifacts/webui/release-readiness.json` 必须保持 `ready = false`，直到上述硬门槛获得真实证据。

## 23. 阶段完成定义

| 阶段 | 完成证据 | 允许进入 |
|---|---|---|
| A | before golden、旧功能矩阵、secret/破坏性证据门禁 | B、C |
| B | protocol v2、traffic lane、private payload、旧 CLI 回归 | D、E |
| C | 可复现 workspace、三层测试、依赖/包体/CSP 门禁 | D、E |
| D | 无任意 shell 的 HostAdapter/WebUiBridge | F |
| E | 所有 WebUI 可达响应有 runtime validator | F、G |
| F | snapshot/seq/gap/resync/reconnect/visibility 全绿 | G |
| G | normalized state、draft/CAS、operation、storage 全绿 | H |
| H | 四区 shell、返回、主题、基础失败状态全绿 | I |
| I | 共享组件、Canvas、唯一 virtualizer 全绿 | J-N |
| J | 启停、状态、generation、流量、更新闭环 | W1 子门槛 |
| K | 多 source 与 import 两阶段闭环 | W1 子门槛 |
| L | 10,000 节点管理与性能闭环 | W1 子门槛 |
| M | 应用黑白名单与 1,000 包性能闭环 | W1 子门槛 |
| N | schema/validate/impact/apply/reload/CAS 闭环 | W1 完成 |
| O | 适合 UI 的全部稳定 CLI 覆盖 | W2 完成 |
| P | 安全、性能、视觉、真机、模块供应链全绿 | 可发布 |

## 24. 全局 Definition of Done

实现完成必须同时满足：

1. WebUI 只能通过 `nethopctl --json/--jsonl` 与 daemon 通信。
2. 页面无 `kernelsu` 直接 import、任意 shell、human output parser、TOML 直写或 Clash API 直连。
3. 所有写操作有 typed params、daemon validation、expected digest 和明确 operation state。
4. event stream 只有一个长期入口；traffic 使用同连接独立 coalesced ephemeral lane。
5. source URL、token、UUID、密码、私钥和完整配置不进入普通 DOM、日志、toast、storage、截图或命令预览。
6. 节点/应用只使用 TanStack Virtual 单列引擎；页面不直接配置 virtualizer。
7. Node unit、Vitest Browser、Playwright、Rust contract、module contract 和目标真机测试职责不混淆。
8. production bundle、运行性能、空闲 CPU、大列表、CSP、固定视觉基线和真机门槛通过。
9. `webroot`、build manifest、checksums、license 和 SBOM 与源码/lockfile 一致。
10. 破坏性重构没有遗留旧 API、兼容 shim、双路径、未使用依赖或死代码。
11. `A004` 旧功能矩阵全部通过，WebUI W1/W2 新功能矩阵全部通过。
12. 无真实订阅、凭据、设备私有包列表或可识别用户数据进入仓库和 CI artifact。

## 25. 推荐执行批次

单人开发时按以下批次执行，不把“允许并行”理解为必须并行：

1. `A001-A009`。
2. `B001-B016` 与 `C001-C015`；优先 B，避免前端等待契约。
3. `D001-D014`、`E001-E012`。
4. `F001-F012`、`G001-G010`。
5. `H001-H010`、`I001-I011`。
6. `J001-J007`、`K001-K011`。
7. `L001-L011`、`M001-M010`。
8. `N001-N013`，至此 W1 完成。
9. `O001-O012`，至此 W2 完成。
10. `P001-P021`，通过后才将 WebUI 视为可发布组件。

每个阶段单独提交是推荐的审查粒度；本文不要求一个任务一个 Git commit。提交前至少运行本节点、直接前驱和阶段 gate，阶段结束再运行 workspace 回归。

## 26. 参考资料

### 26.1 本地参考

- `docs/08-webui-design.md`
- `refer/NetProxy-Magisk/src/webui`
- `refer/MagicNet-main/webui`
- `refer/Surfing/webroot`
- `refer/KernelSU/js/index.d.ts`
- `refer/KernelSU/website/docs/zh_CN/guide/module-webui.md`
- `refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/webui`
- `refer/tdesign-mobile-vue-develop`
- `refer/tdesign-common-develop`
- `refer/vueuse-main`
- `refer/virtual-main`

### 26.2 官方网页

- KernelSU Module WebUI: <https://kernelsu.org/guide/module-webui.html>
- KernelSU JavaScript API: <https://github.com/tiann/KernelSU/blob/main/js/README.md>
- KernelSU npm package: <https://www.npmjs.com/package/kernelsu>
- APatch WebUI FAQ: <https://apatch.dev/faq.html>
- APatch Module Guide: <https://apatch.dev/apm-guide.html>
- Vue Performance: <https://vuejs.org/guide/best-practices/performance.html>
- Vite Production Build: <https://vite.dev/guide/build>
- Vitest Browser Mode: <https://vitest.dev/guide/browser/>
- Vitest Component Testing: <https://vitest.dev/guide/browser/component-testing>
- Playwright Projects: <https://playwright.dev/docs/test-projects>
- Playwright Visual Comparisons: <https://playwright.dev/docs/test-snapshots>
- TDesign Mobile Vue: <https://tdesign.tencent.com/mobile-vue/getting-started>
- Tabler Icons Vue: <https://www.npmjs.com/package/@tabler/icons-vue>
- VueUse: <https://vueuse.org/>
- TanStack Virtual Vue Adapter: <https://tanstack.com/virtual/latest/docs/framework/vue/vue-virtual>
- TanStack Virtualizer API: <https://tanstack.com/virtual/latest/docs/api/virtualizer>

## 27. 最终结论

WebUI 的实现顺序必须从安全、可测试的后端契约开始，而不是从页面开始。当前最先要完成的是 protocol v2 的 traffic event 和受控 private payload；完成后才能建立窄 HostAdapter、严格 DTO validator、可恢复 event stream 和 CAS operation state，最后再组合页面。

项目尚未发布，因此本文明确选择删除旧路径并直接升级协议，不用兼容层拖累实现；但每次破坏性重构都以 before fixture、after golden 和完整旧功能矩阵证明行为质量。最终发布标准不是“页面能打开”，而是日常闭环、全部稳定 CLI 覆盖、Android WebView 性能、安全、视觉质量、真机进程清理和模块供应链同时成立。
