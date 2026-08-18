# NetHop i18n 重构设计

状态：设计已确认，尚未实施
目标版本：开发期下一次破坏性重构
首期语言：简体中文（`zh-CN`）、英文（`en`）
覆盖范围：WebUI、daemon/WebUI 文案代码契约、Android Companion 原生界面

## 1. 决策摘要

NetHop 首期 i18n 采用以下技术边界：

1. WebUI 使用 `vue-i18n` 与 `@intlify/unplugin-vue-i18n`；
2. catalog 在构建期预编译，生产包只保留 runtime，不携带消息编译器；
3. 使用 Vue Composition API，禁用 Legacy API 和隐式全局 `$t`；
4. 首期同时打包 `zh-CN` 与 `en`，不做 locale lazy loading；
5. 日期、时间、数字、流量、相对时间、复数选择和排序使用浏览器原生 `Intl`；
6. daemon、协议 DTO 和领域模型只传递稳定代码及结构化参数，不返回已经本地化的字符串；
7. Android Companion 使用 Android string resource，不复用 WebUI catalog；
8. WebUI 语言偏好和 Android 系统语言互不强制同步；
9. 允许删除旧字段、旧展示模型和旧 fallback 实现，不增加兼容层；
10. 语言入口只有在两种 catalog 完整、测试门禁通过后才进入生产 UI。

目标结构如下：

```text
daemon stable code + structured values
                    |
                    v
        WebUI typed message boundary
                    |
                    v
      precompiled zh-CN/en catalogs
                    |
                    v
      Vue presentation + native Intl

Android status semantics
          |
          v
Android resource identifiers
          |
          v
values/ + values-zh-rCN/
```

本设计不把“允许破坏性改动”理解为无条件修改协议。现有 wire 字段语义正确时保留；只有错误命名、错误分层和双重事实源必须直接移除。

## 2. 开发期重构前提

项目尚未正式发布，因此本次实施遵循以下规则：

- 不迁移不存在的历史 locale 偏好；
- 不保留 `title` 与 `titleKey` 双字段；
- 不保留中文硬编码作为运行时 fallback；
- 不提供旧、新 i18n API 并存期；
- 不为了兼容旧快照保留错误展示模型；
- 可以调整 DTO、view model、组件 props、测试 fixture 和 Android presentation model；
- 每个阶段结束时必须恢复绿灯，禁止长期保留半迁移状态；
- 新功能通过后，订阅、节点、应用策略、实时流量、设置保存、主题、磁贴和 WebView 安全能力必须继续通过回归测试。

破坏性修改的目的必须是消除根因。禁止借重构扩大到翻译管理后台、在线语言包、自动机器翻译或 daemon 日志本地化。

## 3. 目标与非目标

### 3.1 目标

- WebUI 所有用户可见文案完整支持 `zh-CN` 和 `en`；
- 用户可选择“跟随系统”“简体中文”“English”；
- 语言切换即时响应，不刷新页面，不丢失未保存草稿、滚动位置和运行中操作状态；
- 严格 CSP 下不依赖 `eval`、`new Function` 或远程翻译资源；
- 后端动态 `title_key`、`description_key`、`reason_code` 和 `diagnostic_code` 有可验证的本地化覆盖；
- 未知代码不会显示空白、原始 key 或错误语言；
- UI 领域模型不再携带中文展示文本；
- 数字、时间、排序和复数行为随 locale 正确变化；
- Android 磁贴、加载状态和错误状态跟随 Android 系统语言；
- 构建、单元、浏览器、E2E、Android 和真机测试覆盖升级前后行为。

### 3.2 非目标

- 首期不支持繁体中文、日文、韩文或其他语言；
- 不做服务端 locale negotiation；
- 不把语言偏好写入 `nethop.toml`；
- 不让 daemon、CLI 日志和诊断日志输出本地化文本；
- 不远程下载 catalog；
- 不接入翻译 SaaS、CMS 或 AI 自动翻译；
- 不做 locale URL，例如 `/en/settings`；
- 不为两种语言提前建设按 locale 独立构建；
- 不让 WebUI 设置强制修改 Android 系统或磁贴语言；
- 不以本次重构为理由调整无关业务流程和视觉风格。

## 4. 当前实现基线

### 4.1 WebUI 技术和安全边界

当前 WebUI 使用 Vue 3、Vite 8 和 TypeScript，运行于 KernelSU、APatch 或 Android Companion 提供的离线 WebView 环境。

生产 CSP 包含：

```text
default-src 'none'
script-src 'self'
connect-src 'none'
```

因此 catalog 必须随模块构建，生产运行不得访问翻译服务，也不得在浏览器运行时编译消息字符串。

当前 bundle 基线：

| 指标 | 当前值 | 预算 |
|---|---:|---:|
| entry JavaScript gzip | 23,034 bytes | 184,320 bytes |
| 最大 async JavaScript gzip | 67,287 bytes | 81,920 bytes |
| webroot 总大小 | 950,859 bytes | 2,097,152 bytes |

现阶段有足够预算优先保证确定性和维护性。是否拆分 locale chunk 必须以迁移后的实际测量为依据，不能仅引用库宣传体积。

### 4.2 文案和展示模型

当前 WebUI 文案以简体中文硬编码为主。粗略扫描涉及至少 21 个 `src` 文件和数百处中文片段，实际消息数需要在基线阶段通过提取清单确认。

结构性问题包括：

- Vue template、TypeScript view model 和状态 helper 同时负责生成展示文本；
- `node-view.ts` 在领域返回值中嵌入“当前直连”“代理未运行”等中文；
- `latencyView` 返回“超时”“不可用”等本地化结果，而不是结构化状态；
- 节点排序固定使用 `Intl.Collator("zh-CN")`；
- 部分字符串通过模板拼接生成，无法验证占位符完整性；
- 组件可能直接展示后端 reason 或动态 code。

这类文本不能只通过搜索替换成 `t()`。必须先把领域语义与最终展示分离。

### 4.3 daemon 文案代码契约

daemon 的 `config.schema` 已提供：

- `title_key`；
- `description_key`；
- capability `reason_code`；
- 其他状态和诊断中的 `diagnostic_code`。

方向正确，但当前前端 DTO：

- 把 `title_key` 保存为语义错误的 `title`；
- 解析后丢弃 `description_key`；
- 设置页仍可能显示 `field.id`；
- `reasonCode` 仍是无覆盖约束的任意字符串。

协议层已经提供稳定代码，不需要让 daemon 感知 locale。本次应修复前端边界和跨语言覆盖门禁。

### 4.4 UI 本地偏好

当前 `runtime/storage.ts` 对 UI 偏好使用明确 allowlist，已有主题、最后路由和排序设置。语言偏好应复用该机制，禁止新增任意 localStorage key。

### 4.5 Android Companion

当前 Companion 存在三类问题：

1. `res/values/strings.xml` 的默认资源为中文；
2. `TileStateMapper` 直接返回“已关闭”“启动中”“异常”等中文字符串；
3. WebUI 不可用时加载固定 `lang="zh-CN"` 的静态 `fallback/error.html`。

默认 `values/` 应完整提供英文资源，中文资源应放入 `values-zh-rCN/`。纯状态 mapper 不应依赖 `Context` 或最终字符串。

### 4.6 设置设计约束

`21-webui-settings-refactor-design.md` 已明确：语言属于 UI 本地偏好，不进入代理 TOML，且第二种语言完整覆盖前不得显示语言设置。本设计继承该约束。

## 5. 技术选型

### 5.1 评估标准

候选库按以下项目评估：

- Vue 3 响应式集成；
- Vite 8 构建链复杂度；
- 严格 CSP；
- 预编译和 runtime 体积；
- TypeScript catalog schema；
- 动态协议 code；
- locale 即时切换且不刷新文档；
- 离线运行；
- 项目维护状态和长期维护成本。

### 5.2 方案比较

| 方案 | 优点 | 对 NetHop 的主要问题 | 结论 |
|---|---|---|---|
| `vue-i18n` runtime-only | Vue 原生响应式、动态 key、fallback、预编译、成熟测试能力 | runtime 不是最小 | 采用 |
| Paraglide JS | ESM 消息函数、类型安全、tree-shaking 优秀 | 默认切换 locale 会导航或刷新；无刷新需自建响应式适配 | 不采用 |
| Lingui 6 | ICU、catalog 工具完善、Vue extractor 已存在 | Vite 8 宏转换增加 Babel/SWC 链路；Vue 不是当前最短路径 | 不采用 |
| typesafe-i18n | 小 runtime、生成类型完整 | 新增另一套生成工作流；动态协议 code 仍需 registry | 不采用 |
| petite-vue-i18n | 比完整 runtime 更小 | 官方仍标记 experimental，且功能裁剪增加未来限制 | 不采用 |
| 自建字典 | 初始代码少 | 会重新实现 fallback、插值、复数、完整性和类型检查 | 不采用 |

### 5.3 采用方式

依赖职责：

```text
dependencies
└─ vue-i18n

devDependencies
└─ @intlify/unplugin-vue-i18n
```

版本必须与现有依赖一样精确锁定，不使用范围版本。

建议初始化原则：

```ts
createI18n({
  legacy: false,
  globalInjection: false,
  locale,
  fallbackLocale: "zh-CN",
  messages,
})
```

实际类型参数和消息导入以安装版本的官方 API 为准。Vite 必须通过 unplugin 在构建期预编译 JSON，并验证生产 bundle 只引用 runtime 分发。

## 6. 总体架构原则

### 6.1 本地化只发生在展示边界

允许返回本地化字符串的层：

- Vue view/component/composable presentation；
- WebUI `i18n` adapter；
- Android `Activity`、`Service` 或 resource resolver。

禁止返回本地化字符串的层：

- Rust daemon；
- control protocol；
- DTO parser；
- application/node/subscription 领域模型；
- Android 状态 decoder、coordinator 和 tile state mapper；
- 日志、指标和持久化配置。

### 6.2 稳定代码与文案分离

任何可被 UI 展示的状态都必须满足：

```text
stable code + typed arguments -> localized message
```

禁止：

```text
backend prelocalized text -> UI
domain helper Chinese text -> component
unknown dynamic string -> t(dynamicString)
```

### 6.3 catalog 是展示文案事实源

`zh-CN.json` 和 `en.json` 必须有完全相同的 key、参数名和消息结构。品牌名、协议名、包名、IP、CIDR、URL、节点名称和用户输入不是翻译消息。

### 6.4 fallback 不能掩盖缺失

运行时 fallback 用于防止生产空白，不用于允许不完整提交：

- 开发和 CI 中缺失 key 必须失败；
- 生产中未知协议 code 显示通用本地化文案并附带 code；
- 生产中不得直接显示 `config.foo.title` 作为用户文案；
- catalog 完整性门禁通过前不显示语言入口。

## 7. WebUI 详细设计

### 7.1 目录结构

```text
webui/src/i18n/
├─ index.ts                    # 插件创建和只读导出
├─ locale.ts                   # locale 解析、偏好和切换
├─ formatters.ts               # Intl formatter/cache
├─ message-keys.ts             # catalog key 类型或生成结果
├─ protocol-messages.ts        # 动态后端 code 安全边界
└─ locales/
   ├─ zh-CN.json
   └─ en.json
```

不在每个组件建立独立 catalog，避免小型 WebUI 产生过度碎片和重复 fallback 规则。catalog 按语义 key 分组即可。

### 7.2 消息 key 规范

使用稳定、语义化、英文 ASCII key：

```text
common.actions.save
shell.navigation.overview
overview.runtime.running
applications.policy.whitelist
nodes.latency.timeout
settings.appearance.language
config.network.tun_stack.title
capability.probe_supported
diagnostic.config_unavailable
```

规则：

- key 不包含中文或英文展示原文；
- key 不复用仅因当前中文文案相同的消息；
- 相同语义的通用命令可以复用；
- 协议 code 的命名与 wire code 一致或通过显式 registry 映射；
- 参数使用有含义的名字，例如 `{count}`、`{name}`、`{code}`；
- 不通过字符串拼接组成句子；
- HTML 不进入普通消息；带链接的复合内容使用组件分段或库支持的安全插值；
- 所有用户输入按文本渲染，不使用 `v-html`。

### 7.3 locale 类型和偏好

```ts
export const SUPPORTED_LOCALES = ["zh-CN", "en"] as const;
export type SupportedLocale = typeof SUPPORTED_LOCALES[number];
export type LocalePreference = "system" | SupportedLocale;
```

持久化 key：

```text
nethop.ui.locale
```

该 key 加入现有 `UiPreferenceKey` allowlist，允许值仅为 `system`、`zh-CN`、`en`。未知、损坏或旧值直接视为 `system`，不实现迁移器。

设置项使用本地语言名称：

- 跟随系统 / Follow system；
- 简体中文；
- English。

语言偏好立即持久化，不调用 daemon，不进入设置草稿，不显示“应用配置”按钮。

### 7.4 系统语言解析

解析顺序：

1. 若用户选择明确 locale，直接使用；
2. 若为 `system`，按顺序检查 `navigator.languages`；
3. 精确匹配 `zh-CN` 或 `en`；
4. `zh-*` 映射到 `zh-CN`，`en-*` 映射到 `en`；
5. 其他系统语言使用 `en`；
6. catalog 内缺失消息使用 `zh-CN` fallback，但 CI 必须阻止正常 key 缺失。

只在偏好为 `system` 时监听系统语言变化。应用处于前台时重新解析 locale，显式选择不受系统变化影响。

### 7.5 切换行为

切换 locale 必须：

- 只更新 Vue i18n 的响应式 locale；
- 更新 `document.documentElement.lang`；
- 更新 `document.documentElement.dir`，当前两种语言均为 `ltr`；
- 更新 document title 和无障碍标签；
- 触发使用 locale 的 `Intl` formatter 和 collator 重建；
- 保留当前 hash route；
- 保留设置草稿、订阅编辑内容、选中项、滚动位置和运行中 operation；
- 不执行 `location.reload()`；
- 不重新建立不必要的 root shell 或 daemon 会话。

### 7.6 `Intl` 格式化

建立集中 formatter factory/cache，key 至少包含 locale 和 options。覆盖：

- `Intl.NumberFormat`：流量、速率、百分比和计数；
- `Intl.DateTimeFormat`：更新时间、到期时间和日志时间；
- `Intl.RelativeTimeFormat`：相对更新时间；
- `Intl.PluralRules`：库消息无法直接覆盖的复数选择；
- `Intl.Collator`：应用和节点名称排序。

禁止在领域模块固定 `zh-CN` collator。单位协议值仍保持稳定，例如内部统一 bytes、seconds、milliseconds，展示边界再格式化。

格式化结果必须使用不换行单位或明确布局约束，避免英文文案导致速率、流量和按钮溢出。

### 7.7 DTO 破坏性修正

`ConfigSchemaFieldDto` 删除错误字段：

```ts
- readonly title: string
+ readonly titleKey: string
+ readonly descriptionKey: string
```

parser 必须完整验证并保留 `title_key` 和 `description_key`。所有消费者在同一阶段改用新字段，不保留 alias 或 deprecated getter。

`CapabilityItemDto.reasonCode`、subscription diagnostic 和 runtime diagnostic 继续保存稳定 code。可以在 wire 边界保持 `string` 以验证不受信输入，但进入 presentation registry 后必须收窄为已知 key 或 unknown 分支。

### 7.8 动态协议消息 registry

组件禁止直接调用：

```ts
t(field.titleKey)
t(item.reasonCode)
```

必须通过单一 adapter：

```ts
configTitle(field.titleKey, field.id)
configDescription(field.descriptionKey, field.id)
capabilityReason(item.reasonCode)
diagnosticMessage(code)
```

adapter 的行为：

1. 检查 code 是否属于已知集合；
2. 使用对应 catalog key；
3. 验证所需参数；
4. 未知 code 返回 `common.unknownCode({ code })`；
5. 开发环境记录有界诊断，生产环境不抛出导致页面空白。

未知 code 必须保留在展示或可复制诊断中，不能静默转成“未知错误”后丢失证据。

### 7.9 跨 Rust/TypeScript 覆盖契约

为避免手工 registry 漂移，新增共享 golden contract：

```text
tests/i18n/protocol-message-codes.json
```

内容包含 daemon 当前实际可产生的：

- config title keys；
- config description keys；
- capability reason codes；
- runtime/status diagnostic codes；
- subscription diagnostic codes。

约束：

- Rust contract test 从实际 descriptor/枚举生成排序结果并与 golden 比较；
- WebUI i18n 检查读取同一 golden，验证两种 catalog 和 registry 全覆盖；
- 更新 daemon code 时必须显式更新 golden 和两种翻译；
- 不允许通过解析 Rust 源码字符串生成 key；
- 若需要生成 TypeScript union，应由结构化 golden 生成，并在 CI 验证生成文件无漂移。

golden 是跨语言契约，不是 daemon 的运行时依赖。

### 7.10 领域模型重构

所有 model/helper 返回语义数据，不返回本地化句子。例如：

```ts
type NodeLatencyView =
  | { kind: "known"; state: "good" | "medium" | "poor"; milliseconds: number }
  | { kind: "timeout" }
  | { kind: "unavailable" }
  | { kind: "measuring" }
  | { kind: "unknown" };
```

`ActiveNodeView` 返回 `kind`、node、source 数据和 reason code，不返回固定中文 `title`、`detail`、`modeLabel`。Vue presentation composable 将结构化结果转换成 catalog 消息。

同样检查并重构：

- service/runtime presentation；
- subscription health；
- application policy mode/category/sort；
- operation result；
- settings apply impact/risk level；
- empty/loading/error state；
- toast、dialog、aria-label 和 content description。

### 7.11 catalog 加载策略

首期静态打包两种 catalog，理由：

- 只有两个 locale；
- WebUI 必须完全离线；
- 当前 bundle 预算有充足余量；
- 可避免初始语言闪烁和 fallback catalog 未加载；
- 可降低 WebView 相对路径、chunk 和错误恢复复杂度。

只有同时满足以下条件才重新评估 lazy loading：

- 迁移后 catalog 对 entry gzip 造成可测量压力；
- bundle budget 接近阈值；
- 两个 locale chunk 均随模块和 Companion 资产完整打包；
- KernelSU、APatch、Companion 三种 host 均验证动态 import；
- fallback 和加载失败路径已有测试。

禁止远程加载 catalog。

### 7.12 HTML、无障碍和布局

- 根元素 `lang` 必须与当前 locale 一致；
- icon-only 按钮 tooltip 和 `aria-label` 必须进入 catalog；
- 图片 `alt`、表单 label、错误提示、toast 和 dialog 均纳入扫描；
- 不把控件类型写入无障碍名称，例如使用“保存”而不是“保存按钮”；
- 英文最长文案必须在 360x640 到 600x960 现有 viewport 中不溢出；
- 不通过缩小到不可读字号解决英文长度；
- 对固定宽度工具栏优先允许合理换行、截断和 tooltip；
- 颜色和图标含义不得成为语言切换后的唯一状态信息。

## 8. daemon 与协议边界

### 8.1 保留稳定代码

以下 wire 语义保留：

- `title_key`；
- `description_key`；
- `reason_code`；
- `diagnostic_code`；
- 状态 enum 和结构化数值。

它们不是最终显示文本，命名中应明确 `key` 或 `code`。

### 8.2 禁止新增本地化职责

daemon 不接收 locale，不加载 catalog，不返回 `title_zh`/`title_en`，也不根据环境变量决定语言。理由：

- 同一 daemon 可被 WebUI、CLI 和其他客户端消费；
- 本地化会污染协议稳定性和缓存；
- daemon 无法了解每个客户端的语言偏好；
- 日志和错误证据应保持稳定、可搜索。

### 8.3 用户输入和外部数据

以下数据原样保留，不翻译：

- 订阅名称；
- 节点名称；
- 应用名称和包名；
- 域名、IP、CIDR、URL；
- 核心原始错误详情；
- 外部订阅提供的标签。

原始错误详情只能作为技术详情展示，主错误标题由 diagnostic code 本地化。

## 9. Android Companion 设计

### 9.1 资源目录

```text
companion/app/src/main/res/
├─ values/strings.xml             # 完整默认英文
└─ values-zh-rCN/strings.xml      # 完整简体中文
```

`app_name`、磁贴状态、WebUI 加载、WebUI 不可用、错误详情和 content description 均进入资源。默认 `values/` 不再放中文。

### 9.2 Tile presentation 破坏性重构

删除 `TilePresentation.subtitle: String`，改为稳定语义：

```kotlin
enum class TileSubtitle {
    DISABLED,
    SCENE_PAUSED,
    TPROXY,
    TUN,
    STARTING,
    STOPPING,
    PROCESSING,
    ERROR,
    UNAVAILABLE,
}
```

`TileStateMapper` 只返回 `TileVisualState`、`TileSubtitle` 和 `TileAction`。`NetHopTileService.render` 在 Android context 边界把枚举映射到 `R.string.*`。

不建议让 mapper 直接返回 `@StringRes Int`，因为这会让纯业务模型依赖 Android resource，并降低普通 JVM 单元测试的独立性。

`contentDescription` 使用带占位符的 string resource，禁止 Kotlin 字符串拼接。

### 9.3 WebUI 加载和错误界面

删除固定中文 `assets/fallback/error.html` 及其专用 fallback WebView 路径，统一使用原生错误 view：

- 标题和详情来自 Android string resource；
- JavaScript 保持关闭；
- 不需要第二套 HTML locale 选择逻辑；
- 保留当前离线、安全和无外链约束；
- 更新 `TrustedWebOrigin`、fallback asset manifest 和相关 contract test。

这是允许破坏性重构后的最简根治方案。不要维护 `error-en.html` 和 `error-zh-CN.html` 两份重复静态页面。

### 9.4 WebUI 与 Android locale 所有权

- WebUI locale preference 只控制 WebUI；
- Android 磁贴和原生错误 view 跟随 Android 系统 locale；
- KernelSU/APatch 环境没有 Companion，因此 WebUI 不依赖 native bridge 获取 locale；
- 首期不通过 bridge 同步 WebUI 手动选择；
- Android 系统语言变化后，Activity 重建或 TileService 下一次 render 必须解析新资源。

## 10. 错误和回退矩阵

| 场景 | 目标行为 | 禁止行为 |
|---|---|---|
| storage locale 损坏 | 视为 `system` | 崩溃或使用任意字符串 |
| 系统语言不支持 | WebUI 使用 `en` | 远程下载语言包 |
| catalog 正常 key 缺失 | CI 失败 | 依赖生产 fallback 混合语言发布 |
| daemon 返回未知 code | 通用本地化文案 + 原 code | 空白、原 key、静默丢弃 code |
| formatter 输入缺失 | 结构化占位符，例如 `--` | 本地化 `NaN`/`Invalid Date` |
| 切换语言时有设置草稿 | 原地更新文案并保留草稿 | 刷新或重建整个应用 |
| Companion WebUI 不可用 | 原生资源错误页 | 固定中文 fallback HTML |
| Android locale 变化 | 下次 render 使用新资源 | 缓存最终字符串跨 locale 使用 |

## 11. 测试策略

### 11.1 重构前基线

开始修改前必须记录并通过：

```text
WebUI typecheck、unit、browser、E2E、build、bundle、security gate
Companion JVM unit test 和 assembleDebug
Rust 与 config/capability/status 协议相关 contract test
```

同时保存以下行为基线：

- 概览运行状态、流量速率和曲线；
- 订阅新增、编辑、保存、更新和错误恢复；
- 节点排序、测速、自动/手动选择和旗帜；
- 应用搜索、排序、分类、黑白名单保存和应用图标；
- 设置主题、配置草稿、validate/apply/conflict；
- operation toast/dialog 的加载、成功和失败状态；
- Companion WebUI 打开、原生错误状态和快捷磁贴 start/stop；
- 现有移动端亮色/暗色截图。

如当前基线已有失败，必须记录为独立已知问题；不得通过删除测试把它并入 i18n 重构。

### 11.2 WebUI 单元测试

新增测试：

- locale preference parser；
- `navigator.languages` 精确匹配和语言族匹配；
- unsupported locale 回退；
- locale 切换不触发 reload；
- `document.lang`/`dir` 同步；
- `Intl` number/date/relative-time/collator；
- catalog key 完全相等；
- catalog 参数和复数分支相等；
- protocol golden 全覆盖；
- unknown code fallback 保留 code；
- DTO `titleKey`/`descriptionKey` 解析和异常输入；
- model 只返回语义数据，不返回本地化文案；
- 排序随 locale 使用正确 collator。

### 11.3 WebUI browser/E2E

两种 locale 均覆盖：

- 所有一级路由和关键二级页；
- 空、加载、正常、降级和错误状态；
- 语言设置即时切换和持久化；
- 切换语言时设置草稿、表单、滚动和 route 不丢失；
- 订阅保存、应用策略保存和节点选择无回归；
- toast、dialog、dropdown、排序菜单和 tooltip；
- 360x640、393x873、412x915、600x960；
- light/dark；
- 长英文、长节点名、长错误 code 和大数字不溢出。

截图不能只覆盖中文。英文至少拥有核心页面和最长文案状态的独立基线。

### 11.4 静态门禁

新增 `check:i18n` 并纳入 `npm run gate`：

- catalog schema/key/参数一致；
- protocol golden 覆盖；
- 生成类型无漂移；
- `.vue`、`.ts` 中不得新增用户可见中文/英文硬编码；
- allowlist 仅包含品牌、协议、单位符号、测试数据和明确技术常量；
- production bundle 不含消息 compiler、`eval`、`new Function`；
- locale 资源不得引用 HTTP(S) URL；
- bundle budget 继续通过。

硬编码扫描是辅助门禁，不能代替代码审查；英文普通单词与标识符难以仅靠正则准确区分。

### 11.5 Rust contract 测试

- 实际 config schema key 与 i18n golden 相等；
- capability reason code 集合与 golden 相等；
- typed diagnostic enum 与 golden 相等；
- daemon payload 不新增本地化 title/message 字段；
- 原有协议、配置应用和状态行为不变。

### 11.6 Android 测试

JVM/resource contract：

- `TileStateMapper` 所有状态映射到 `TileSubtitle`；
- mapper 不含最终中文或英文字符串；
- 默认和 `values-zh-rCN` 资源 key 完整一致；
- content description 使用格式化资源；
- fallback HTML 和旧 trusted fallback route 已移除；
- 现有 icon、root operation、status decoder 测试继续通过。

instrumentation/真机：

- 英文系统下磁贴、加载和错误状态为英文；
- 简体中文系统下对应状态为中文；
- 运行中切换系统语言后下一次 render 正确；
- 磁贴 start/stop 行为、图标和 Root session 无回归；
- WebUI 手动 locale 不错误修改 Android 磁贴语言。

### 11.7 真机 host 矩阵

| Host | `zh-CN` | `en` | system | 关键验证 |
|---|---:|---:|---:|---|
| KernelSU WebUI | 必测 | 必测 | 必测 | storage、路由、CSP、无远程资源 |
| APatch WebUI | 必测 | 必测 | 必测 | 与 KernelSU 同语义 |
| Companion WebView | 必测 | 必测 | 必测 | native/WebUI 语言边界、草稿保留 |
| Android Quick Settings | 系统中文 | 系统英文 | 不适用 | subtitle、contentDescription、start/stop |

## 12. 分阶段实施

### 阶段 I0：冻结基线

- 运行并记录第 11.1 节测试；
- 建立文案清单和硬编码 allowlist 初稿；
- 记录 bundle 和移动端截图基线；
- 提取 daemon 动态 code 集合；
- 不引入语言设置。

退出条件：现有行为基线可重复，失败项有独立记录。

### 阶段 I1：i18n 基础设施

- 精确锁定 `vue-i18n` 和 unplugin；
- 接入预编译 runtime-only 构建；
- 建立 locale 类型、解析、storage 和 formatter；
- 添加空的结构化 catalog 与完整性检查框架；
- 验证 CSP、bundle 和三种 host 构建资源。

退出条件：基础设施存在但生产 UI 尚无不完整语言入口；现有中文行为不变。

### 阶段 I2：领域展示边界重构

- 删除 model 中的最终展示文本；
- 重构 node、service、subscription、application、operation presentation；
- 修复固定 `zh-CN` collator；
- 更新相关 unit/browser 测试；
- 不保留旧字段或双路径。

退出条件：领域层只返回语义数据，中文展示仍可通过 presentation catalog 完整工作。

### 阶段 I3：静态 WebUI 文案迁移

按以下顺序迁移，减少跨页耦合：

1. common、PageState、dialog、toast；
2. shell/navigation；
3. overview；
4. subscriptions；
5. nodes；
6. applications；
7. operations；
8. settings。

每个域同时提交 `zh-CN`、`en` 和测试。禁止先把所有中文替换为 key、以后再补英文。

退出条件：静态扫描只剩 allowlist，所有页面两种语言可用。

### 阶段 I4：协议动态消息

- 修正 `ConfigSchemaFieldDto`；
- 保留并使用 `description_key`；
- 建立 protocol golden 和 TypeScript registry；
- 接入 config/capability/diagnostic adapter；
- 增加 unknown code 和跨 Rust/TypeScript 测试。

退出条件：所有后端 code 有双语消息或显式 unknown 分支，设置页不再显示字段 ID 代替文案。

### 阶段 I5：Android Companion

- 默认资源改为英文；
- 新增 `values-zh-rCN`；
- `TilePresentation` 改为语义 enum；
- 删除 Kotlin 硬编码最终字符串；
- 删除 fallback HTML/WebView 路径，改为原生错误 view；
- 更新 JVM、resource 和 instrumentation test。

退出条件：Companion 两种系统语言正确，磁贴与 WebView 原有行为回归通过。

### 阶段 I6：语言入口和完整回归

- 在设置 > 外观显示语言选项；
- 验证即时切换、持久化和系统跟随；
- 执行完整 WebUI gate、Rust contract、Companion build/test；
- 构建最新模块并传递到手机；
- 执行第 11.7 节真机矩阵；
- 更新设置设计文档中语言状态。

退出条件：所有自动和真机门禁通过，才能声明 i18n 完成。

## 13. 回归保护矩阵

| 原有能力 | i18n 可能影响 | 必须保护的行为 |
|---|---|---|
| 配置草稿/保存 | locale 响应式重渲染 | 草稿、digest、validate/apply 不变 |
| 订阅保存 | dialog/toast 文案迁移 | 新增、编辑、错误恢复不崩溃 |
| 应用黑白名单 | mode label 和计数复数 | UID 策略、自动保存和回滚不变 |
| 节点排序 | collator 改为 locale-aware | 延迟优先级、稳定 tie-break 不变 |
| 实时流量 | NumberFormat | 原始采样和曲线不变，仅展示格式变化 |
| operation 状态 | code 到文案映射 | loading/success/failure 生命周期不变 |
| 主题 | 同属 UI preference | theme storage 与 locale storage 互不覆盖 |
| WebView CSP | 新构建插件和 catalog | 无远程请求、无 runtime compiler |
| 快捷磁贴 | subtitle enum/resource | start/stop、icon、Root session 不变 |

## 14. 不采纳方案

### 14.1 daemon 返回双语字段

会把展示职责放入核心协议，增加所有客户端负担，拒绝。

### 14.2 使用中文原文作为 key

原文变更会变成协议和代码改动，无法表达上下文，拒绝。

### 14.3 组件直接翻译任意后端字符串

绕过类型和覆盖校验，未知 key 会泄漏到 UI，拒绝。

### 14.4 语言切换刷新页面

可能丢失配置草稿和操作状态，也是本次不选择 Paraglide 默认 locale 策略的核心原因，拒绝。

### 14.5 首期 locale lazy loading

两种语言和当前预算下收益不足，增加离线 WebView 失败面，拒绝。

### 14.6 WebUI 和 Android 共用同一 catalog

两个运行时、生命周期和可用性边界不同，WebUI 故障时原生错误仍必须可本地化，拒绝。

### 14.7 保留旧中文 fallback

会掩盖迁移遗漏并造成混合语言，开发期不需要兼容，拒绝。

## 15. 完成定义

i18n 重构完成必须同时满足：

1. `zh-CN`、`en` catalog key、参数和复数结构完全一致；
2. WebUI 所有用户可见文案来自 catalog 或 locale-aware formatter；
3. 领域模型、DTO parser 和 daemon 不返回最终本地化句子；
4. `title_key`、`description_key`、reason 和 diagnostic code 全部有测试覆盖；
5. 未知 code 显示安全 fallback 并保留诊断 code；
6. locale 切换无刷新、无草稿丢失、无会话重建；
7. HTML `lang`、无障碍名称和 document title 正确；
8. Android 默认英文和简体中文资源完整；
9. Tile mapper 无最终字符串，静态中文 fallback HTML 已删除；
10. WebUI gate、Rust contract、Companion test/build 全部通过；
11. KernelSU、APatch、Companion 和快捷磁贴真机矩阵通过；
12. 订阅、节点、应用策略、流量、设置、主题、Root session 等原有能力无回归；
13. bundle 和 CSP 预算继续通过；
14. `21-webui-settings-refactor-design.md` 的语言状态更新为已实现。

## 16. 资料

- Vue I18n Optimization：<https://vue-i18n.intlify.dev/guide/advanced/optimization>
- Vue I18n TypeScript Support：<https://vue-i18n.intlify.dev/guide/advanced/typescript>
- Vue I18n Lazy Loading：<https://vue-i18n.intlify.dev/guide/advanced/lazy>
- Vue I18n Lite Distribution：<https://vue-i18n.intlify.dev/guide/advanced/lite>
- Paraglide JS Basics：<https://inlang.com/m/gerre34r/library-inlang-paraglideJs/basics>
- Lingui 6 / Vite 8：<https://lingui.dev/blog/2026/04/22/announcing-lingui-6.0>
- Android localization：<https://developer.android.com/guide/topics/resources/localization>
- Android app resources：<https://developer.android.com/guide/topics/resources/providing-resources>
