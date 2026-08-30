# NetHop Design Baseline

> 状态：Design Baseline v0.3  
> 日期：2026-08-28  
> 适用范围：NetHop Companion APK 原生 UI  
> 关联入口：KernelSU 模块 WebUI、Rust `nethopd`/`nethopctl`

## 1. 决策摘要

Companion 采用以下技术和边界：

```text
Kotlin
  + Jetpack Compose
  + Material 3（仅作为内部组件实现）
  + NetHop Theme
  + NetHop 自有组件封装
  + StateFlow / ViewModel
  + kotlinx.serialization
  + 现有 RootCommandExecutor / PersistentRootShell / libsu
  + Rust daemon 协议
```

KernelSU 模块继续提供 WebUI。WebUI 与 Companion 不共享界面代码，但共享 Rust daemon 的协议、状态语义、错误码、事件类型和测试 fixture。

Rust daemon 是唯一的业务、配置和运行时状态事实源。任何 UI 都不得自行实现网络数据面、配置持久化或独立的状态机。

Material 3 不作为 NetHop 的产品品牌。它只负责提供经过验证的基础交互、语义、无障碍和 Android 适配能力；业务页面只能依赖 `NetHopTheme` 与 `nethop.*` 组件 API。

已确认的产品决策：

- Applications 是 Companion 首版一级导航；
- 提供应用内语言切换，默认语言仍为简体中文；
- 不启用 Material You 动态中性表面；
- 不在 APK 内置中文可变字体，使用系统字体和明确 fallback；
- Compose 使用稳定 BOM 和稳定 AndroidX 依赖，不使用 alpha/beta/RC；
- daemon protocol 版本由 Rust `nethop-protocol` 单一事实源导出，客户端只声明兼容范围。

## 2. 目标与非目标

### 2.1 目标

- 为 Companion 提供稳定、原生、可维护的 Android 交互体验。
- 保持与 KernelSU WebUI 的信息架构和状态语义一致，不要求像素级一致。
- 让视觉令牌、组件状态和文案规则可测试、可导出、可演进。
- 支持浅色、深色、中文、英文、动态字体、系统返回键和 edge-to-edge。
- 保持 APK、启动、内存和依赖成本可测量。

### 2.2 非目标

- 不把 WebUI 改写成 Compose，也不删除 KernelSU WebUI。
- 不在 Kotlin 侧复制 Rust 的网络代理、配置解析或持久化逻辑。
- 不追求 Material 3 默认外观，也不引入完整的第三方 UI 框架。
- 不为未确定的平板、桌面或多窗口产品形态预留复杂组件 API。
- 不通过 JNI 将 Rust 逻辑嵌入 Companion；优先使用 root transport 和 daemon 协议。

## 3. 双客户端边界

```text
                         Rust nethopd
       状态、配置、事件、错误码、能力、数据面和持久化
                                  |
                 稳定的 JSON / JSONL 协议与能力协商
                    +-------------+-------------+
                    |                           |
           KernelSU WebUI                 Companion Native UI
              Vue / WebView                  Kotlin / Compose
```

### 3.1 KernelSU WebUI

适合完整管理、复杂编辑、诊断和模块内调试。WebUI 可以继续使用现有 Vue 组件体系和浏览器运行时，但不得改变 daemon 的协议语义以适应某个页面实现。

### 3.2 Companion

适合快速查看状态、启停服务、捕获控制、节点和订阅的常用操作、设置、日志以及 Quick Settings 集成。原生 UI 应优先解决 Android 的生命周期、输入法、返回键、通知、无障碍和系统栏问题。

### 3.3 一致性要求

两端必须对下列内容保持一致：

- protocol version 和 capability negotiation；
- command ID、参数、超时和幂等语义；
- DTO 字段、错误码和 event kind；
- 配置 digest、request ID、operation ID 和 event session ID 规则；
- Active、Inactive、Warm、Idle、Cold、Unavailable、Error 等状态含义。

视觉表现、导航层级和页面编排可以因平台不同而不同。

## 4. Compose 分层与依赖方向

```text
Screen
  -> Composite components
  -> Primitive components
  -> NetHop Theme / tokens
  -> Material 3 implementation
  -> Compose Foundation / UI
```

业务代码禁止直接消费 `androidx.compose.material3.*`。Material 3 的引用集中在 `ui-foundation` 模块，便于将来替换单个组件或整个实现。

建议的模块边界：

```text
ui-foundation/
  theme/
  tokens/
  components/

feature-overview/
feature-settings/
feature-operations/
feature-nodes/
feature-subscriptions/
feature-applications/

data-daemon/
root-transport/
```

每个 feature 只依赖 `ui-foundation`、自己的 ViewModel/UseCase 和数据接口；UI 不直接调用 `RootCommandExecutor`。

## 5. 设计令牌

### 5.1 单一事实源

HTML 原型不再作为 Kotlin 代码的手工复制来源。正式实现应建立一份机器可读的 token 文件，例如：

```text
design/tokens.json
```

该文件生成或校验以下产物：

- `NethopColorScheme.kt`；
- `NethopTypography.kt`；
- `NethopDimensions.kt`；
- WebUI 的语义 CSS 变量；
- token 合法性和对比度报告。

任何导出的 Kotlin、XML 或 CSS 都是生成产物，不应反向修改。

### 5.2 基础刻度

初始刻度沿用现有原型的有效部分，并解决原型之间的冲突：

| 类别 | 基准值 |
|---|---|
| 间距网格 | 4dp 基线；常用 4、8、12、16、20、24、32、40、48、64dp |
| 最小触控区域 | 48 x 48dp |
| 顶部应用栏 | 56dp |
| Companion 底部导航 | 64dp，不含系统手势 inset |
| 默认按钮 | 最小高度 48dp |
| 紧凑按钮 | 最小高度 40dp，仅用于次级或密集场景 |
| 输入控件 | 最小高度 48dp |
| 默认图标 | 24dp；辅助图标 16/20dp，强调图标 28/32dp |
| 圆角 | 0、4、8、12、16、24、full |
| 动效 | 100ms、200ms、300ms 三档 |

组件的真实尺寸必须由这些 token 解析，禁止在页面中散落新的 magic number。

### 5.3 排版

原型采用中文优先排版和零字距原则，保留其信息层级，但 Android 实现收敛为可验证的字体权重：

| 角色 | 默认字号/行高 | 权重 |
|---|---:|---:|
| Display | 32/40sp | 700 |
| Headline | 24/30sp | 700 |
| Title Large | 17/24sp | 600 |
| Title Medium | 15/22sp | 600 |
| Body Large | 15/22sp | 400 |
| Body Medium | 13/20sp | 400 |
| Body Small | 12/18sp | 400 |
| Label | 12/16sp | 600 |
| Label Small | 11/14sp | 600 |

原型中的 620、650、660、670 只作为视觉研究记录，不作为 Android 字体权重契约。Companion 不内置中文可变字体，实际实现使用系统字体和明确 fallback；字体必须在真机上验证中文、英文、数字、等宽日志和 fallback 行为。字号使用 `sp`，不能用屏幕宽度动态缩放。

## 6. 颜色与主题

### 6.1 语义方向

界面主体使用中性灰，功能色只表达状态和风险：

| 语义 | 用途 |
|---|---|
| Neutral | 页面、表面、文字、边框、选中和停用 |
| Info / Steel Blue | 信息、处理中、可访问性焦点、诊断提示 |
| Warning / Amber | 等待、资源不足、风险提示、Warm/Pending |
| Error / Red | 失败、危险、不可撤销或权限问题 |
| Success / Green | 完成、运行正常、应用成功 |

颜色不是唯一线索。任何状态至少同时提供文字或图标；关键状态还应有明确的操作或下一步。

### 6.2 Material 3 映射

`NetHopTheme` 内部使用 Material 3 `ColorScheme`，但语义来源是 NetHop token：

```text
primary              <- action-primary
onPrimary            <- action-on-primary
surface              <- surface
onSurface            <- text-primary
surfaceVariant       <- surface-component
onSurfaceVariant     <- text-secondary
outline              <- border-control
outlineVariant       <- border-default
error                <- error
errorContainer       <- error-container
onErrorContainer     <- error-on-container
```

`info`、`warning`、`success` 和领域状态放在 `NethopExtendedColors` 中，通过 `CompositionLocal` 提供。不得把扩展色伪装成 Material 的 `tertiary`。

### 6.3 动态颜色

首版不启用 Material You 动态中性表面。页面、表面、outline、primary、导航选中、开关、复选框以及 info/warning/error/success 均使用 NetHop 固定语义色，保证 Companion 与 KernelSU WebUI 的视觉和状态含义稳定。

### 6.4 浅色和深色

浅色和深色是两套明确的语义映射，不是简单反转。必须验证：

- 正文对比度至少 4.5:1；
- UI 边界、焦点环和图标至少 3:1；
- disabled 内容不被误解为可用状态；
- red/green、active/inactive 在色觉缺陷和灰度下仍可区分。

## 7. 组件 API 基线

### 7.1 Primitive

第一批只实现实际需要的窄接口：

```text
NethopText
NethopIcon
NethopSurface
NethopDivider
NethopButton
NethopIconButton
NethopTextField
NethopSwitch
NethopCheckbox
NethopRadio
NethopProgress
```

组件负责视觉、语义和交互状态，不负责 daemon 状态、配置保存或业务导航。

所有可交互组件至少覆盖：

```text
rest / pressed / focused / disabled / loading / error
```

图标按钮必须要求可访问名称；loading 时保持尺寸稳定并阻止重复提交。

### 7.2 Composite

业务共用结构使用组合组件：

```text
NethopScaffold
NethopTopBar
NethopBottomNavigation
NethopListItem
NethopSettingItem
NethopStatusCard
NethopSection
NethopEmptyState
NethopErrorState
NethopConfirmDialog
NethopActionSheet
NethopSnackbarHost
```

Composite 组件可以接收领域无关的状态模型和事件回调，但不能直接读取 Repository 或执行 root 命令。

### 7.3 NetHop 领域组件

领域组件只放在 feature 模块或明确的 domain UI 模块中：

```text
DaemonStatusCard
ServiceControl
CaptureControl
NodeListItem
SubscriptionListItem
ApplicationPolicyRow
OperationTask
LogEntry
ConfigField
CapabilityBadge
```

这些组件必须消费 Rust DTO 映射后的 immutable UI model，不得重新解释原始 JSON。

## 8. Companion 信息架构

首版 Companion 以高频 Android 操作为中心，不复制 KernelSU WebUI 的全部页面。底部导航固定为四个一级页面，与当前 WebUI 的信息架构一致：

```text
Overview       概览：状态、服务/核心操作、捕获状态、最近事件
Subscriptions 订阅：订阅状态、更新、启用/禁用
Applications   应用：分应用代理策略、应用列表和选择状态
Settings       设置：Android 侧偏好与 daemon 配置入口
```

`Nodes` 和 `Operations` 是二级目的地：节点列表、测试和选择从概览的代理质量卡进入；长任务、日志和诊断从操作反馈或设置页进入。它们可以拥有独立的导航栈和深链，但不占用首版底部导航位置。

一级导航的实际呈现可以使用底部导航或大屏自适应导航栏，但必须保持四个页面的顺序、标签和可发现性。导航数量属于产品信息架构，不应照搬媒体原型的三项导航。

页面排序和展示密度应服务于网络运维任务：状态优先、操作可见、错误可解释、长列表可搜索和可刷新。

## 9. 平台行为

### 9.1 WindowInsets 和 edge-to-edge

所有页面使用 `WindowInsets` 处理状态栏、导航栏、显示 cutout 和 IME。底部导航的 64dp 是内容高度，必须额外加系统手势 inset，不能把系统 inset 算进 token。

默认页面支持竖屏手机；横屏、分屏和较大窗口至少不能发生内容遮挡或操作不可达。只有确有产品理由的沉浸式页面才允许隐藏系统栏。

### 9.2 返回键与弹层

返回优先级固定为：

```text
最上层 Dialog / Sheet
  -> 页面内编辑或搜索状态
  -> 当前导航栈
  -> Activity finish
```

弹层关闭必须幂等，关闭后恢复合理焦点；不得由每个组件各自安装全局返回监听器。

### 9.3 输入法、字体和无障碍

- 支持系统字体缩放，至少验证 1.0x、1.3x 和 2.0x；
- 文本不得依赖固定单行高度，英文长文案可以换行；
- TalkBack 能读出标题、状态、错误、按钮动作和当前选中项；
- 触控目标不小于 48dp；
- 不使用颜色作为唯一状态线索；
- reduced motion 下关闭非必要位移动画；
- 密集列表仍保留可访问的完整语义，不以视觉截断替代内容。

## 10. 文案与本地化

### 10.1 语言策略

简体中文是当前默认语言，英文作为受支持语言。Companion 提供应用内语言切换，设置项至少包含“跟随系统 / 简体中文 / English”。语言选择持久化在 Android 应用设置中，并通过 Android 13+ `LocaleManager` 应用到整个 Activity；不能在页面内临时拼接或维护第二份语言状态。

切换语言后必须重新创建或重新配置 Activity，当前导航目的地、未提交表单和正在进行的 operation 不能因为语言切换丢失。语言资源使用 `values/strings.xml`、`values-en/strings.xml` 和 `plurals`；不能把 WebUI 的英文文案直接复制为 Kotlin 硬编码。

所有文本进入 `strings.xml`，英文计数使用 `plurals`。文件名、路径、节点 ID、协议字段和错误码不翻译。

### 10.2 文案规则

- 动词优先，例如“启动服务”“停止核心”“应用配置”；
- 破坏性操作按钮使用具体动作，不使用泛化的“确定”；
- 错误至少说明发生了什么；有确定方案时说明下一步；
- 状态文案平静、短、可扫描，不使用感叹号和表情符号；
- 长任务显示当前阶段、进度、结果和可执行操作；
- 数字、时长、流量和日期由共享 formatter 生成，页面不得各写一套规则。

NetHop 示例：

```text
错误：找不到 daemon，可能服务尚未启动
操作：启动服务

错误：配置校验失败，第 3 个字段缺少值
操作：查看配置

不可撤销：永久删除该订阅？
操作：永久删除
```

## 11. 动效、图标和资源

动效只用于状态反馈、展开折叠、页面转场和弹层生命周期：

| Token | 用途 |
|---|---|
| 100ms | 勾选、按压、轻量状态变化 |
| 200ms | 展开折叠、Snackbar、普通进出场 |
| 300ms | 页面或大范围位移 |

动效必须尊重系统 reduced-motion；实时流量、事件和进度更新不得使用连续装饰动画。

图标优先使用本地 VectorDrawable 或 Compose ImageVector。生产包不依赖 CDN、远程字体或远程图片。原型中使用的 Tabler CDN 和 Unsplash 图片只用于展示，不进入 Companion 或 KernelSU WebUI 运行时。

## 12. 数据和状态实现约束

推荐单向数据流：

```text
Rust daemon
  -> DaemonClient
  -> Repository
  -> ViewModel
  -> immutable UiState
  -> Compose

Compose event
  -> ViewModel action
  -> Repository / UseCase
  -> DaemonClient
  -> Rust daemon
```

UI 不得：

- 直接构造 root 命令参数；
- 直接解析 stdout 或 JSONL；
- 使用固定 sleep 等待状态变化；
- 为 daemon 状态建立第二份长期缓存；
- 在 Activity 中持有跨页面业务状态；
- 让多个页面并发执行未协调的 mutating command。

长任务使用 operation ID 或 event session 对账。Activity 重建后必须重新获取 snapshot，并按 event/状态恢复，而不是依赖内存中的旧回调。

## 13. 构建基线、依赖与预算

### 13.1 版本策略

以 `YingLi-Player` 已冻结并通过稳定版本门禁的组合，作为 NetHop Companion 的初始基线。两个 Android 工程应共享这组版本，避免 Compose 编译器、AndroidX 和 Gradle 缓存分别漂移：

| 依赖 | 版本策略 | 初始版本 |
|---|---|---:|
| Compose BOM | 使用稳定 BOM，所有 Compose artifact 不单独写版本 | `2026.06.01` |
| Compose UI/Foundation/Runtime | 由 BOM 对齐 | `1.11.4`（BOM 对齐） |
| Material 3 | 由 BOM 对齐，不单独覆盖 | `1.4.0`（BOM 对齐） |
| `androidx.activity:activity-compose` | 精确稳定版本 | `1.13.0` |
| `androidx.lifecycle:lifecycle-viewmodel-compose` | 精确稳定版本 | `2.10.0` |
| `androidx.navigation:navigation-compose` | 精确稳定版本 | `2.9.8` |
| `androidx.compose.material3:material3-window-size-class`（如使用） | 精确稳定版本 | `1.4.0` |
| Kotlin / Compose compiler plugin | 与现有 Kotlin `2.4.0` 同版本 | `2.4.0` |
| AGP / Gradle / JDK | 沿用仓库已锁定基线 | AGP `9.3.1` / Gradle `9.5.0` / JDK `21` |

Material 3、Activity、Lifecycle 和 Navigation 的版本必须使用稳定发布物。禁止 alpha、beta、RC、SNAPSHOT、动态版本和未审计的 BOM override。Compose BOM 只负责 Compose 家族，Activity、Lifecycle、Navigation 仍需各自精确锁定和 dependency verification。

Compose Compiler 使用 Kotlin 2.4.0 对应的 `org.jetbrains.kotlin.plugin.compose`，不能再引入旧的 `kotlinCompilerExtensionVersion` 配置。引入 Compose 后，必须同步更新 Version Catalog、依赖验证、SBOM、许可证报告和根构建门禁。

### 13.2 初始预算

预算用于第一版 release gate，须在真实 R8 release APK 和同一 Android 设备上测量：

| 指标 | 目标 | 硬门禁 |
|---|---:|---:|
| Companion APK 增量（相对当前 Kotlin/Tile 基线） | <= 2.0 MiB | <= 3.0 MiB |
| 首屏可操作时间（冷启动 p95） | <= 700 ms | <= 1200 ms |
| 首次 Compose composition 到首屏 | <= 250 ms | <= 500 ms |
| Activity 重建后的可操作时间 | <= 500 ms | <= 1000 ms |
| 首屏期间主线程 Root/网络 I/O | 0 | 0 |
| 空闲时后台定时器和 wakeup | 0 | 0 |
| Overview 长列表滚动 | 无可见 jank | 不得出现连续丢帧 |

APK 预算只计算新增 Android 原生 UI 和依赖，不重复计算模块已有的 WebUI。若超过目标，先提交 R8 mapping、dependency tree、size breakdown 和启动 trace，再讨论调整预算；不能直接放宽硬门禁。

### 13.3 版本升级规则

稳定版本不是永久锁死。升级只能在独立变更中进行，并同时通过：

- Gradle dependency verification 和离线构建；
- Compose、Material 3、Activity、Lifecycle、Navigation 的组件回归；
- APK 体积和启动基准对比；
- Android 13+ 真机、字体缩放、深色主题和 TalkBack 验证。

## 14. 验收门禁

### 14.1 Token 与主题

- 所有组件只消费 semantic/component token；
- light/dark 的颜色和扩展色均可解析；
- 对比度矩阵全部通过；
- token 文件、HTML 展示和 Kotlin 导出无漂移；
- 不出现未经登记的字号、间距、圆角和动画时长。

### 14.2 组件与页面

- 组件覆盖 loading、disabled、error、long text 和重复点击；
- Dialog/Sheet 覆盖返回键、IME、焦点恢复和重复关闭；
- 列表使用稳定 key，长列表滚动和刷新不抖动；
- Overview、Operations、Settings 在真机完成浅色/深色和字体缩放验证；
- 节点、订阅、应用和日志数据使用真实 DTO fixture。

### 14.3 Android 真机

至少记录以下 before/after 数据：

- 冷启动到首个可操作状态；
- Activity 重建和 Companion 进程重建恢复时间；
- APK 增量体积、安装体积和 release R8 结果；
- RSS、CPU、线程、FD 和长列表滚动抖动；
- root command 排队、超时和取消行为；
- 键盘、返回键、系统栏和 TalkBack 行为。

不能用浏览器或 Windows Host 的数据替代 Android 真机结论。

### 14.4 协议一致性

Rust、WebUI 和 Companion 必须共同通过：

- protocol handshake；
- snapshot DTO；
- JSONL event；
- error code；
- capability matrix；
- mutating command 的 digest/冲突行为。

## 15. 实施顺序

1. 建立 `design/tokens.json`，把三个 HTML 原型中的有效令牌收敛为单一事实源。
2. 建立 `ui-foundation`，实现 `NetHopTheme` 和最小 primitive/composite 组件。
3. 建立 Kotlin `DaemonClient`、DTO、Repository 和事件订阅，不改变 Rust 协议语义。
4. 实现 Overview 垂直切片，覆盖状态、启停、错误、事件和 Activity 重建。
5. 实现 Operations、Settings、Applications，再按使用证据决定 Nodes 与 Subscriptions 的后续页面深度。
6. 保留 KernelSU WebUI 作为完整管理入口，并使用共享 fixture 做双端回归。
7. 只有在真机性能、组件覆盖和协议门禁稳定后，才考虑移除 Companion 内的 WebView 兼容代码。

## 16. 原型文件的定位

`prototypes/01-layout-typography-system.html`、`02-color-system-components.html` 和 `03-content-format-guidelines.html` 保留为设计研究和视觉校验工具。

可继承的内容：

- Token 分层；
- 4dp 网格和 48dp 触控基线；
- 中性骨架与功能色语义；
- 浅色/深色和对比度审计；
- 文案、格式化、本地化和破坏性操作规则。

不可直接继承的内容：

- “影里”品牌和媒体领域文案；
- 视频、播放器、播放列表、回收站和应用锁组件；
- 首页/视频/整理三项导航；
- 原型中的 CSS 尺寸、外部 CDN、远程图片和浏览器专用效果。

进入 Kotlin 实现前，必须以本文件和机器可读 token 为准，不能直接复制 HTML 中的单个 CSS 值。

## 17. 协议版本单一事实源

Rust `crates/nethop-protocol/src/lib.rs` 中的 `PROTOCOL_VERSION` 是唯一 canonical version。当前仓库值为 `6`；`nethopd`、`nethopctl` 和协议测试直接引用该常量，不允许再次复制数字。

推荐的握手流程：

```text
Rust nethop-protocol::PROTOCOL_VERSION = 6
        |
        +-- nethopctl hello --protocol-min 6 --protocol-max 6
        +-- WebUI PROTOCOL_VERSION（由生成/校验 fixture 对齐）
        +-- Companion BuildConfig 兼容范围（由构建任务注入）
```

具体规则：

1. Rust 发布构建导出 protocol version 和 capability manifest；
2. Companion 不在 Kotlin 源码中手写协议数字，`DAEMON_PROTOCOL_MIN/MAX` 由 Gradle 读取受版本控制的 `protocol-baseline.json`，或由 CI 在构建时从 Rust 生成并校验；
3. 若暂时无法跨 Cargo/Gradle 直接生成，至少只允许在仓库根目录维护一份 `protocol-baseline.json`，Rust、WebUI、Companion 的测试都必须核对它；
4. Companion 启动或首次 Root 操作先执行 hello，只有协商成功才允许执行 mutation；
5. `min/max` 表示客户端兼容范围，不得用它掩盖 wire 不兼容。无法兼容时 fail-closed，显示升级提示；
6. protocol version 变更必须同时更新 Rust、WebUI、Companion fixture、安装包兼容声明和 release notes；
7. `BridgeCommandPolicy` 的 hello allowlist、WebUI `PROTOCOL_VERSION` 和 `BuildConfig` 不得各自维护独立版本常量。

当前 Companion 中 `BuildConfig.DAEMON_PROTOCOL_MIN/MAX` 为 `5`，而 bridge hello allowlist 和 WebUI 已要求 `6`，这是发布前必须消除的版本漂移。建议第一步把所有客户端基线统一到 `6`，再实现生成或校验任务。

## 18. 待决事项

剩余事项主要是实施细节：

- `protocol-baseline.json` 的最终路径以及 Cargo/Gradle 生成接线；
- Applications 页面是否需要独立的深链和通知入口；
- 语言切换时未提交编辑状态的保存/阻止策略；
- Compose 首个垂直切片的真实 APK、启动和内存测量。

已确认的产品决策不再作为待决事项，必须按本基准执行。
