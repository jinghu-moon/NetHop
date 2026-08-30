# NetHop Companion UI 组件设计

> 状态：设计基线 v0.1  
> 日期：2026-08-29  
> 适用范围：NetHop Companion Kotlin / Jetpack Compose UI  
> 上位文档：[`29-companion-native-ui-design-baseline.md`](./29-companion-native-ui-design-baseline.md)  
> 参考实现：KernelSU WebUI 的概览、订阅、应用和设置页面

## 1. 设计目标

本文件定义 Companion 的组件契约和组装方式。目标是让四个一级页面共享同一套视觉、无障碍和状态行为，同时保持 Rust daemon 为唯一业务状态源。

组件库采用：

```text
Jetpack Compose
  + Material 3（ui-foundation 内部实现）
  + NetHopTheme
  + NetHop 自有 primitive / composite / domain 组件
```

不引入完整第三方 UI 框架，不提供 Material 3 API 的镜像层，也不允许页面直接依赖 `androidx.compose.material3.*`。Material 3 的依赖和适配集中在 `ui-foundation`，业务 feature 只依赖 NetHop 组件 API。

## 2. 组件分层

```text
feature screen
    -> domain components
    -> composite components
    -> primitive components
    -> NetHopTheme / tokens
    -> Material 3 implementation
```

建议目录：

```text
companion/app/src/main/java/com/jinghumoon/nethop/companion/
├── ui/foundation/
│   ├── theme/
│   ├── tokens/
│   ├── primitive/
│   ├── composite/
│   └── feedback/
├── feature/overview/
├── feature/subscriptions/
├── feature/applications/
└── feature/settings/
```

职责边界：

| 层级 | 负责 | 禁止 |
|---|---|---|
| Primitive | 基础视觉、语义、交互状态 | 读取 daemon、导航、保存配置 |
| Composite | 通用页面结构和组合行为 | 解析 JSON、执行 root 命令 |
| Domain | 领域无关的 DTO 映射后 UI 模型呈现 | 重新解释协议语义、持有 Repository |
| Screen | 页面布局、ViewModel 状态收集、事件转发 | 直接调用 RootCommandExecutor |
| ViewModel / UseCase | 单向数据流、命令编排、错误映射 | 输出可变对象给 Composable |

所有组件使用 immutable 参数和事件回调。组件内部可持有短期视觉状态，例如展开状态、按压状态和文本输入草稿，但不能持有 daemon 的长期业务状态。

## 3. Theme 与 token

### 3.1 主题入口

所有 Activity 只安装一个主题入口：

```kotlin
@Composable
fun NethopTheme(
    darkTheme: Boolean,
    content: @Composable () -> Unit,
) {
    MaterialTheme(
        colorScheme = nethopColorScheme(darkTheme),
        typography = nethopTypography(),
        shapes = nethopShapes(),
        content = content,
    )
}
```

`NethopTheme` 是产品契约；Material 3 `ColorScheme`、`Typography` 和 `Shapes` 是内部实现细节。动态颜色首版关闭，颜色必须来自 NetHop semantic token。

### 3.2 初始 token

| Token 类别 | 规则 |
|---|---|
| 间距 | 4dp 基线，常用 4、8、12、16、20、24、32dp |
| 屏幕水平边距 | 16dp；大屏由窗口宽度约束调整 |
| 触控区域 | 不小于 48 x 48dp |
| 顶部栏 | 56dp，不含系统 inset |
| 底部导航 | 64dp，额外叠加导航栏/手势 inset |
| 卡片圆角 | 8dp；弹层按 16dp；不使用无意义的大圆角 |
| 图标 | 默认 24dp，辅助 20dp，强调 28dp |
| 动效 | 100ms、200ms、300ms 三档 |
| 字重 | 400、500、600、700；不依赖可变中文字体 |

组件不得接受页面传入的任意颜色、尺寸或圆角来绕过 token。确有领域语义时，新增 token 必须先登记在 `design/tokens.json`。

## 4. 公共状态与事件契约

所有页面采用以下数据流：

```text
Rust snapshot / event
        -> DaemonClient
        -> Repository
        -> ViewModel
        -> immutable UiState
        -> Composable

用户事件 -> ViewModel action -> UseCase / Repository -> daemon command
```

### 4.1 页面状态

页面至少区分：

```kotlin
sealed interface LoadState<out T> {
    data object Loading : LoadState<Nothing>
    data class Ready<T>(val value: T) : LoadState<T>
    data class Failed(val message: String, val retryable: Boolean) : LoadState<Nothing>
}
```

写操作统一使用：

```kotlin
enum class OperationPhase { Idle, Running, Success, Failure, Conflict }
```

`Running` 时按钮保持尺寸稳定并阻止重复提交；`Failure` 必须显示原因和下一步；`Conflict` 必须要求重新获取 daemon snapshot，不得静默覆盖外部修改。

### 4.2 组件回调

组件回调只描述用户意图，不暴露协议命令：

```kotlin
NethopSwitch(
    checked = state.enabled,
    enabled = state.canChange,
    loading = state.operation == OperationPhase.Running,
    onCheckedChange = { viewModel.onServiceToggle(it) },
)
```

组件不得出现 `runCommand("capture.enable")`、JSON 字段名或 root transport 参数。

## 5. Primitive 组件

第一批 primitive 只实现实际页面需要的窄接口：

| 组件 | 责任 | 必须覆盖的状态 |
|---|---|---|
| `NethopText` | 统一排版角色和最大行数策略 | 长文本、字体缩放 |
| `NethopIcon` | 统一图标尺寸和语义描述 | 装饰/语义图标 |
| `NethopSurface` | 页面表面、边框、圆角和点击语义 | 默认、选中、禁用 |
| `NethopDivider` | 语义分隔线 | 浅色、深色 |
| `NethopButton` | 主要、次要、危险操作 | pressed、focused、loading、disabled |
| `NethopIconButton` | 刷新、排序、更多、返回 | contentDescription、loading、disabled |
| `NethopTextField` | 单行输入、错误和 IME 行为 | focus、error、disabled |
| `NethopSwitch` | 二态开关 | checked、loading、disabled |
| `NethopCheckbox` | 多选 | checked、indeterminate、disabled |
| `NethopRadio` | 单选 | selected、disabled |
| `NethopProgress` | 确定/不确定进度 | 无数据、进行中、失败 |
| `NethopTag` | 系统、警告、失败等短标签 | 对比度、长文案 |

推荐 API 形态：

```kotlin
@Composable
fun NethopButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    variant: NethopButtonVariant = NethopButtonVariant.Primary,
    enabled: Boolean = true,
    loading: Boolean = false,
    leadingIcon: ImageVector? = null,
)
```

文字按钮只用于明确命令。刷新、排序、更多、关闭、返回等使用图标按钮，并提供 TalkBack 名称和 tooltip。

## 6. Composite 组件

### 6.1 应用外壳

```kotlin
@Composable
fun NethopScaffold(
    selectedRoute: PrimaryRoute,
    onRouteSelected: (PrimaryRoute) -> Unit,
    content: @Composable (PaddingValues) -> Unit,
)
```

`NethopScaffold` 负责：

- edge-to-edge 和 `WindowInsets`；
- 四项一级导航：概览、订阅、应用、设置；
- 键盘出现时隐藏底部导航；
- 内容区 padding 和导航栏手势 inset；
- 当前路由的语义选中状态。

Nodes 和 Operations 不进入 `PrimaryRoute`，由页面事件进入二级导航栈。

### 6.2 结构组件

| 组件 | 用途 |
|---|---|
| `NethopTopBar` | 页面标题、副标题、返回和右侧工具操作 |
| `NethopSection` | 标题、辅助说明和内容分组 |
| `NethopSegmentedControl` | 代理模式、接管方式、订阅模式和应用策略 |
| `NethopListItem` | 图标、标题、说明、尾部控件的统一布局 |
| `NethopStatusCard` | 状态标题、说明、操作和状态色 |
| `NethopMetricCard` | 运行时长、流量、CPU、内存等指标 |
| `NethopEmptyState` | 无数据且可解释的空页面 |
| `NethopErrorState` | 错误原因、重试和恢复动作 |
| `NethopOperationBanner` | 长任务阶段、结果、冲突和关闭 |
| `NethopConfirmDialog` | 删除、应用配置等破坏性确认 |
| `NethopActionSheet` | 手机端更多操作菜单 |
| `NethopSnackbarHost` | 短暂反馈，不承载唯一错误信息 |

Composite 组件只接收领域无关的状态模型和回调。例如 `NethopStatusCard` 不知道“daemon”或“代理”，领域名称由页面传入。

## 7. 三个首版页面的领域组件

### 7.1 Overview

```text
OverviewScreen
├── NethopTopBar
├── DaemonStatusCard
├── ProxyModeCard
│   ├── NethopSegmentedControl
│   └── NethopOperationBanner（可选）
├── CaptureModeCard
├── TrafficCard
├── ProxyQualityCard -> Nodes 二级页面
└── NethopMetricGrid
    ├── RuntimeMetricCard
    └── ResourceMetricCard
```

领域组件：

- `DaemonStatusCard`：区分 `Running`、`Unavailable`、`Starting`、`Stopping`、`Error`；开关只反映真实状态，不能用本地点击结果冒充 daemon 已生效。
- `TrafficCard`：消费最近 60 秒 immutable samples；无样本时显示“暂无流量样本”，不得绘制伪造曲线。
- `ProxyQualityCard`：显示当前节点、延迟、协议和出口探测状态；测试动作产生 operation ID。
- `RuntimeMetricCard`：数字使用等宽数字和共享 formatter，缺失值显示 `--`。

### 7.2 Subscriptions

```text
SubscriptionsScreen
├── NethopTopBar + refresh
├── SubscriptionModeCard
├── LazyColumn
│   └── SubscriptionCard
│       ├── NethopRadio / NethopCheckbox
│       ├── quota progress
│       └── update / more actions
├── SubscriptionHistory
└── NethopAddFab -> SubscriptionEditorSheet
```

领域组件：

- `SubscriptionCard`：单订阅模式使用 Radio，合并模式使用 Checkbox；卡片点击和尾部操作必须有清晰的语义边界。
- `SubscriptionQuota`：进度、剩余时间和节点数量缺失时显示明确的未知状态，不将空值显示为 0。
- `SubscriptionEditorSheet`：名称、 HTTPS 地址、高级选项、协议过滤和文本导入预览均由 ViewModel 草稿管理。
- `SubscriptionHistoryRow`：按健康状态显示文字和图标，不只显示颜色点。

更新、删除、导入和模式切换都必须经过 daemon digest 校验。编辑 Sheet 关闭时，如果草稿未提交，应要求用户确认丢弃。

### 7.3 Applications

```text
ApplicationsScreen
├── NethopTopBar + sort
├── ApplicationPolicyControl
├── ApplicationFilterBar
│   ├── NethopTextField
│   └── category menu
├── ApplicationSelectionSummary
└── LazyColumn
    └── ApplicationPolicyRow
```

领域组件：

- `ApplicationPolicyControl`：`all`、`blacklist`、`whitelist` 三种模式来自 daemon；`all` 模式不显示无意义的选择列表。
- `ApplicationPolicyRow`：应用图标由 Android PackageManager 提供；显示名称、包名、系统应用标签、shared UID/root UID 风险和策略开关。
- `ApplicationSelectionSummary`：显示已选数量、当前策略影响和全选/清空动作。

应用列表必须使用 `LazyColumn` 和稳定 package name key。筛选、排序和选中优先均为派生 UI 状态；切换采用短 debounce 合并提交，失败时恢复到最近一次 daemon 确认值。

## 8. Overlay、输入和返回键

弹层统一使用 `NethopConfirmDialog` 或 `NethopActionSheet`，由屏幕级导航/弹层协调器管理返回键，优先级固定为：

```text
Dialog / Sheet
  -> 输入法或编辑状态
  -> 当前二级导航栈
  -> 一级导航栈
  -> Activity finish
```

订阅编辑、配置冲突和删除确认使用 Dialog/Sheet；不使用多个组件各自注册全局返回监听器。

输入组件要求：

- `KeyboardOptions` 与字段语义匹配；
- 错误信息通过 supporting text 和语义树提供；
- 语言切换、Activity 重建和进程恢复不丢失未提交草稿；
- URL、JSON 和包名输入不可因字体缩放而截断；
- 长文本允许换行或滚动，不设置固定单行高度。

## 9. 响应式与无障碍

手机竖屏是首版基线；大屏采用 Material 3 Window Size Class，仅改变导航形态和内容列数，不改变业务语义：

```text
Compact  -> 底部导航 + 单列
Medium   -> 导航栏 + 单列或双列指标
Expanded -> 导航栏 + 内容约束宽度 + 双列/三列信息区
```

验收要求：

- 1.0x、1.3x、2.0x 字体缩放无内容遮挡；
- 所有交互目标不小于 48dp；
- 图标按钮具有稳定 contentDescription；
- 状态同时提供文字/图标，不依赖颜色唯一表达；
- TalkBack 能读出标题、状态、错误、动作和选中项；
- 深色主题正文对比度至少 4.5:1，控件边界至少 3:1；
- reduced motion 下关闭非必要位移动画；
- 横屏、分屏和 Activity 重建后操作仍可达。

## 10. 性能约束

- 首屏使用 `LazyColumn`，禁止在 Composable 中执行 root、网络或阻塞文件 I/O；
- 事件流只更新必要的状态分支，避免整页重组；
- 应用图标按 package name 缓存，并限制并发加载；
- 流量曲线固定 60 个样本，不能无限增长；
- loading、error 和空状态不改变主要布局尺寸；
- Overview 长列表和 Applications 列表不得出现连续丢帧；
- 具体 APK、启动和内存门禁以基准文档第 13 章为准。

## 11. 测试门禁

### 11.1 Primitive / Composite

- 每个可交互组件覆盖 rest、pressed、focused、disabled、loading、error；
- 组件测试验证尺寸稳定、语义树、键盘导航和重复点击；
- Dialog/Sheet 测试验证返回键、IME、焦点恢复和重复关闭；
- 浅色、深色、长文案和 2.0x 字体缩放均有截图或语义测试。

### 11.2 页面

- Overview 验证 daemon 不可用、启动中、运行中、接管未生效、流量无样本和节点测试失败；
- Subscriptions 验证单订阅/合并、空列表、更新失败、digest 冲突、导入预览和删除确认；
- Applications 验证全部/黑名单/白名单、搜索筛选、root UID 保护、自动保存失败和列表稳定 key；
- 页面测试使用 Rust DTO 和 event fixture，不构造假 JSON 字符串绕过解析层。

### 11.3 真机

- Android 13+ 验证应用内语言切换和 `LocaleManager`；
- 验证冷启动、Activity 重建、进程重建、系统返回键、键盘、手势 inset 和 TalkBack；
- 以 R8 release APK 测量体积、首屏可操作时间、RSS、CPU 和列表滚动。

## 12. 实施顺序

1. 从 `design/tokens.json` 生成颜色、排版、尺寸和形状 token。
2. 实现 `NethopTheme`、`NethopScaffold`、四项一级导航和状态反馈组件。
3. 实现 Primitive，再实现不含领域语义的 Composite。
4. 以 Overview 完成首个垂直切片，接通真实 Rust snapshot/event。
5. 实现 Subscriptions 和 Applications 领域组件及其 ViewModel 测试。
6. 接入二级 Nodes/Operations 页面和设置页入口。
7. 完成真机无障碍、性能、协议一致性和 release 构建门禁。

## 13. 明确不做

- 不在组件层实现 daemon 状态机或配置持久化；
- 不在 Kotlin 中复制 WebUI 的业务 JSON 解析；
- 不引入完整 Material 3 外观作为 NetHop 品牌；
- 不为未确认的桌面端、平板端和多窗口场景增加复杂 API；
- 不把 WebUI 的 CSS、CDN 图标或远程图片带入 APK；
- 不为了复用而把三个页面强行抽象成同一个“万能卡片”。
