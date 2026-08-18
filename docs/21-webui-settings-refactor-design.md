# NetHop WebUI 设置界面重构设计

> 状态：设计基线（已补充配置准入矩阵与 TDesign Mobile Vue 组件映射）
>
> 日期：2026-08-18
>
> 适用范围：NetHop Alpha、Android 13+ arm64 Root 设备、NetHop Companion WebView、KernelSU/APatch Module WebUI
>
> 参考原型：[`../refer/proxy_settings.html`](../refer/proxy_settings.html)
>
> 上位文档：[`06-configuration-toml-refactor-design.md`](./06-configuration-toml-refactor-design.md)、[`08-webui-design.md`](./08-webui-design.md)
>
> 当前实现基线：配置 schema v3、control protocol v6、`config.schema`、`config.mutate`、`config.validate/apply`、`capability.get`

## 1. 决策摘要

设置首页重构为“分类、当前摘要和二级页面入口”，不再直接把 daemon schema 按原始字段名完整铺在一级页面。

本次设计遵循以下原则：

1. `nethopd` 和 `config.schema` 仍是代理配置的唯一事实源。
2. 已经在概览、订阅、应用和节点页面拥有领域交互的配置，不在设置页增加第二个编辑入口。
3. 常用标量设置使用明确中文语义和适合的数据控件；集合、敏感对象和高风险参数进入专用二级页面。
4. 没有后端契约的项目不得先放一个不生效的开关或菜单项。
5. 设备不支持的能力显示明确状态和原因，不伪造可用值。
6. UI 偏好、daemon 配置、只读状态和破坏性操作必须使用不同的保存与反馈路径。

目标信息架构：

```text
设置
├─ 运行环境与代理核心（只读摘要）
├─ 更新与自动选择
├─ 网络接管
├─ 路由策略
├─ 日志
├─ 外观
├─ 数据与诊断
├─ 高级设置
└─ 关于
```

当前 `SettingsView` 的 schema-driven 编辑能力不删除，但应收口为“高级配置”页面的基础设施。设置首页不显示原始 field ID、apply impact 内部枚举或开发者风险标记。

## 2. 范围与状态定义

### 2.1 本文覆盖

- 设置首页需要展示的只读状态、配置摘要和操作入口；
- 常用配置的专用控件和保存语义；
- 需要保留的高级配置；
- 已有前后端实现及其可复用边界；
- 需要展示但前端、后端或双方尚未完成的项目；
- 参考原型中不适合 NetHop 的项目。

### 2.2 本文不覆盖

- 本轮直接修改 WebUI、daemon、CLI 或 Companion 代码；
- 为缺失项目临时增加占位按钮；
- 修改配置 schema、control protocol 或模块安装流程；
- 兼容开发期旧设置页面 DOM、路由或视觉快照。

### 2.3 实现状态

| 状态 | 定义 |
|---|---|
| 完整 | 事实源、读写契约和可消费的前端路径均已存在；本次主要重做信息架构和视觉交互 |
| 部分 | 后端或前端已有主要能力，但缺少专用 UI、结构化 DTO、验证反馈或完整操作闭环 |
| 缺失 | 前端和后端均没有满足目标语义的实现，必须先设计并测试新契约 |
| UI 本地 | 该项目有意只属于 WebUI 本地偏好，不需要 daemon 字段 |
| 排除 | 不应成为用户配置，不能以“待实现”名义加入设置页 |

“前后端已有相关实现”不等于当前设置页体验已经完成。当前通用 schema 表单能修改多数标量，但仍需要专用中文文案、摘要、控件和二级页面。

## 3. 当前实现基线

### 3.1 后端配置事实源

默认配置位于 `module/defaults/nethop.toml`，当前顶层域为：

```text
service
subscriptions
proxy
applications
network
routing
logging
advanced
```

daemon 已提供：

- `config.get`：读取活动配置和 digest；
- `config.schema`：提供字段类型、分组、枚举、范围、风险、生效影响和 capability key；
- `config.mutate`：对受控字段执行携带 expected digest 的 typed mutation；
- `config.validate/apply`：事务化验证并发布完整草稿；
- `config.reload`：重新读取外部配置并保留失败前活动状态；
- `capability.get`：返回设备能力、冲突和不可用原因。

### 3.2 当前前端

`webui/src/views/SettingsView.vue` 当前会：

1. 并行读取 `config.get`、`config.schema` 和 `capability.get`；
2. 过滤数组元素路径和敏感字段；
3. 按 schema group 生成折叠面板；
4. 使用 `SchemaField` 渲染 bool、enum、数字、字符串和简单数组；
5. 通过完整草稿执行 validate/apply；
6. 显示配置冲突、apply impact 和失败保留旧配置的结果。

现状的主要问题不是“不能修改配置”，而是：

- 一级页面直接暴露原始字段 ID；
- 常用项、高级项和实验项缺少清晰层级；
- 对象数组和领域集合无法用通用 textarea 正确表达；
- 运行状态、版本、数据操作和关于信息散落在“运维”页面；
- 已由其他一级页面管理的字段仍可能在设置页重复出现。

当前还存在一个必须在重构前修正的 schema 元数据不一致：`config_model` 和 `MAX_AUTO_CANDIDATES` 将 `proxy.urltest.max_candidates` 限制为 `1..64`，但 `config.schema` 当前错误声明为 `1..256`。前端如果信任该元数据，会允许用户输入后端必然拒绝的值。修正该元数据不改变持久配置 ABI，但必须增加 schema 与 validator 同源边界测试。

## 4. 单一编辑入口

以下字段属于配置，但已有更合适的领域页面。设置首页可以显示摘要或跳转，不能增加第二套编辑控件。

| 配置 | 唯一编辑入口 | 设置页处理 |
|---|---|---|
| `service.enabled` | 概览代理开关、快捷设置磁贴 | 只显示当前状态，不增加“开机自启动”开关 |
| `proxy.outbound_mode` | 概览的规则/全局/直连模式 | 显示摘要并跳转概览 |
| `network.capture_mode` | 概览的自动/TPROXY/TUN 模式 | 显示摘要并跳转概览 |
| `subscriptions.mode`、`subscriptions.sources` | 订阅页面 | 不进入通用设置表单 |
| `applications.mode`、`applications.targets` | 应用页面 | 不进入通用设置表单 |
| 节点 `auto/manual` intent | 节点页面 | 不写入 TOML，不在设置页编辑 |

`service.enabled` 同时表达当前持久代理意图和下次启动恢复语义。项目不得新增独立 `auto_start`，否则会产生两个互相冲突的事实源。

## 5. 需要显示且已有相关实现的项目

本节项目已经具备正确的事实源和主要读写链路，可以优先完成设置 UI 重构。

### 5.1 运行环境与代理核心

| 显示项目 | 当前事实源 | 后端现状 | 前端现状 | 目标呈现 | 状态 |
|---|---|---|---|---|---|
| 代理运行状态 | `status.get`、事件 snapshot | 已实现配置意图、有效意图、runtime state 和诊断码 | AppShell/概览已消费 | 设置页顶部只读状态横幅 | 完整 |
| Root 能力状态 | `capability.get` 的 `root` item | 已实现 supported/unsupported/unavailable 和 reason code | Settings 已读取 capability | 横幅显示“Root 可用/不可用”，不猜测具体 Root 管理器 | 完整 |
| 当前接管模式 | 活动配置与 topology | 已实现 | 概览、运维已显示 | 状态横幅或网络摘要 | 完整 |
| sing-box 当前版本 | `core.version_check` | 已实现当前版本和版本检查状态 | 运维页面已查询 | “代理核心”只读行，存在新版时显示状态点 | 完整 |
| 配置兼容状态 | `protocol.hello` | 已实现协议和 schema 兼容判定 | AppShell 已消费 | 仅异常时展示，不占用正常页面空间 | 完整 |

第一阶段不得显示“Magisk”“KernelSU”或“APatch”具体名称，因为当前 capability DTO 的 `root_backend` 仍为 `null`。仅显示后端能够证明的 Root 能力状态。

### 5.2 更新与自动选择

| 设置项 | 配置字段 | 控件 | 默认值 | 生效影响 | 当前前端 | 当前后端 |
|---|---|---|---:|---|---|---|
| 订阅自动更新 | `subscriptions.auto_update` | Switch | `true` | runtime only | 通用 schema bool | 完整 |
| 订阅更新间隔 | `subscriptions.update_interval_hours` | Picker/Stepper，1..168 小时 | `24` | runtime only | 通用 schema int | 完整 |
| 自动测速间隔 | `proxy.urltest.interval_minutes` | Picker/Stepper，5..1440 分钟 | `10` | generation activation | 通用 schema int | 完整 |
| 节点切换容差 | `proxy.urltest.tolerance_ms` | Stepper，0..1000 ms | `50` | generation activation | 通用 schema int | 完整 |
| 最大自动候选数 | `proxy.urltest.max_candidates` | Stepper，1..64；默认放高级 | `64` | generation activation | 通用 schema int，但当前错误显示上限 256 | validator 完整，schema 元数据待修正 |

订阅自动更新属于全局调度设置，设置首页可显示摘要；订阅源本身仍由订阅页面管理。

`proxy.urltest.max_candidates` 在 schema 上限修正并通过契约测试前按“部分”处理，不能直接进入生产专用控件。

### 5.3 网络接管

| 设置项 | 配置字段 | 目标控件/文案 | 当前实现 | 状态 |
|---|---|---|---|---|
| TCP 代理 | `network.proxy_tcp` | Switch | schema、验证、应用和通用前端均存在 | 完整 |
| UDP 代理 | `network.proxy_udp` | Switch | schema、验证、应用和通用前端均存在 | 完整 |
| IPv6 策略 | `network.ipv6_mode` | 自动/代理/阻断三态选择 | capability admission、enum 和前端 dropdown 已存在 | 完整 |
| DNS 模式 | `network.dns_mode` | 自动/代理 DNS/系统 DNS 三态选择 | enum、配置生成和前端 dropdown 已存在 | 完整 |
| 移动网络 | `network.interfaces.mobile` | Switch | schema、接口计划和 capability 已存在 | 完整 |
| Wi-Fi | `network.interfaces.wifi` | Switch | schema、接口计划和 capability 已存在 | 完整 |
| 热点代理 | `network.interfaces.hotspot` | Switch + 实验状态 | 复合 NetworkPlan 已实现，schema 标记 experimental | 完整但实验 |
| USB 网络共享 | `network.interfaces.usb` | Switch + 实验状态 | 复合 NetworkPlan 已实现，schema 标记 experimental | 完整但实验 |
| Wi-Fi 场景总开关 | `network.wifi_scenes.enabled` | Switch | 配置和 matcher 已实现，通用 bool 可编辑 | 完整 |
| Wi-Fi 探测间隔 | `network.wifi_scenes.probe_interval_seconds` | 15..3600 秒，放二级页 | 后端与通用数字输入已存在 | 完整 |
| TUN 栈 | `network.tun_stack` | system/gVisor，放高级页并显示风险 | 后端、capability 和通用 enum 已存在 | 完整但高级 |

IPv6 不是布尔能力。设置页不得把 `auto/proxy/block` 压缩成“IPv6 支持”开关。

DNS 当前也不是可编辑 DoH 服务器列表。设置摘要只能显示“自动”“代理 DNS”或“系统 DNS”，不得显示原型中的“远程 DoH”。

### 5.4 路由策略

| 设置项 | 配置字段 | 目标控件 | 当前实现 | 状态 |
|---|---|---|---|---|
| 绕过私有网络 | `routing.bypass_private` | Switch | 后端、schema、通用前端均存在 | 完整 |
| 绕过中国大陆规则 | `routing.bypass_cn` | Switch + 实验状态 | 规则集供应链和配置生成已存在 | 完整但实验 |
| 阻断 QUIC | `routing.block_quic` | Switch + 实验状态 | 路由生成和 schema 已存在 | 完整但实验 |
| 强制代理 CIDR | `routing.force_proxy_cidrs` | 专用 CIDR 列表页 | 后端结构化验证完整；当前仅有通用数组输入 | 部分 |
| 绕过 CIDR | `routing.bypass_cidrs` | 专用 CIDR 列表页 | 后端结构化验证完整；当前仅有通用数组输入 | 部分 |
| 强制代理域名 | `routing.force_proxy_domains` | 专用域名规则页 | 后端规范化和冲突检测完整；当前仅有通用数组输入 | 部分 |
| 绕过域名 | `routing.bypass_domains` | 专用域名规则页 | 后端规范化和冲突检测完整；当前仅有通用数组输入 | 部分 |
| 阻断域名 | `routing.block_domains` | 专用域名规则页 | 后端规范化和冲突检测完整；当前仅有通用数组输入 | 部分 |

前三个标量可以在设置二级页直接操作。CIDR 和域名规则必须复用 daemon 校验，但需要新的前端领域编辑器，不能继续依赖多行字符串输入作为最终体验。

### 5.5 日志

| 设置/操作 | 字段或 API | 当前实现 | 目标位置 | 状态 |
|---|---|---|---|---|
| 日志级别 | `logging.level` | error/warn/info/debug/trace 全链路已实现 | 设置 > 日志 | 完整 |
| 日志保留天数 | `logging.retention_days` | 1..30 天全链路已实现 | 设置 > 日志 | 完整 |
| 查看日志 | `logs.get` | 运维页面已有结构化/原始、有界列表 | 设置页跳转运维日志 | 完整 |
| 清除日志 | `logs.clear` | daemon、CLI、Companion allowlist、确认弹窗已实现 | 日志二级页或跳转运维 | 完整 |

设置页不复制一套日志查看器，只提供摘要和跳转。

### 5.6 外观

| 设置项 | 保存位置 | 当前实现 | 状态 |
|---|---|---|---|
| 界面主题 | allowlisted `localStorage`：system/light/dark | `useTheme`、设置 dropdown 和系统主题跟随均已实现 | UI 本地 |

界面主题不进入 `nethop.toml`，不触发 daemon mutation，也不需要保存按钮。

### 5.7 数据与诊断

| 操作 | 当前后端 | 当前前端 | 目标位置 | 状态 |
|---|---|---|---|---|
| 导出配置备份 | `backup.export` 私有 payload/CLI 管道 | 运维备份 Tab 已实现 | 设置 > 数据与诊断 | 完整 |
| 验证并恢复备份 | `backup-restore` + expected digest | 运维备份 Tab 已实现粘贴和恢复 | 设置 > 数据与诊断 | 完整 |
| 生成脱敏诊断包 | `diagnostics.bundle` | 运维系统 Tab 已实现 | 设置 > 数据与诊断 | 完整 |
| 更新规则集 | `ruleset.update/status` | 运维系统 Tab 已实现 | 路由设置摘要或运维跳转 | 完整 |

第一阶段重构应复用现有操作，不重新实现 daemon 命令。备份和诊断可以迁移视觉入口，但必须保持私有 payload、大小上限、expected digest 和错误反馈不变。

### 5.8 高级配置

| 设置项 | 配置字段 | 当前实现 | 目标呈现 | 状态 |
|---|---|---|---|---|
| 入站端口 | `advanced.inbound_port` | 端口冲突 admission 和 schema 已实现 | 数字输入 + 当前 capability | 完整 |
| bypass mark | `advanced.bypass_mark` | 结构化校验已实现 | 数字/十六进制输入 + 风险说明 | 完整 |
| IPv6 guard | `advanced.ipv6_guard` | capability 约束和网络计划已实现 | Switch，默认保持开启 | 完整 |
| dry-run | `advanced.dry_run` | 后端计划验证已实现 | Switch | 完整 |
| 健康检查超时 | `advanced.health_timeout_seconds` | 1..30 秒已实现 | Stepper | 完整 |
| reconcile 间隔 | `advanced.reconcile_interval_seconds` | 60..3600 秒已实现 | Stepper | 完整 |
| 资源候选 | `advanced.resource_candidates` | 后端对象模型、校验和 capability probe 已实现 | 专用候选列表编辑器和探测结果 | 部分 |

高级设置默认折叠。修改前展示 `apply_impact`、风险等级和 capability；修改后必须先 validate，再确认 apply。

## 6. 需要显示但实现不完整或不存在的项目

本节不能先做静态 UI。每个项目必须补齐列出的契约后才进入生产设置页。

### 6.1 运行环境身份与完整版本信息

目标横幅需要显示：

```text
Root 管理环境 · 模块运行状态
Companion 版本 · nethopd 版本 · sing-box 版本
```

当前缺口：

| 层 | 现状 | 缺口 |
|---|---|---|
| daemon capability | 只能证明 Root supported/unsupported，`root_backend` 固定为 null | 缺少经过验证的 Magisk/KernelSU/APatch identity |
| protocol hello | 返回兼容范围，但 WebUI DTO 丢弃 manager/schema 版本详情 | 缺少 daemon/module 构建版本字段和前端保留 |
| Companion | APK 自身有版本，但 WebView host API 未提供只读版本 DTO | 缺少受控 host identity API |
| WebUI | AppShell 有兼容状态，运维能读取 sing-box 版本 | 缺少聚合后的设置状态横幅 |

建议新增只读 `environment.get`，或在不破坏职责的前提下扩展 hello/status。DTO 只返回枚举和版本，不返回 Root 路径、命令或敏感环境变量。

状态：**部分，后端与前端均需扩展。**

### 6.2 Wi-Fi 场景规则编辑器

后端已经支持最多 64 条 SSID/BSSID 场景规则，但 `network.wifi_scenes.rules` 标记为 sensitive，当前 Settings 会过滤，前端没有专用编辑器。

需要新增：

- 新建、编辑、删除和排序规则的领域 UI；
- SSID/BSSID 的敏感输入和显示脱敏；
- enable_proxy/disable_proxy 动作选择；
- 规则冲突、当前匹配和瞬态 override 状态；
- expected digest、保存失败回滚和返回键草稿保护测试。

状态：**后端完整，前端缺失。**

### 6.3 自定义接口、CIDR 和域名规则编辑器

后端已经具备有界解析、规范化、排序、去重和冲突拒绝。当前通用数组 textarea 只能作为开发期工具，不能成为最终设置体验。

需要新增：

- `network.interfaces.include/exclude` 的受控接口模式编辑器；
- CIDR 单项校验、规范化预览和重复提示；
- 域名后缀单项校验和跨动作冲突提示；
- 大列表虚拟化或有界分页；
- daemon validation error 到具体条目的映射。

状态：**后端完整，前端专用实现缺失。**

### 6.4 高级资源候选编辑与探测结果

`advanced.resource_candidates` 是对象数组，当前 `SchemaField` 的字符串数组模型无法安全表达。后端能够校验候选并报告总体 resource capability，但前端缺少：

- mark、mask、route table、priority 的结构化表单；
- 候选排序和重复检查；
- 每个候选的 probe 结果与冲突原因；
- 恢复内置候选的局部操作。

如果要显示逐候选 probe，后端 capability DTO 也需要从总体状态扩展为有界候选结果。

状态：**配置后端完整，前端缺失；逐候选诊断后端缺失。**

### 6.5 恢复默认设置

参考原型包含“恢复默认设置”，当前项目没有对应 control method 或 WebUI 操作。

正确实现必须：

1. 由 daemon 生成当前 schema 的冻结默认文档，不能由 WebUI 内置第二份默认值；
2. 接收 expected digest，先 validate 再 apply；
3. 明确保留或清除订阅源、应用名单、节点 intent、UI 偏好和 daemon-owned state 的边界；
4. 显示影响摘要并二次确认；
5. 失败时保持活动配置、generation 和 source registry 不变。

建议新增 `config.reset_preview` 和 `config.reset_apply`，或等价的 typed reset mutation。

状态：**前后端均缺失。**

### 6.6 缓存与存储管理

项目已有订阅 last-known-good、规则集、WebUI 静态资源和少量运行状态文件，但没有统一缓存 inventory 或安全清理 API。

不得直接实现一个“清除全部缓存”按钮。需要先定义：

- 可清理类别和不可清理的容灾状态；
- 各类别占用字节数、更新时间和清理影响；
- 清理时是否需要停止调度或持有 mutation lock；
- 清理后如何保持活动 generation 和离线可用性；
- Companion/WebUI 缓存与 Root 模块缓存的职责边界。

建议未来提供 `storage.summary` 和按固定枚举执行的 `storage.clear`，禁止自由路径。

状态：**前后端均缺失。**

### 6.7 完整备份文件交互

当前后端能导出固定私有路径，前端能粘贴 JSON 恢复，但尚未形成普通用户完整的文件交互：

- Companion 使用 Android Storage Access Framework 导出/导入；
- KernelSU/APatch 宿主使用受控下载能力或明确私有路径结果；
- WebUI 显示文件名、大小、生成时间和导入预览；
- 导入前展示 schema 兼容性和变更摘要。

状态：**后端事务完整，跨宿主文件 UX 部分缺失。**

### 6.8 代理核心更新执行

当前只有版本检查和更新状态，没有从设置页安全安装新 sing-box 核心的操作闭环。若“代理核心”行提供进入或更新操作，必须先完成：

- 固定发行源和签名/摘要验证；
- 下载大小、架构和版本 admission；
- staged install、健康检查和失败回滚；
- 与模块升级的所有权边界；
- 进度事件和重启影响说明。

在此之前只显示当前版本和“有新版”状态，不显示“更新”按钮。

状态：**版本检查完整，更新执行前后端均缺失。**

### 6.9 语言

当前 WebUI 文案为简体中文，没有 i18n catalog、locale negotiation、日期数字格式层或语言偏好存储。

语言设置只有在至少第二种语言完整覆盖、具备 fallback 和自动化检查后才显示。该偏好应属于 WebUI 本地或 Companion locale，不进入代理 TOML。

状态：**前端缺失；后端按设计不需要。**

### 6.10 关于、许可与反馈

当前模块 `licenses/` 没有可直接展示的完整许可索引，WebUI 也没有 About 二级页。需要：

- 构建时生成依赖许可索引；
- Companion、WebUI、Rust 和 sing-box 许可归属；
- 固定 GitHub 仓库与 issue URL；
- 离线可读的版本和许可页面；
- 外链打开失败的明确反馈。

这不是 daemon 配置，不需要可变后端字段；版本信息复用 6.1 的只读身份 DTO，许可资产由构建系统生成。

状态：**前端与构建产物缺失，运行后端不需要。**

## 7. 明确排除的原型项目

以下项目不是待办，不得进入设置界面。

### 7.1 独立“开机自启动”

`service.enabled` 已是唯一持久代理开关。daemon 控制面随模块启动保持可用，数据面是否恢复由该字段决定。增加第二个开关会造成“当前开启但开机关闭”等双状态冲突。

### 7.2 “断线自动重连”开关

核心退出、TUN 漂移、网络计划 reconcile、退避和熔断属于运行时安全机制，不是用户偏好。高级 `reconcile_interval_seconds` 不能包装成可关闭的自动重连。

### 7.3 布尔“IPv6 支持”

必须保留 `auto/proxy/block` 三态和 capability admission，不能用开关丢失 fail-closed 语义。

### 7.4 “远程 DoH”

当前 `network.dns_mode` 只表达 auto/proxy/system。项目没有用户自定义 DoH URL、证书策略或 resolver 列表，不显示原型中的假摘要。

### 7.5 泛化“局域网设备接入”

当前只支持经过 capability 识别的热点和 USB tether 接口，不开放 `0.0.0.0` 监听或任意 LAN 客户端。UI 必须使用“热点代理”“USB 网络共享”的精确名称。

### 7.6 通用性能模式、GMS 修复和任意 shell 参数

这些项目没有冻结、可逆、可验证的行为，不进入首版设置。安全不变量仍禁止自由命令、自由路径和关闭 TLS/健康检查。

## 8. 目标页面结构

### 8.1 设置首页

首页只展示摘要，不承载长表单：

| 区域 | 行 | 摘要示例 | 行为 |
|---|---|---|---|
| 状态 | 运行环境 | `Root 可用 · 模块运行中` | 只读；异常时可进入诊断 |
| 核心 | 代理核心 | `sing-box 1.13.15` | 进入版本详情 |
| 更新 | 订阅自动更新 | `开启 · 每 24 小时` | 进入更新设置 |
| 更新 | 自动选择 | `10 分钟 · 50 ms 容差` | 进入自动选择设置 |
| 网络 | DNS | `自动` | 进入网络设置 |
| 网络 | IPv6 | `自动` | 进入网络设置 |
| 网络 | 接口范围 | `移动网络、Wi-Fi` | 进入接口设置 |
| 路由 | 路由策略 | `绕过私网 · 中国大陆规则` | 进入路由设置 |
| 日志 | 日志 | `Info · 保留 7 天` | 进入日志设置 |
| 外观 | 界面主题 | `跟随系统` | 首页内分段控件或下拉 |
| 数据 | 备份与诊断 | `备份、恢复、诊断包` | 进入数据页 |
| 高级 | 高级设置 | `默认` 或 `已修改 N 项` | 进入高级页 |
| 关于 | 关于 NetHop | 版本摘要 | 进入关于页 |

状态横幅不能写死 `Magisk`、版本号或 Root 已授权。所有值必须来自宿主和 daemon 的已验证 DTO。

### 8.2 建议路由

```text
#/settings
#/settings/updates
#/settings/network
#/settings/interfaces
#/settings/routing
#/settings/routing/cidrs
#/settings/routing/domains
#/settings/logging
#/settings/data
#/settings/advanced
#/settings/about
```

Wi-Fi 场景和资源候选在专用实现完成前不注册生产路由。

## 9. 控件与保存语义

### 9.1 简单标量

bool、enum 和有界数字使用 typed `config.mutate`：

```text
读取 active config + digest
  -> 用户修改单个字段
  -> config.mutate(expected digest, field id, typed value)
  -> daemon 校验并事务化发布
  -> 更新 active digest
  -> 成功/失败 OperationBanner
```

失败后恢复最近一次活动值，不让草稿伪装成已生效状态。

### 9.2 集合和高级配置

CIDR、域名、Wi-Fi 场景和资源候选使用草稿流程：

```text
编辑草稿
  -> config.validate
  -> 展示 apply impact、disruption、逐项错误
  -> 用户确认
  -> config.apply(expected digest)
```

### 9.3 UI 本地偏好

主题和未来语言偏好立即写入 allowlisted storage，不调用 daemon，不显示“应用配置”按钮。

### 9.4 只读状态

运行环境、版本、capability 和当前拓扑来自 query/event state，不写入配置草稿。状态读取失败显示“未知”和原因，不回退硬编码值。

### 9.5 破坏性操作

清除日志、恢复备份、未来恢复默认和未来清理缓存必须：

- 使用明确对象和影响描述；
- 二次确认；
- 禁止并发重复提交；
- 返回成功、失败或冲突；
- 失败保持活动配置和运行状态。

## 10. 前端重构边界

### 10.1 保留

- `validatedQuery`、DTO bounds 和 strict parser；
- `uiStores.config` 的 active/draft/digest 分离；
- `OperationBanner`；
- `ConfirmDialog`；
- `OptionDropdown`、`SegmentedControl`、`TSwitch`；
- `config.schema` 的类型、范围、capability、risk 和 apply impact；
- 完整草稿 validate/apply 和 conflict 处理。

### 10.2 新增

- 设置首页 row/group 组件；
- field ID 到用户文案、摘要和值标签的受控映射；
- 设置二级路由和返回键草稿保护；
- CIDR、域名、Wi-Fi 场景和资源候选领域组件；
- 环境与版本 DTO 完成后新增状态横幅和 About 页面。

### 10.3 移除或迁移

- 设置首页原始 schema group 折叠列表；
- 以 `field.id` 直接作为可见标签；
- 在一级页面直接显示内部 `applyImpact · riskLevel`；
- 对对象数组使用通用 textarea；
- 与概览、订阅和应用页面重复的编辑入口。

通用 schema 编辑器迁移到高级页后仍必须 capability-aware，不能退化为无约束 TOML 文本编辑器。

## 11. 后端新增契约清单

只有本表项目需要新增后端工作；其余设置优先复用现有 schema 和操作。

| 优先级 | 契约 | 目的 | 边界 |
|---|---|---|---|
| P0 | 修正 `max_candidates` schema 上限 | 让 UI 元数据与 validator 的 64 上限一致 | 不改变配置 ABI，增加同源边界测试 |
| P1 | 环境/版本只读 DTO | Root backend、模块、daemon、Companion 版本 | 不返回路径、命令、环境变量 |
| P1 | Wi-Fi 场景 typed mutation 或领域 API | 安全编辑敏感规则 | 日志和响应脱敏、expected digest |
| P1 | validation 逐项错误映射 | CIDR/域名/对象数组定位错误 | 有界 path/index，不回显 secret |
| P2 | `config.reset_preview/apply` | 事务化恢复默认 | daemon 生成默认值、失败不改变活动状态 |
| P2 | `storage.summary/clear` | 有界缓存和存储管理 | 固定类别、禁止自由路径、保护 last-known-good |
| P2 | resource candidate probe detail | 显示逐候选冲突 | 有界候选、稳定 reason code |
| P2 | 跨宿主备份文件能力 | 普通文件导入导出 | 私有 payload、大小上限、SAF/宿主能力 |
| P3 | core staged update | 安全核心更新 | 签名、架构、健康检查、原子回滚 |

优先级仅表示设置重构依赖顺序，不表示已经授权实施这些协议变更。

## 12. 测试与验收要求

### 12.1 重构前测试

必须冻结以下现有行为：

- Settings 能读取 config/schema/capability；
- 标量字段遵守 schema 类型和范围；
- capability 不支持时控件禁用并显示原因；
- validate/apply 使用 expected digest；
- 冲突不会覆盖外部新配置；
- apply 失败保持旧活动配置；
- 主题切换即时持久化；
- 运维中的日志、备份、诊断和版本检查正常。

### 12.2 重构后测试

至少覆盖：

1. 设置首页所有摘要来自活动配置或只读状态，不读取未提交草稿。
2. 首页不存在原始 field ID 和无后端能力的占位项。
3. 单个标量 mutation 成功后更新摘要，失败后恢复旧值。
4. Overview、Subscriptions、Applications 与 Settings 不出现同一配置的两个编辑控件。
5. IPv6 和 DNS 保持三态 enum，不退化为 bool。
6. hotspot/usb 和实验路由能力显示实验状态及 capability 原因。
7. 高级修改必须 validate、展示 impact 并确认 apply。
8. Android 返回键优先关闭 dropdown/dialog，再处理有修改的二级页。
9. 360x640、393x873、412x915、600x960 的明暗主题无重叠、截断和横向滚动。
10. 离线环境不请求远程字体、图标、许可或帮助内容。

### 12.3 缺失能力测试顺序

每个新增后端契约必须执行：

```text
before contract
  -> RED：证明 UI 或 API 尚不可用
  -> GREEN：实现严格 DTO、权限和有界输入
  -> REFACTOR：删除临时分支和重复事实源
  -> after regression
  -> 真机验证
```

## 13. 分阶段实施建议

### 阶段 S1：只使用现有能力重构首页

- 分类和摘要首页；
- 主题；
- 更新与自动选择；
- DNS、IPv6、TCP/UDP 和接口标量；
- 基础路由；
- 日志设置与运维跳转；
- 高级 schema 编辑入口；
- 当前可证明的 Root 状态和 sing-box 版本。
- 修正 `proxy.urltest.max_candidates` schema 上限并增加 validator 对照测试。

S1 不提升 control protocol 版本，也不改变持久配置 `schema_version`；允许修正 `config.schema` 的错误元数据。

### 阶段 S2：领域二级页面

- CIDR 和域名规则编辑器；
- 自定义接口模式；
- Wi-Fi 场景规则；
- 资源候选编辑器；
- 完整备份文件交互。

### 阶段 S3：新增系统能力

- 完整环境与版本 DTO；
- 恢复默认；
- 存储 inventory 和按类别清理；
- 核心 staged update；
- i18n 与语言设置。

任何阶段都不得通过静态占位项提前宣称后续能力已经存在。

## 14. 完成定义

设置界面重构完成需要同时满足：

1. 一级页面只呈现用户可理解的分类和活动摘要；
2. 所有可操作项都有真实、typed、可验证的事实源；
3. 缺失能力不出现在生产 UI；
4. 复杂集合和高级对象不再由错误的通用文本控件承载；
5. 配置编辑入口唯一，概览、订阅、应用和节点语义不重复；
6. UI 本地偏好、daemon 配置、只读状态和破坏性操作边界清晰；
7. capability、CAS 冲突、validate/apply 和失败回滚保持有效；
8. WebUI 回归、生产构建和 Android 真机验收通过。

## 15. 配置项目准入矩阵

设置页新增项目必须先回答三个问题：

1. 该项目的事实源属于 daemon 配置、只读运行状态还是 WebUI/Companion 本地偏好；
2. 是否已经存在 typed schema、validator、capability 和失败回滚语义；
3. 是否会与概览、订阅、应用、节点或磁贴形成第二个编辑入口。

没有明确答案的项目不得以静态开关、假摘要或“即将支持”菜单项进入生产 UI。建议按下面的准入等级实施：

| 优先级 | 建议项目 | 所属事实源 | 当前状态 | 进入设置页的条件 |
|---|---|---|---|---|
| P0 | 订阅自动更新、更新间隔 | daemon `subscriptions.*` | 已有完整契约 | 仅需专用摘要和控件，复用 `config.mutate` |
| P0 | 自动测速间隔、容差、候选上限 | daemon `proxy.urltest.*` | 已有契约；`max_candidates` 元数据需先修正为 `1..64` | schema/validator 同源测试通过 |
| P0 | TCP/UDP、IPv6、DNS、接口开关 | daemon `network.*` | 已有契约 | enum 保持三态，实验能力显示 capability 原因 |
| P0 | 私网/中国大陆/QUIC 路由开关 | daemon `routing.*` | 已有契约 | 不与概览模式重复，不包装成“性能模式” |
| P0 | 日志级别、保留天数 | daemon `logging.*` | 已有契约 | 设置只编辑标量，日志查看/清理由运维页承载 |
| P1 | CIDR、域名、接口 include/exclude | daemon `routing.*`、`network.interfaces.*` | 后端完整，前端领域编辑器缺失 | 有界列表、规范化、逐项错误和 CAS 草稿流程 |
| P1 | Wi-Fi 场景规则 | daemon `network.wifi_scenes.rules` | 后端完整，前端缺失 | 敏感字段脱敏、动作互斥、冲突与当前匹配状态可解释 |
| P1 | 资源候选 | daemon `advanced.resource_candidates` | 后端完整，逐候选 probe 缺失 | 结构化编辑器、候选排序、重复检查；probe 详情先补 DTO |
| P1 | 运行环境/版本摘要 | 只读 `environment.get` 或扩展 status/hello | 后端 DTO 不完整 | 只返回版本、枚举和 reason code，不返回路径/命令/环境变量 |
| P1（本地） | 应用图标来源 | WebUI local preference | 设计文档 19 当前记为主题方案取消 | 只有重新批准双模式需求后才加入；不得写入 TOML 或 protocol |
| P2 | 恢复默认设置 | daemon `config.reset_preview/apply` | 尚无契约 | daemon 生成默认值、expected digest、影响预览和失败不变 |
| P2 | 存储/缓存摘要与按类清理 | `storage.summary/clear` | 尚无契约 | 固定类别、保护 LKG/活动 generation，禁止自由路径 |
| P2 | 完整备份文件导入导出 | 宿主能力 + 私有 payload | 部分实现 | SAF/宿主能力、大小上限、schema 预览和回滚闭环 |
| P3 | sing-box 核心更新 | staged update API | 尚无安全安装契约 | 签名/摘要、架构 admission、健康检查和原子回滚全部具备 |

### 15.1 可以新增但不应立即加入的配置

以下项目有产品价值，但必须先补齐后端语义，不能只在前端增加一个开关：

- **恢复默认设置**：必须按配置域提供预览和选择性恢复，不能直接覆盖整个 TOML；
- **按类别清理缓存**：只能清理已定义的订阅缓存、规则集缓存或 WebUI 缓存，不能提供“清除全部”；
- **代理核心更新**：必须由 daemon/模块安装器拥有下载、校验、切换和回滚，设置页只作为入口；
- **完整环境身份**：只读显示 Root 能力、模块/daemon/Companion 版本和协议状态，不显示 Root 路径；
- **应用图标来源**：若恢复该需求，应作为 `localStorage` 偏好（`theme`/`original`），不进入 daemon 配置，也不改变应用代理语义。

### 15.2 明确不新增

- `auto_start`：与 `service.enabled` 重复；
- “断线自动重连”：属于运行时安全机制，不是用户偏好；
- 布尔“IPv6 支持”：会丢失 `auto/proxy/block` 三态；
- 远程 DoH URL：当前没有 resolver、证书和隐私契约；
- 通用“性能模式”“GMS 修复”“任意 shell 参数”：无法形成冻结、可逆、可验证的配置契约；
- 任意 LAN 监听、自由 Root 路径、自由命令：违反安全边界。

## 16. TDesign 组件选型与项目映射

### 16.1 选型依据

`refer/tdesign-common-develop` 作为设计语言和分类依据，负责组件命名、输入/导航/沟通类别、间距/颜色/字体等设计 token；运行时只使用项目已安装的 `tdesign-mobile-vue`。实现时必须以 `refer/tdesign-mobile-vue-develop` 的 Vue API 为准，特别是 `value`/`v-model`、`change`、`confirm` 和 `visible-change` 事件，不能照搬 TDesign Web API。

### 16.2 设置首页与二级入口

| 场景 | 组件 | 关键 API/行为 | 采用理由 |
|---|---|---|---|
| 分类标题和摘要分组 | `CellGroup` + `Cell` | `title`、`description`、`note`、`arrow`、`leftIcon/rightIcon`、`click` | Cell 原生支持“标题 + 摘要 + 右侧当前值/箭头”，适合移动设置列表 |
| 高级/实验分组 | `Collapse` + `CollapsePanel` | 受控展开值；默认折叠高级项 | 保留当前项目的折叠语义，避免一级页面过长 |
| 短枚举摘要 | `DropdownMenu` + `DropdownItem` | `options`、`value`、`change`、`disabled`、`showOverlay` | 适合主题、DNS、IPv6、TUN 栈等少量选项；已有 `OptionDropdown` 封装 |
| 二级编辑器 | `Popup`（通常 `placement="bottom"`） | `v-model:visible`、`destroyOnClose`、`visible-change` | 移动端编辑表单和规则项使用底部抽屉，关闭时可保护草稿 |

### 16.3 标量和枚举配置

| 配置类型 | 组件 | 约束 |
|---|---|---|
| bool | `Switch` | 只用于真正二值字段；使用 `value`/`change`，异步保存时使用 `loading`/`disabled` |
| 三态或互斥枚举 | `RadioGroup`/`Radio` 或 `DropdownMenu` | IPv6、DNS 等需要保留完整枚举；选项必须来自 schema，不得将 enum 压成 Switch |
| 有界整数 | `Stepper` | 设置 `min`、`max`、`step`、`integer`；`change` 后只更新草稿或触发 typed mutation，不绕过 validator |
| 离散时间/天数 | `Picker` | `columns` 使用 `{ label, value }`；只在选项集合比连续输入更易理解时使用，并在 `confirm` 时提交 |
| 连续范围参数 | `Slider` | 仅用于容差/间隔等可快速拖动且不要求精确审计的参数；保留数字输入或当前值文本，不能替代边界精确输入 |
| 单值端口、CIDR、域名、接口名 | `Input` | 使用 `type`、`maxlength`、`status`、`tips`；错误必须来自 daemon path/index 映射 |

### 16.4 集合、规则和破坏性操作

| 场景 | 组件 | 约束 |
|---|---|---|
| 多接口选择 | `CheckboxGroup`/`Checkbox` | include/exclude 使用有界选项、多选上限和冲突提示，不用逗号字符串隐藏集合语义 |
| CIDR/域名/Wi-Fi/资源候选列表 | `Cell` + `SwipeCell` + `Popup` | Cell 展示规范化摘要；SwipeCell 提供编辑/删除；Popup 承载结构化表单；删除仍需确认 |
| 批量导入或专家 JSON | `Textarea` | 只保留在高级/导入入口；必须有 `maxlength`/大小限制和 schema validate，不能作为规则列表的默认编辑器 |
| 应用配置、恢复默认、清除日志/缓存 | `Dialog` | 使用 `closeOnOverlayClick=false`，确认按钮明确显示影响和不可逆性；复用 `ConfirmDialog` |
| 短暂结果反馈 | `Toast`/`MessagePlugin` | 只反馈成功、失败或冲突的短消息；详细错误留在 `OperationBanner`/字段 `tips` |

### 16.5 不建议使用的组件

- 不使用 `TabBar` 改造设置页内部层级；它属于主导航，项目已有 AppShell 导航。
- 不使用 `Upload`，除非完整备份文件导入正式纳入范围并具备宿主 SAF/私有 payload 契约。
- 不使用 `Slider` 作为所有数字字段的统一控件；端口、超时、保留天数和候选数需要可审计输入。
- 不使用 `Textarea` 表示对象数组的最终状态；对象必须有字段级校验和错误定位。

## 17. TDesign 实施约束

1. 组件只负责交互和视觉状态，配置事实仍由 `uiStores.config` 的 active/draft/digest 管理。
2. `Switch`、`RadioGroup`、`Stepper`、`Picker` 的 `change/confirm` 事件先更新草稿；是否立即 mutation 由字段 metadata 的 `applyImpact` 决定。
3. 二级页离开前检查 dirty 状态；返回键处理顺序为关闭 Popup/Dropdown/Dialog，再询问是否丢弃草稿。
4. `Cell.note` 只显示活动值，不显示未提交草稿；草稿状态使用统一的 dirty 标记和 OperationBanner。
5. capability 不支持时，优先使用组件的 `disabled`，同时显示 reason code 的用户文案；禁止只变灰而不解释。
6. 组件尺寸、颜色和间距使用 `tdesign-common` 设计 token 或项目现有 CSS 变量，不能在每个设置页复制一套主题值。
7. 所有列表操作提供稳定 key、虚拟化/有界数量和可取消的异步请求；`SwipeCell` 展开状态不能跨数据刷新错误复用。

## 18. 文档与实现同步要求

每次新增设置项必须同时更新：

- daemon schema、validator、capability 和 protocol/CLI allowlist（若属于 daemon 配置）；
- WebUI 字段文案、摘要格式、控件映射和错误码映射；
- 设置页路由、返回键和 dirty 草稿测试；
- e2e 断言：真实字段可见、无重复编辑入口、无未实现占位项；
- 本文第 15 节准入矩阵和第 16 节组件映射。

任何只修改 WebUI 而没有事实源、校验和失败语义的配置项，都视为未完成，不得进入完成定义。
