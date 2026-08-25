# NetHop WebUI 自有 UI 基础层设计

> 状态：设计中
>
> 日期：2026-08-18
>
> 目标：定义 NetHop 自有的轻量 WebUI 基础层——组件分层、目标目录、设计变量体系、Overlay 基础设施、组件 API 约束与测试策略。本设计独立描述目标形态，不涉及从 TDesign 迁移的具体批次和验收；迁移过程见姊妹文档 [`26b-webui-tdesign-removal-migration-plan.md`](./26b-webui-tdesign-removal-migration-plan.md)。
>
> 上位文档：[`08-webui-design.md`](./08-webui-design.md)、[`21-webui-settings-refactor-design.md`](./21-webui-settings-refactor-design.md)、[`25-webui-settings-refactor-tdd-task-list.md`](./25-webui-settings-refactor-tdd-task-list.md)

## 1. 设计总览

组件命名不使用 `Ui` 前缀。目录已经提供命名空间，因此使用：

```text
components/ui/primitives/Button.vue
components/ui/overlay/Dialog.vue
components/ui/navigation/Tabs.vue
```

模板中使用：

```vue
<Button variant="primary">保存</Button>
<Switch v-model="enabled" />
<Dialog v-model="visible">...</Dialog>
```

本设计不重造通用 UI 框架，只实现 NetHop 当前业务和移动 WebView 交互实际需要的能力；组件命名、分层和取舍原则见下文。

## 2. 设计原则

### 2.1 业务优先，不重造通用 UI 框架

只实现当前 NetHop 实际使用的状态和交互，不提供 TDesign 的完整 API 兼容层。基础组件应保持窄接口：

- 只暴露当前业务需要的 props、events 和 slots；
- 不复制 TDesign 的几十种变体；
- 不为未来未确认的页面预留复杂抽象；
- 页面业务状态仍由页面或领域 store 持有，组件不拥有 daemon 状态。

### 2.2 语义优先

能使用原生 HTML 语义就不使用自定义 role：

- 按钮使用 `<button>`；
- 输入使用 `<input>`、`<textarea>`；
- 单选/复选优先使用原生控件；
- 只有视觉和交互确实需要时才使用 `role="switch"`、`role="dialog"` 等 ARIA 语义。

### 2.3 Overlay 是系统能力

`Dialog`、`Popup`、`ActionSheet`、`Toast`、`AnchoredDropdown` 不能各自实现返回键、z-index、滚动锁定和焦点管理。它们都依赖同一套基础设施。

```text
OverlayManager
├── z-index / stack
├── BackDispatcher
├── scroll lock
├── safe-area
├── focus / inert
└── lifecycle
       ├── Dialog
       ├── Popup
       ├── ActionSheet
       ├── Toast
       └── AnchoredDropdown
```

当前 `webui/src/shell/useBackDispatcher.ts` 已具备 handler 栈雏形，应在此基础上扩展，而不是让每个弹层单独注册全局监听器。

### 2.4 运行时边界

NetHop WebUI 的目标环境是 Vite + Vue 3 SPA + Companion Android WebView。当前明确不支持 SSR、hydration、Shadow DOM、Web Components 或第三方组件消费者 API；不得为这些边界外能力增加抽象。浏览器端 fallback 只覆盖项目声明的 WebView 基线和开发调试浏览器。

## 3. 目标目录

```text
webui/src/
├── components/
│   ├── ui/
│   │   ├── primitives/
│   │   │   ├── Button.vue
│   │   │   ├── IconButton.vue
│   │   │   ├── Input.vue
│   │   │   ├── Textarea.vue
│   │   │   ├── InputNumber.vue
│   │   │   ├── Switch.vue
│   │   │   ├── Checkbox.vue
│   │   │   ├── Radio.vue
│   │   │   ├── Divider.vue
│   │   │   └── Tag.vue
│   │   ├── composite/
│   │   │   ├── IconTextButton.vue
│   │   │   └── CompoundButton.vue
│   │   ├── form/
│   │   │   ├── Field.vue
│   │   │   ├── FieldLabel.vue
│   │   │   ├── FieldDescription.vue
│   │   │   ├── FieldError.vue
│   │   │   └── RadioGroup.vue
│   │   ├── overlay/
│   │   │   ├── Popup.vue
│   │   │   ├── Dialog.vue
│   │   │   ├── ActionSheet.vue
│   │   │   ├── AnchoredDropdown.vue
│   │   │   └── Toast.vue
│   │   ├── menu/                       # 仅供 AnchoredDropdown 业务组合的稳定 parts
│   │   │   ├── MenuList.vue
│   │   │   ├── MenuSection.vue
│   │   │   └── MenuItem.vue
│   │   ├── navigation/
│   │   │   ├── Tabs.vue
│   │   │   ├── Segmented.vue
│   │   │   └── TabBar.vue
│   │   └── feedback/
│   │       ├── PageState.vue
│   │       └── NoticeBar.vue
│   │   ├── composites/                 # 候选复合组件，按真实调用点逐项晋级
│   │   │   ├── Surface.vue
│   │   │   ├── PageHeader.vue
│   │   │   ├── SectionHeader.vue
│   │   │   ├── EditorSheet.vue
│   │   │   ├── FormActions.vue
│   │   │   ├── InlineNotice.vue
│   │   │   └── IconBadge.vue
│   └── domain/
│       ├── SettingsFieldControl.vue
│       ├── SubscriptionEditor.vue
│       └── NodeEditor.vue
├── views/
│   └── UiFoundationView.vue           # 开发期真实组件实验页，不进入业务导航
├── composables/ui/
│   ├── use-controllable.ts
│   ├── use-disclosure.ts
│   ├── use-press.ts
│   └── use-roving-focus.ts
└── infrastructure/overlay/
    ├── overlay-manager.ts
    ├── back-dispatcher.ts
    ├── scroll-lock.ts
    ├── focus-manager.ts
    └── viewport.ts
```

现有 `AnchoredDropdown.vue` 保留并升级为 overlay 基础能力，不复制为第二套下拉定位实现。现有设置组件在迁移过程中合并：

```text
SettingsSwitch        → ui/primitives/Switch.vue
SettingsConfirmDialog → ui/overlay/Dialog.vue
SettingsFieldControl  → primitives + 领域字段组合
OptionDropdown        → AnchoredDropdown + 业务选项组件
```

### 3.1 组件组合层级

自有 UI 基础层必须支持由简单组件组合成复杂组件，但不建立一个脱离 NetHop 业务的通用组件框架。非视觉 Foundation 与视觉组件分开：

```text
Foundation（非视觉）
  DOM/A11y + Interaction Composables + Overlay Runtime
      ↓
Primitive
  Button / IconButton / Input / Textarea / InputNumber / Switch / Checkbox / Radio / Divider / Tag
      ↓
Reusable Composite
  CompoundButton / Field / RadioGroup / PageState / Dialog / Popup / ActionSheet / Tabs / FormActions
      ↓
Domain Composite
  SettingsFieldControl / ApplicationSortDropdown / NodeActionsDropdown /
  SubscriptionEditor / NodeEditor
      ↓
Page
  Overview / Applications / Nodes / Operations / Subscriptions / Settings
```

依赖只能向下：

- Primitive 不得导入 Composite 或领域 store；
- Foundation 不得知道订阅、节点或配置协议，且不提供视觉业务组件；
- Reusable Composite 可以组合多个 Primitive，但不得直接调用 daemon；
- Domain Composite 可以理解领域 DTO 和状态，但不重复实现基础交互；
- Page 负责编排数据加载、事务、路由和业务反馈。

不要求所有复杂 UI 都抽成通用组件。只有跨两个以上真实调用点、或者包含独立交互状态机的组合，才提升为 Reusable Composite。

### 3.2 多基础组件组合契约

多个基础组件组合时必须遵循统一契约，避免每个页面自行决定间距、禁用传播和事件顺序。

#### 状态所有权

- 业务值由 Page 或 Domain Composite 持有；
- Primitive 采用受控 `modelValue` / `update:modelValue`；
- Dialog、Popup 的 open 状态由调用方持有，OverlayManager 只维护层级；
- loading、saving、conflict 等业务状态不得隐藏在 Button、Input 内部；
- Composite 不复制父级业务状态，只计算展示状态。

#### 事件与提交

- 一个用户动作只能产生一个领域事件；
- `loading=true` 或 `disabled=true` 时 Button 必须阻止重复提交；
- 表单使用原生 `submit` 语义，不能同时绑定 form submit 和按钮 click 执行同一操作；
- Checkbox、Switch 发出布尔值；Radio 单项暴露 checked，RadioGroup 暴露选中的业务值，不暴露第三方组件的字符串兼容值；
- Composite 只向外发出领域事件，例如 `save`、`select`、`exclude`，不透传内部 click。

#### 布局与间距

- Primitive 不设置外边距；
- 父级 Composite 使用 gap 和设计变量控制子组件间距；
- Button 组合由 `FormActions` 或页面 action container 决定伸展和对齐；
- Field 负责 label、control、description、error 的垂直节奏；
- 禁止在 Primitive 内硬编码针对某个页面的宽度；
- 固定格式控件必须有稳定高度，loading 图标不能改变布局。

#### 输入与焦点

- Android WebView 的触摸顺序、视觉顺序和 IME 流程是主契约；
- Input 和 actions 组合必须支持 Android IME 的 `done` / `next` 流程、键盘收起和可视区域滚动；
- Dialog/Popup 打开后不得让背景内容继续响应触摸；focus/inert 属于增强能力，不作为移动端键盘验收门槛；
- IconButton 必须有可见文本或可读名称；
- 电脑键盘焦点顺序、focus-visible 和 roving focus 仅在组件提供桌面增强路径时验证。

#### 禁用和错误传播

- Field 的 disabled 状态传递到实际交互控件；
- Composite 的 loading 只禁用会造成冲突的操作，不应冻结只读浏览；
- 错误文本与对应 control 建立 `aria-describedby`；
- Overlay 内提交失败时保持 overlay 打开，保留用户输入并聚焦错误摘要或首个错误字段。

### 3.3 真实组合场景

以下组合是迁移必须验证的真实使用方式：

| 场景 | 组合 |
|---|---|
| 设置标量字段 | `Field + Switch/Input/InputNumber/AnchoredDropdown + StatusLine` |
| 节点编辑 | `EditorSheet + Field + Input + Textarea + FormActions + Button` |
| 订阅编辑 | `EditorSheet + Field + Input/Textarea/Checkbox + FormActions + Button` |
| 删除/排除确认 | `Dialog + Button(text) + Button(danger)` |
| 应用排序 | `AnchoredDropdown + Tabs/SegmentedControl + Switch + IconButton` |
| 页面异步状态 | `PageState + Button` |
| 全局操作反馈 | `Toast + Operation state` |
| AppShell | `NoticeBar + RouterView + TabBar` |

例如订阅编辑器不应成为一个巨大的基础组件：

```text
SubscriptionEditor（领域状态与保存事务）
├── Popup（容器、返回、滚动锁定）
├── Field
│   ├── Input
│   ├── Textarea
│   └── Checkbox
└── FormActions
    ├── Button[outline]
    └── Button[primary, loading]
```

Popup 不知道订阅内容，Button 不知道保存事务，SubscriptionEditor 不重复实现 overlay 行为。

### 3.4 设计变量体系

当前 `webui/src/styles/base.css` 的颜色变量已经比较完整，但非颜色样式仍有大量 `6px`、`8px`、`12px`、`34px`、`.16s` 等散落值。自有 UI 基础层必须先建立设计变量，再迁移组件。

本设计交叉审计了三份实现：

| 参考 | 可借鉴内容 | 不直接照搬的内容 |
|---|---|---|
| `refer/variables-mono.css` | 原始值、语义别名、组件消费三层关系 | 面向通用设计系统的完整变量规模 |
| `refer/tdesign-common-develop/style` | 主题、基础变量、组件私有变量、结构样式分离；组件尺寸独立映射；复合组件局部覆盖子组件 token | Less 动态变量、大量 mixin、全局 BEM、数千个公开组件变量和历史兼容别名 |
| `D:/100_Projects/110_Daily/Design-System` | 数字即值的 Primitive Token、严格单向引用、独立 density 轴、排版角色、ConfigProvider 和共享表单行为 | 面向发布型组件库的 monorepo、运行时主题注入、消费者覆写 API 和尚未落地的规划性组件 |

审计时必须区分设计文档和真实实现。`Design-System` 当前真实代码主要覆盖 Token、ConfigProvider 和少量 hooks，其组件架构、浮层体系及大量组件仍是规划；`packages/tokens` 还存在内容相同的重复嵌套目录。因此它只作为设计输入，不作为已经通过生产验证的证据。

TDesign Mobile 的 71 个组件目录中有 69 个独立 `_var.less`，说明专业组件库会明确组件变量所有权；但 NetHop 是单一应用，不需要为每个组件暴露可供外部消费者覆写的完整变量 API。NetHop 采用更紧凑的三层模型：

```text
Primitive Tokens
  主题无关、数值稳定、数字即值的原始色板、尺寸、字号和时长
       ↓
Semantic Tokens
  间距等级、控件高度、排版角色、焦点、遮罩、层级和动效语义
       ↓
Component Tokens（仅必要时）
  Dialog 宽度、TabBar 高度、Button 内边距等稳定组件约束；默认由组件私有
```

Token 定义方向是单向的：Component 的默认值向下引用 Semantic，Semantic 的尺度值向下引用 Primitive。稳定的产品布局约束、`env()`、缓动函数和主题色计算可以直接定义在 Semantic，避免为一次性值制造伪 Primitive。禁止反向引用、循环引用和页面直接消费 Primitive。复合组件可以在自身局部边界把父组件变量赋给子组件公开变量，但不得形成全局的组件间依赖链。若某个组件存在一次性几何常量，允许留在组件 scoped style 内；不得为了满足形式上的三层结构制造没有复用价值的全局别名。

目标文件：

```text
webui/src/styles/
├── index.css          # 唯一全局入口，只负责 import 顺序
├── tokens/
│   ├── primitives.css # 原始色板和数字即值的原始尺寸、字号、时长
│   ├── semantics.css  # 间距、控件、排版、动效和层级语义
│   └── themes.css     # 浅色/深色语义颜色、阴影和遮罩
├── foundations.css    # reset、根节点、安全区、focus-visible
├── layout.css         # AppShell、页面 viewport 和全局内容宽度
└── legacy.css         # 迁移期临时文件，最终删除
```

#### 原始数值层

Primitive Token 使用实际数值作为后缀，避免 `size-1` 在不同资料中代表不同像素。它只是原料，不对组件或页面表达用途：

```css
:root {
  --length-0: 0;
  --length-2: 2px;
  --length-4: 4px;
  --length-6: 6px;
  --length-8: 8px;
  --length-12: 12px;
  --length-16: 16px;
  --length-20: 20px;
  --length-24: 24px;
  --length-28: 28px;
  --length-32: 32px;
  --length-36: 36px;
  --length-40: 40px;
  --length-44: 44px;
  --length-48: 48px;
  --length-56: 56px;

  --duration-80: 80ms;
  --duration-160: 160ms;
  --duration-280: 280ms;
  --duration-350: 350ms;
}
```

`--length-*` 可以作为 spacing、控件高度和圆角的共同原料，但业务代码不得直接使用它。`--size-m` 仍然禁止；数字后缀是固定原始值，字母后缀是所属类别内的语义等级，两者不能混用。

#### 语义间距和尺寸

变量等级统一采用七级顺序：

```text
XS < S < M < L < XL < XXL < XXXL
```

`M` 是默认等级。等级只能在所属类别内比较，不能假设 `space-m`、`font-size-m` 和 `control-height-m` 具有相同数值。禁止定义缺少类别语义的 `--size-m`、`--small` 等变量。

只保留项目中高频且可证明的紧凑阶梯：

```css
:root {
  --space-xs: var(--length-2);
  --space-s: var(--length-4);
  --space-m: var(--length-8);
  --space-l: var(--length-12);
  --space-xl: var(--length-16);
  --space-xxl: var(--length-24);
  --space-xxxl: var(--length-32);

  --control-height-xs: var(--length-24);
  --control-height-s: var(--length-28);
  --control-height-m: var(--length-32);
  --control-height-l: var(--length-36);
  --control-height-xl: var(--length-40);
  --control-height-xxl: var(--length-48);
  --control-height-xxxl: var(--length-56);

  --touch-target-min: var(--length-44);

  --page-gutter-mobile: var(--space-xl);
  --page-gutter-wide: var(--length-28);
  --content-max-width: 820px;
}
```

`touch-target-min` 是交互命中区域，不要求所有可见控件都绘制为 44px；紧凑 IconButton 可以通过伪元素或外层命中区域满足触摸要求，同时保持视觉尺寸稳定。

组件公开的 `size` API 不必暴露全部七级。例如 Button 当前只需要：

```ts
type ButtonSize = "s" | "m" | "l";
```

完整七级是设计变量能力，不是要求每个组件都实现七种尺寸。

#### 圆角

```css
:root {
  --radius-xs: var(--length-2);
  --radius-s: var(--length-4);
  --radius-m: var(--length-6);
  --radius-l: var(--length-8);
  --radius-xl: var(--length-12);
  --radius-xxl: var(--length-16);
  --radius-xxxl: var(--length-24);
  --radius-round: 999px;
}
```

卡片默认不超过 8px 的现有设计约束继续有效。`radius-xl/xxl/xxxl` 仅用于 Dialog、Popup、设备预览等明确的大型容器，不用于普通列表行。

#### 字体和行高

```css
/* tokens/primitives.css */
:root {
  --font-family-sans: "NetHop HarmonyOS Sans SC", Roboto, "Noto Sans SC", "Microsoft YaHei", system-ui, sans-serif;
  --font-family-number: "NetHop TCloud Number", var(--font-family-sans);
  --font-family-mono: "NetHop Fira Code", ui-monospace, SFMono-Regular, Consolas, monospace;

  --font-size-10: 10px;
  --font-size-11: 11px;
  --font-size-12: 12px;
  --font-size-14: 14px;
  --font-size-16: 16px;
  --font-size-20: 20px;
  --font-size-24: 24px;

  --line-height-14: 14px;
  --line-height-16: 16px;
  --line-height-18: 18px;
  --line-height-20: 20px;
  --line-height-24: 24px;
  --line-height-28: 28px;
  --line-height-32: 32px;
}

/* tokens/semantics.css */
:root {
  --font-size-xs: var(--font-size-10);
  --font-size-s: var(--font-size-11);
  --font-size-m: var(--font-size-12);
  --font-size-l: var(--font-size-14);
  --font-size-xl: var(--font-size-16);
  --font-size-xxl: var(--font-size-20);
  --font-size-xxxl: var(--font-size-24);

  --font-body-s: 400 var(--font-size-m) / var(--line-height-18) var(--font-family-sans);
  --font-body-m: 400 var(--font-size-l) / var(--line-height-20) var(--font-family-sans);
  --font-body-l: 400 var(--font-size-xl) / var(--line-height-24) var(--font-family-sans);
  --font-label: 600 var(--font-size-m) / var(--line-height-18) var(--font-family-sans);
  --font-title-s: 600 var(--font-size-l) / var(--line-height-20) var(--font-family-sans);
  --font-title-m: 600 var(--font-size-xl) / var(--line-height-24) var(--font-family-sans);
  --font-title-l: 600 var(--font-size-xxl) / var(--line-height-28) var(--font-family-sans);
}
```

`HarmonyOS_SansSC_Regular.woff2` 通过 `styles/fonts.css` 本地注册，是 WebUI 的主字体；`TCloudNumber-Regular.woff2` 只用于速度、流量、延迟、资源、计数器等数字指标；`FiraCode-Regular.woff2` 只用于代码编辑器和原始日志，不用于普通正文。三者都必须保留 Android 系统字体回退，禁止远程字体。数字字号和行高属于 Primitive；`body/label/title` 等完整 font shorthand 才是组件优先消费的排版语义。角色不强制提供七档，只提供真实界面需要的组合。不使用 viewport width 缩放字号。单个组件不得创建新的全局字体阶梯，除非先更新 token 契约和视觉回归。

#### 组件变量与复合组件覆盖

组件变量默认定义在对应 Vue 的 `<style scoped>` 中，并从 Semantic Token 派生：

```css
.button {
  --button-height: var(--control-height-m);
  --button-padding-block: var(--space-m);
  --button-padding-inline: var(--space-l);
  --button-content-gap: var(--space-m);
  --button-font: var(--font-label);
}

.button[data-size="s"] {
  --button-height: var(--control-height-s);
  --button-padding-block: var(--space-s);
  --button-padding-inline: var(--space-m);
  --button-content-gap: var(--space-s);
}
```

复合组件需要改变子组件视觉时，只允许使用以下两种稳定入口：

1. 子组件公开的视觉 CSS 变量，例如 Dialog 在自己的边界内覆盖 Popup 圆角；
2. 子组件明确声明的 `size`、`variant`、`placement` 等视觉 prop。

文本 Button 的水平留白采用紧凑尺寸契约：S/M/L 分别为 `8px / 10px / 14px`。该值用于控制文字与边框之间的视觉距离，不用于推导按钮外部高度；按钮高度仍由尺寸 Token 约束。这样可以避免短中文操作词在移动端界面中产生过宽的按钮，同时保留四向 padding 和稳定的触摸区域。

行为配置必须通过 prop、emit、provide/inject 或 composable 传递，不能通过 CSS 变量表达。禁止父组件使用深层选择器覆写子组件内部类名，也禁止像传统 Form 样式那样批量清除 Input、Textarea、Cell 的内部 padding。需要组合态时，应由子组件提供明确的 `embedded`/`inset` 契约，或者提取共享布局 Primitive。

Scoped 不等于不可主题化：Component Token 定义在组件根节点的 scoped style 中，CSS 自定义属性仍会沿 DOM 继承。局部主题通过组件根节点的 class、`data-theme` 或 inline custom properties 覆盖；只有当同一 Token 必须跨多个无关 SFC 或 Teleport 边界共享时，才提升到 `tokens/themes.css` 或独立组件 Token 文件。NetHop 不因为未来可能的局部主题而预先建立全局组件变量表。

#### 密度是独立轴

`Design-System` 将 `compact/default/comfortable` 密度与 `S/M/L` 组件尺寸分离，这个方向正确：尺寸描述同一组件的规格，密度描述整个界面的信息密度，两者不能合并成一条等级轴。

NetHop 当前只实现默认密度，不增加没有真实设置入口和完整适配的假配置。未来只有在用户确实需要紧凑/舒适模式时，才允许通过 `data-density` 覆盖 `control-height`、列表行高、padding 和 gap 等 Semantic Token；字体、颜色、圆角及 `touch-target-min` 不随密度缩小。引入密度模式必须同时完成全页面视觉回归、触摸命中和 Android WebView 真机测试。

#### 等级使用规则

1. CSS 变量名使用小写后缀：`xs/s/m/l/xl/xxl/xxxl`；文档和设计稿使用大写等级；
2. `M` 是默认值，组件省略 `size` 时必须落到 M；
3. 相邻等级原则上只跨一级，避免同一组件在不同页面任意跳级；
4. 领域页面只能消费语义或组件 token，不能重新解释等级数值；
5. 不是所有类别都必须强行使用七级；如果阴影只有三档，使用 `shadow-s/m/l`，不创建无实际用途的空等级；
6. `round`、`circle`、`touch-target-min`、safe-area 等特殊语义不纳入从小到大等级；
7. 迁移旧 magic number 时先映射到最近等级，只有视觉或稳定尺寸确实无法满足时才保留局部常量；
8. 设计变量等级不等于组件 props 数量，组件仍遵循最小 API 原则。
9. 新增全局 Token 必须有至少两个真实消费点、明确稳定的产品语义，或者属于可访问性/基础设施必需值；否则保留为组件局部常量。

#### 动效

参考 `variables-mono.css` 的动效梯度，并加入当前设置页使用的整页切换时长：

```css
:root {
  --motion-duration-fast: var(--duration-80);
  --motion-duration-base: var(--duration-160);
  --motion-duration-slow: var(--duration-280);
  --motion-duration-page: var(--duration-350);
  --motion-easing-standard: cubic-bezier(.4, 0, .2, 1);
  --motion-easing-emphasized: cubic-bezier(.32, .72, 0, 1);
}
```

- hover/pressed 使用 fast/base；
- Popup/Dialog 使用 slow；
- 完整一级/二级页面切换使用 page；
- 禁止 `transition: all`，每个组件必须声明实际发生变化的属性；
- 所有动效在 `prefers-reduced-motion: reduce` 下关闭或立即完成；
- 不为装饰引入 spring 动画，除非真实交互验证需要。

#### 阴影、遮罩和层级

```css
:root {
  --scrim-default: rgb(0 0 0 / .56);
  --shadow-raised: 0 1px 4px rgb(0 0 0 / .12);
  --shadow-overlay: 0 8px 24px rgb(0 0 0 / .18);

  --z-content: 0;
  --z-sticky: 20;
  --z-dropdown: 100;
  --z-popup: 200;
  --z-dialog: 300;
  --z-toast: 400;
}
```

层级变量由 OverlayManager 消费。业务页面不得通过任意 `9999`、`20000` 绕过 overlay 栈。

#### 安全区和可视视口

```css
:root {
  --safe-top: env(safe-area-inset-top, 0px);
  --safe-right: env(safe-area-inset-right, 0px);
  --safe-bottom: env(safe-area-inset-bottom, 0px);
  --safe-left: env(safe-area-inset-left, 0px);
  --visual-viewport-height: var(--nh-visual-height, 100dvh);
}
```

Popup、Dialog、TabBar 和页面容器只能通过这些语义变量处理刘海、底部手势区和 Android 键盘造成的可视高度变化。

#### 变量消费规则

1. Primitive 使用数字即值的命名，值固定且不随主题变化；
2. Semantic 的尺度值引用 Primitive，Component 默认引用 Semantic；页面不得直接消费 Primitive；
3. 主题只覆盖语义颜色、阴影和必要的遮罩，不重复定义间距和尺寸；
4. Primitive 组件不定义外边距；Composite 使用 spacing token 控制布局；
5. 重复两次以上的尺寸必须评估提升为 token；一次性的 SVG 几何值可以保留局部常量；
6. 禁止为了消灭所有数字而创建没有语义的变量；
7. TDesign 删除后同时删除全部 `--td-*` 映射；
8. 新组件不得新增未登记的 z-index；
9. 视觉覆盖使用公开 CSS 变量或视觉 prop，行为传递使用 Vue 契约，禁止跨组件深层样式覆写；
10. token 变更必须运行 light/dark、多 viewport 和 200% 字体缩放视觉回归。

### 3.5 CSS 文件所有权与 `base.css` 重构

CSS 采用混合所有权，不采用“全部独立 CSS”或“全部写进 Vue”的单一方案。

`Design-System` 作为对外发布的组件库选择全局 BEM 并禁用 scoped，是为了允许消费者覆写内部样式；NetHop 是内部业务应用，不提供第三方样式兼容面，因此继续使用 `<style scoped>` 隔离组件结构。跨组件定制只通过明确公开的 CSS 变量或视觉 prop 完成，不把内部类名变成公共 API。

#### 独立 CSS 文件负责

- design tokens；
- light/dark 主题语义映射；
- reset、字体继承、`box-sizing`；
- `html/body/#app` 等文档根规则；
- 全局 `focus-visible`、safe-area、visual viewport；
- 确实跨页面共享的 AppShell/Page 布局契约；
- 迁移期尚未归属的 legacy 样式。

#### Vue `<style scoped>` 负责

- 组件内部结构；
- 组件状态和变体；
- 组件自身的动画和媒体查询；
- 页面专属布局；
- 领域复合组件的子组件组合关系；
- 只在该 SFC 模板中出现的选择器。

不建议为每个 Vue 组件额外创建同名独立 CSS 文件。组件模板、状态和样式应保持同一所有权边界。只有多个组件共同消费的稳定规则才提升到独立 CSS。

#### 当前问题

`webui/src/styles/base.css` 当前约 563 行、41 KB，同时承担：

```text
颜色与主题变量
TDesign 变量映射和覆盖
全局 reset
AppShell / Page 布局
Overview 页面样式
Subscriptions 页面样式
Nodes 页面样式
Applications 页面样式
Operations 页面样式
Settings 遗留样式
多个手写组件样式
响应式规则
```

这使任意页面修改都可能影响全局选择器，且 TDesign 删除后难以证明遗留样式已经清理。`base.css` 必须在组件迁移前完成结构性拆分。

#### 目标结构

```text
webui/src/styles/
├── index.css        # 唯一全局 CSS 入口，只负责 import 顺序
├── tokens/
│   ├── primitives.css # 数字即值的原始尺寸、字号、时长和原始色板
│   ├── semantics.css  # 间距、控件、排版、动效和层级语义
│   └── themes.css     # 浅色/深色语义颜色、阴影和遮罩
├── foundations.css  # reset、根节点、表单继承、focus-visible、reduced-motion
├── layout.css       # AppShell、page viewport、safe-area、全局内容宽度
└── legacy.css       # 迁移期临时存放尚未归属的旧规则，最终删除
```

`main.ts` 最终只加载：

```ts
import "./styles/index.css";
```

`index.css` 只允许按顺序导入，不允许新增业务选择器：

```css
@import "./tokens/primitives.css";
@import "./tokens/semantics.css";
@import "./tokens/themes.css";
@import "./foundations.css";
@import "./layout.css";
@import "./legacy.css";
```

`legacy.css` 是临时迁移工具，不是新的全局垃圾桶。每个进入 `legacy.css` 的选择器必须登记来源组件和删除阶段；TDesign 依赖归零时该文件必须为空并删除。

每个 legacy 块必须带机器可解析元数据：

```css
/* @legacy
 * source: NodesView.vue
 * owner: nodes
 * phase: P5
 * remove-after: nodes-view-migration
 */
```

CI 同时记录 selector 数量、rule 数量和字节数，三项都不得超过已登记 baseline；P6 完成时文件必须不存在。

#### 当前选择器迁移表

| `base.css` 内容 | 目标所有者 |
|---|---|
| 原始尺寸、字号、时长和原始色板 | `tokens/primitives.css` |
| spacing、排版角色、圆角、控件高度、motion、z-index | `tokens/semantics.css` |
| 语义颜色、dark theme、阴影和遮罩 | `tokens/themes.css` |
| `*`、`html/body/#app`、表单字体、focus、reduced motion | `foundations.css` |
| `.app-shell`、`.app-content`、`.page`、全局 page gutter | `layout.css` 或 `AppShell.vue` |
| `.overview-*`、`.service-*`、`.traffic-*`、`.insight-*` | `OverviewView.vue` 及其子组件 |
| `.subscription-*`、`.source-*`、`.editor-*` | `SubscriptionsView.vue` 及拆出的领域组件 |
| `.node-*`、`.active-node-*` | `NodesView.vue`、`NodeCard.vue`、`ActiveNodeSummary.vue` |
| `.application-*`、`.app-*` | `ApplicationsView.vue` 及应用组件 |
| `.operation-*`、`.connection-*`、`.log-*` | `OperationsView.vue` 及领域组件 |
| `.settings-*`、`.schema-*` | 设置组件和新 Field/SchemaField 组合 |
| `.ui-foundation-*` | `UiFoundationView.vue` scoped styles；不回流 `base.css` |
| 所有 `.t-*` 和 `--td-*` | 随对应 TDesign 迁移批次直接删除 |

#### 样式提升规则

组件样式只有满足以下至少一项才允许提升为独立全局规则：

1. 被两个以上无领域关系的组件真实复用；
2. 属于文档根、主题或 AppShell 布局契约；
3. 必须跨 Teleport 边界共享；
4. 属于全局可访问性或 reduced-motion 规则。

否则保留在 Vue SFC 中。一个 View 的 scoped style 持续增长时，应先拆分领域组件，不应把页面样式重新移动到全局文件。

#### CSS 门禁

新增静态检查：

- `index.css` 禁止出现 `@import` 之外的规则；
- `tokens/*.css` 禁止业务类选择器；
- `primitives.css` 禁止引用 Semantic 或 Component Token；
- `semantics.css` 禁止引用 Component Token；
- `themes.css` 禁止页面和组件选择器；
- 禁止未定义 token、循环引用和逆层引用；
- Primitive 组件的 scoped style 禁止外边距和页面宽度；
- 禁止新的 `.t-*`、`--td-*`；
- 禁止未登记的全局业务选择器；
- 禁止业务页面使用任意 z-index；
- `legacy.css` 选择器数量必须单调下降。

### 3.6 主流组件库架构调研与吸收边界

本节用于防止“看到主流方案就全部照搬”。NetHop 是单一产品、Vue 3、Android WebView 应用，不是需要被第三方消费的通用 npm 组件库；采纳原则是：吸收可验证的行为契约，拒绝与当前产品无关的发布、SSR、主题算法和扩展 API。

| 方案 | 官方做法 | NetHop 采纳 | NetHop 不采纳 |
|---|---|---|---|
| Ant Design | Seed → Map → Alias 的 Token 派生；组件级 Token；dark/compact algorithm；ConfigProvider | 语义 Token 派生、组件局部视觉覆盖、主题与密度概念分离 | 运行时主题算法、全局 ConfigProvider API、完整消费者定制面 |
| Element Plus | BEM 全局类、SCSS 变量、CSS 变量、按组件组织主题变量 | 组件变量按所有权组织；CSS 变量作为视觉覆盖入口；样式变量与组件结构分离 | 全局 BEM 和为了第三方覆盖而保留的内部类名 |
| Radix Primitives | 无样式、可组合、无障碍行为组件；默认可 uncontrolled，也支持 controlled | 行为与视觉分离；组合通过 slot/part；只实现真实使用的行为状态 | 新增 Radix 依赖；不把无样式行为层扩展成第二套组件库 |
| React Aria | 将交互、键盘、ARIA、状态封装为 hooks，视觉由业务决定 | 将 focus、controllable、disclosure、roving focus 等逻辑放进 composables；组件只负责渲染 | React 专属 API、完整国际化和大量跨平台适配 |
| Floating UI | `autoUpdate` + middleware 组合定位，处理滚动、resize、flip、shift、size | AnchoredDropdown 统一定位生命周期和碰撞策略；只在手写定位无法满足时引入或复用定位库 | 为普通 Dialog/ActionSheet 引入定位依赖；每个浮层自行监听 window |
| Vue 官方 | composable 复用有状态逻辑；`v-model` 以 `modelValue/update:modelValue` 建立受控契约；provide/inject 解决深层依赖 | 使用 composable 拆行为；Primitive 默认遵循受控模型；Field/Overlay 上下文使用 Symbol 注入 | Primitive 直接读 store、router、daemon 或页面业务状态 |

#### TDesign 桌面端与移动端源码审计

本设计进一步审计了：

- `refer/tdesign-common-develop/style/web`：桌面端样式、主题、组件变量和 PBEM 结构；
- `refer/tdesign-common-develop/style/mobile`：移动端样式、触摸尺寸、hairline、安全区和组件变量；
- `refer/tdesign-common-develop/js`：跨框架的算法、数据模型和工具；
- `refer/tdesign-mobile-vue-develop/src`：Vue 移动组件、hooks、Popup/Overlay/Form 组合和组件测试。

TDesign 的顶层架构是：

```text
Design/API contract
        ↓
tdesign-common
  ├── framework-neutral JS / algorithms
  ├── web style and tokens
  └── mobile style and tokens
        ↓
framework repository
  ├── Vue component rendering
  ├── hooks / context
  ├── props / types
  ├── tests
  └── demos / documentation
```

另外，`tdesign-mobile-vue-develop/src/*/props.ts` 明确标注为自动生成文件，组件目录同时维护 `type.ts`、实现、测试、demo 和独立 style 入口。这种组织的真正价值是减少“设计 API、TypeScript 类型、运行时 props、文档示例”之间的漂移，而不是要求 NetHop 也建立代码生成平台。

NetHop 采用更轻的做法：每个组件以 TypeScript props/emits 类型和契约测试作为单一事实来源，设计文档只描述稳定语义，不手工复制完整 API 表；当前不引入 props 生成器。组件样式随 SFC 或显式样式模块进入构建，禁止为了模拟组件库的按需加载而建立额外 package/export 层。

这说明“共享”不等于让桌面端和移动端使用同一套组件尺寸与交互。真正共享的是命名、语义、算法和设计语言；触摸、密度、hover、弹层位置和安全区仍由平台层独立决定。NetHop 是单一 Android WebView 产品，不拆成独立 npm 包或多仓库，但应保留相同的职责分离：

```text
tokens / contracts
        ↓
composables + infrastructure
        ↓
mobile-first Vue components
        ↓
desktop browser enhancement for development/testing
```

##### 桌面端组件库值得吸收的部分

| 架构/理念 | TDesign 证据 | NetHop 吸收方式 |
|---|---|---|
| 组件变量所有权 | 每个组件通常具有 `_var.less`、`_mixin.less`、`_index.less`，组件变量由全局语义变量派生 | 每个 SFC 拥有私有 Component Token；重复视觉规则才进入共享 composable/token，不把所有样式堆回 `base.css` |
| 状态矩阵完整 | Button 明确处理 rest、hover、focus-visible、active、loading、disabled 和不同尺寸 | Primitive 在实现前登记真实状态矩阵；移动端以 pressed/focus-visible 为主，桌面浏览器补 hover |
| 排版角色而非裸字号 | `body/title/headline/display` 同时定义 size、line-height、weight | 组件消费 `font-body-*`、`font-label`、`font-title-*`，不得直接选择裸 `font-size-*` |
| 基础数值与组件尺寸分层 | `--td-size-*` 派生 `--td-comp-size-*`、padding、margin、popup padding | NetHop 保留 Primitive 数值与 Semantic control/space 之间的映射，但只定义实际使用的等级 |
| 组件依赖显式 | 组件 `_index.less` 明确导入 base、var、mixin 和所需 reset/scrollbar | Vue 组件显式 import composable/子组件；禁止通过全局 CSS 或隐式注册形成不可追踪依赖 |
| API 契约单一来源 | 公共 API 生成 props/type，框架实现消费生成结果，组件旁维护测试与 demo | TypeScript props/emits + contract test 为实现来源；不复制多份手写 API，不为当前规模引入生成器 |
| 独立组件入口 | Web/Mobile 都为组件维护独立样式入口，也提供全集入口 | NetHop 使用显式组件 import 和 SFC 样式，`index.css` 只装载 foundations/tokens，不聚合全部组件私有 CSS |
| 设计与开发共同命名 | 基础、布局、导航、数据、反馈、输入等分类保持统一语言 | `UiFoundationView`、文档和目录使用 Primitive/Form/Overlay/Navigation/Feedback/Composite 的稳定分类 |

桌面端实现中不适合 NetHop 的部分：

- 为第三方消费者提供 PBEM 全局 class、prefix、ConfigProvider 和完整 CSS 覆盖面；
- Button 的 theme × variant × ghost × shape × size 全组合；
- Dialog 的 modeless、draggable、任意挂载节点、任意宽度和桌面大屏定位；
- 面向鼠标的复杂 hover、tooltip、右键菜单和拖拽能力；
- `transition: all`。NetHop 必须只声明会变化的属性，避免布局与主题切换产生意外动画。

##### 移动端组件库值得吸收的部分

| 架构/理念 | TDesign 证据 | NetHop 吸收方式 |
|---|---|---|
| mobile-first 尺寸 | 移动 spacing 以 8/12/16/24/32 为主要节奏，Cell 默认 16px 内边距，控件使用更大的触摸区域 | NetHop 视觉密度保持紧凑，但触摸目标与视觉尺寸分离，IconButton 命中区域不得低于 touch target token |
| active 反馈优先 | 移动 Button/Cell 通过 touch/hover directive 产生 pressed 反馈，并使用 `touch-action: manipulation` | `usePress` 统一 pointer/touch/keyboard pressed 状态；不依赖仅鼠标 hover 的反馈 |
| hairline 与 inset divider | Cell 使用伪元素绘制 hairline，并通过左右 inset 避免分隔线贴边 | Divider/list owner 统一绘制分隔线，适配 DPR，严格遵循容器 padding，禁止行与容器双重绘制 |
| Popup 分层组合 | Dialog 组合 Popup，Popup 组合 Overlay、Teleport、Transition、scroll lock | NetHop 保留 `Dialog/ActionSheet → Popup/Overlay Runtime` 的单向依赖，但业务 API 不透传底层全部 props |
| 明确弹层生命周期 | Popup 区分 wrapper/inner visible，并提供 open/opened/close/closed；destroy-on-close 等待 leave 完成 | Overlay entry 使用 `closed/opening/open/closing` 状态机，资源释放和 DOM 销毁在 transition 完成后执行 |
| 嵌套弹层事件隔离 | Popup 遮罩点击阻止冒泡，避免子 Popup 关闭父 Popup | Overlay stack 只允许 topmost entry 消费 Back/Escape/backdrop，嵌套关闭必须有契约测试 |
| 滚动边界识别 | `getScrollParent`、touch direction 和 lock count 处理滚动穿透及嵌套滚动 | ScrollLock 集中维护引用计数、滚动容器边界和单一全局监听；不能由每个实例独立注册 document listener |
| 组合上下文 | Form 通过 provide/inject 下发 disabled、布局和字段注册；Button/Cell 继承 Form disabled | NetHop 只吸收 Field/RadioGroup 所需的 readonly Symbol context；不实现通用 Form rule engine |
| 原生表单语义 | Button 默认 `type=button`，Form 渲染真实 `<form>` 并使用原生 submit/reset | NetHop 保持原生 button/input/form，提交由 `nativeType=submit` 触发，不用 click 模拟 |
| 行为 hooks 独立 | `useTouch`、`useLockScroll`、`useTeleport`、`useResizeObserver`、`useElementRect` 各自承担单一职责 | 对应能力进入 composables/infrastructure，SFC 不重复实现观察器、Teleport 和滚动逻辑 |
| 组件旁验证材料 | `src` 中约有 214 个测试/快照文件和 414 个 demo 文件 | 每个基础组件必须同时提供 browser contract test 和 `UiFoundationView` 真实示例；快照只作为辅助，不能替代交互断言 |

移动端实现中不适合直接照搬的部分：

- `visible`、`modelValue`、`defaultVisible`、自定义 prop v-model 同时支持的兼容层；NetHop Primitive 默认单一受控契约，仅 Input/动画保留内部瞬态；
- `attach`、`overlayProps`、`transitionName`、任意 `zIndex`、任意 width/style 等通用组件库逃逸口；
- Dialog 内置 confirm/cancel/actions Button 配置并透传 Button 全部属性；NetHop 使用 `actions` slot 组合真实业务按钮；
- Toast 默认可以锁滚动、显示 Overlay 且插件调用会清除上一实例；NetHop Toast 非模态、不锁滚动，以 operation ID 更新；
- 通用 Form 的 rules、reset、scroll-to-error 和错误消息模板系统；NetHop 已有 schema、daemon validation、CAS 与事务模型，Field 只负责语义关联和展示；
- 仅使用 CSS `100vh` 和静态 safe-area；NetHop 还必须处理 Android WebView 的 Visual Viewport、IME 和底部栏联动。

##### NetHop 的平台决策

NetHop 的组件不是“桌面端一套、移动端一套”，而是：

```text
移动 WebView 是唯一主契约
  ├── touch / pointer
  ├── safe-area / IME / Visual Viewport
  ├── Android Back
  ├── pressed feedback
  └── compact visual + sufficient touch target

桌面浏览器是增强契约
  ├── hover
  ├── focus-visible
  ├── optional keyboard navigation
  └── development fixture / desktop E2E
```

两种环境共享核心 DOM、状态机和业务行为，不允许通过 user-agent 分叉组件实现。ARIA 属性可以作为低成本增强保留，但不是移动端完成条件。差异优先由媒体能力查询、safe-area/viewport token 和输入方式处理；只有真机证据证明必要时才增加平台分支。

#### ChengJing 本地组件实现审计与吸收边界

除主流组件库外，本设计还审计了同工作区项目 `D:/100_Projects/110_Daily/ChengJing/src/components`。该目录包含 19 个顶层组件目录、95 个 Vue 文件和 83 个 TypeScript 文件，目标目录内未直接引用 `tdesign-vue-next`，可以作为“真实业务项目手写组件”的实现样本；但该目录内未发现组件测试文件，因此不能把 API 数量、README 完整度或视觉效果直接等同于行为可靠性。

| ChengJing 模块 | 可吸收的设计 | 不吸收的设计 | NetHop 落点 |
|---|---|---|---|
| `Button` / `ButtonGroup` | 默认原生 `<button>`、默认 `type=button`、loading/disabled 阻止重复提交、图标尺寸跟随控件；`provide/inject` 适合传递真实组合上下文 | `button/a/div/span` 多态、ripple 动态 DOM、ghost/dashed/sweep 等效果、任意 gap、theme/variant/shape 全组合 | 继续保持窄 `Button` API；按钮组合由 `FormActions` 负责，当前不实现 `ButtonGroup` |
| `Container` / `List` | Header 的 icon/title/badge/actions 稳定 parts；选择状态可由独立 composable 管理；Card 只作为视觉容器时边界清楚 | Card 同时承担 tabs/loading/grid/clickable；List 自绘 radio/checkbox；ContainerGroup 同时管理网格、选择和手风琴；`value:any` | `Surface` 只负责视觉；列表维持 `List + VirtualListViewport + 领域行组件`，不建立万能 `ListItem` |
| `Dialog` | `destroyOnClose`、进入/离开生命周期、关闭原因、异步关闭拦截、`ResizeObserver` 处理内容高度变化 | 每实例监听 `document.keydown`、直接改 `body.style`、任意 z-index/class/style、Enter 自动确认、固定 `setTimeout` 销毁、内置业务按钮和业务类型 | 继续使用 Overlay Runtime；销毁跟随 transition 生命周期；危险操作不响应 Enter 自动确认；异步关闭守卫由领域提交/脏状态驱动 |
| `SelectMenu` | 定位、键盘导航、子面板行为拆成 composable；Teleport；Arrow/Home/End/type-ahead；跳过 disabled/divider；RAF 节流和碰撞修正 | hover/grid/drag、多选标签、搜索、三级子菜单、安全三角区和任意尺寸字符串；每实例监听 document/window | `AnchoredDropdown` 统一定位和生命周期；新增内部 `MenuList/MenuSection/MenuItem` 稳定 parts，不复制完整 Select 组件 |
| `Toast` | 全局单例状态、集中 timer、最大数量、同 ID 更新、timer 清理、loading 到 success/error 的状态替换 | 八种位置、任意 action callback、仅按消息文本去重、桌面宽度假设 | 使用 operation ID 作为稳定 key；限制可见数量；提供 `aria-live` 和 `status/alert` 语义；固定在移动安全区内 |
| `Tooltip` | 单例复用、timer 集中清理、定位与渲染分离、focus 触发和边界翻转 | 依赖 hover、13 个方向、指令/组件双 API、富文本内容 | 仅作为 P2 候选，用于不熟悉的 `IconButton`；首版只考虑 focus、manual 和经真机验证的 long-press/click，不阻塞 TDesign 移除 |
| `BaseSearchInput` | 原生 input、prefix/suffix parts、清除后恢复焦点、暴露 focus/blur | Escape 无条件清空、清除按钮不可聚焦、缺少 IME composition 与明确 aria-label、将搜索语义写入 Input | 保留通用 `Input`；当前 `ApplicationSearch` 继续作为领域组合，第二个真实调用点出现后再晋级 `SearchField` |
| `SettingSwitch` | 隐藏原生 checkbox 可保留原生语义 | 同时渲染完整 label 行、固定 magic number、缺少 disabled/loading/focus-visible | `Switch` 只负责布尔控件；label、说明和错误由 `Field` 组合 |
| Design Token | 大量业务样式使用 spacing/height/radius/transition/divider token，说明高覆盖率比变量数量更重要 | primitive/semantic 重名、`transition: all`、任意 gap/width/z-index/bodyStyle 逃逸口 | 坚持三层 Token、AST 门禁和显式 transition property；禁止任意视觉值 props |

从该审计得到的组合原则是：复杂组件优先拆成“稳定 parts + 少量 composable + 共享基础设施”，而不是继续增加一个大组件的 props。稳定 parts 必须属于一个明确的组合体系，例如 `MenuItem` 只服务菜单语义，不能顺手演化成列表行、设置行和 ActionSheet 行的万能基类。

同时增加以下负向约束：

- 禁止组件暴露任意 `class`、内部 selector、`style`、`zIndex`、像素 `gap/width` 作为常规扩展口；确有布局需要时必须先形成 Semantic 或 Component Token；
- 禁止每个组件实例重复注册 `document/window` 级监听器；全局事件由 Runtime 或共享单例集中治理；
- 动画后的 DOM 销毁以 Vue transition hook、`transitionend` 或明确的 reduced-motion 路径为准，不使用与 CSS 时长分离的固定 timeout；
- Dialog 不因 Enter 自动确认删除、覆盖、应用配置等危险操作；所有 dismiss 进入同一原因明确、可取消、幂等的 pipeline；
- README/API 文档不能替代 Unit、Browser、E2E 和 Android WebView 真机契约测试。

#### Behavioral Foundation 的边界

“Behavioral Foundation”不作为一个新的组件层，而拆为三个责任域：

```text
DOM / A11y
  focus、inert、keyboard、aria

Interaction Composables
  useControllable、useDisclosure、usePress、useRovingFocus

Overlay Runtime
  OverlayManager、BackDispatcher、ScrollLock、FocusRestore、Positioning
```

视觉组件只组合这些能力，不再继续抽象 `BaseOverlay → AbstractOverlay → ...` 的继承链。任何基础层都不得导入 View、store、router、daemon 或领域 DTO；领域组件也不得把业务保存、请求和 daemon 调用塞入 Primitive。

#### 组件展示与调试边界

暂不引入 Storybook、Histoire 或新的组件文档依赖。NetHop 已有 browser component tests 和 E2E 流水线，迁移期间使用真实组件 fixture、浏览器测试和开发期 `UiFoundationView` 作为组件浏览器。只有当组件数量、跨项目复用或视觉回归规模证明现有方式不足时，才重新评估独立组件文档站。

当前的开发期组件浏览器统一实现为 `UiFoundationView.vue`，访问路径为：

```text
/dev/ui-foundation
```

它是组件实验台、真机验收页和视觉回归入口，不是面向最终用户的业务页面，也不加入 AppShell 底部导航。访问应通过显式开发开关控制，例如：

```text
VITE_ENABLE_UI_FOUNDATION=true
```

不依赖 `import.meta.env.DEV` 作为唯一条件，因为 Android 调试 WebView 可能使用 production Vite 构建模式。

`UiFoundationView` 必须直接组合真实组件，不允许为实验页编写仿真控件或第二套样式。至少按以下类别展示：

```text
Primitive / Form / Overlay / Navigation / Feedback / Composite / Infrastructure
```

每个已进入目标 UI 基础层的组件至少提供默认、禁用、loading、错误、选中、dark/light、reduced-motion 和窄屏示例；交互组件必须优先展示触摸、Android Back、IME、Visual Viewport 和滚动行为。只有实际提供桌面增强交互的组件，才额外展示 hover、focus-visible 或键盘导航。领域组件不要求全部进入实验页，但基础组件不能只有代码而没有可视化验收入口。

组件实验页本身不得持有 daemon 业务状态，不替代真实页面 E2E；它只负责通过受控 fixture 验证组件契约。旧的 `FoundationView.vue` 占位页已在 P0/P1 实施中移除，不再维护第二个基础状态入口。

开发期不建立面向外部消费者的 SemVer 稳定等级；目录职责就是当前契约。只有基础设施必须标记 `Infrastructure`，尚未有真实调用点的功能标记 `Experimental`，并且 Experimental 不得进入默认页面路径。

#### 组件反模式门禁

以下行为直接视为架构违规：

- Primitive import store、router、bridge、daemon 或领域服务；
- Primitive 发出领域事件、修改父对象或自行保存配置；
- 组件自行注册全局 Back、resize、keydown 监听而不经过基础设施；
- 组件查询兄弟 DOM、依赖页面专属 class 或通过深层选择器修改其他组件；
- 页面直接消费 Primitive Token 或组件私有 Token；
- 为尚无真实调用点的未来场景增加 props、slot、variant 或密度选项。

### 3.7 项目重复结构审计与候选组件

对当前 `webui/src/views`、`webui/src/components` 和 `styles/base.css` 审计后，发现重复的不只是 TDesign 控件，还有容器、标题行、编辑底部操作区、状态提示和图标容器。以下结论以真实调用点为依据：

| 重复结构 | 当前证据 | 建议组件 | 处理决定 |
|---|---|---|---|
| 带边框、圆角、背景和内边距的非交互容器 | `service-panel`、`overview-mode`、`traffic-section`、`overview-insight-card`、`subscription-mode-panel`、`source-card`、`app-row`、`active-node-summary`、`node-card`、`settings-group-body` 等 | `Surface`（或 `Panel`） | P1 候选；只负责视觉表面、`tone`、`radius`、`padding`，不负责业务数据和点击行为 |
| 页面标题行 | `page-heading` 在 5 个页面重复，另有 `subscriptions-heading`、`applications-heading` | `PageHeader` | P1 候选；标题、说明和 actions slot；`SettingsPageHeader` 继续是设置领域组件 |
| 内容区标题行 | `section-heading`、`overview-section-heading`、`traffic-heading`、`settings-group-heading`、`editor-heading` | `SectionHeader` | P1 候选；只抽取稳定的标题/说明/actions 结构，不做几十个对齐和字号 props |
| 弹层编辑内容壳 | `.subscription-editor` 在订阅和节点编辑中复用 4 次，均包含标题、滚动内容和 `.editor-actions` | `EditorSheet` | P1 推荐；组合 `Popup + SectionHeader + FormActions`，不保存订阅或节点数据 |
| 编辑/命令操作区 | `editor-actions` 在节点、订阅编辑和导入流程重复，另有 `heading-actions`、`row-actions`、`log-actions`、`command-band` | `FormActions`；必要时 `InlineActions` | P1 先实现 `FormActions`；其它仅提取 layout token，避免建立万能 action 容器 |
| 内联表单错误 | `.form-error` 出现在节点、设置和订阅流程 | `FieldError` / `ValidationMessage` | P1；字段错误使用 `FieldError`，原始 JSON 或批量导入错误使用窄 `ValidationMessage`，不让 Button 或 Popup 处理校验 |
| 持续型状态提示 | `settings-notice`、`impact-panel`、`conflict-panel`、`SettingsStatusBanner` | `InlineNotice` | P1 候选；支持 `tone`、标题、说明和 action slot，与自动消失的 `Toast`、领域 `OperationBanner` 分开 |
| 图标背景容器 | `service-symbol`、`insight-icon`、`settings-row-icon`、`settings-status-icon` | `IconBadge` | P2 候选；统一 `size`、`tone`、圆角和背景混合色，不承担图标选择或点击行为 |
| 可点击列表行 | `SettingsRow`、`connection-row`、`source-history-row`、`single-target-row`、`app-row` | 先建立窄接口 `ListItem`/`ListItemButton` 实验组件；暂不迁移真实行 | 这些行的数据、选中态和键盘语义不同；基础组件只提供结构与原生交互，领域组件继续保留 |
| 指标卡片 | Overview 内 3 个 `overview-insight-card`，内容分别是节点质量、运行时间和资源 | `InsightCard` | 暂保留 Overview 领域组件；尚未跨页面复用，不提升到通用 UI 层 |
| 配额/进度条 | `source-quota-track` 只有订阅列表使用 | `Progress` | 暂不抽象；第二个真实调用点出现且语义一致后再实现 |
| 搜索输入 | `ApplicationSearch` 当前只有应用页面调用 | `SearchField` | 暂不抽象；先迁移到 `Input + IconButton`，出现第二个真实搜索场景后再提取 |
| 虚拟滚动视口 | `VirtualListViewport` 用于应用、节点、连接和日志 | `VirtualListViewport` | 已是技术组件，继续独立维护，不与视觉 `Surface` 或列表行合并 |

#### `Surface` 的边界

“重复盒子”优先抽象为 `Surface`，而不是无语义的 `Box`。`Surface` 只解决稳定的视觉契约：

```text
Surface
├── tone: default | muted | selected | success | warning | danger
├── radius: compact | container
├── padding: none | s | m
└── interactive: false（默认）
```

规则如下：

1. `Surface` 不设置外边距、不读取 store、不处理点击、不渲染业务标题；
2. 交互元素仍由领域组件渲染原生 `<button>`、`<a>` 或语义 section，不能把 `Surface` 变成可点击万能卡片；
3. `radius` 和 `padding` 只能映射到已有 Semantic Token，不能让页面传入任意像素值；
4. 只有视觉边界完全一致的容器才使用 `Surface`，`SettingsGroup`、`NodeCard`、`SourceCard` 等仍保留自己的业务结构；
5. 如果迁移后只剩一个调用点，删除 `Surface` 使用并保留局部样式，避免为减少几行 CSS 引入组件层。

#### 不应抽象的重复

以下重复暂时只通过 token 或 CSS 组合解决：

- 多个页面都有 `display: flex; align-items: center; gap`，不因此建立 `Flex`/`Stack` 组件；
- 多个列表都有 divider，不因此建立通用 `ListItem`；
- 多个状态都有颜色圆点，不因此建立业务状态万能组件；
- 多个页面都有 `padding: 12px`，只映射到 spacing token；
- 仅因为 TDesign DOM 结构相似而创建 `Cell`、`Card`、`FormItem` 的兼容包装。

候选组件的晋级条件：至少两个真实调用点、结构和交互契约稳定、能减少重复状态逻辑或可访问性代码，并且有独立 browser component test。仅重复颜色、边框、间距时，优先新增或修正 Design Token，不新增 Vue 组件。

#### 列表组件分层

当前项目的列表调用点包括：

| 列表场景 | 当前实现 | 列表类型 | 目标处理 |
|---|---|---|---|
| 订阅源 | `source-list` + `source-card` | 普通可选择列表 | 保留领域 `SubscriptionSourceList`/`SourceCard` 结构，使用基础 `Checkbox`/`Radio` |
| 订阅更新历史 | `source-history-list` + `source-history-row` | 普通分隔列表 | 保留领域行组件，复用 divider 和 spacing token |
| 设置分组 | `SettingsGroup` + `SettingsRow` | 分组设置列表 | 保留设置领域组件，不泛化成通用 `ListItem` |
| 应用 | `VirtualListViewport` + `app-row` | 虚拟列表 | 继续使用虚拟视口，`ApplicationRow` 负责应用 DTO 展示 |
| 节点 | `VirtualListViewport` + `node-grid-row` + `NodeCard` | 虚拟网格列表 | 继续使用虚拟视口，`NodeCard` 保留节点领域语义 |
| 连接和日志 | `VirtualListViewport` + `connection-row` / `log-row` | 虚拟异构列表 | 虚拟化和行渲染分离，不能让 `VirtualList` 知道连接或日志 DTO |
| 单订阅选择 | `single-target-row` | 弹层内选择列表 | 属于订阅领域的 `SelectionList`，不提升到基础层 |
| ActionSheet 菜单 | TDesign ActionSheet `items` | 短操作列表 | 由 `ActionSheet` 负责列表行为和 Back，不单独复制一套 ActionList |

目标分层：

```text
List（基础语义/布局容器）
  ├── ListItem（leading/content/trailing 三段结构）
  ├── ListItemButton（原生 button 行）
  └── ListSection（标题/说明分组）

VirtualListViewport（技术基础设施）
  └── virtual row slot → ApplicationRow / NodeCard / ConnectionRow / LogRow

Domain List
  ├── SubscriptionSourceList
  ├── SettingsGroup
  ├── ApplicationList
  └── NodeList
```

列表组件的具体约束：

1. `List` 如果实现，只负责语义容器、间距、divider 和空白布局，不负责数据加载、选择状态或路由；
2. 需要语义时优先使用原生 `<ul>`、`<ol>`、`<li>`；复杂交互行可以使用原生 `<button>`、`<article>` 或 `<section>`，不得为了套组件强行改成 `<li>`；
3. `ListItem` 固定抽象为三段：可选的 `leading` 装饰图标、`content` 主文本与辅助说明、可选的 `trailing` 操作区域；它只负责布局、对齐和视觉状态，不猜测右侧控件类型；`ListItemButton` 只负责原生 button 语义和交互，不访问业务状态；
4. `VirtualListViewport` 只负责可视窗口、回收、滚动和键值，不负责行的业务 DOM、选中逻辑或操作菜单；
5. 设置行、节点卡片、应用行和订阅卡片的交互、状态和可访问语义不同，当前测试阶段继续保留领域组件，不替换真实页面；
6. 列表的 loading、empty、error 使用 `PageState`，不在 `List` 内部隐藏异步状态；
7. 可选择列表必须明确选择模型：单选使用 `Radio`/selected value，多选使用 `Checkbox`/value[]，不能由列表组件猜测业务值；
8. 列表行的 key 必须来自稳定业务标识，禁止使用可变数组索引作为虚拟列表 key；
9. 普通列表达到真实性能阈值后才使用现有 `VirtualListViewport`，不能因为组件“更专业”而默认虚拟化；
10. 列表滚动容器、下拉刷新和 Overlay 滚动锁必须有明确所有权，禁止多个组件同时阻止 `touchmove` 或修改 `overflow`。

设置类列表优先组合为：

```text
Surface / 圆角分组容器
└── List
    └── ListItem
        ├── leading：功能装饰图标（可选）
        ├── content：标题 + 辅助说明
        └── trailing：Switch / IconButton / Segmented / Dropdown / Chevron
```

右侧操作由 slot 组合现有基础组件。开关、图标按钮和下拉菜单属于独立操作时使用 `ListItem`；进入二级界面时使用 `ListItemButton`，右侧 Chevron 只是装饰图标，禁止在原生 button 内嵌套 `IconButton`。基础 `List` 不提供 `switchable`、`navigable`、`dropdown` 等业务变体。

因此，项目需要的是“基础 `List` 结构 + 列表基础设施 + 领域行组件”的组合，而不是复制 TDesign 的 `ListCell` 体系。当前已在开发期实验页实现 `List`、`ListItem`、`ListItemButton` 和 `ListSection`；现有 `VirtualListViewport` 继续作为 TanStack Virtual 的技术适配器。真实订阅、设置、应用和节点页面暂不迁移，待基础组件契约稳定后再单独评估。

## 4. Overlay 基础设施

### 4.1 OverlayManager

OverlayManager 是 Overlay Runtime 的状态所有者，不只是一个关闭函数栈。每个 overlay 注册后得到一个可撤销 token，并记录结构化入口：

```ts
type OverlayEntry = {
  id: number;
  type: "dialog" | "popup" | "action-sheet" | "dropdown" | "toast";
  close: (reason: "back" | "escape" | "backdrop" | "action") => void;
  modal: boolean;
  dismissible: boolean;
  restoreFocus?: HTMLElement | null;
  element?: HTMLElement | null;
};
```

每个 overlay 必须遵循幂等生命周期：

```text
closed → opening → open → closing → closed
```

`close()` 在 `closing` 或已经 `closed` 时不得重复触发业务回调；只有 leave transition 完成后才注销 entry、释放 scroll lock、恢复 focus 和移除 inert。Toast 默认不消费 Back；dropdown 只消费自身的 Escape/Back；modal overlay 才能阻止背景交互。

```text
页面
 ├── Popup A
 │    └── Dialog B
 └── Back
      ↓
   关闭 Dialog B
      ↓
   Back
      ↓
   关闭 Popup A
      ↓
   Back
      ↓
   router.back()
```

禁止同时存在多个互相独立的 `keydown`、`nethop:back` 或 Android 返回监听器。

Back、Escape、遮罩点击和显式关闭按钮必须进入同一条 dismiss pipeline，并由最顶层可关闭 entry 决定是否消费事件。

### 4.2 Visual Viewport 与键盘

`position: fixed`、`100dvh` 和 Android WebView 键盘在不同系统版本上并不完全一致。Overlay Runtime 只注册一个 `window.visualViewport` 监听器，在支持时把 `visualViewport.height` 写入 `--visual-viewport-height`；不支持时使用 `100dvh`，再回退到 `100vh`。每个 Dialog/Popup 不得自行注册 resize 监听。

```css
.overlay-surface {
  height: 100vh;
  height: 100dvh;
  height: var(--visual-viewport-height, 100dvh);
  overflow: auto;
}
```

Input 聚焦、键盘显示、键盘收起和旋转都必须触发一次位置/可视高度重算。需要底部滚动的表单优先使用局部滚动容器，不把页面整体交给浏览器自动顶起。

### 4.3 inert 与降级策略

打开 modal overlay 时优先使用 `inert`。必须先用能力检测确认 WebView 支持情况；不支持时采用受控降级：

1. 给应用内容根节点设置 `aria-hidden="true"` 和 `pointer-events: none`；
2. 在 Backdrop 层阻止指针事件穿透；
3. 由 FocusManager 将焦点限制在当前 overlay；
4. 关闭时精确恢复原始属性，不遍历页面任意修改业务节点的 `tabindex`。

该降级只作为兼容路径，必须在项目实际支持的最低 Android WebView 上通过焦点、点击、返回键和恢复焦点测试。

### 4.4 Dialog

Dialog 只负责容器、遮罩、焦点和关闭行为，业务内容通过 slot 提供：

第一版公共契约：

```text
open / update:open（或项目统一的 v-model）
title slot
default slot
actions slot
ariaLabel / ariaDescribedby
closeOnBackdrop: boolean
closeOnEscape: boolean
destroyOnClose: boolean
beforeDismiss?: (reason) => boolean | Promise<boolean>
```

`Dialog` 的确认、取消、删除等业务语义不写死在组件内部；调用方通过 `actions` slot 组合 `Button`。组件只负责把关闭原因（`back`、`escape`、`backdrop`、`action`）交给 Overlay Runtime，并保证重复关闭幂等。

```vue
<Dialog v-model="visible" aria-label="删除节点">
  <template #title>删除节点？</template>
  <p>该操作无法撤销。</p>
  <template #actions>
    <Button variant="text">取消</Button>
    <Button variant="danger">删除</Button>
  </template>
</Dialog>
```

要求：

- `role="dialog"`、`aria-modal="true"`；
- 初始焦点按 `explicit target → first safe action/control → dialog root` 顺序选择，不自动聚焦危险操作；
- 关闭后焦点回到触发元素；
- 默认不允许遮罩误关闭危险操作；
- 返回键只关闭最上层 dialog；
- 使用 `inert` 隔离背景内容；
- Escape 与 Android Back 使用同一 dismiss pipeline；
- 编辑器存在未提交变更、提交中或异步校验时，领域层可以向 dismiss pipeline 提供异步 guard；Dialog 只等待并遵守结果，不拥有保存协议；
- Enter 不得自动触发危险主操作；普通表单提交仍由原生 `<form>` 和 `Button nativeType="submit"` 明确控制；
- DOM 销毁必须跟随 leave transition 完成或 reduced-motion 立即完成路径，不使用固定毫秒 timeout；
- 支持 `prefers-reduced-motion`。

### 4.5 Popup / ActionSheet

Popup 面向表单编辑，ActionSheet 面向短操作列表。两者不共享业务内容，但共享：

- Teleport 到应用 overlay root；
- scroll lock；
- safe-area padding；
- 过渡动画；
- BackDispatcher；
- destroy-on-close 策略。

### 4.6 Toast / NoticeBar

Toast 由 operation 状态驱动，不允许页面直接创建任意全局消息。`OperationBanner` 应改为使用结构化 operation 输入，由 Toast 负责展示和自动消失。同一 operation ID 的 `pending → success/error` 必须更新同一条消息，不能重复堆叠；timer、可见数量和卸载清理由全局单例集中管理。

Toast 必须提供 `aria-live`：普通进度与成功结果使用 `role="status"`/polite，错误结果使用 `role="alert"`/assertive。它固定在移动端安全区内，不提供任意位置、任意 HTML 和任意回调逃逸口。

## 5. 组件 API 约束

### Button

Button 是文本按钮的原生交互原语。按钮的视觉变体（`default`、`primary`、`danger`、`outline`、`text`）与尺寸是独立维度；图标独占、同一操作的图标+文本组合、图标+文本分段组合分别由 `IconButton`、`IconTextButton`、`CompoundButton` 承担。

#### 内容形态

Button 体系覆盖以下三类真实形态，但不把三种形态塞进同一个组件 API：

1. **文本 Button**：只包含可见文本，适用于保存、取消、重试、确认等明确动作；
2. **图标 Button**：只包含一个表达动作的图标，不显示文本，必须有可访问名称；对外使用 `IconButton`，视觉上通常为方形或圆形；
3. **图标+文本 Button**：由一个真实 `<button>` 承载图标和文本，使用 `IconTextButton`；两者属于同一个动作和同一个视觉盒子，支持水平和垂直排列。
4. **分段混合 Button**：由一个 `IconButton` 和一个文本 `Button` 组合成 `CompoundButton`。两部分是两个真实按钮，支持水平和垂直拼接，并分别拥有独立动作和状态。

文本 Button 四个方向都必须有 padding，不能只依赖 `min-height` 形成上下留白。Button 不接受图标专用 prop，也不负责图标区与文本区的布局。若按钮没有可见文本，必须改用 `IconButton`；若图标和文本属于同一个动作，使用 `IconTextButton`；只有需要两个独立点击区域时才使用 `CompoundButton`。

`IconTextButton` 统一管理图标的实际 SVG 尺寸，调用方不应依赖图标组件自身的 `size` 属性决定视觉大小。尺寸映射为：S `18px`、M `20px`、L `22px`。映射针对 SVG 的 CSS 宽高，因此即使图标的 `viewBox` 含有内部留白，按钮内的图标盒尺寸仍保持稳定。

只保留以下行为和尺寸 API：

```text
variant: default | primary | danger | outline | text
size: s | m | l
shape: Button/IconTextButton = rounded | pill；IconButton = rounded | pill | circle
nativeType: button | submit | reset
loading
disabled
```

底层必须渲染原生 `<button>`，`nativeType` 默认是 `button`；表单主操作使用 `submit`，不得用 click 模拟原生提交。Button 默认不强制宽度，伸展、对齐和并排关系由 `FormActions` 或页面 action container 决定。

不实现通用 `ButtonGroup`、`ButtonDropdown`、`Ghost`、`Link`、`block`、任意像素 `shape`、任意 `iconPosition` 或“图标数量”属性。形状只提供有限枚举：`rounded`（小圆角，默认 `6px`）、`pill`（最大圆角）和仅限等宽等高 `IconButton` 的 `circle`（`50%`）。需要下拉菜单时组合 `Button + AnchoredDropdown`；需要一组操作时组合 `FormActions + Button`；需要可访问图标按钮时使用 `IconButton`；需要同一操作的图标和文本时使用 `IconTextButton`；需要图标段与文本段拼接时使用 `CompoundButton`。

Button 状态矩阵必须覆盖 `rest/hover/pressed/focus-visible/disabled/loading`。`pressed` 只使用背景色、边框色或状态层表达反馈，不使用 `translate`、缩放或其它会改变按钮位置和几何尺寸的抖动动画；按钮按下前后必须保持相同的边界盒。`loading` 阻止重复提交并设置 `aria-busy`，但不得改变按钮宽高、文字位置或导致 spinner 挤压布局。

当前不默认实现 Material 风格 ripple。Ripple 可以作为后续经真机验证的增强，但不能替代原生按钮语义、`focus-visible`、禁用态和 reduced-motion 支持；首版优先采用 CSS 状态层，减少 WebView 中额外 DOM、事件监听和动画开销。所有按钮过渡必须在 `prefers-reduced-motion: reduce` 下关闭。

交互反馈参考：Material 3 [States](https://m3.material.io/foundations/interaction/states/applying-states) 将 pressed 定义为高强调状态层，并将 ripple 作为可选表现；Apple [Buttons](https://developer.apple.com/design/human-interface-guidelines/buttons) 将按钮的样式、内容和角色分离。NetHop 吸收其状态语义，但不直接复制平台组件的动画实现。

### Icon

项目当前统一使用 `@tabler/icons-vue` 提供图标，不新增图标字体、手写重复 SVG 或第二套图标库。图标使用约束如下：

- 通过组件的 `size` 和 `stroke-width` 控制尺寸与线宽，默认继承当前文字颜色；
- 装饰性图标必须设置 `aria-hidden="true"`，不能被屏幕阅读器重复朗读；
- 图标按钮必须由 `IconButton` 提供可访问名称，不能把图标名称当作用户可见文案；
- 同一语义在全局使用同一个 Tabler 图标，不在页面内临时绘制近似 SVG；
- 图标只表达视觉和状态，不承载点击、路由或 daemon 业务逻辑。

图标不单独包装成一个万能 `Icon.vue`。只有当项目出现稳定的图标尺寸映射、状态颜色或主题切换需求时，才增加窄接口的图标展示组合组件。

### IconButton

`IconButton` 是“图标 Button”的语义受限组合，不另造一套点击行为和状态机。它复用 Button 的 `variant`、`size`、`nativeType`、`loading` 和 `disabled`，只增加图标按钮必需的约束：

```text
icon
ariaLabel（必填，或由 aria-label 提供）
variant: default | primary | danger | outline | text
size: s | m | l
shape: rounded | pill | circle
nativeType: button | submit | reset
loading
disabled
```

`IconButton` 必须只接受一个图标视觉内容，不接受可见文本 slot；若业务需要图标和文本分段，必须使用 `CompoundButton`。组件的可见宽度与高度必须严格相等，并在 S/M/L 三种尺寸下通过实际几何测试；支持 `rounded`、`pill` 和 `circle` 三种有限形状，其中 `circle` 使用 `50%` 圆角且只适用于等宽等高的 IconButton。

图标按钮必须同时具备固定的外部尺寸和真实的四向内部留白，不能使用 `padding: 0`，也不能依赖 SVG 或字体图标自身的透明区域伪造留白。当前尺寸契约为：S `32×32px / 6px padding`、M `36×36px / 8px padding`、L `44×44px / 10px padding`。padding 包含在固定尺寸内，修改内部留白不得扩大按钮外部几何。视觉尺寸可以小于触摸命中区域，但命中区域必须满足 `touch-target-min`；loading 时保留原尺寸，不得因为替换图标而发生布局跳动。

图标按钮的名称来源按以下优先级处理：显式 `aria-label`、`aria-labelledby`、调用方传入的 `ariaLabel`。缺少名称时，开发期契约测试必须失败；不能以图标组件名、CSS class 或 title 猜测名称。

### CompoundButton

`CompoundButton` 对应“图标按钮 + 文本按钮”的连续组合。它由一个 `IconButton` 和一个文本 `Button` 构成，而不是在单个 `<button>` 内放置图标和文字。

```text
orientation: horizontal | vertical
size: s | m | l
iconVariant / textVariant
iconLoading / textLoading
iconDisabled / textDisabled
iconNativeType / textNativeType
iconAriaLabel
```

水平模式中图标按钮在左、文本按钮在右；垂直模式中图标按钮在上、文本按钮在下。两个按钮尺寸对齐、边框连续，只保留外侧圆角，并以相邻边框形成分隔线。组件分别发出 `iconClick` 和 `textClick`，不提供含义模糊的整体 click。若图标和文本本质上只有一个动作，应退化为文本 Button 或重新评估是否需要图标，而不是伪造两个点击区域。

### Tag

`Tag` 只承担短文本状态展示，不承担删除、下拉或业务操作。它与 Button 共享颜色、圆角和尺寸 Token，但不是可操作控件，不渲染 `<button>`，不处理点击、键盘、loading、disabled 或触摸命中区域。当前实现：

```text
tone: neutral | success | warning | danger | info
variant: soft | solid | outline
shape: rounded | pill
size: s | m
icon?: slot
```

`soft` 使用语义容器色，`solid` 使用语义强调色，`outline` 使用透明背景和语义边框；`pill` 使用大圆角。文本由内部 label 容器承载，调用方可以通过 `max-width` 控制长文本省略。图标通过 `#icon` slot 传入，组件只负责尺寸和间距。是否可选择、可关闭由业务组合组件决定；不要把 Tag 扩展成 TDesign 风格的 closable、checkable 或主题全集。

Tag 的交互组合不回写到 `Tag` 本身，当前提供三个窄职责组件：

```text
RemovableTag = Tag + compact close button
SelectableTag = button + Tag visual contract
TagGroup = multiple SelectableTag value coordination
```

`RemovableTag` 的关闭图标位于 `Tag` 的视觉边界内，通过 `Tag` 的 `#end` 插槽放置独立的紧凑原生关闭按钮。它不复用通用 `IconButton` 的 32/36px 最小高度，以免撑大 Tag；但保持相同的可访问名称、键盘焦点和按钮语义。调用方必须提供上下文明确的 `removeLabel`，组件发出 `remove` 事件。关闭按钮默认使用次要文字色，悬停和按下切换为危险色并增加轻微状态背景，让用户明确知道该区域可关闭。删除后由业务层决定如何更新数据和恢复焦点。

`SelectableTag` 直接渲染真实 `<button type="button">`，使用 `aria-pressed` 表达多选切换状态，选中态默认映射到 `solid`。它不能再嵌套另一个交互控件；需要图标时只能使用视觉 `#icon` 插槽。

`TagGroup` 当前只实现多选值协调，使用 `v-model` 传递 `string[]`，子级 `SelectableTag` 通过 `value` 接入。它只负责选择状态和布局，不实现异步保存、筛选请求或 radio/listbox 键盘模型。单选语义等真实需求出现后单独设计，不把多个语义塞进同一个 API。

### Divider

`Divider` 是低层视觉原语，只表达内容分组或列表分隔，不承担布局和业务语义。第一版支持：

```text
orientation: horizontal | vertical
variant: solid | dashed
align: start | center | end
inset: none | s | m
label?: string
```

默认渲染语义分隔元素；使用 `role="separator"` 和正确的 `aria-orientation`。水平分隔线可以通过 `label` 和 `align` 显示左、中、右对齐文字，垂直方向不渲染 label。`variant` 只提供实线和虚线两种受控样式，`inset` 只能映射到 spacing token，不能由页面传入任意像素值。

分隔线的所有权规则：

- 列表行之间的分隔线由列表容器或领域列表负责，避免每个行组件重复绘制；
- `Field`、`Dialog`、`Popup` 内部的分隔线由对应组合组件负责；
- 不使用双边框叠加制造分隔线，分隔线两端必须遵循容器 padding；
- 只有真正需要文字分组时才使用带 label 的 Divider，普通间距不使用 Divider 代替。

### InputNumber

项目中 TDesign `Stepper` 的目标组件统一命名为 `InputNumber`，因为它本质上是一个带步进操作的数值输入控件。开发期不保留 `Stepper` 兼容别名。

`InputNumber` 只负责受约束的数值编辑：

```text
modelValue: number
min?: number
max?: number
step?: number
precision?: number
disabled
readonly
name?: string
ariaLabel?: string
```

实现要求：

- 原生数值输入和加减操作必须可触摸使用；物理键盘操作仅作为桌面增强；
- 加减按钮使用 `IconButton`，边界时只禁用对应方向；
- `min`、`max`、`step` 和 `precision` 在组件层做确定性约束，业务层仍负责 schema 和事务校验；
- 空值、非法字符和超范围输入必须有明确策略，不得静默变成 `0` 覆盖用户输入；
- `modelValue` 仍为 `number`，输入过程中的空字符串只作为内部编辑状态存在，不向业务层发出伪造数字；
- 失焦、IME 完成、加减按钮和表单提交的提交时机必须有浏览器组件测试；桌面 Enter 仅在提供该增强时测试；
- 不实现滚轮调节、长按连增、货币格式化等当前项目未使用的扩展。

### Dropdown / AnchoredDropdown

`AnchoredDropdown` 是定位和面板生命周期基础设施，业务选项组件通过 slot 或窄 `options` 接口组合它。统一契约包括：

```text
placement: top | bottom | left | right（支持碰撞修正）
open / update:open
disabled
dismissible
closeOnSelect
```

必须支持 trigger/menu slot、Android Back、触摸点击外部关闭、滚动/resize 重定位和 nested panel 栈。Escape、焦点恢复和物理键盘行为仅作为桌面增强；普通菜单项使用原生 `<button>`；需要选择值时由 `OptionDropdown` 或领域下拉组件维护 `modelValue`，不复制 TDesign `DropdownMenu/DropdownItem` 的参数集合。

重复出现的菜单内部结构统一为窄 parts：

```text
AnchoredDropdown（定位、Overlay、dismiss、focus）
  └── MenuList（固定 semantic: menu | listbox，并管理 roving focus）
      ├── MenuSection（可选分组标题）
      └── MenuItem（label/description/disabled/danger/prefix/suffix）
```

`MenuList/MenuSection/MenuItem` 不独立处理 Teleport、定位、Back 或 window 监听，也不用于普通数据列表或 ActionSheet。命令菜单固定使用 `menu/menuitem`，值选择固定使用 `listbox/option`；同一实例不得动态混合两套语义。移动主路径覆盖触摸选择、disabled 与 divider 跳过；Arrow Up/Down、Home/End、type-ahead 仅在桌面增强路径提供时实现。子菜单仅在 NetHop 出现真实调用点后实现。

### Dialog / Popup / ActionSheet

三者共享 Overlay Runtime，但语义和布局不同：

| 组件 | 用途 | 默认关闭方式 | 内容模型 |
|---|---|---|---|
| `Dialog` | 确认、错误、风险提示 | 明确按钮；是否允许 backdrop 关闭由调用方声明 | 标题、描述和 `actions` slot |
| `Popup` | 表单编辑、较长内容 | 顶部关闭、完成/取消或显式 Back | 任意业务 slot，内部局部滚动 |
| `ActionSheet` | 少量短操作 | 选择一项后关闭，或取消 | `items` + 可选描述 |

三者必须统一处理 Teleport、z-index、scroll lock、safe-area、Visual Viewport、focus/inert、Back/Escape 和 `closed → opening → open → closing → closed` 生命周期。`Dialog` 不接受 HTML 字符串，`Popup` 不知道业务保存协议，`ActionSheet` 不负责路由或 daemon 调用。

### Toast / NoticeBar

两者都属于反馈，但生命周期不同：

```text
Toast      → 短时、非阻塞、自动消失的操作结果
NoticeBar  → 持续显示的上下文告警或兼容性提示，可包含一个明确操作
```

`Toast` 由结构化 operation 状态驱动，默认不消费 Back、不阻塞焦点、不锁页面滚动；同一 operation 的新状态应更新已有消息，而不是叠加无限队列。`NoticeBar` 由页面或 AppShell 持有可见状态，操作内容通过 slot 提供，不能执行任意 HTML 或 shell 命令。两者的层级必须来自 Overlay/feedback token，禁止散落硬编码 `z-index`。

### Tooltip 候选边界

Tooltip 不是移除 TDesign 的前置组件。只有不熟悉的 `IconButton` 在移动 WebView 中确实需要补充提示，且无可见标签或现有可访问名称不能满足发现性时才实现。首版只允许纯文本内容，复用共享定位和单例 timer；focus 必须可触发，long-press/click 必须先通过 Android WebView 真机验证，hover 只能作为桌面调试增强。Tooltip 不消费 Back、不锁滚动、不接受富文本，也不建立指令和组件两套 API。

### Tabs / TabBar

`Tabs` 是页面内部内容切换，`TabBar` 是 AppShell 的一级路由导航。`TabBar` 的第一版契约为：

```text
items: { value: string; label: string; icon: Component }[]
modelValue / update:modelValue
hidden
```

它必须支持当前项、图标和文字、safe-area 底部 padding、键盘可视时隐藏，以及路由切换后的状态恢复；不在 `TabBar` 内部加载页面或调用 daemon。`Tabs` 负责 Tab/TabPanel 的 ARIA 关联和键盘导航，不能把 `TabBar` 的路由语义混入其中。

### PullRefresh 边界

项目当前在 Applications 和 Nodes 使用了 TDesign `PullDownRefresh`。它不属于 Primitive，也不进入最小基础控件迁移批次，而作为独立 Gesture Feature 实现。为了保持现有功能，迁移 TDesign 前必须完成最小可用 `PullRefresh`：支持 touch、nested scroll、overscroll、loading、取消和 Android Back/键盘边界，并通过 Android WebView 真机测试。未完成真机基线前，不得在 P6 删除 TDesign 后声称下拉刷新已迁移；如产品明确取消该功能，必须同步修改页面交互和验收文档。

### PageState

使用状态模型，不设计几十个互斥 props：

```ts
type PageStateModel =
  | { type: "loading"; title?: string }
  | { type: "empty"; title: string; detail?: string }
  | { type: "error"; title: string; detail?: string }
  | { type: "warning"; title: string; detail?: string }
  | { type: "ready" };
```

### Input / Textarea

`Input` 和 `Textarea` 是原生表单控件的薄视觉层，不负责 label、说明、错误和业务保存。字段语义由 `Field` 提供。

第一版公共契约：

```text
modelValue / update:modelValue
type: text | url | search | password（Input）
size: s | m | l
variant: plain | outline
placeholder?
maxlength?
disabled
readonly
required
autocomplete?
name?
invalid
prefix?: slot
suffix?: slot
```

`Textarea` 额外支持：

```text
rows?
minRows?
maxRows?
resize: none | vertical
```

Input/Textarea 对外保持 Vue 标准 `modelValue/update:modelValue` 契约；原生 input/textarea 负责 IME 组合期间的 DOM 瞬态，不建立一个延迟回写的第二份字符串状态。必须正确处理 `compositionstart/compositionend`，不得在拼音组合期间 trim、格式化或防抖覆盖输入值。

只有真实性能测试证明父级更新造成可见闪烁时，才允许引入 `useControllable` 的临时状态，并为外部值覆盖、IME、失焦、表单 reset 和卸载建立明确同步测试。Switch 等离散控件继续由 `modelValue` 驱动，CSS transition 负责动画，不等待异步业务保存结果改变视觉位置。

必须验证 Android WebView 键盘场景：

- 键盘出现后不横向溢出；
- Popup/Dialog 不被键盘完全遮挡；
- textarea 自动扩展不导致页面抖动；
- focus 后滚动到可见区域；
- 键盘收起后布局恢复。

### Field / Form

Field 是表单语义组合，不是另一个输入控件：

```text
Field
├── FieldLabel
├── FieldControl
├── FieldDescription
└── FieldError
```

Field 通过 Symbol `provide/inject` 管理唯一 id、`aria-describedby`、`aria-invalid`、required 和 disabled 上下文；状态修改函数留在 provider，injector 只消费 readonly 状态和显式操作。Field 不保存业务配置，不调用 schema apply/validate API，只负责把控件与标签、说明和错误建立可访问关联。

当前第一版 `Field` API：

```text
label?
description?
error?
required?
disabled?
id?
```

`Input` 在 `Field` 上下文中自动继承控件 id、required、disabled 和 invalid，并把说明与错误节点合并到 `aria-describedby`。调用方仍然可以显式传入 `id`、`aria-describedby` 或 `invalid` 覆盖/增强字段状态；业务保存、schema 校验和异步状态不进入 `Field`。

### Switch / Checkbox / Radio

模型必须区分：

```text
Switch      → boolean
Checkbox    → boolean（单选框）或 CheckboxGroup value[]
Radio       → checked:boolean + 自身 value
RadioGroup  → selected value
```

RadioGroup 的 `modelValue` 是选中值，不是 boolean。移动主路径使用触摸选择；roving focus、方向键和 Tab 行为仅作为桌面增强路径，不作为 Android WebView 基础验收条件。

## 6. 组件测试策略

### 6.1 单元测试

- props、emits、v-model；
- Divider 的方向、inset 和 separator 语义；
- Input/Textarea 的 maxlength、readonly、invalid 和 IME 事件；
- InputNumber 的 min/max/step/precision、空值和边界禁用；
- loading、disabled、error；
- overlay 栈和 BackDispatcher；
- PageState 状态模型；
- dropdown 选择和键值映射。

### 6.2 浏览器组件测试

- 实际点击、触摸和 IME 操作；
- 触摸隔离和必要的语义属性；focus、aria、inert 仅在实现或桌面增强路径提供时验证；
- inert 原生路径和 fallback 路径；
- nested overlay；
- scroll lock；
- Visual Viewport resize、键盘显示和旋转；
- reduced motion；
- Popup/Dialog 动画生命周期。
- Dialog 的初始焦点、关闭原因、重复关闭幂等和焦点恢复；
- MenuList 的触摸选择、disabled/divider 跳过；桌面增强路径可选验证 Arrow Up/Down、Home/End、type-ahead 和 roving focus；
- Toast 的 operation ID 原位更新、队列上限、timer 清理和 `aria-live`；
- Divider 在列表、Field、Popup 中的两端 inset，避免出现双分隔线或贴边分隔线。

### 6.3 Token 契约测试

- 所有 `var(--*)` 引用都有定义或明确 fallback；
- Token 图不存在循环、逆层引用和同名重复定义；
- `--length-N`、`--font-size-N`、`--duration-N` 的后缀与实际值一致；
- 每个实际定义的 Semantic Token 等级序列严格单调递增；只有声明为完整七级的类别才要求 `XS < S < M < L < XL < XXL < XXXL` 全部存在；
- 组件省略 `size` 时解析为 M，且只暴露真实支持的尺寸；
- light/dark 下所有语义颜色均能解析为有效 computed value；
- 组件样式不直接消费 Primitive Token，页面样式不直接消费 Primitive 或组件私有 Token；
- 未启用 density 功能前，源码和设置模型中不得出现无效的密度选项。

### 6.4 组件契约矩阵

每个组件在实现前登记状态与测试层级。示例：

| Button 契约 | Unit | Browser | E2E |
|---|---:|---:|---:|
| 文本内容形态 | ✓ | ✓ |  |
| 图标内容形态与可访问名称 | ✓ | ✓ | ✓ |
| 混合内容：水平排列 |  | ✓ |  |
| 混合内容：垂直排列 |  | ✓ |  |
| 任意 slot 子项不改变按钮语义 | ✓ | ✓ |  |
| disabled/loading 防重复提交 | ✓ | ✓ | ✓ |
| focus-visible/keyboard（桌面增强） |  | 可选 |  |
| 原生 submit/reset | ✓ | ✓ | ✓ |
| layout/尺寸稳定 | ✓ | ✓ |  |
| loading 布局稳定 |  | ✓ |  |
| aria-busy/type | ✓ | ✓ |  |

| IconButton 契约 | Unit | Browser | E2E |
|---|---:|---:|---:|
| 只允许单个图标视觉内容 | ✓ | ✓ |  |
| 缺少可访问名称时失败 | ✓ | ✓ |  |
| 方形视觉尺寸与触摸命中区域 |  | ✓ | ✓ |
| loading/disabled 复用 Button 语义 | ✓ | ✓ |  |

| CompoundButton 契约 | Unit | Browser | E2E |
|---|---:|---:|---:|
| 由两个原生 button 构成 | ✓ | ✓ |  |
| iconClick/textClick 独立触发 | ✓ | ✓ | ✓ |
| 水平两段等高且边界连续 |  | ✓ |  |
| 垂直两段边界连续 |  | ✓ |  |
| 两段 loading/disabled 独立 | ✓ | ✓ |  |

| Dialog 契约 | Unit | Browser | E2E |
|---|---:|---:|---:|
| lifecycle/幂等 close | ✓ | ✓ |  |
| stack/Android Back | ✓ | ✓ | ✓ |
| Escape/focus/inert（桌面增强） |  | 可选 | 可选 |
| Visual Viewport/IME |  | ✓ | ✓ |
| animation/reduced motion |  | ✓ |  |

状态矩阵按组件实际能力裁剪，禁止为了“完整”给每个组件强加全部状态：Button 覆盖 `rest/hover/pressed/focus-visible/disabled/loading`；Input 覆盖 `rest/focus/readonly/disabled/invalid`；Switch 覆盖 `off/on/focus/disabled`；Tag 只在真实可选择时才增加 `selected`。

## 7. 资料与实现依据

本设计参考并核对了以下资料：

1. Vue `Transition`：<https://vuejs.org/guide/built-ins/transition.html>
   - 用于页面、Popup、Dialog 的进入/离开动画；
   - 动画状态应由组件状态控制，不在业务页面中重复实现。
2. Vue `Teleport`：<https://vuejs.org/guide/built-ins/teleport.html>
   - 用于将 overlay 渲染到应用根节点，避免被页面 `overflow` 和 stacking context 截断。
3. WAI-ARIA Dialog Modal Pattern：<https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/>
   - 用于 `role="dialog"`、`aria-modal`、初始焦点、关闭后焦点恢复和键盘关闭行为。
4. WAI-ARIA Switch Pattern：<https://www.w3.org/WAI/ARIA/apg/patterns/switch/>
   - 用于自定义开关的 `role="switch"`、`aria-checked` 和键盘交互。
5. MDN `inert`：<https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Global_attributes/inert>
   - 用于 Dialog/Popup 打开时隔离背景内容，阻止焦点和交互穿透。
6. MDN `overscroll-behavior`：<https://developer.mozilla.org/en-US/docs/Web/CSS/overscroll-behavior>
   - 用于移动端滚动边界、Popup 内滚动和 PullRefresh 冲突控制。
7. TDesign Web 尺寸阶梯：`refer/tdesign-common-develop/style/web/theme/_size.less`
   - 用于核对原始数值、组件高度、padding 和 margin 的分层映射。
8. TDesign Mobile 主题入口：`refer/tdesign-common-develop/style/mobile/theme/_index.less`
   - 用于核对 light/dark、字体、圆角、间距和特殊组件主题变量的拆分方式。
9. TDesign Mobile Button、Switch、Dialog、Cell：`refer/tdesign-common-develop/style/mobile/components/`
   - 用于核对组件独立尺寸、状态矩阵、复合组件局部 token 覆盖和 inset 分隔线。
   - `button/_var.less`、`cell/_var.less`、`popup/_var.less` 同时展示组件私有 CSS Variable fallback；NetHop 只保留实际需要的 Component Token。
10. TDesign Common 目录与命名规范：`refer/tdesign-common-develop/README.md`、`refer/tdesign-common-develop/naming.md`
    - 核对公共算法/样式与框架实现分离，以及设计、开发共同使用组件分类和名称的原则。
11. TDesign Mobile Vue Popup/Overlay：`refer/tdesign-mobile-vue-develop/src/popup/`、`refer/tdesign-mobile-vue-develop/src/overlay/`
    - 核对 Teleport、Transition、destroy-on-close、嵌套遮罩隔离和滚动穿透处理；任意 attach/z-index/overlayProps 不进入 NetHop API。
12. TDesign Mobile Vue Hooks：`refer/tdesign-mobile-vue-develop/src/hooks/`、`refer/tdesign-mobile-vue-develop/src/_util/`
    - 核对 touch direction、scroll parent、lock count、ResizeObserver、ElementRect 和 Teleport 的职责拆分；全局监听改由 NetHop Runtime 集中治理。
13. TDesign Mobile Vue Form：`refer/tdesign-mobile-vue-develop/src/form/`
    - 核对原生 form、字段注册、disabled context 和 submit/reset；NetHop 不复制通用 rules/错误模板/数据写入引擎。
14. TDesign Mobile Vue 组件测试与 demo：`refer/tdesign-mobile-vue-develop/src/*/__test__/`、`refer/tdesign-mobile-vue-develop/src/*/demos/`
    - 本地审计约有 214 个测试/快照文件和 414 个 demo 文件；NetHop 对应为 contract test + `UiFoundationView`，快照不能替代交互断言。
15. Yue Design System Token 设计：`D:/100_Projects/110_Daily/Design-System/docs/refactor/04-token-system.md`
    - 用于核对 Primitive、Semantic、Component 三层契约；其中规划性内容不能替代真实实现验证。
16. Yue Design System 实际 Token：`D:/100_Projects/110_Daily/Design-System/packages/tokens/`
    - 用于核对数字即值、排版角色、主题和 density 的真实 CSS；重复嵌套副本不纳入 NetHop 设计。
17. Yue Design System 组件架构：`D:/100_Projects/110_Daily/Design-System/docs/refactor/05-component-architecture.md`
    - 用于核对视觉维度正交、ConfigProvider、共享表单行为和复合组件基础设施；NetHop 不采用组件库消费者 API 和全局 BEM 样式策略。
18. Ant Design Customize Theme：<https://ant.design/docs/react/customize-theme/>
    - 核对 Seed/Map/Alias 派生、Component Token 隔离、dark/compact algorithm 和零运行时样式；NetHop 只吸收派生关系和组件隔离，不引入运行时主题算法。
19. Element Plus Theming：<https://element-plus.org/en-US/guide/theming.html>
    - 核对 BEM、SCSS 变量和 CSS 变量的主题组织方式；NetHop 只吸收按组件组织变量和局部 CSS 变量覆盖，不采用全局 BEM 作为内部应用公共 API。
20. Radix Primitives Introduction：<https://www.radix-ui.com/primitives/docs/overview/introduction>
    - 核对无样式行为、可组合 parts、controlled/uncontrolled 和渐进采用；NetHop 只保留行为/视觉分离原则，不新增 Radix 依赖。
21. React Aria：<https://react-spectrum.adobe.com/react-aria/>
    - 核对将键盘、ARIA、状态和自适应交互封装为 hooks 的方式；NetHop 用 Vue composables 实现相同职责，不复制 React API。
22. Floating UI `computePosition`：<https://floating-ui.com/docs/computeposition>
    - 核对 `autoUpdate`、middleware、flip、shift、size 等定位生命周期；AnchoredDropdown 统一消费该契约，普通 modal 不引入定位依赖。
23. Vue Composables：<https://vuejs.org/guide/reusability/composables.html>
    - 核对有状态逻辑抽取、组合 composable 和生命周期清理；基础行为不得复制到多个 SFC。
24. Vue Component `v-model`：<https://vuejs.org/guide/components/v-model.html>
    - 核对 Vue 3.4+ `defineModel` 及 `modelValue/update:modelValue` 契约；输入控件保留 IME 原生瞬态，不制造滞后的镜像状态。
25. Vue Provide / Inject：<https://vuejs.org/guide/components/provide-inject>
    - 核对 Symbol key、响应式注入和 provider 负责变更；Field、Overlay 上下文只注入 readonly 状态和显式操作函数。
26. ChengJing Button：`D:/100_Projects/110_Daily/ChengJing/src/components/Button/`
    - 核对原生 button、loading/disabled、图标尺寸和 provide/inject 组合上下文；拒绝多态 tag、动态 ripple DOM 和过量视觉变体。
27. ChengJing Container/List：`D:/100_Projects/110_Daily/ChengJing/src/components/Container/`
    - 核对稳定 header parts、选择 composable 和视觉 Card；拒绝万能 ContainerGroup、List 自绘选择控件和 `value:any`。
28. ChengJing Dialog：`D:/100_Projects/110_Daily/ChengJing/src/components/Dialog/`
    - 核对 destroy-on-close、异步 beforeClose、ResizeObserver 和关闭原因；将每实例监听、body 样式修改、任意 z-index、Enter 确认和固定 timeout 作为反例。
29. ChengJing SelectMenu：`D:/100_Projects/110_Daily/ChengJing/src/components/SelectMenu/`
    - 核对键盘导航、定位碰撞、Teleport、RAF 更新和 composable 拆分；只吸收 NetHop 需要的 menu parts，不复制桌面多级菜单复杂度。
30. ChengJing Toast：`D:/100_Projects/110_Daily/ChengJing/src/components/Toast/`
    - 核对全局状态、ID 更新、timer 和最大数量；补齐其缺少的移动安全区和 live region 契约。
31. ChengJing Tooltip：`D:/100_Projects/110_Daily/ChengJing/src/components/Tooltip/`
    - 核对单例、timer 和定位复用；移动 WebView 不照搬 hover、富文本和多方向 API。
32. ChengJing Search/Switch：`D:/100_Projects/110_Daily/ChengJing/src/components/BaseSearchInput.vue`、`D:/100_Projects/110_Daily/ChengJing/src/components/SettingsPanel/components/SettingSwitch.vue`
    - 核对原生输入、清除后恢复焦点和 checkbox 语义；搜索和设置行继续由领域组合承担。

本文档只定义目标组件设计。从 TDesign 迁移的当前基线、批次顺序、依赖契约、性能/安全验收和完成定义见 [`26b-webui-tdesign-removal-migration-plan.md`](./26b-webui-tdesign-removal-migration-plan.md)。
