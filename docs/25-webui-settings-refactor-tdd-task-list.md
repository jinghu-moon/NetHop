# NetHop WebUI 设置界面重构 TDD 测试开发任务清单

> 状态：S0/S1 已实施并通过前后测试；S2/S3 按后端契约门禁暂不开放；S4 功能回归通过，既有概览视觉快照仍需基线确认
>
> 日期：2026-08-18
>
> 设计来源：[`21-webui-settings-refactor-design.md`](./21-webui-settings-refactor-design.md)
>
> 上位约束：[`06-configuration-toml-refactor-design.md`](./06-configuration-toml-refactor-design.md)、[`08-webui-design.md`](./08-webui-design.md)、[`09-webui-tdd-task-list.md`](./09-webui-tdd-task-list.md)
>
> 当前基线：配置 schema v3、control protocol v6、`SettingsView` 通用 schema 编辑器、`config.get`、`config.schema`、`config.mutate`、`config.validate/apply`、`capability.get`
>
> 影响范围：Rust daemon/协议/CLI、WebUI 设置页和二级页面、TDesign Mobile Vue 组件、Companion/WebView 只读状态桥接、Android 真机回归

## 1. 目标与边界

本清单把 D21 转换为可失败、可复现、可逐项验收的 TDD 任务。最终目标是：

1. 设置首页只展示分类、活动摘要和二级入口，不再暴露原始 field ID；
2. 每个可编辑配置只有一个领域入口；
3. 标量使用 typed schema 和适当的 TDesign 控件，集合/对象使用专用编辑器；
4. active config、draft、expected digest 和运行状态严格分离；
5. capability、validate/apply、CAS 冲突和失败回滚继续有效；
6. 没有后端事实源的项目不进入生产 UI；
7. WebUI、Companion、daemon 和 CLI 的旧行为在重构后保持正常。

本清单不授权自动执行 Git 提交、推送、模块安装、设备配置修改或删除用户文件。真机任务必须在明确授权后执行。

### 当前实施记录（2026-08-18）

- 已完成真实设置首页、分类摘要和 `/settings/{updates,network,interfaces,routing,logging,advanced}` 二级路由；首页分类由 daemon `config.schema` 动态决定，schema 没有字段时不渲染虚假控件。
- 已完成标量字段的中文展示映射、唯一编辑入口过滤、`Switch`/`Stepper`/枚举下拉、capability 禁用原因、active/draft/digest 状态和 validate/apply/CAS 事务链路。
- 已修正前端 schema `u32` 范围解析，并修正 daemon `proxy.urltest.max_candidates` 的上限契约为 64。
- 已修正 `ConfirmDialog` 使用 TDesign Mobile Vue 的 `title`、`cancel-btn`、`confirm-btn` API；确认按钮现在真实可用。
- 新增 WebUI unit/browser/e2e 测试覆盖字段分组、u32 范围、设置路由、能力禁用、草稿状态和 apply 后 digest 更新。
- S2 的 CIDR/域名/接口/Wi-Fi/资源对象编辑器，以及 S3 的环境、恢复默认、存储、备份和核心更新，在对应 daemon/CLI/Companion DTO 完成前保持未开放；不注册静态占位页。

### 1.1 开发期破坏性重构规则

项目尚未正式发布，允许一次性移除旧设置页面和兼容 wrapper，但不能留下不可验证的中间状态：

```text
BASELINE -> RED -> GREEN -> REFACTOR -> AFTER REGRESSION -> DEVICE VERIFY
```

1. 不保留旧的一级 schema group 页面作为第二入口；
2. 不保留以 `field.id` 作为用户文案的生产路径；
3. 不保留用通用 `Textarea` 编辑对象数组的最终路径；
4. 不为缺失协议增加假开关或静态菜单项；
5. 旧行为必须先有 before 测试，重构完成后再由 after 测试证明仍然成立。

## 2. TDD 执行规则

每个任务必须记录以下内容：

| 阶段 | 要求 |
|---|---|
| BASELINE | 在当前代码上运行直接相关测试，保存命令、版本、退出码和行为摘要 |
| RED | 添加一个因目标能力尚不存在而失败的最小测试；失败原因必须是产品缺口 |
| GREEN | 只实现使当前测试通过的最小代码，不能提前实现后续阶段 |
| REFACTOR | 删除旧路径、重复事实源和临时分支，保持窄接口和单一职责 |
| VERIFY | 运行任务测试、所有前驱回归、静态门禁和必要的真机测试 |

失败分类：

- `PRODUCT_RED`：允许，表示目标能力尚未实现；
- `FIXTURE_ERROR`、`TOOLCHAIN_ERROR`、`NETWORK_ERROR`：不算有效 RED，必须先修复环境或 fixture；
- `REGRESSION`：阻止进入下一任务；
- `SECURITY_GATE`：任何自由路径、自由命令、敏感值泄露或绕过 CAS 均直接失败。

## 3. 测试分层与证据

| 层级 | 主要范围 | 工具/命令 | 建议证据目录 |
|---|---|---|---|
| Rust unit | schema metadata、validator 边界、摘要格式、默认值 | `cargo test -p nethopd` | `artifacts/settings-tdd/rust-unit/` |
| Rust contract | `config.schema`、`config.mutate`、reset/storage/environment DTO | `cargo test -p nethopd --tests` | `artifacts/settings-tdd/rust-contract/` |
| CLI contract | CLI 参数、JSON、错误域、allowlist | `cargo test -p nethopctl --tests` | `artifacts/settings-tdd/cli/` |
| WebUI unit | 字段映射、摘要、草稿、CAS、错误映射、localStorage | `npm run test:unit` | `artifacts/settings-tdd/webui-unit/` |
| WebUI browser | Cell/Popup/Stepper/Picker/SwipeCell 交互和状态机 | `npm run test:browser` | `artifacts/settings-tdd/webui-browser/` |
| WebUI e2e | 设置路由、无重复入口、保存失败、viewport、主题 | `npm run test:e2e` | `artifacts/settings-tdd/webui-e2e/` |
| Companion JVM | host identity、版本 DTO、返回键和 bridge allowlist | Gradle JVM tests | `artifacts/settings-tdd/companion-jvm/` |
| Android instrumentation | WebView、返回键、状态加载、离线资源 | Gradle connected tests | `artifacts/settings-tdd/companion-device/` |
| 静态/构建 | typecheck、imports、dependency、CSP、bundle、模块契约 | `npm run gate`、项目 PowerShell gates | `artifacts/settings-tdd/gates/` |
| 真机 | Android 13+、Root/非 Root、暗色/亮色、网络能力 | ADB + 人工验收 | `artifacts/settings-tdd/device/` |

证据文件不得包含 Root 路径、私有 token、订阅 URL、应用包名清单或用户配置正文。每个任务的 `manifest.json` 至少记录任务 ID、Git revision、命令、退出码、耗时、测试数量和 `contains_sensitive_data=false`。

## 4. 需求追踪矩阵

| ID | D21 目标 | 主要任务 | 验收测试 |
|---|---|---|---|
| R01 | 设置首页分类摘要和二级入口 | S1-UI-01~04 | 首页不显示 raw field ID，摘要来自 active state |
| R02 | 单一编辑入口 | S1-UI-05 | 概览/订阅/应用/节点与设置无重复编辑控件 |
| R03 | schema/validator 边界一致 | S0-BE-01 | `max_candidates` 的 1、64 通过，0、65 拒绝 |
| R04 | typed 标量编辑 | S1-UI-06~08 | Switch、Radio/Dropdown、Stepper 的值域和禁用态 |
| R05 | CAS、validate/apply 和失败回滚 | S1-UI-09 | 冲突不覆盖外部修改，失败保留旧活动配置 |
| R06 | TDesign 移动交互 | S1-UI-10、S2-UI-01~04 | Popup、Dialog、Cell、SwipeCell、返回键行为 |
| R07 | CIDR/域名/接口领域编辑器 | S2-UI-01~03 | 规范化、去重、逐项错误和有界列表 |
| R08 | Wi-Fi 场景和敏感字段 | S2-UI-04 | 脱敏、动作互斥、冲突和 digest |
| R09 | 资源候选编辑和 probe | S2-UI-05 | 对象字段校验、排序、重复检查、probe reason |
| R10 | 环境和版本只读 DTO | S3-BE-01、S3-UI-01 | 不返回路径/命令，状态失败显示未知原因 |
| R11 | 恢复默认与存储管理 | S3-BE-02~03、S3-UI-02 | preview/apply、固定类别清理、失败不变 |
| R12 | 完整备份和核心更新边界 | S3-BE-04~05 | 私有 payload、签名/摘要、原子回滚 |
| R13 | 响应式、离线和无障碍 | S4-REG-01~05、S4-DEVICE-01 | 四个 viewport、明暗主题、无远程资源、键盘/返回键 |

## 5. 阶段 S0：基线和后端元数据

### S0-BASE 基线冻结

- [ ] **S0-BASE-01 设置加载基线**
  - `BASELINE`：冻结 `config.get`、`config.schema`、`capability.get` 并行加载、loading/error 状态和 raw JSON 编辑行为。
  - `AFTER`：重构后仍能读取配置、schema、capability；请求失败不显示伪造值。
  - `scope`：`webui/tests/e2e/foundation.spec.ts`、`webui/tests/unit/dto.test.ts`。

- [ ] **S0-BASE-02 保存事务基线**
  - 冻结 expected digest、validate/apply、冲突、失败回滚和 reload 行为。
  - 验证 apply 失败时 generation、active config 和运行状态不变。

- [ ] **S0-BASE-03 一级页面唯一入口基线**
  - 为概览、订阅、应用、节点和设置建立字段入口快照/选择器清单。
  - RED 前不得删除旧测试，避免重构后出现重复入口而无人发现。

### S0-BE schema 与 validator

- [x] **S0-BE-01 修正 `max_candidates` 元数据**
  - `RED`：增加 schema 上限 256 与 validator 上限 64 不一致的失败测试。
  - `GREEN`：让 `config.schema`、`config_model`、`config.mutate` 和 WebUI DTO 统一为 `1..64`。
  - `REFACTOR`：删除重复硬编码，使用同源边界常量或生成 metadata。
  - `VERIFY`：Rust schema/validator contract、CLI JSON 和 WebUI schema parser 全部通过。

- [x] **S0-BE-02 schema 字段展示元数据契约**
  - 为 field 增加/确认稳定的 `group`、`order`、`valueType`、`options`、`min/max`、`experimental`、`sensitive`、`riskLevel`、`applyImpact` 和 capability key。
  - 测试未知字段、未知 enum、范围缺失和敏感数组不能被前端误渲染。

- [ ] **S0-BE-03 config mutation allowlist 契约**
  - 覆盖允许的标量字段、禁止重复入口字段和集合字段的 mutation 方式。
  - 任何未知 path、错误类型、越界值和 capability 不支持值必须 fail closed。

## 6. 阶段 S1：设置首页和现有能力重构

### S1-UI 信息架构

- [x] **S1-UI-01 设置首页路由与分类**
  - `RED`：browser 测试要求 `/settings` 出现“运行环境、更新、网络、路由、日志、外观、数据、高级、关于”分类摘要，但当前 raw schema 页面不满足。
  - `GREEN`：引入设置首页 view model 和二级路由，不改变 daemon API。
  - `REFACTOR`：移除首页 raw field ID、内部 `applyImpact/riskLevel` 和通用数组 textarea。

- [x] **S1-UI-02 用户文案与摘要映射**
  - 建立受控 `field id -> 中文标题/摘要格式/单位/状态文案` 映射。
  - 测试未知 field 不直接回退为内部 ID；活动配置缺失时显示“未知”而不是默认值。

- [x] **S1-UI-03 `Cell`/`CellGroup` 结构契约**
  - 使用 `title`、`description`、`note`、`arrow`、`click` 构造首页摘要行。
  - 测试摘要只来自 active config/readonly state，不读取未提交 draft。

- [x] **S1-UI-04 二级路由和返回键**
  - 覆盖 `/settings/updates`、`network`、`interfaces`、`routing`、`logging`、`data`、`advanced`、`about`。
  - 返回顺序必须是关闭 Dropdown/Popup/Dialog，再处理 dirty 草稿确认。

- [x] **S1-UI-05 单一编辑入口 gate**
  - e2e 检查 `service.enabled`、`proxy.outbound_mode`、`network.capture_mode`、订阅源、应用名单和节点 intent 在设置页只有摘要/跳转，不出现第二控件。
  - 明确拒绝 `auto_start`、布尔 IPv6 和断线重连占位项。

### S1-UI 控件和保存

- [x] **S1-UI-06 TDesign 标量控件映射**
  - `Switch`：TCP/UDP、接口开关、日志相关二值项；
  - `RadioGroup`/`DropdownMenu`：IPv6、DNS、TUN stack 等 enum；
  - `Stepper`：端口、超时、间隔、候选数；
  - `Picker`：离散小时/天数选项；
  - 测试真实 `value`/`change`/`confirm` 事件，不使用桌面端 API。

- [x] **S1-UI-07 capability 禁用和原因**
  - capability 不支持时控件 disabled，同时显示用户可理解的 reason 文案。
  - 测试 hotspot/USB experimental、IPv6 guard 和 TUN stack 的 unsupported/degraded 状态。

- [x] **S1-UI-08 active/draft/digest 状态机**
  - draft 编辑后摘要不得提前变成已生效值；成功 mutation/apply 后才更新 active。
  - 刷新、路由离开和 host event 到达时覆盖 dirty、冲突和重载策略。

- [x] **S1-UI-09 validate/apply/CAS 交互**
  - 使用 `config.validate` 产生 impact/disruption，再由 `Dialog` 二次确认 `config.apply`。
  - 覆盖 stale digest、字段错误、daemon 拒绝、网络断开和重复提交。
  - 失败必须显示 `OperationBanner`，且旧活动配置保持不变。

- [x] **S1-UI-10 主题本地偏好**
  - `system/light/dark` 只写 allowlisted `localStorage`，不调用 daemon。
  - 若未来重新批准 D19 的应用图标双模式，只能新增独立 local preference；本任务不实现主题 provider。

## 7. 阶段 S2：领域二级页面

### S2-UI 规则和集合

- [ ] **S2-UI-01 CIDR 列表编辑器**
  - `RED`：通用 textarea 无法定位重复、非法 CIDR 和跨列表 overlap。
  - `GREEN`：使用 `Cell + SwipeCell + Popup + Input` 实现增删改、规范化预览、有界数量。
  - `VERIFY`：daemon path/index 错误能定位到具体条目；保存走 validate/apply 和 expected digest。

- [ ] **S2-UI-02 域名规则编辑器**
  - 覆盖后缀规范化、大小写、前导点、重复和 force/bypass/block 冲突。
  - `Textarea` 只作为批量导入/专家入口，不能成为最终列表状态。

- [ ] **S2-UI-03 接口 include/exclude 编辑器**
  - 使用 `CheckboxGroup` 选择已发现接口，支持 include/exclude 互斥提示和 capability 缺失状态。
  - 不允许用户提交任意路径或任意网卡名绕过 daemon 校验。

- [ ] **S2-UI-04 Wi-Fi 场景规则编辑器**
  - 使用 `Input` 编辑 SSID/BSSID，使用 `RadioGroup` 选择 enable/disable proxy，列表用 `SwipeCell`。
  - 测试敏感值脱敏、最多 64 条、规则冲突、排序、当前匹配和返回键草稿保护。

- [ ] **S2-UI-05 资源候选编辑器**
  - 结构化编辑 mark、mask、route table、priority，禁止用字符串数组模拟对象。
  - `RED` 先证明逐候选 probe DTO 缺失；只有新增有界 DTO 后才实现逐候选状态。

- [ ] **S2-UI-06 领域列表通用组件**
  - 抽取稳定 key、编辑草稿、删除确认、异步取消、空状态和错误映射，避免 CIDR/域名/Wi-Fi 重复实现。
  - `SwipeCell` 展开状态不能在刷新后错误复用到其他条目。

## 8. 阶段 S3：新增后端契约后再开放的能力

### S3-BE 契约任务

- [ ] **S3-BE-01 `environment.get` 只读 DTO**
  - `RED`：证明当前 hello/status DTO 无法完整提供 daemon/module/Companion/core 版本和 Root capability。
  - `GREEN`：新增严格枚举、版本字符串、协议/schema 版本和 reason code。
  - 禁止返回 Root 路径、命令、环境变量、token 或安装应用清单。
  - WebUI/Companion 增加正反向 DTO fixtures 和兼容失败测试。

- [ ] **S3-BE-02 `config.reset_preview/apply`**
  - 默认值必须由 daemon 当前 schema 生成；WebUI 不得内置第二份默认配置。
  - 覆盖保留订阅源、应用名单、节点 intent、UI 偏好和 generation 的边界。
  - stale digest、部分 reset、apply 失败均不得改变活动配置。

- [ ] **S3-BE-03 `storage.summary/clear`**
  - 只允许固定类别和有界清理，不接受自由路径。
  - 测试 last-known-good、活动 generation、规则集和 WebUI cache 的保护规则。
  - 清理结果包含字节数、更新时间、影响摘要和稳定 reason code。

- [ ] **S3-BE-04 跨宿主备份文件能力**
  - Companion 使用 SAF；KernelSU/APatch 使用受控宿主能力；WebUI 仍只处理私有 payload。
  - 覆盖大小上限、schema 兼容预览、expected digest、导入失败回滚和离线使用。
  - 在契约完成前不引入 TDesign `Upload`。

- [ ] **S3-BE-05 核心 staged update**
  - 固定发行源、签名/摘要、架构 admission、staged install、健康检查、回滚和进度事件。
  - 设置页在契约完成前只显示版本检查状态，不显示“更新”按钮。

### S3-UI 新增能力消费

- [ ] **S3-UI-01 环境横幅和 About 页面**
  - 只显示 DTO 已证明的信息；unsupported/unavailable 显示未知和原因。
  - 外链、许可和版本页面离线可用，不加载远程字体或帮助内容。

- [ ] **S3-UI-02 恢复默认和存储操作**
  - 使用 `Dialog` 展示影响摘要并二次确认；重复提交被禁用；成功/失败/冲突进入 `OperationBanner`。

- [ ] **S3-UI-03 备份文件交互**
  - 文件名、大小、生成时间、schema 兼容性和变更摘要可见；不在日志或 URL 中暴露私有 payload。

## 9. 阶段 S4：回归、质量和真机验收

- [ ] **S4-REG-01 WebUI 现有行为回归**
  - `npm run typecheck`；
  - `npm run test:unit`；
  - `npm run test:browser`；
  - `npm run test:e2e`；
  - `npm run build`、imports/dependencies/bundle/security gates。

- [ ] **S4-REG-02 Rust/CLI 回归**
  - `cargo fmt --all -- --check`；
  - `cargo test -p nethop-protocol --tests`；
  - `cargo test -p nethopctl --tests`；
  - `cargo test -p nethopd --tests --features subscription-update`；
  - 相关 clippy 和配置契约测试。

- [ ] **S4-REG-03 Companion 回归**
  - JVM tests、lint、debug assemble、Android instrumentation compile；
  - WebView host 状态加载、离线资源、返回键、磁贴跳转和 Root bridge allowlist 保持正常。

- [ ] **S4-REG-04 视觉和响应式**
  - viewport：`360x640`、`393x873`、`412x915`、`600x960`；
  - 明暗主题无重叠、横向滚动、截断和不可点击控件；
  - Cell、Popup、Dialog、Stepper、Picker、SwipeCell 在 Android WebView 中布局稳定。

- [ ] **S4-REG-05 安全与离线 gate**
  - 不请求远程字体、图标、许可和帮助内容；
  - 不出现自由 shell、自由路径、固定 secret 或敏感配置回显；
  - schema、protocol、allowlist、CSP、bundle 和模块 manifest 互相一致。

- [ ] **S4-DEVICE-01 真机手工验收**
  - Android 13+ arm64，Companion、KernelSU/APatch WebUI 分别验证；
  - Root 可用、不可用、能力 degraded、daemon 未启动、网络断开、配置冲突和恢复重试；
  - 返回键关闭浮层、保护草稿、应用配置后首页摘要实时更新；
  - 不把一次真机通过当作协议或单元测试替代品。

## 10. 阶段依赖与并行关系

```text
S0-BASE + S0-BE
      |
      +--> S1-UI 首页/标量/事务
      |          |
      |          +--> S2-UI 领域二级页
      |
      +--> S3-BE 新契约 --> S3-UI 新能力

S1 + S2 + S3
      |
      +--> S4 回归、构建、真机
```

允许并行：

- S1-UI-02、S1-UI-03、S1-UI-06 可在 S0-BE-02 完成后并行；
- S2-UI-01、S2-UI-02、S2-UI-03 可共享领域列表基础设施并行开发；
- S3-BE-01、S3-BE-03、S3-BE-04 可在协议负责人确认 DTO 边界后并行；
- S4 视觉测试可在 S1 首页 GREEN 后提前建立，但只能在全部阶段完成后标记通过。

## 11. 完成定义

本任务清单全部完成必须满足：

1. S0 基线和 schema/validator 契约通过；
2. 设置首页不显示 raw field ID、内部风险枚举或未实现占位项；
3. 所有已开放标量控件遵守 schema、capability、CAS 和失败回滚；
4. CIDR、域名、接口、Wi-Fi 和资源候选不再使用通用 textarea 承载最终状态；
5. 概览、订阅、应用、节点和设置不存在重复编辑入口；
6. 所有新增 daemon 能力都有 Rust、CLI、WebUI、Companion（适用时）契约测试；
7. WebUI unit/browser/e2e、Rust、CLI、Companion、构建安全 gate 全部通过；
8. Android 真机通过状态加载、返回键、离线、能力失败和配置事务验收；
9. 生成证据不包含敏感数据，且文档、schema、protocol、allowlist 和实现保持同步。

任务完成不等于 Git 提交或推送完成；提交和推送必须由用户另行明确授权。
