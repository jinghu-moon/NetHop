# NetHop WebUI 去除 TDesign 迁移文档

> 状态：设计中
>
> 日期：2026-08-18
>
> 目标：在不改变现有业务协议和真实功能的前提下，彻底移除 `tdesign-mobile-vue`，将现有 WebUI 迁移到 NetHop 自有的 UI 基础层。目标组件的分层、目录、设计变量体系、Overlay 基础设施和组件 API 见姊妹文档 [`26a-webui-native-ui-foundation-design.md`](./26a-webui-native-ui-foundation-design.md)，本文档只覆盖迁移决策、当前基线、批次顺序和验收标准。
>
> 上位文档：[`08-webui-design.md`](./08-webui-design.md)、[`21-webui-settings-refactor-design.md`](./21-webui-settings-refactor-design.md)、[`25-webui-settings-refactor-tdd-task-list.md`](./25-webui-settings-refactor-tdd-task-list.md)

## 1. 决策摘要

本项目处于开发期，允许破坏性重构，不保留 TDesign 兼容 wrapper。迁移目标不是把 TDesign 标签逐一替换成同名组件，而是以 NetHop 的真实业务和移动 WebView 交互为边界，迁移到 [`26a-webui-native-ui-foundation-design.md`](./26a-webui-native-ui-foundation-design.md) 定义的小而稳定的 UI 基础层，组件命名和目录规则以该文档为准。

迁移顺序采用“由小到大”，但 Overlay 的共享基础设施必须在第一个业务组件迁移前完成：

```text
审计与门禁
    ↓
Overlay / Back / Scroll 基础设施
    ↓
最小基础控件
    ↓
通用复合组件
    ↓
页面级组件
    ↓
大型业务页面
    ↓
删除 TDesign 依赖与产物痕迹
```

## 2. 当前基线

### 2.1 自有组件规模

当前 `webui/src/components` 中有 27 个手写 Vue 组件和 1 个虚拟列表工具。

其中 20 个组件及 1 个工具已经完全脱离 TDesign；以下组件仍直接或间接依赖 TDesign：

```text
ConfirmDialog
OperationBanner
OptionDropdown
PageState
SchemaField
ApplicationSortDropdown
NodeActionsDropdown
```

### 2.2 TDesign 使用面

当前使用的 TDesign 能力包括：

| 能力 | 当前使用位置 | 目标自有能力 |
|---|---|---|
| Button | AppShell、Overview、Foundation、Applications、Nodes、Operations、Subscriptions 及若干组件 | `Button.vue`、`IconButton.vue` |
| Switch | Overview、Applications、SchemaField、ApplicationSortDropdown | `Switch.vue` |
| Input / Textarea / Stepper | Nodes、Subscriptions、SchemaField、Operations | `Input.vue`、`Textarea.vue`、`InputNumber.vue` |
| Checkbox / Radio | Subscriptions | `Checkbox.vue`、`Radio.vue` |
| Popup | Nodes、Subscriptions | `Popup.vue` |
| Dialog | ConfirmDialog | `Dialog.vue` |
| ActionSheet | Subscriptions | `ActionSheet.vue` |
| DropdownMenu / DropdownItem | OptionDropdown | 现有 `AnchoredDropdown` 扩展 |
| PullDownRefresh | Applications、Nodes | 独立 Gesture Feature，暂不进入基础迁移主路径 |
| Tag | Applications、Subscriptions | `Tag.vue` |
| Tabs / TabPanel | Operations | `Tabs.vue` |
| TabBar / TabBarItem | AppShell | `TabBar.vue` |
| NoticeBar | AppShell | `NoticeBar.vue` |
| Loading / Empty / Result | PageState | `PageState.vue` |
| MessagePlugin | OperationBanner | `Toast.vue` |

TDesign 入口目前还包括：

- `webui/src/main.ts` 的全局 TDesign CSS；
- `webui/package.json` 和 `webui/package-lock.json`；
- 依赖审计与契约 fixture；
- 生产 bundle、license、SBOM 和发布报告。

## 3. 迁移原则：开发期破坏性重构

不保留：

- `UiButton` 到 `Button` 的兼容别名；
- `TButton` 参数透传；
- 旧 TDesign 组件和新组件并存的长期路径；
- 为通过旧选择器而保留的假 DOM。

每个迁移批次必须先有 before 测试，完成后运行 after 回归。功能不变，视觉可以按 NetHop 设计系统重新定义。

处置目标中的组件路径（如 `ui/overlay/Dialog.vue`、`ui/primitives/Switch.vue`）以 [`26a-webui-native-ui-foundation-design.md`](./26a-webui-native-ui-foundation-design.md) 第 3 节目标目录为准。

## 4. 现有组件处置矩阵

已有组件按职责处置，不按文件名机械重命名。迁移期间可以存在一个很短的过渡桥接，但桥接组件必须在其全部调用点迁移后删除，不能形成长期兼容层。

| 现有组件 | 当前职责 | 处置 | 目标归属 / 说明 |
|---|---|---|---|
| `AnchoredDropdown.vue` | 锚定定位、面板栈、高度过渡 | 升级纳入 | `ui/overlay/AnchoredDropdown.vue`；保留 nested panel 经验，接入 Overlay Runtime、定位、Back、focus 和 Visual Viewport |
| `ConfirmDialog.vue` | TDesign Dialog 薄包装 | 合并后删除 | 使用 `ui/overlay/Dialog.vue`；若调用点只需要确认语义，保留窄接口 `ConfirmDialog` 一段迁移期，最终按调用点决定直接使用 `Dialog` |
| `settings/SettingsConfirmDialog.vue` | 已手写的设置确认弹层 | 合并后删除 | 将真实可用的 Teleport、动画、ARIA、backdrop 行为沉淀到 `Dialog.vue`，不保留第二套 Dialog |
| `OperationBanner.vue` | `OperationPhase` 到消息展示的领域适配 | 重构保留 | 继续位于领域组件层；底层改用 `Toast`/`NoticeBar`，operation 状态映射不下沉到通用反馈组件 |
| `OptionDropdown.vue` | TDesign Dropdown 薄包装 | 重构或删除 | 多调用点时保留窄接口并基于 `AnchoredDropdown` 实现；调用点少时直接改为业务选项组件并删除 |
| `PageState.vue` | loading/empty/error/warning 页面状态 | 重构纳入 | `ui/feedback/PageState.vue`；采用结构化状态模型，不复制 TDesign Empty/Result API |
| `SchemaField.vue` | schema 字段到 TDesign 控件的映射 | 合并后删除 | 字段布局和 ARIA 使用 `ui/form/Field*`，控件使用 primitives；schema DTO 映射由领域 `SettingsFieldControl` 负责 |
| `SegmentedControl.vue` | 受控分段选择、禁用和 active indicator | 升级纳入 | `ui/navigation/Segmented.vue`；与 `Tabs` 分离，不合并成万能导航组件 |
| `settings/SettingsSwitch.vue` | 已手写的 switch 行为 | 提取后删除 | 受控值、`role=switch`、loading、reduced-motion 沉淀到 `ui/primitives/Switch.vue` |
| `settings/SettingsFieldControl.vue` | 设置字段业务映射和原生控件组合 | 重构保留 | 作为领域 Composite；只组合 `Field`、`Input`、`Textarea`、`Switch`、`InputNumber`、选项组件，不重复实现基础交互 |
| `settings/SettingsGroup.vue` | 设置页分组布局 | 保留为领域组件 | 不泛化成通用 Card/List；它依赖设置页信息架构 |
| `settings/SettingsRow.vue` | 设置页行布局和导航激活 | 保留为领域组件 | 不迁移到 Primitive；后续只替换内部 Button/IconButton 等基础控件 |
| `settings/SettingsPageHeader.vue` | 设置页标题、返回、刷新、校验操作 | 保留为领域组件 | 组合基础 Button，不下沉业务操作语义 |
| `settings/SettingsSecondaryShell.vue` | 设置二级页壳和切换动画 | 保留为领域组件 | 页面壳职责明确，不抽象为通用 Shell |
| `settings/SettingsStatusBanner.vue` | 设置配置状态摘要 | 保留为领域组件 | 只消费通用 Notice/状态 token，不迁移为通用 PageState |
| `applications/ApplicationCategoryDropdown.vue` | 应用分类业务选项 | 保留为领域 Composite | 内部改用升级后的 `AnchoredDropdown`，不进入 `ui/primitives` |
| `applications/ApplicationSortDropdown.vue` | 应用排序、方向和已选优先 | 保留为领域 Composite | 内部组合 `AnchoredDropdown`、`Segmented`、`Switch`；不把排序语义放入基础组件 |
| `applications/ApplicationSearch.vue` | 应用搜索输入和清除 | 保留为领域 Composite | 当前实现可用；出现第二个真实搜索场景后再组合通用 `Input`/`IconButton` |
| `applications/ApplicationIcon.vue` | Android 包图标、主题图标和 fallback | 保留为领域组件 | 依赖 HostAdapter、PackageInfo 和 Android bridge，不能进入 UI 基础层 |
| `nodes/NodeActionsDropdown.vue` | 节点操作业务菜单 | 保留为领域 Composite | 内部改用 `AnchoredDropdown` 和基础 Button，不保留 TDesign Button 参数 |
| `nodes/NodeCard.vue` | 节点列表卡片 | 保留为领域组件 | 依赖节点 DTO、探测状态和 TerritoryFlag |
| `nodes/ActiveNodeSummary.vue` | 当前活动节点摘要 | 保留为领域组件 | 依赖活动节点模型和 TerritoryFlag |
| `nodes/TerritoryFlag.vue` | 节点地区旗帜展示 | 保留为领域展示组件 | 不是通用 Icon；地区代码和资源映射属于节点领域 |
| `MetricValue.vue` | 指标数值展示 | 暂保留领域组件 | 只有在概览、运维等多个领域形成稳定一致契约时，才提升为 feedback/display composite |
| `StatusLine.vue` | 配置/运行状态行 | 暂保留领域组件 | 先避免和 `PageState`、`NoticeBar` 语义重叠，待调用点审计后再决定是否合并 |
| `TrafficSparkline.vue` | 流量趋势画布 | 保留为领域可视化 | 不属于基础控件；继续独立维护 canvas 生命周期和数据采样 |
| `virtual/VirtualListViewport.vue` | 虚拟列表滚动视口 | 保留为技术基础设施 | 位于 `components/virtual`，不放入视觉 primitives；继续单独测试滚动和回收行为 |

处置判定规则：

1. 组件若只提供原子交互且不依赖业务 DTO，才允许进入 `components/ui`；
2. 组件若组合两个以上基础控件并拥有独立交互状态机，可进入 Reusable Composite；
3. 组件若读取领域 DTO、store、router、bridge 或 daemon 协议，必须留在领域或技术目录；
4. 仅包裹一个 TDesign 组件且没有独立业务语义的组件，迁移调用点后直接删除；
5. “暂保留”不是永久豁免：每次页面迁移都要重新统计调用点、重复样式和可复用契约；
6. 不为了目录整齐把 `NodeCard`、`ApplicationIcon`、`SettingsRow` 等领域组件改造成通用组件。

迁移每个已有组件时必须记录：调用点、当前 TDesign 依赖、目标组件、删除条件、before/after 测试和视觉验收截图。只有目标组件已被真实调用且旧组件引用归零，才允许删除旧文件。

## 5. 迁移顺序：由小到大

Overlay 基础设施属于准备阶段，不计入业务页面迁移。实际组件和页面按复杂度递增。

### P0：审计、契约和门禁

1. 固定当前测试基线、bundle 体积、CSS 体积和首屏加载指标；
2. 建立 `check:no-tdesign-new-import`，从此禁止新增 TDesign import；
3. 建立组件目录和命名规则，禁止 `Ui` 前缀；
4. 从现有配色变量提取 `tokens/primitives.css`、`tokens/semantics.css`、`tokens/themes.css` 和 `foundations.css`，先覆盖间距、尺寸、字体、圆角、动效、层级和安全区；
5. 新增 `scripts/check-ui-architecture.mjs`，使用直接声明的 PostCSS 解析依赖建立 Token 引用图并检查未定义、循环、逆层引用、任意 z-index 和 legacy 元数据；
6. 扩展 import graph gate，禁止 Primitive 导入 View、store、router、bridge、daemon 和领域模块；
7. 新增开发期 `UiFoundationView.vue` 和 `/dev/ui-foundation` 路由，使用 `VITE_ENABLE_UI_FOUNDATION=true` 显式开启，不加入底部导航；
8. 明确 before/after 测试命令和证据目录；
9. 将 ChengJing 审计结论固化为负向门禁：禁止任意 z-index/style/gap 逃逸、禁止每实例全局监听、禁止固定 timeout 销毁 overlay；
10. 建立 mobile-first 平台契约：同一 DOM/ARIA/状态机同时支持 Android WebView 与桌面测试浏览器，禁止按 user-agent 复制组件实现；
11. 建立组件样式所有权门禁：组件私有规则不得回流 `base.css`，共享 Token/样式必须有两个以上真实使用点；
12. 禁止 `transition: all`，仅允许显式登记的颜色、opacity、transform 等属性；
13. 不删除旧依赖，不迁移业务页面。

验收：现有功能通过；新增 TDesign import、未登记 overlay 层级或新的 `--td-*` 映射会使 gate 失败；组件实验页只能在显式开发开关开启时访问。

### P1：Overlay / Back / Scroll 基础设施

1. 扩展 `BackDispatcher` 为可观察的 handler 栈；
2. 实现带 `closed/opening/open/closing` 生命周期的 OverlayManager；
3. 实现 scroll lock、safe-area、Visual Viewport 单监听、focus/inert 和 inert fallback；
4. 统一 Back、Escape、backdrop 和 action dismiss pipeline；
5. 为 Popup、Dialog、ActionSheet、Toast、AnchoredDropdown 编写契约测试；
6. 实现并验证 `ui/overlay/Dialog.vue` 的基础契约，包括窄 `beforeDismiss(reason)` 异步关闭守卫、危险操作不自动响应 Enter、transition 生命周期销毁；
7. Toast 使用 operation ID 原位更新，集中管理 timer 和最大可见数量，并验证 `aria-live`/`role=status|alert`；
8. Overlay Runtime 统一持有全局 Back、keydown、resize 和 Visual Viewport 监听，单个组件实例不得重复注册；
9. ScrollLock 使用引用计数并识别内部滚动容器边界，验证嵌套 Popup 内可滚动、背景不可滚动；
10. Popup 将 wrapper/inner visibility 与 transition 生命周期分开，destroy-on-close 只在 leave 完成后销毁；
11. P3 只迁移 `ConfirmDialog` 和 `SettingsConfirmDialog` 的调用点，不重复实现 Dialog；
12. 暂不迁移大型页面。

验收：嵌套 Popup/Dialog 只按栈顺序关闭，重复 close 幂等，页面返回不被重复消费，输入法弹出后表单仍可见且可滚动。

### P2：最小基础控件

迁移顺序：

```text
Button（文本） → IconButton（图标独占） → CompoundButton（图标段+文本段） → Divider → Tag → Switch → Checkbox/Radio → Input/Textarea → InputNumber
```

每完成一个组件就删除对应页面中的 TDesign 使用，不等待整页迁移。

验收：组件浏览器测试、Android touch/IME 测试、禁用/loading/错误状态测试通过；Button 必须覆盖文本和四向 padding；IconButton 必须覆盖单图标、可读名称、方形命中区域和 loading 布局稳定；CompoundButton 必须覆盖水平/垂直拼接、两段尺寸对齐、边框连续及两个独立动作。桌面键盘测试只适用于明确提供键盘增强的组件，不作为移动基础组件的通用门槛。

每完成一个基础组件，必须同步完成 `UiFoundationView` 的真实示例和对应 browser component test。实验页不得使用仿真控件、页面专属样式或绕过组件公开 API 的 DOM 操作。

本阶段以 Android WebView 为主验证：pressed、touch cancellation、`touch-action`、IME `done/next`、键盘收起和 Visual Viewport。hover、focus-visible 和桌面键盘提交属于可选增强验证。视觉尺寸可以紧凑，但触摸命中区域必须满足 touch target token。

### P3：低复杂度通用组件

迁移：

1. `Field`、`FieldLabel`、`FieldDescription`、`FieldError`；
2. `RadioGroup`；
3. `PageState`；
4. `ConfirmDialog`；
5. `OperationBanner`；
6. `SchemaField`；
7. `List`、`ListItem`、`ListItemButton`、`ListSection` 实验组件；
8. `MenuList`、`MenuSection`、`MenuItem` 内部 parts；
9. `OptionDropdown`。

本阶段必须同时执行已有组件处置矩阵：`ConfirmDialog` 与 `SettingsConfirmDialog` 合并到 `Dialog`，`SchemaField` 合并到 `Field` + 基础控件 + 领域 `SettingsFieldControl`，`OptionDropdown` 改为 `AnchoredDropdown + MenuList/MenuSection/MenuItem` 的窄业务组合；旧组件不得因为“暂时还能工作”而继续扩散。菜单 parts 不独立实现定位、Back 或全局监听，也不得用于普通数据列表或 ActionSheet；命令菜单固定为 `menu/menuitem`，值选择固定为 `listbox/option`。每个调用点迁移后立即删除对应 TDesign import，全部调用点归零后删除旧文件。

`Field` 和 `RadioGroup` 可使用 Symbol `provide/inject` 下发 readonly 的 id、disabled、invalid、describedby 和组选中状态。不得复制 TDesign Form 的 rules、数据写入、reset、错误模板或 scroll-to-error 引擎；schema/daemon validation 和事务提交仍由现有领域层负责。

验收：加载、空数据、错误、确认、配置字段变更和枚举选择保持真实可用。

### P4：领域复合组件和导航

迁移：

1. `ApplicationSortDropdown`；
2. `NodeActionsDropdown`；
3. `Tabs`；
4. `TabBar`；
5. `NoticeBar`。
6. `EditorSheet`、`FormActions`、`InlineNotice` 等候选复合组件：只有真实调用点和 browser contract test 达标才实现。
7. `PullRefresh` Gesture Feature：在删除 TDesign 前实现最小可用版本，并完成 Android WebView 真机基线。
8. `Tooltip` 仅作为后续候选评估：只有真实 IconButton 发现性问题和真机交互证据时实现，不阻塞后续 TDesign 删除。

`PullRefresh` 不进入 Primitive 主路径，但属于保持现有功能所必需的独立迁移任务。P6 删除 TDesign 前必须完成 Applications 和 Nodes 的替换；如果产品决定取消下拉刷新，则必须同步修改页面交互、测试和完成定义，不能仅删除组件依赖。

验收：排序、节点操作、底栏路由、运维页签和下拉刷新行为不回归。

### P5：页面迁移

按复杂度从低到高：

```text
OverviewView
    ↓
SettingsView
    ↓
AppShell
    ↓
ApplicationsView
    ↓
NodesView
    ↓
OperationsView
    ↓
SubscriptionsView
```

设置页已经拥有自有组件，迁移时只需接入新的 `Field`、`Input`、`InputNumber`、`Dialog`、`InlineNotice` 和 Overlay 基础设施，不重新设计设置页业务模型；但必须作为独立页面迁移和验收对象。

### P6：删除依赖和产物审计

全部页面迁移后：

1. 删除所有 `tdesign-mobile-vue` import；
2. 删除 `main.ts` 中的 TDesign CSS；
3. 删除 `package.json` 和 lockfile 依赖；
4. 更新 import/dependency/security 检查；
5. 重新生成 bundle、license、provenance 和 SBOM；
6. 对 source、test、fixture、docs、production JS/CSS、license 和 SBOM 执行负向搜索；
7. 检查 `.t-*`、`--td-*`、`TButton/TInput` 等旧 selector、Token 和组件名全部归零；
8. 检查 `legacy.css` 已删除，旧 snapshot 和组件 fixture 不再包含 TDesign DOM 结构。

## 6. 迁移验收与测试

### 6.0 UI Foundation 实验页

- `/dev/ui-foundation` 仅在 `VITE_ENABLE_UI_FOUNDATION=true` 时注册或可访问；
- 不出现在 AppShell 底部导航，不影响默认业务路由和业务页面首屏；
- Primitive、Form、Overlay、Navigation、Feedback、Composite 和 Infrastructure 至少各有一个真实示例；
- Dialog、Popup、ActionSheet、AnchoredDropdown 通过真实交互验证 Android Back、触摸关闭、滚动锁和动画生命周期；Escape、焦点恢复和 inert 仅在桌面增强或实现确实提供时验证；
- Popup/Overlay 示例验证内部滚动、背景锁定、嵌套遮罩隔离、safe-area、IME 和 transition 后销毁；
- 菜单示例验证触摸选择、disabled/divider 跳过和碰撞重定位；桌面增强路径可选验证 Arrow Up/Down、Home/End 和 type-ahead；
- Toast 示例验证同一 operation ID 的 pending/success/error 原位更新、最大数量、timer 清理和 live region；
- Dialog 示例验证异步 dismiss guard、重复 close 幂等、危险操作不自动响应 Enter，以及 transition 完成后才销毁；
- Input、Textarea、InputNumber 通过真实交互验证 IME、边界值、错误和窄屏键盘场景；
- light/dark、reduced-motion、窄屏和 Android WebView 截图纳入视觉回归证据；
- 同一组件 fixture 必须覆盖 Android WebView touch/coarse pointer；fine pointer/keyboard 仅作为可选桌面增强，不维护两套平台组件；
- 实验页不调用 daemon，不修改持久化设置，不替代业务 E2E。

### 6.1 E2E

- AppShell 底部导航；
- Overview 启停和测速；
- Applications 筛选、排序、批量选择；
- Nodes 编辑、测速、导出和排除；
- Operations tabs、连接关闭、备份恢复；
- Subscriptions 编辑、保存、导入和 ActionSheet；
- Settings schema、validate/apply/CAS；
- Android 返回键和长文本输入。

### 6.2 依赖契约

每个迁移批次都必须检查：

```text
源码 import = 0（已迁移范围）
旧组件引用 = 0（已迁移范围）
TDesign CSS = 不新增
业务协议调用 = 不改变
```

`check-ui-architecture.mjs` 必须把目录层级转换成可执行的 import graph 规则：Primitive 不得依赖 Composite/Domain/View，Reusable Composite 不得依赖 Domain/View，所有 UI 基础层不得依赖 bridge/daemon。CSS AST 检查和 import graph 检查进入 `npm run gate`，不只依赖 pre-commit。

## 7. 性能与安全验收

记录迁移前后：

- 首次交互时间；
- WebUI 首屏 ready 时间；
- JS/CSS 未压缩和 gzip 体积；
- 页面切换延迟；
- overlay 打开延迟；
- 典型组件 mount/update/unmount 耗时；
- Primitive DOM 节点数量和嵌套深度；
- 内存和事件监听数量；
- 全局 listener 数量，目标是同一事件由基础设施注册一次，组件实例不重复注册。

安全要求：

- 新组件不得引入远程资源；
- Toast/Dialog 不展示敏感 payload；
- Popup 内容只能由业务 slot 提供，不执行 HTML 字符串；
- focus/inert 不得导致隐藏页面仍可操作；
- 不改变 root-only bridge、私有 payload 和 daemon 协议边界。

## 8. 完成定义

只有满足以下条件，才允许删除 TDesign 依赖：

1. `webui/src` 中没有 `tdesign-mobile-vue` import；
2. `main.ts` 不再加载 TDesign CSS；
3. `package.json`、lockfile、bundle、license、SBOM 均无 TDesign；
4. 所有现有业务页面完成迁移；
5. unit、browser、E2E、静态门禁和安全门禁通过；
6. Android WebView 真机验证输入、返回键、弹层、滚动、底栏和 `PullRefresh`；
7. Applications 和 Nodes 不再依赖 TDesign `PullDownRefresh`，且新的 `PullRefresh` 通过 touch、nested scroll、overscroll、loading 和取消测试；
8. 没有遗留兼容 wrapper、假控件或绕过 daemon 真实状态的 UI。

本设计不授权自动执行 Git 提交、推送、模块安装、设备修改或删除用户文件。后续实现应另行编写对应 TDD 任务清单=按 P0-P6 逐项推进。
