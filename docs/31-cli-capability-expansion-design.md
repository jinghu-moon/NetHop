# NetHop CLI 功能扩展设计

> 状态：讨论稿 v0.1  
> 日期：2026-08-29  
> 适用范围：`nethopctl`、`nethopd`、NetHop WebUI、Companion 和模块 Action  
> 上位文档：[`00-nethop-system-design.md`](./00-nethop-system-design.md)

## 1. 背景与结论

NetHop 运行在 Android Root/Magisk、KernelSU 或 APatch 环境中。普通用户主要通过 WebUI、Companion 或文本编辑器操作配置，直接进入终端的频率较低。因此 `nethopctl` 的首要职责不是提供桌面软件式的交互体验，而是：

1. 为 WebUI、Companion、Action 和排障脚本提供稳定的结构化控制入口；
2. 为文本编辑器修改 TOML 后提供校验、reload、错误定位和恢复状态；
3. 将订阅、节点、应用策略、网络接管和诊断能力包装成受限 typed command；
4. 保持 daemon 是唯一业务状态源，不允许 CLI 绕过 digest、generation、校验和回滚事务。

结论是：应优先补齐 Android 模块的服务、配置、订阅、节点、网络和诊断闭环；`--help`、补全、漂亮表格和通用网络工具只作为开发者便利功能，不应占用 P0。

## 2. 参考项目

### 2.1 Proxylink-main

参考路径：[`refer/Proxylink-main`](../refer/Proxylink-main/README.md)

Proxylink 是节点和订阅转换工具，重点能力包括：

- 从单条 URI、文件、stdin、订阅 URL、Clash YAML、Xray JSON 和 sing-box JSON 导入；
- 批量解析和部分失败统计；
- 输出 Profile JSON、Xray 配置、sing-box 配置或 URI；
- 多节点拆分为文件，并进行文件名去重和清理；
- 通过明确的 `-format`、`-file`、`-sub`、`-o` 参数支持脚本调用。

对 NetHop 的启示是：移动端需要稳定的“导入、预览、应用、导出”数据流，而不是让用户在终端拼接复杂参数。NetHop 已有受限订阅导入 preview/apply，后续重点应放在导入结果展示、历史和失败诊断。

### 2.2 Surfing

参考路径：[`refer/Surfing`](../refer/Surfing/README_CN.md)

Surfing 是面向 Magisk、KernelSU 和 APatch 的集成模块，用户路径具有明显的 Android 特征：

- 用户直接编辑 `/data/adb/box_bll/clash/config.yaml` 或模块配置文件；
- Web App 用于订阅填写和后台路由管理；
- 模块开关、Wi-Fi SSID 和状态栏磁贴控制启停；
- 规则和订阅支持定时更新；
- 更新前备份配置、订阅地址和旧运行文件。

对 NetHop 的启示是：`config reload`、配置错误提示、备份、Wi-Fi/网络场景状态和即时启停，比终端帮助和补全更重要。

### 2.3 MagicNet-main

参考路径：[`refer/MagicNet-main`](../refer/MagicNet-main/README.md)

MagicNet 将 CLI 作为 Android 模块的统一控制面，典型命令包括：

```text
health
transparent status
service restart sing-box
sub status|update
node test-all|current|list
wifi status
hotspot status
network status
support bundle
repair
```

其关键经验是：

- 健康检查同时验证核心、TUN 和实际设备接口；
- 订阅更新失败时保留当前可用配置；
- 配置应用、透明代理应用和完整服务重启具有不同语义；
- 支持包用于移动端排障，并且必须脱敏；
- WebUI 显示运行事实，不伪造命令输出或状态。

### 2.4 NetProxy-Magisk-main

参考路径：[`refer/NetProxy-Magisk-main/docs/guide/cli.md`](../refer/NetProxy-Magisk-main/docs/guide/cli.md)

NetProxy 是四个项目中最完整的 Android CLI 参考。其命令面包括：

```text
service status|start|stop|restart|reload|check
catalog list|show
node list|current|show|import|edit|remove|use|delay|export
sub list|show|add|edit|update|update-all|history|cancel
mode
network evaluate
app list|mode|users|add|remove|enable|disable
ebpf status
config list|read|check|validate|apply
logs show|clear|export
```

其 CLI 契约也值得吸收：所有机器输出为版本化 JSON，stdout 只输出一个结果，失败同时依赖退出码、稳定错误码和 message；命令有统一 timeout；日志导出包包含脱敏后的运行摘要。

## 3. 当前 NetHop 基线

当前 [`nethopctl`](../crates/nethopctl/src/lib.rs) 已覆盖：

- service start/stop、capture enable/disable/status、core start/stop/status；
- status、probe、metrics、traffic、topology、diagnose；
- config get/schema/validate/apply/mutate/reload；
- subscription list/mode/select/add/remove/move/enable/disable/import/update；
- node list/test/test-all/selection/select/remove/export/override get/remove；
- application list/mode、package/UID 增删、network set；
- connections list/close/close-all、logs get/tail/clear；
- backup export/restore、ruleset status/update、WebUI private payload。

当前 CLI 已经是薄客户端，并通过 root-only UDS 使用版本化协议。扩展时不应重新引入直接读写 TOML、iptables、sing-box API secret 或任意 shell command 的路径。

## 4. 功能扩展优先级

### 4.1 P0：移动端业务闭环

| 功能 | 建议命令/接口 | 说明 |
|---|---|---|
| 服务健康和恢复 | `service check/status/restart/reload` | `check` 汇总核心、TUN/TPROXY、DNS、接口和当前 generation；`restart` 是保留配置的运行时操作，不等价于修改 `service.enabled`。 |
| 文本配置校验 | `config check` | 校验磁盘上的 TOML，返回错误阶段、行号/列号、字段、当前 digest、上次有效 generation 和恢复建议。 |
| 保存后应用 | `config reload --wait` | 返回完整 reconcile 阶段和可恢复状态；失败时继续保留旧运行配置。 |
| 订阅详情 | `subscription show <source-id>` | 返回来源摘要、启用状态、格式、节点计数、最近成功/失败和最近 generation，不返回 URL、Header 或凭据。 |
| 订阅诊断 | `subscription diagnose <source-id>` | 执行有界的 fetch/detect/parse 诊断，返回稳定 diagnostic code、阶段耗时和摘要；不得改变活动 generation。 |
| 订阅编辑和批量更新 | `subscription edit ...`、`subscription update-all`、`subscription history` | 为 WebUI 提供 typed source settings、更新周期、过滤器和历史查询。敏感 Header 使用 private payload 或受限文件输入。 |
| 节点当前状态 | `node current`、`node show <node-id>` | 将 requested intent、active terminal、延迟、来源和覆盖状态一次返回，避免 WebUI 组合多个低层查询。 |
| 节点导入/编辑 | `node import`、`node edit` | 复用 Proxylink/现有 parser 的有界 URI、YAML、JSON 和文本导入；导入必须经过 preview、校验和原子提交。是否持久化为 manual source 需要单独冻结数据模型。 |
| 节点使用和测速 | `node use auto|manual`、`node delay` | 为移动端节点页提供明确的即时切换和单节点测速语义。 |
| 网络和场景状态 | `network status/evaluate`、`wifi status`、`hotspot status` | 输出实际接口、路由、DNS split、IPv4/IPv6、TUN/TPROXY 和 Wi-Fi/热点场景结果。 |
| 捕获能力诊断 | `capture check` | 类似 NetProxy 的 `ebpf status`，默认返回面向用户的能力摘要；原始探测结果只能通过受控 debug 字段获取。 |

### 4.2 P1：WebUI 和排障增强

| 功能 | 建议命令/接口 | 说明 |
|---|---|---|
| Android 多用户应用策略 | `application users`、所有 app mutation 增加 `--user` 或 typed user 集合 | 当前实现将 `android_user_id` 固定为 `0`，应支持主用户、分身和应用多开用户。 |
| 应用策略快捷操作 | `application policy set`、批量 targets 操作 | 将已有 `SetApplicationPolicy` 和 `ReplaceApplicationTargets` 暴露为稳定入口，避免前端构造通用 mutation。 |
| 脱敏支持包 | `logs export`、`support bundle` | 输出到用户可访问目录，包含运行摘要、核心日志、订阅状态和版本信息；凭据、UUID、Header、URL token 和节点 secret 必须脱敏。 |
| 实时流量 | `traffic live --jsonl` 或 events traffic lane | 主要供 WebUI 使用，支持有界采样间隔、最长运行时间和断线重连，不为每个采样启动 root shell。 |
| 连接详情 | `connection show <id>`、连接筛选参数 | 供 WebUI 展示应用、目标、出站、规则、速率和累计流量；不要求终端表格化输出。 |
| 节点覆盖 | `node override apply` | 仅允许 bounded file/stdin 或 WebUI private payload，禁止把凭据直接放入 argv。 |
| 规则集详情 | `ruleset list/show` | 展示 provider、digest、版本、最近更新时间和失败原因；不在手机端增加本地规则集编译器。 |

### 4.3 P2：开发者便利功能

| 功能 | 建议命令/接口 | 说明 |
|---|---|---|
| 帮助和版本 | `--help`、子命令帮助、`--version` | 便于开发、Issue 排障和手工验收，但不是普通用户主路径。 |
| Shell 补全 | Bash/Zsh/Fish completion | 可从静态命令定义生成，不影响 Android 运行时依赖。 |
| 网络工具 | `tools connect/fetch/stun/network-quality` | 仅作为 debug/开发构建能力，必须复用 daemon 安全网络边界。 |

## 5. 协议和安全约束

1. 只读查询优先增加 typed `ControlMethod`，不要让 WebUI 发送任意 JSON mutation。
2. 所有写操作继续携带 expected digest 或使用 daemon 已定义的事务前置条件。
3. 大 payload 使用 private payload 或有界文件/stdin；不得把完整订阅、配置、Header 和节点凭据放进命令行。
4. CLI stdout 保持单个版本化 JSON envelope；日志和调试信息写 stderr；退出码、稳定 `code` 和 message 三者语义固定。
5. 所有查询结果进行 schema、大小、数组数量、枚举和敏感字段校验；未知结构不能被静默接受。
6. `service restart`、`config reload`、`capture check` 和 `support bundle` 必须有明确 timeout，并在 daemon 不可用时返回稳定错误。
7. 任何导出到共享存储的功能都必须由 daemon/Companion 负责路径白名单、文件权限和脱敏，不能开放任意路径写入。

## 6. 不纳入本阶段

- 不提供直接 Clash HTTP API URL、secret 或任意 API 代理。
- 不开放任意 socket、工作目录、配置路径和 shell command 参数。
- 不照搬 Mihomo 的任意 proxy group、provider、DNS/cache 管理接口；NetHop 对外使用稳定 node ID 和 generation 抽象。
- 不在 CLI 中实现规则集编译、GeoIP/GeoSite 数据处理或绕过 daemon 的配置 merge。
- 不把复杂终端交互、表格渲染、颜色主题作为移动端功能目标。

## 7. 实施顺序与验收

建议按以下顺序实施：

1. `config check` 和 `config reload --wait` 的错误模型、行列定位和旧 generation 保留；
2. `service check/restart`、`network status/evaluate` 和 `capture check`；
3. `subscription show/edit/diagnose/history/update-all`；
4. `node current/show/import/edit/use/delay`；
5. Android 多用户应用策略和 `support bundle`；
6. WebUI 所需的 traffic、connection、ruleset 和 private override 操作；
7. 最后补充 help、version、completion 和 debug tools。

每项功能至少需要：

- protocol、daemon、CLI 三层 contract test；
- JSON/JSONL 正常响应和错误响应 golden；
- 超时、超限、并发冲突、daemon 不可用和旧 generation 保留测试；
- secret scan，确认 URL、token、UUID、Header 和节点凭据不会出现在 stdout、日志或诊断包中；
- WebUI/Companion 的固定 argv、timeout 和状态映射测试。

## 8. 开发期 TDD 分阶段任务

### 8.1 开发策略

本项目仍处于开发期，没有正式发布版本和外部兼容性承诺。因此允许并鼓励：

- 删除错误的旧命令、旧 DTO、旧协议字段和重复实现；
- 直接重构状态模型、命令分组、协议方法和存储结构；
- 通过提高 protocol/schema version 彻底删除兼容分支，而不是长期保留 shim；
- 在根因处修复问题，不为了兼容历史实现而增加转换层或双写路径。

“不考虑兼容性”不等于“不回归”。每个破坏性改动都必须先建立变更前基线，再以同一测试矩阵证明：

1. 现有功能在新实现中继续通过；
2. 新功能的正常、错误、超限、并发和恢复行为均有测试；
3. 删除的旧行为不会被隐式重新接受；
4. WebUI、Companion、Action 和 CLI 的固定契约同步更新。

### 8.2 阶段任务

任务格式遵循 `RED -> GREEN -> REFACTOR -> VERIFY`。每个阶段开始前运行上一阶段的完整回归，阶段结束后不得保留 ignored、skipped 或仅测试旧兼容路径的用例。

| 阶段 | 任务 | RED / GREEN / REFACTOR / VERIFY |
|---|---|---|
| A | CLI 基线与契约冻结 | **RED**：为现有命令、退出码、JSON envelope、错误码和 WebUI argv 建立缺失断言；**GREEN**：记录当前行为 golden；**REFACTOR**：删除重复 parser 分支和无效兼容字段；**VERIFY**：`cargo test -p nethopctl --tests`、CLI 全量 negative tests。 |
| B | 配置检查和服务恢复 | **RED**：`config check`、TOML 行列错误、`service check/restart` 和旧 generation 保留测试先失败；**GREEN**：增加 protocol/daemon/CLI 实现；**REFACTOR**：统一 reload、restart、health 状态机，删除绕过 daemon 的路径；**VERIFY**：`nethopctl`、`nethopd` 配置/服务 contract 全绿。 |
| C | 订阅生命周期 | **RED**：`subscription show/edit/diagnose/history/update-all`、失败保留旧 generation、并发 digest 冲突测试；**GREEN**：增加 typed source status/history/diagnose；**REFACTOR**：合并重复 fetch/update 报告模型，删除旧的模糊 update 语义；**VERIFY**：订阅 parser、fetch、daemon worker、CLI 和 secret scan 全量通过。 |
| D | 节点和应用 | **RED**：`node current/show/import/edit/use/delay`、manual source、Android user 集合和应用策略测试；**GREEN**：实现稳定 node DTO 与 typed app mutation；**REFACTOR**：删除内部 tag 泄露、UID 猜测和固定 user 0 的实现；**VERIFY**：节点选择、测速、覆盖回滚、应用多用户和 WebUI mapping 全绿。 |
| E | 网络、捕获和诊断 | **RED**：`network status/evaluate`、Wi-Fi/热点、`capture check`、`support bundle` 和脱敏导出测试；**GREEN**：接入 Android capability 和运行事实；**REFACTOR**：统一状态快照，删除从日志文本推断状态的代码；**VERIFY**：Android host、Companion、模块 Action 和真机 smoke 全绿。 |
| F | 实时查询和细节页 | **RED**：`traffic live`、connection detail/filter、ruleset detail、private node override 测试；**GREEN**：增加 JSONL lane 和有界查询；**REFACTOR**：统一事件订阅和取消语义，删除每秒启动命令进程的实现；**VERIFY**：事件顺序、断线重连、慢消费者、payload 清理和全量 WebUI 回归通过。 |
| G | 开发者便利功能 | **RED**：help/version/completion 和 debug tools 的命令 golden；**GREEN**：实现静态帮助和生成脚本；**REFACTOR**：不引入重量级 CLI 框架或运行时依赖；**VERIFY**：Host 构建、Android release 构建和现有命令全量回归。 |

### 8.3 前后测试门禁

每个阶段都要保存变更前和变更后的结果，但不维护旧协议兼容运行时。建议固定以下顺序：

```text
baseline tests (before)
  -> write failing contract tests
  -> minimal implementation
  -> destructive refactor / remove obsolete path
  -> unit + contract + integration + WebUI/Companion regression (after)
  -> Android device smoke and performance gate
```

基线至少包括：

- `cargo test --locked --workspace`；
- `cargo test -p nethopctl --tests`；
- `cargo test -p nethopd --tests`；
- protocol golden、secret scan、WebUI unit/browser/e2e 和 Companion JVM/Android tests；
- 模块安装后 `status`、`config reload`、`node list`、`subscription list`、`logs get` 的真机结果。

协议或 schema 发生破坏性修改时，应同步更新所有生产 fixture、DTO 和 bridge allowlist；旧 fixture 只能作为迁移前证据保存，不得让生产代码继续解析旧格式。

## 9. 性能测试

### 9.1 测试原则

性能测试分为 Host 算法基准、Android CLI/Root bridge 基准和真机端到端基准。不能用 Windows Host 的结果替代 Android 结论；网络下载耗时必须与本地解析、事务提交和核心重启分开记录。

每次性能改动都要记录：设备型号、Android/API、Root 管理器、ABI、构建 profile、commit、输入规模、冷/热启动、p50/p95/p99、峰值 RSS 和失败数。性能回归超过预算时先定位阶段，再决定重构或调整实现，不允许通过放宽安全上限掩盖回归。

### 9.2 测试矩阵

| 场景 | 规模/变量 | 测量内容 |
|---|---|---|
| CLI 冷启动 | 首次 root shell、无 daemon 缓存 | 进程启动、参数解析、UDS 建连、首帧延迟、峰值 RSS |
| CLI 热调用 | 连续 20/100 次 status、node list、config get | p50/p95 延迟、进程残留、stdout 大小和错误率 |
| 配置检查 | 最小配置、典型配置、接近 frame 上限的配置、错误行列 | TOML 读取、校验、诊断序列化和 reload 提交耗时 |
| 节点列表 | 64、500、2,000 节点；10,000 节点 conversion-only | list/query 序列化、筛选、内存峰值和分页行为 |
| 订阅导入 | URI、Base64、Clash YAML、sing-box JSON；5 MiB/10,000 节点 | detect、parse、dedupe、report、candidate commit 各阶段耗时 |
| 节点测速 | 1、16、64 candidate；成功、超时、混合失败 | dispatch、首结果、截止时间、terminal report 和 worker RSS |
| 事件/流量 | 1 秒采样、traffic lane、慢消费者、重连 | 帧间隔、丢帧/跳序、重连时间、daemon CPU 和队列上限 |
| 日志/诊断导出 | 100/128 条日志、完整 support bundle | 脱敏、压缩/写盘、输出大小、共享存储写入耗时 |
| 服务恢复 | stop/start/restart/reload、核心异常退出 | 从请求到稳定状态、旧 generation 保留、重复操作和回滚时间 |

### 9.3 初始预算

以下预算以现有性能文档和 Android 参考设备目标为基线；若实现路径改变，必须通过新的实测证据更新，而不是静默删除指标。

| 指标 | 初始目标 |
|---|---|
| `nethopctl` 单次进程峰值 RSS | `<= 4 MiB`，不含 daemon 和核心进程 |
| 本地只读 UDS 命令 p95 | `<= 200 ms`（不含网络下载和核心启动） |
| 配置 mutation/reload p95 | `<= 500 ms`（不含订阅远端下载） |
| 节点选择到 daemon 确认 p95 | `<= 200 ms`；新连接切换耗时另行记录 |
| 稳定 parser 5 MiB/10,000 节点 detect..serialize | 参考 Android 设备 `<= 300 ms`；超过 10,000 节点只承诺 conversion-only |
| 单条 JSONL 事件帧 | `<= 16 KiB`；超限必须拒绝或截断为稳定错误，不能拆成无界输出 |
| 订阅导入 payload | 沿用现有 CLI `768 KiB` 输入上限和 protocol frame 上限，不得因性能测试放宽 |
| support bundle/详细诊断 | 沿用现有有界序列化预算；凭据脱敏耗时和输出大小必须单独记录 |
| traffic live 采样 | 默认 `1 s`，每个采样最多一次 daemon 查询，不得按采样启动新 root 进程 |
| 服务恢复 | start/restart/reload 必须在命令 timeout 内返回明确 terminal state；超时不能报告成功 |

### 9.4 性能门禁

1. Host 基准、Android release 真机基准和端到端基准必须分别通过；不能以其中一项替代其他两项。
2. 任一 P0/P1 功能若使既有命令 p95、RSS、frame 大小或事件队列超过预算，阶段 VERIFY 失败。
3. 性能测试必须同时覆盖冷启动、热调用、空数据、满上限、错误和并发冲突；只测成功样本不算通过。
4. 测速、订阅下载和核心重启的远端/进程耗时必须拆分，避免把网络抖动误判为 CLI 算法性能。
5. 性能优化不得移除大小限制、超时、SSRF 校验、digest 检查、脱敏或回滚步骤。
6. 所有基准结果和原始环境信息写入 `artifacts/`，仅提交摘要和可复核脚本，不把敏感订阅内容写入 artifact。
