# NetHop WebUI 设计方案

> 状态：Design Baseline v0.4
>
> 日期：2026-08-11
>
> 适用范围：当前 Alpha、KernelSU/APatch Module WebUI、后续 CLI 能力覆盖
>
> 上位文档：[`00-nethop-system-design.md`](./00-nethop-system-design.md)
>
> 配置契约：[`06-configuration-toml-refactor-design.md`](./06-configuration-toml-refactor-design.md)

## 1. 决策摘要

NetHop WebUI 采用以下已确认决策：

| 项目 | 决策 |
|---|---|
| 第一宿主 | KernelSU Module WebUI |
| 第二宿主 | APatch WebUI，在通过兼容性测试后与 KernelSU 同级支持 |
| Magisk | 官方 Manager 不内置同等 WebUI 宿主；保留 Action/CLI，未来由 Manager APK 统一覆盖 |
| 前端 | Vue 3 + TypeScript + Vite + Vue Router |
| 组件库 | `tdesign-mobile-vue`，显式按需导入 |
| 业务图标 | `@tabler/icons-vue`，静态命名导入 |
| Composition 工具 | `@vueuse/core`，显式按需导入并与 TDesign 依赖去重 |
| 长列表引擎 | `@tanstack/vue-virtual`，节点和应用列表的唯一虚拟化实现 |
| root bridge | `kernelsu` npm bridge |
| 后端入口 | 固定路径 `nethopctl --json/--jsonl` |
| 当前 wire | Protocol v3；schema v3；订阅 `single/merge` 与节点 requested/active 分离 |
| 实时状态 | 一个长驻 JSONL 事件流，snapshot + 单调序号 + 背压恢复 |
| 配置写入 | typed mutation 或完整事务 apply，WebUI 不直接改 TOML |
| 首版范围 | 完整日常闭环，随后覆盖全部稳定 CLI 能力 |
| 设计方向 | 安静、紧凑、移动端优先、系统明暗主题、自适应但不装饰化 |

WebUI 是 `nethopd` 控制面的一个受限客户端，不是第二个配置发布者、shell 控制台或 sing-box 面板包装器。任何写操作继续服从 daemon 的 validation、capability admission、digest CAS、generation 事务和回滚语义。

推荐数据路径：

```text
KernelSU / APatch WebView
          |
          v
Vue WebUI -> WebUiBridge -> kernelsu spawn/exec
          |                    |
          |                    v
          +------------> nethopctl --json/--jsonl
                               |
                               v
                         root-only UDS
                               |
                               v
                            nethopd
                               |
              typed validation + CAS + generation
```

## 2. 调研结论

### 2.1 NetHop 当前基础

当前代码已经具备 WebUI 所需的主要后端能力：

- `protocol.hello` 版本协商；
- `status.get`、`service.start/stop`；
- `config.get/validate/apply/reload/schema/mutate`；
- `capability.get/probe`；
- `subscription.update/import_preview/import_apply/mode_get/mode_set/select/set_enabled`；
- `node.list/test/test_all/selection_get/select_auto/select_manual/remove/export`；
- `connections.get/close/close_all`；
- `logs.get/clear`、`diagnostics.bundle`；
- `topology.get`、`traffic.get`；
- `events.subscribe` JSONL 事件流；
- snapshot、单调 `seq`、有界 ring、`resync_required` 和最多四个订阅者；
- 配置 watcher、结构化事件日志和敏感信息脱敏。

因此首版不需要新增 HTTP 服务、放宽 UDS 权限或让 WebUI 直连 Clash API。当前核心是独立 sing-box `v1.13.15` 子进程，只有 daemon 通过 loopback-only、随机 secret Clash API 查询 group、测速和切换；sing-box 1.14 原生 gRPC API service 只作为 [`11-deferred-capabilities-and-future-design.md`](./11-deferred-capabilities-and-future-design.md) 中的未来候选。

### 2.2 NetProxy-Magisk

可吸收：

- Vue 3、Vite、TypeScript 和 hash router；
- 概览、节点、应用、设置四区信息架构；
- KernelSU 应用列表、应用图标、Toast 和 edge-to-edge；
- 浏览器 mock 与移动端页面布局；
- 流量、节点、应用和日志形成日常闭环。

不吸收：

- WebUI 直接读写 `module.conf`、`tproxy.conf` 和 sing-box JSON；
- 页面直接调用 `subscription.sh`、`switch.sh` 等内部脚本；
- 拼接任意 shell 字符串；
- WebUI 直连 Clash API 并持有 API secret；
- 只改文件但不确认活动 generation 的状态表达。

### 2.3 Surfing

Surfing 的 `webroot/index.html` 仅跳转到 `http://127.0.0.1:9090/ui`。这种方式适合复用 Clash 面板，但不适合 NetHop：

- 需要开放本地 HTTP 控制面；
- UI 语义由 Clash API 决定，不能表达 NetHop 的 TOML、capability、CAS 和 generation；
- 无法覆盖多 source、应用范围、Android 接管状态和受控回滚；
- 面板与数据面 API 绑定，不能保持 daemon 单一发布者边界。

NetHop 不采用跳转式或 iframe 式面板。

### 2.4 MagicNet

可吸收：

- WebUI 到 CLI 的单一 composable/bridge；
- 写操作排队、操作锁和后台任务状态；
- 私有 payload 分块 staging、受控目录、consume-before-apply 和失败清理；
- 命令预览脱敏；
- 页面级 parser、纯函数 view model、契约测试和可访问性测试；
- 页面隐藏时停止日志、流量轮询；
- 用户操作立即获得 accepted/running/done/error 状态。

需要改进后吸收：

- MagicNet 多处依赖刷新命令和定时轮询；NetHop 应优先使用已有事件流；
- MagicNet 页面与 CLI 文本输出耦合较强；NetHop 只接受版本化 JSON/JSONL；
- MagicNet 功能密度较高；NetHop 首屏只保留日常决策信息。

### 2.5 Magisk 与 KernelSU

KernelSU 官方定义 `webroot/index.html` 为模块 WebUI 入口，并通过 WebView 提供 `exec`、`spawn`、`toast`、`moduleInfo`、`listPackages`、`getPackagesInfo`、`enableEdgeToEdge` 和 `exit` 等 API。KernelSU Manager 使用 `WebViewAssetLoader` 加载模块本地资源，并关闭普通文件访问。

APatch 官方说明其 WebUI 实现与 KernelSU 兼容，但 NetHop 仍以真实安装测试为准，不只凭文档声明稳定支持。

Magisk 官方模块规范包含 `action.sh`，但当前不提供与 KernelSU 相同的内嵌 Module WebUI 契约。首版不启动本地 HTTP server 补齐这一差异，也不把 MMRL/WebUI X 作为强依赖。Magisk 用户继续使用 Action/CLI；未来 Manager APK 复用同一 typed IPC 语义。

## 3. 设计目标

1. WebView 打开后立即呈现稳定页面骨架，并在后端 snapshot 到达后增量填充。
2. 运行状态、配置重载、订阅更新和 generation 切换通过事件驱动实时更新。
3. 只在用户可见且确有需要时采样高频流量，页面隐藏后停止高频工作。
4. 首版覆盖启停、订阅、节点、应用范围、基础设置、日志和诊断的完整日常闭环。
5. 后续页面逐项覆盖全部稳定 CLI 能力，不改变控制面边界。
6. 所有写操作显示 accepted、running、success、failed 或 conflict，不使用无法验证的乐观成功。
7. 初始包体、JS 解析、DOM 数量、事件更新和 WebView 内存有明确预算。
8. 视觉简约、信息层级清晰、适合单手操作，并跟随系统明暗主题。
9. 对长节点名、长应用名、长错误信息和窄屏设备保持稳定布局。
10. 前端无法绕过 daemon 安全不变量。

## 4. 非目标

- 不实现浏览器可访问的 TCP HTTP 管理服务。
- 不让 WebUI 连接 root-only UDS、Clash API 或 sing-box API。
- 不把 WebUI 变成任意 shell 终端。
- 不直接编辑 `nethop.toml`、generation、SQLite、规则集或订阅缓存。
- 不允许 WebUI 生成、修改或发布完整 sing-box JSON。
- 不把所有 CLI 输出当字符串解析；稳定功能必须提供 JSON schema。
- 不为 Magisk 官方 Manager 不具备的 WebUI 能力引入常驻 server。
- 不引入大型图表库、动画库、CSS-in-JS 或第二套图标库供业务页面使用。
- 不引入 TDesign 全部组件，也不使用 `app.use(TDesign)` 全量注册。
- 不 fork TDesign 只为替换其内部图标。
- 不在 `localStorage` 保存配置、URL、节点、token、日志或运行状态。

## 5. 技术栈与依赖策略

### 5.1 依赖基线

调研基线与候选版本为：

| 包 | 调研版本 | 许可证 | 用途 |
|---|---:|---|---|
| `tdesign-mobile-vue` | 1.16.1 | MIT | 移动端 UI 组件 |
| `@tabler/icons-vue` | 3.46.0 | MIT | 业务图标 |
| `@vueuse/core` | 10.7.0 基线，14.4.0 候选 | MIT | 浏览器生命周期、事件、尺寸和节流 |
| `@tanstack/vue-virtual` | 3.13.35 源码基线 | MIT | 节点、应用等大列表虚拟化 |
| `kernelsu` | 3.x | MIT，以发布包许可证为准 | Module WebUI bridge |
| `vue` | 3.x | MIT | 响应式视图 |
| `vue-router` | 4.x | MIT | hash router 与页面返回栈 |
| `vite` | 8.x | MIT | 构建与按需 chunk |
| `typescript` | 当前稳定版 | Apache-2.0 | 类型检查 |

实现时优先使用当日最新稳定版本，但不能忽略同一依赖在组件库中的精确约束。所有直接依赖和完整传递依赖必须锁入 `package-lock.json`。依赖升级通过单独变更完成，并重新执行包体、截图、Android WebView、组件契约和许可证门禁。

### 5.2 TDesign 引入规则

TDesign 官方同时支持全量注册和按需导入。NetHop 只允许显式命名导入：

```ts
import {
  Button,
  Cell,
  CellGroup,
  Dialog,
  Input,
  Switch,
  TabBar,
  TabBarItem,
} from 'tdesign-mobile-vue';

import 'tdesign-mobile-vue/es/style/index.css';
```

禁止：

```ts
import TDesign from 'tdesign-mobile-vue';
app.use(TDesign);
```

禁止全量注册的原因不是断言 tree-shaking 无效，而是全量插件入口显式遍历全部组件，增加误打包和审计不确定性。NetHop 以最终 Vite metafile 和 gzip 结果为准。

首版不增加 `unplugin-vue-components` 与 `unplugin-auto-import`。显式 import 更容易：

- 审查页面实际依赖；
- 检查每个组件的 bundle 增量；
- 避免自动 resolver 隐藏传递依赖；
- 让代码搜索直接得到组件使用范围。

如果后续页面数量导致 import 维护明显重复，再基于实际问题评估 resolver，不提前引入。

### 5.3 TDesign 依赖现实

`tdesign-mobile-vue` 包含以下传递依赖：

```text
@babel/runtime
@use-gesture/vanilla
@vueuse/core
dayjs
lodash-es
tdesign-icons-vue-next
tinycolor2
validator
```

tree-shaking 可以消除未使用组件和多数未到达代码，但不能消除被选中组件真正使用的内部依赖。例如 `Cell`、`Dialog`、`Input`、`Search`、`TabBarItem` 和 `Toast` 会使用 `tdesign-icons-vue-next` 的内部图标。

设计约束：

- Tabler 是 NetHop 页面和业务语义的唯一图标系统；
- TDesign 内部结构图标允许保留，作为组件实现细节；
- 优先使用 TDesign 已提供的 icon slot 替换可见业务图标；
- 不 patch `node_modules`，不 fork TDesign；
- CI 记录 `tdesign-icons-vue-next` 的实际产物占比，超预算时先替换具体重型组件，而不是维护全库分叉。

### 5.4 Tabler 图标规则

只允许静态命名导入：

```vue
<script setup lang="ts">
import { IconActivity, IconPlayerPlay, IconPlayerStop } from '@tabler/icons-vue';
</script>
```

禁止：

```ts
import * as TablerIcons from '@tabler/icons-vue';
```

也禁止按字符串动态查找整个图标表。导航、状态和按钮的图标映射必须在编译期显式声明。

统一规格：

| 场景 | size | stroke |
|---|---:|---:|
| 底部导航 | 22 | 1.8 |
| 普通按钮 | 18 | 1.8 |
| 紧凑工具按钮 | 17 | 1.8 |
| 页面状态 | 20 | 1.8 |
| 空状态 | 32 | 1.6 |

图标颜色使用 `currentColor`；状态不能只靠颜色表达，必须同时有文本或形状差异。

### 5.5 本地源码完整性

`refer/tdesign-mobile-vue-develop/src/_common` 对应的完整公共源码位于 `refer/tdesign-common-develop`。本次已经交叉核对组件实现、npm 元数据与公共样式源码，并确认：

- `style/mobile/index.less` 依次引入 base、theme 和 components；
- light 主题同时作用于 `:root` 与 `:root[theme-mode='light']`；
- dark 主题由 `:root.dark` 或 `:root[theme-mode='dark']` 显式启用，不自动绑定 `prefers-color-scheme`；
- 已提供颜色、背景、文本、边框、阴影、字体、间距和圆角 CSS variables；
- 圆角基线为 3px、6px、9px、12px，NetHop 默认 6px 与其体系一致；
- TabBar、Popup 和 BackTop 已处理 `safe-area-inset-bottom`；
- 未发现远程字体、`@font-face` 或运行时远程样式资源；
- 默认字体偏向 iOS/Windows，且 Cell 存在 `PingFangSC-Regular` 硬编码，NetHop 必须增加 Android 系统字体覆盖；
- 部分 Tabs、Popup、Toast 等组件使用约 300-350ms 的过渡；NetHop 只约束自有普通过渡时长，不把 reduced-motion 或 WCAG 作为发布门槛。

这些差异通过 NetHop token、窄 CSS override 和截图契约处理，不 fork TDesign Common，也不直接修改 `node_modules`。Phase W0 仍需把最终 npm 锁定版本与本地参考 commit 对齐，防止审计源码与实际产物漂移。`refer/` 只作源码审计，不参与发布构建。

### 5.6 VueUse 使用边界

引入 VueUse 的目标是复用已经正确处理 scope dispose、SSR/window guard、监听器解绑、observer 释放和 timer 清理的组合式工具，减少 WebView 生命周期代码的重复实现。它不是新的状态管理层、网络控制面或后台调度器。

TDesign Mobile Vue 1.16.1 精确依赖 `@vueuse/core@10.7.0`，并实际使用 `useEventListener`、`useWindowSize`、`useElementBounding`、`useElementSize`、`useIntersectionObserver`、`useFocus`、`useRafFn` 和 `onClickOutside`。本地 `refer/vueuse-main` 为 `14.4.0`，要求 Vue `^3.5.0`，且与 10.7.0 的包结构和依赖图不同。NetHop 不得无验证地同时打入两套 VueUse，也不得用 npm override 强行替换 TDesign 的精确版本。

Phase W0 默认把 `@vueuse/core@10.7.0` 声明为 WebUI 的直接依赖，使应用代码与 TDesign 共享同一版本。必须做三组 production bundle 与组件回归对比：

1. 应用直接依赖 10.7.0，与 TDesign 去重；
2. 应用直接依赖 14.4.0，允许包管理器保留双版本；
3. override 到 14.4.0，并运行全部已选 TDesign 组件、截图和 WebView 测试。

只有 14.4.0 在 API、包体或运行性能上有可测收益，且 override 方案通过完整兼容测试时，才通过 ADR 升级。CI 必须检查 lockfile 和 Vite metafile，禁止无意打入两份 `@vueuse/core`、`@vueuse/shared` 或 `@vueuse/metadata`。应用必须显式声明直接依赖，不能依赖 TDesign 的传递依赖偶然存在。

首版允许清单：

| composable | 用途 | 边界 |
|---|---|---|
| `useDocumentVisibility` | 页面隐藏时暂停 traffic、日志跟随和非必要查询 | 恢复后先取 daemon snapshot，不把可见性当运行状态 |
| `usePreferredColorScheme` / `useMediaQuery` | 系统明暗主题和宽屏布局 | 结果映射到根元素 `theme-mode` |
| `useEventListener` | Android 返回、visibility、resize 等监听 | 只在组件或 effect scope 内注册 |
| `useResizeObserver` | Canvas、虚拟列表和稳定容器尺寸 | callback 必须轻量，高频处理另行 throttle |
| `useDebounceFn` | 节点、应用和日志搜索 | 默认 100-150ms，提交命令不 debounce |
| `useThrottleFn` | resize、scroll、traffic 绘制合并 | 不得节流 daemon 的事务结果或错误事件 |
| `useTimeoutFn` | Toast、操作反馈和受控 UI 超时 | 不替代 bridge/daemon 命令超时 |
| `useStorage` | 主题和非敏感 UI 偏好 | 显式 key 前缀、浅对象、禁止业务状态和敏感数据 |

禁止或默认不引入：

- `useWebSocket`：NetHop 不开放 WebSocket 或 HTTP 控制面；
- `useFetch`：WebUI 不直接下载订阅，也不绕过 `nethopctl` 请求控制 API；
- `useIntervalFn` 等自动轮询替代 daemon JSONL 事件流；
- `useLocalStorage`/`useStorage` 保存配置、订阅 URL、节点、日志、digest、token 或运行状态；
- VueUse Motion、RxJS、Firebase 等 integrations 包；
- 为一个简单 getter 引入 composable，能用 Vue 原生 `computed`/`watch` 清楚表达时保持原生实现。

### 5.7 TanStack Virtual 选型与边界

`refer/virtual-main` 是 TanStack Virtual 源码快照。当前快照中 `@tanstack/vue-virtual` 为 3.13.35，发布记录对应 `@tanstack/virtual-core` 3.17.7；Vue adapter 的 peer dependency 支持 Vue `^2.7.0 || ^3.0.0`，包声明 `sideEffects: false`。NetHop 只直接依赖 `@tanstack/vue-virtual`，由锁文件固定其 core 版本，不直接引入 TanStack Query、Table、Router、Pacer 或 Devtools。

源码审计确认以下实现特征：

- Vue adapter 只负责把 options 连接到 core，使用 `shallowRef`/`triggerRef` 降低深层响应式成本，并在 effect scope dispose 时执行 virtualizer cleanup；
- 默认单列路径使用 `Float64Array` 保存 start/size，并按访问懒创建 `VirtualItem`，减少 10,000 节点时的对象分配；
- 可见范围起点通过二分查找定位，随后只向前扫描视口范围，不在每次滚动时从索引 0 累加；
- `ResizeObserver`、尺寸缓存、`pendingMin` 增量重建和稳定 `getItemKey` 支持动态高度；
- 上方条目尺寸变化时具有滚动位置补偿，避免 estimate 到实测尺寸造成视口跳动；
- 提供 `scrollToIndex`、`initialMeasurementsCache`、`takeSnapshot`、sticky `rangeExtractor`、overscan 和显式 `enabled` 等控制面；
- observer、scroll listener、RAF 和内部 timer 在 cleanup 中释放；
- 仓库基准覆盖 10,000 及以上条目、固定/动态高度和滚动压力，但主要基于 React 与桌面浏览器，不能直接证明 Android WebView 表现。

因此 NetHop 选择 TanStack Virtual 作为唯一大列表引擎，替代 VueUse `useVirtualList` 和自研虚拟滚动。选择依据是动态测量、滚动锚定、稳定 key、二分范围查找和成熟的清理语义，而不是 README 中未经本项目复测的 “60FPS” 或 “10-15kb” 宣称。

使用约束：

1. 节点列表和应用列表统一通过 `useBoundedVirtualizer` 封装，页面不直接散落 TanStack options。
2. `getItemKey` 必须返回稳定的 node ID 或 package name，禁止使用筛选后数组 index 作为长期 key。
3. 固定行高是默认快路径；长文本单行省略，详情在独立页面展开。
4. 只有确实存在展开行、多行诊断或可变辅助文本时才启用 `measureElement`，并在元素上提供正确的 `data-index`。
5. 动态高度的 `estimateSize` 取舒适范围内的偏大估计，减少首次实测后的向下修正。
6. overscan 从 4 开始，允许在 2-8 内通过真机基准调整，不暴露为用户配置。
7. 页面隐藏但组件仍挂载时使用缓存测量策略，避免 `display:none` 导致高度被测为 0；路由卸载时释放 virtualizer。
8. 筛选、排序或 source 切换后保持 stable key，并根据产品语义决定回到顶部或恢复锚点，不隐式保留错误 index。
9. `takeSnapshot` 和 scroll offset 只允许保存在内存 query cache，用于页面返回恢复；不得写入 `localStorage`。
10. 动态测量时默认不使用 smooth `scrollToIndex`。只有真机证明目标收敛稳定后才允许平滑滚动。
11. 不覆盖 `shouldAdjustScrollPositionOnItemSizeChange` 默认策略，除非有可复现的 NetHop 场景和回归测试。
12. 不使用 masonry、multi-lane、window virtualizer 或无限加载，除非后续出现明确需求；首版仅使用单列 element virtualizer。

## 6. 项目结构

建议新增：

```text
webui/
  package.json
  package-lock.json
  tsconfig.json
  vite.config.ts
  index.html
  src/
    main.ts
    App.vue
    router.ts
    styles/
      tokens.css
      theme.css
      base.css
    composables/
      useTheme.ts
      useVisibilityBudget.ts
      useEventLifecycle.ts
      useBoundedVirtualizer.ts
    bridge/
      host.ts
      nethopctl.ts
      event-stream.ts
      private-payload.ts
      mock-host.ts
    stores/
      session.ts
      runtime.ts
      config.ts
      subscriptions.ts
      nodes.ts
      applications.ts
    models/
      protocol.ts
      view-models.ts
      diagnostics.ts
    components/
      AppShell.vue
      StatusLine.vue
      MetricValue.vue
      TrafficSparkline.vue
      OperationBanner.vue
      SecretField.vue
      VirtualListViewport.vue
    pages/
      OverviewPage.vue
      SubscriptionsPage.vue
      NodesPage.vue
      ApplicationsPage.vue
      SettingsPage.vue
      ConnectionsPage.vue
      LogsPage.vue
      DiagnosticsPage.vue
      AboutPage.vue
    tests/
      fixtures/
module/
  webroot/                 # 只存构建产物，不手工编辑
```

页面不直接 import `kernelsu`，也不直接构造命令。所有宿主访问只通过 `src/bridge`，所有协议数据先通过 runtime validator，再进入 store。

## 7. 信息架构

### 7.1 一级导航

底部保留四个一级入口：

| 路由 | 标题 | 主要任务 |
|---|---|---|
| `#/overview` | 概览 | 启停、状态、当前节点、流量、快速更新 |
| `#/subscriptions` | 订阅 | source 管理、更新、节点入口 |
| `#/applications` | 应用 | all/blacklist/whitelist 与应用选择 |
| `#/settings` | 设置 | 代理、网络、路由、日志、高级、诊断 |

使用 hash router，原因是 WebUI 从模块本地资源加载，不能依赖 server history fallback。

### 7.2 二级页面

```text
/subscriptions/nodes
/subscriptions/source/:sourceId
/nodes/:nodeId
/settings/proxy
/settings/network
/settings/routing
/settings/logging
/settings/advanced
/settings/connections
/settings/logs
/settings/diagnostics
/settings/about
```

一级页面保持日常操作紧凑，完整 CLI 能力通过二级页面逐步加入，避免首页变成运维控制台。

### 7.3 Android 返回行为

- 二级页面返回其父页面；
- 打开的 Dialog/Popup 先关闭，不直接退出 WebUI；
- 一级页面返回由宿主处理退出；
- 有未提交表单时显示离开确认；
- 操作已经提交给 daemon 后，离开页面不取消后台事务；状态由事件流继续跟踪。

## 8. 视觉系统

### 8.1 总体方向

WebUI 是操作工具，不采用营销页面、沉浸式 Hero、渐变背景、装饰性光球或大量浮动卡片。

视觉原则：

- 中性背景承载信息；
- 主色只用于选中、主要操作和可交互焦点；
- 成功、警告、错误使用不同色相；
- 页面分区优先使用标题、留白、分隔线和 CellGroup；
- 卡片只用于单个指标或确实需要框定的工具；
- 禁止卡片嵌套卡片；
- 圆角默认 6px，弹窗最多 9px；
- 阴影只用于浮层，不用于普通页面区块；
- 字体使用 Android 系统字体，不打包 Web Font。

### 8.2 主题 token

NetHop 复用 TDesign 已有 light/dark、背景、文本、边框、状态色、圆角和间距 token，只增加领域语义别名，不建立重复的完整设计系统：

```css
:root {
  --td-font-family: system-ui, -apple-system, BlinkMacSystemFont, "Roboto",
    "Noto Sans CJK SC", sans-serif;
  --td-font-family-medium: var(--td-font-family);
  --nh-accent: var(--td-brand-color);
  --nh-success: var(--td-success-color);
  --nh-warning: var(--td-warning-color);
  --nh-danger: var(--td-error-color);
  --nh-surface: var(--td-bg-color-container);
  --nh-surface-muted: var(--td-bg-color-secondarycontainer);
  --nh-text: var(--td-text-color-primary);
  --nh-text-secondary: var(--td-text-color-secondary);
  --nh-border: var(--td-component-stroke);
  --nh-radius: var(--td-radius-default);
}

.t-cell__description {
  font-family: var(--td-font-family);
}

```

`useTheme` 使用 `usePreferredColorScheme` 解析 system/light/dark，并显式设置：

```ts
document.documentElement.setAttribute('theme-mode', resolvedTheme);
```

不能只依赖 `prefers-color-scheme`，因为 TDesign Common 的暗色 token 由 `theme-mode='dark'` 或 `.dark` 激活。主题偏好可以通过受限 `useStorage` 保存在 `localStorage`；配置和运行数据不得保存。NetHop 自有普通过渡控制在 120-180ms，长操作进度由真实状态驱动，不用动画掩盖等待。

### 8.3 稳定尺寸

- 底部导航高度固定，并包含 safe-area inset；
- 按钮采用 TDesign 原生 `small = 32px`、`medium = 40px` 尺寸；紧凑工具按钮固定为 32x32 CSS px；
- 开关、状态点、测速结果和流量数字使用稳定列宽；
- 节点卡或列表行使用固定最小高度；
- 长名称单行省略，详情页显示完整名称；
- 数字使用 `font-variant-numeric: tabular-nums`；
- 不使用基于 viewport width 的字体缩放。

## 9. 页面设计

### 9.1 概览

首屏按以下顺序呈现：

1. `NetHop` 标题、daemon 连接状态和更多菜单；
2. 代理主开关，显示实际 runtime state，不只显示配置意图；
3. 当前 generation、接管模式、当前节点和最近一次健康结果；
4. 当前上传/下载速度和 60 秒轻量曲线；
5. 订阅最近更新时间与“更新全部”命令；
6. 仅在 degraded、conflict、rejected 或 backoff 时显示问题摘要。

主开关至少区分：

```text
off
starting
running
stopping
degraded
fail_open_direct
backoff
```

用户切换后立即显示“正在提交”，收到 daemon accepted 后显示“正在启动/停止”，最终只由 runtime/generation 事件确认成功。

### 9.2 订阅

订阅列表显示：

- 用户名称；
- enabled；
- URL 仅显示 host 或“已配置”，默认不显示完整 URL；
- source 健康状态；
- 节点接受/重复/拒绝数量；
- 最近成功、最近尝试、last-known-good；
- 更新、编辑、排序、删除。

列表顶部使用 `single/merge` 分段控制。`single` 只允许一个已配置 source 处于 active set，source 卡片使用圆点单选；`merge` 允许多个 source，卡片使用复选开关。`single -> merge` 保留当前 source，不自动启用其他 source；`merge -> single` 在存在多个 active source 时必须显式选择目标。所有变更等待 daemon 事务结果，不在前端伪造 enabled 快照。

新增 source 只要求名称和 HTTPS URL。ID 由 daemon 生成，UI 不提供 ID 输入框。

删除最后一个有效 source 属于 destructive 操作，必须显示后果并二次确认。更新 source 不阻塞整个页面，列表行显示独立操作状态。

### 9.3 节点

节点页从订阅页和概览代理质量卡片进入，提供：

- 搜索；
- source、协议、可用性筛选；
- 独立“自动优选”控制项；
- requested manual 目标与实际 active terminal 的独立状态；
- 延迟测试与结果时间；
- 以稳定 node ID 执行 manual 选择；
- 删除/排除；
- URI 导出。

节点凭据和内部 sing-box tag 永不进入列表数据。列表 DTO 使用 `is_requested` 和 `is_active`，不得恢复含混的 `selected` 字段；active terminal 无法从受控 group snapshot 解析时显示 degraded/null，不回退第一个节点。全部测速只更新延迟；manual intent 保持不变，auto intent 下由 sing-box urltest 自行更新 active terminal。长列表使用 TanStack Virtual，由 `useBoundedVirtualizer` 统一 stable key、estimate size、overscan、空列表、滚动定位、测量缓存和测试契约。节点行默认固定高度并单行省略长名称；只有展示多行诊断或显式展开时才启用动态测量。筛选结果变化不得复用旧 index 作为身份，requested/active 状态也必须通过稳定 node ID 保持语义。

### 9.4 应用

应用页提供 all、blacklist、whitelist 三段选择：

- 通过 KernelSU/APatch API 获取包名和应用信息；
- 用户应用优先，系统应用单独筛选；
- 应用图标使用宿主 `ksu://icon/<package>` 能力，失败显示 Tabler 占位图标；
- 搜索按应用名和包名匹配；
- 大列表分批读取 package info；
- 应用列表达到虚拟化阈值后使用与节点页相同的 `useBoundedVirtualizer`，key 固定为 package name；
- 切换模式和增删应用使用 typed mutation + expected digest；
- root UID `0` 的安全默认不允许通过 UI 删除。

### 9.5 设置

设置首页只显示分类及当前摘要：

- 代理策略；
- 网络接管；
- 路由；
- 日志；
- 高级；
- 连接；
- 诊断；
- 关于。

字段 UI 优先由 `config.schema` 元数据驱动，bool 使用 Switch，enum 使用分段选择或 Picker，数字使用 Stepper/Input，集合和领域对象使用专用页面。unsupported/unavailable/conflict/experimental capability 需要明确状态和原因。

高级页不隐藏已实现参数，但默认折叠，并在 apply 前展示 `apply_impact` 与 disruption 档位。

### 9.6 日志、连接与诊断

- 日志默认读取最近有界条数，不一次加载完整文件；
- 实时日志只显示 NetHop 结构化事件，不持续 tail 原始 sing-box 凭据文本；
- 连接页按需刷新，关闭连接需要目标 ID，不拼接自由文本；
- `close_all` 属于 disruptive 操作，需要确认；
- 诊断包先显示将包含的脱敏类别，再生成文件；
- 拓扑与路由使用结构化列表，不渲染超大 SVG 图。

## 10. 前端状态模型

状态分为五层：

| 层 | 内容 | 生命周期 |
|---|---|---|
| Shell | 路由、主题、宿主能力 | WebUI 会话 |
| Snapshot | daemon 首次完整摘要 | 事件连接生命周期 |
| Event state | 按 `seq` 应用的增量 | 事件连接生命周期 |
| Query cache | 节点、连接、日志等有界查询 | 页面/会话 |
| Draft | 尚未提交的表单 | 页面 |

subscription state 额外保存 `mode + configured sources + active source set`；selection state 额外保存 `intent + requested_node_id + active_node_id + degraded reason`。两者都以 Protocol v3 snapshot/event 为事实源，不能从卡片选中状态、列表顺序或内部 core tag 反推。

禁止把 draft 直接覆盖 active state。配置页同时显示：

```text
observed_config_digest
active_config_digest
draft_base_digest
candidate_sequence
```

当事件报告 observed digest 已变化而 draft 仍基于旧 digest 时，页面进入 conflict pending，禁用直接提交并要求重新加载或人工合并。

不引入通用状态管理库。首版使用 Vue composable + `reactive`/`readonly` 实现领域 store；VueUse 只负责浏览器生命周期和通用响应式机制，不承载 daemon 业务状态。只有出现跨页面状态失控的实际证据后再评估 Pinia。

## 11. 宿主桥与命令边界

### 11.1 WebUiBridge

页面只能依赖以下窄接口：

```ts
interface WebUiBridge {
  hello(): Promise<HelloResult>;
  request<T>(operation: AllowedOperation, params?: unknown): Promise<T>;
  subscribe(kinds: EventKind[]): EventStream;
  listPackages(kind: 'user' | 'system' | 'all'): Promise<string[]>;
  getPackagesInfo(packages: string[]): Promise<PackageInfo[]>;
  toast(message: string): void;
  exit(): void;
}
```

不向页面暴露 `exec(command: string)`。bridge 内部维护 operation 到固定 executable/argument shape 的映射。

### 11.2 命令约束

- executable 固定为 `/data/adb/modules/nethop/bin/nethopctl`；
- 不接受页面提供 executable、工作目录或环境变量；
- 参数按 allowlist validator 构造；
- node/source/connection ID 只接受协议层定义的安全字符和长度；
- URL、完整配置、备份等敏感或大 payload 不进入可见命令预览；
- stdout/stderr 有界收集，超限终止子进程并返回稳定错误；
- 每类命令有超时；
- 同类写操作由前端 action lock 串行，最终串行性仍由 daemon 保证。

### 11.3 私有 payload

KernelSU `spawn` API 不提供通用 stdin 写入接口。为避免把完整 URL、TOML 或导入内容长期放入命令行，吸收 MagicNet 的受控 staging 思路，在 `nethopctl` 增加窄命令：

```text
nethopctl webui payload create <namespace> <basename>
nethopctl webui payload append <namespace> <basename> <base64-chunk>
nethopctl webui payload commit <namespace> <basename> <operation>
nethopctl webui payload remove <namespace> <basename>
```

安全要求：

- namespace 固定且数量有界；
- basename 使用 CSPRNG，字符和长度有界；
- 目录 `0700`、文件 `0600`；
- create-new，不覆盖；
- `O_NOFOLLOW | O_CLOEXEC`，拒绝 symlink、hard link 和非普通文件；
- 单 chunk 和总 payload 大小有界；
- payload 不写日志；
- commit 先取得并删除 staging 文件，再交给 operation 消费；
- WebUI 退出、命令失败和 daemon 启动时清理过期文件；
- 前端只显示 `[private-payload]`，不显示 base64；
- staging 只负责传输，所有语义仍由 daemon typed validation 决定。

base64 chunk 会短暂出现在 root 子进程参数中，因此该方案不声称抵御已经拥有本机 root 的对手。它解决的是 WebUI 日志、命令预览、持久文件和失败残留泄漏；本项目威胁模型仍不承诺对抗同级 root。若未来宿主 bridge 提供可写 stdin，再迁移为 stdin framing 并删除 chunk 参数路径。

## 12. 实时更新设计

### 12.1 启动序列

```text
render static shell
  -> detect KernelSU/APatch bridge
  -> protocol.hello
  -> start events.subscribe JSONL
  -> receive snapshot
  -> parallel bounded queries for current page
  -> ready
```

静态 shell 与 skeleton 不等待 root 命令。只有收到 hello 和 snapshot 后才启用写操作。

`useDocumentVisibility` 和 `useVisibilityBudget` 只决定前端是否维持高频消费、绘制和页面查询，不能替代 daemon event stream，也不能据此推断代理是否运行。页面隐藏时仍由 bridge 负责有序关闭或降级事件子进程；回到前台必须通过 snapshot/seq 恢复事实状态。

### 12.2 常驻事件流

通过 `kernelsu.spawn` 启动：

```text
nethopctl events --jsonl --event-kinds config,runtime,subscription,generation,network,traffic
```

当前协议缺少 `traffic` kind。WebUI 实现前允许开发期破坏性扩展 `EventKind::Traffic`，但必须保持一个事件订阅入口，不新增五套 watch API。

事件处理规则：

- 第一帧必须是 snapshot；
- 后续 `seq` 必须严格递增；
- 重复帧忽略；
- 序号跳跃、解析失败或 `resync_required` 触发重新订阅；
- 未知 kind 在协议精确匹配的 Alpha 中视为不兼容；
- 单行继续受 16 KiB 限制；
- 慢消费者不能阻塞 daemon worker。

### 12.3 高频 traffic lane

流量不能以每秒一项写入持久事件日志或占满普通事件 ring。`traffic` 在同一 JSONL 连接中使用独立的 ephemeral/coalesced lane：

- 仅当订阅者显式请求 traffic 且页面可见时启用；
- 采样默认 1 秒；
- daemon/CLI 只保留最新样本，不进入普通事件 replay ring；
- WebUI 每个 animation frame 最多提交一次绘制；
- 60 秒曲线使用固定长度 typed array/ring buffer；
- 页面隐藏立即暂停 traffic 请求；
- 回到前台先取一份 `traffic.get` snapshot，再恢复流；
- 动画不参与数据更新和状态判断。

如果宿主无法动态调整 event kinds，首版允许关闭并重建事件子进程，不允许每秒启动一次 root shell。

### 12.4 重连

事件子进程异常退出后采用有界退避：

```text
0.25s -> 0.5s -> 1s -> 2s -> 5s -> 10s
```

加入稳定 jitter，最大 10 秒。应用回到前台或用户点击重试可以立即触发一次重连。连续失败时页面保留最后 snapshot，但显著标记“状态可能已过期”，并禁用需要新 digest 的写操作。

## 13. 性能预算

### 13.1 构建产物

| 指标 | 硬门槛 | 挑战目标 |
|---|---:|---:|
| 首屏 JS gzip | <= 180 KiB | <= 140 KiB |
| 首屏 CSS gzip | <= 45 KiB | <= 32 KiB |
| 单个异步页面 chunk gzip | <= 80 KiB | <= 50 KiB |
| 完整 `webroot` | <= 2.0 MiB | <= 1.5 MiB |
| 单个 Tabler 图标平均产物增量 | 记录并审计 | 无整包导入 |
| TanStack Virtual gzip 增量 | 实测并记录 | 仅包含 Vue adapter + virtual-core 到达代码 |

TDesign 已包含完整 npm 依赖图，但验收看生产 bundle，不用 `node_modules` 大小冒充设备运行成本。CI 同时保留 metafile，列出占比最大的 20 个模块。

Phase W0 额外记录三组 VueUse 依赖图与 bundle delta：10.7.0 去重、14.4.0 双版本、14.4.0 override。默认发布方案不得包含重复 VueUse runtime；若 bundle analyzer 发现两份 `@vueuse/shared` 或 `@vueuse/metadata`，构建失败。

TanStack Virtual 必须单独记录引入前后 production bundle delta。README 的 10-15kb 只能作为上游说明，不能替代 NetHop 的 Vite 实测。metafile 中只允许 `@tanstack/vue-virtual` 和 `@tanstack/virtual-core` 到达代码，不得因生态同名自动引入其他 TanStack 包。

### 13.2 运行性能

| 指标 | 目标 |
|---|---:|
| 本地 WebView 首次可见骨架 | <= 100 ms |
| hello + snapshot 后可操作 P95 | <= 500 ms |
| 普通事件到可见状态 P95 | <= 150 ms |
| traffic 样本到曲线 P95 | <= 250 ms |
| 页面切换 P95 | <= 100 ms |
| 前台静止无 traffic 时 WebUI CPU | <= 1% 单核均值 |
| 页面隐藏后的周期 root 命令 | 0 |
| 1,000 应用搜索更新 P95 | <= 50 ms |
| 10,000 固定高度节点首次范围计算 P95 | <= 16 ms |
| 10,000 固定高度节点连续滚动 | 帧率 P5 >= 55 FPS，0 个 >= 100 ms 长任务 |
| 10,000 动态高度节点快速滚动 | 帧率 P5 >= 50 FPS，无持续空白或锚点跳动 |
| 深度跳转到第 9,000 项 P95 | <= 100 ms，最终索引准确 |

以上在 release build、真机 WebView 和固定 fixture 上验收。开发服务器性能不作为结论。

### 13.3 更新优化

- store 以稳定 ID 归一化实体；
- 事件只更新变化实体，不替换整棵大对象；
- 列表 item props 保持稳定；
- 大列表搜索使用预规范化小写字段；
- 输入搜索 100-150 ms debounce；
- 大列表统一使用 TanStack Virtual 单列 element virtualizer，overscan 由 `useBoundedVirtualizer` 限制；
- 固定行高走 estimate-only 快路径，动态测量只用于确有高度变化的条目；
- `getItemKey` 使用稳定业务 ID，筛选和排序不破坏测量缓存身份；
- 只把当前 virtual items 暴露给 Vue 模板，不复制 10,000 个响应式行对象；
- resize、scroll 和 traffic 绘制通过 `useThrottleFn` 合并，但协议事件按序完整消费；
- 页面隐藏通过 `useDocumentVisibility` 停止非必要 observer、timer 和绘制；
- traffic Canvas 不进入 Vue 大对象深度响应式；
- 时间显示由单个低频 clock 驱动，不为每行创建 timer；
- 不在模板调用重型排序、过滤或 JSON stringify；
- 路由页面使用动态 import；
- 首屏不预加载日志、连接、完整节点和完整应用信息。

## 14. 安全边界

1. 所有 WebUI 资产随模块本地分发，不从 CDN 加载 JS、CSS、字体或图标。
2. `index.html` 设置严格 CSP，默认拒绝外部资源和不必要连接。
3. 外部 URL 只通过宿主安全打开能力，不能在带 root bridge 的同一 WebView 导航加载。
4. WebUI 不提供通用 shell、文件浏览器或任意路径输入。
5. URL、密码、UUID、token、private key 和完整配置不进入日志、toast、错误详情或 analytics。
6. 不接入第三方 analytics、错误上报 SDK、远程配置或在线字体。
7. `useStorage`/`localStorage` 只保存带 `nethop.ui.` 前缀的主题、最后一级导航和非敏感 UI 偏好；禁止配置、URL、节点、日志、digest、token、operation result 和 runtime snapshot。
8. 所有响应先做 schema、大小、枚举和数组上限检查。
9. 所有写操作携带 expected digest；conflict 不自动覆盖。
10. WebUI 无权改变 protocol whitelist、SSRF、active limit、nodes-only 或网络安全不变量。
11. TDesign、Tabler、VueUse 和 TanStack Virtual 的许可证文本、版本与 SBOM 进入 release 产物。
12. 构建禁止 sourcemap 进入模块 ZIP；CI 可单独保存私有构建 artifact。

## 15. 构建与模块集成

### 15.1 Vite

关键配置：

```ts
export default defineConfig({
  base: './',
  build: {
    outDir: '../module/webroot',
    emptyOutDir: true,
    target: 'chrome105',
    sourcemap: false,
  },
});
```

TDesign 官方当前 Android Browser 基线为 105，因此首版明确要求 Android System WebView/Chrome >= 105。宿主版本不满足时显示静态不兼容页，不尝试运行半兼容 UI。目标版本后续可根据真实设备矩阵上调，但不能默默依赖更高语法。

### 15.2 模块结构

```text
module/
  module.prop
  action.sh
  service.sh
  bin/
    nethopd
    nethopctl
    sing-box
  webroot/
    index.html
    assets/
```

`module.prop` 增加：

```properties
webuiIcon=webroot/icon.svg
```

图标是仓库自有静态资产，不使用在线 URL。

### 15.3 构建顺序

```text
npm ci
npm run test
npm run typecheck
npm run build
cargo tests / Android build
stage module
generate checksums and manifest
verify webroot budget and licenses
create ZIP
```

现有 `build-android-module.ps1` 在复制 `module/` 模板前要求 WebUI release build 已完成，或由脚本显式调用独立的 `build-webui.ps1`。不允许打包陈旧 `webroot`。

build manifest 增加：

```json
{
  "webui": {
    "version": "...",
    "source_digest": "...",
    "asset_digest": "...",
    "tdesign_mobile_vue": "1.16.1",
    "tabler_icons_vue": "3.46.0"
  }
}
```

版本示例由构建时锁文件生成，不手工重复维护。

## 16. 兼容与降级

### 16.1 KernelSU

P0 宿主。必须验证：

- WebUI 入口；
- `spawn` 流式输出；
- edge-to-edge 与 safe area；
- Android 返回；
- package list/info/icon；
- Manager 切后台后的子进程处理；
- WebUI 销毁后事件进程退出。

### 16.2 APatch

通过同一套宿主契约测试后启用。任何 API 缺失必须由 `host.ts` capability 表达，不在页面散布 APatch 分支。

### 16.3 Magisk

- 模块功能、Action 和 CLI 保持完整；
- `webroot` 随 ZIP 存在不代表官方 Magisk Manager 会展示；
- 不自动启动 HTTP server；
- 不把第三方 WebUI 宿主作为模块安装前提；
- 后续 Manager APK 使用 `su -c nethopctl` 管道和相同 DTO，不复用 KernelSU 特有 JS API。

### 16.4 浏览器开发模式

提供 `MockHost`：

- 使用固定脱敏 fixtures；
- 支持 snapshot、事件、断线、重连、conflict 和失败注入；
- 不在浏览器模式执行本机 shell；
- 页面显著标记 Preview；
- mock 不伪造真机性能结论。

## 17. 测试策略

### 17.1 TDD 顺序

每个任务遵循：

```text
RED: contract/view-model/component test
GREEN: minimum implementation
REFACTOR: remove duplication and stabilize boundary
VERIFY: production build + screenshot/device check
```

### 17.2 单元与契约测试

- protocol hello 与精确版本不匹配；
- JSON/JSONL 分帧、半行、超长行、无效 UTF-8 和未知字段；
- snapshot、seq、duplicate、gap、resync；
- event reconnect 与 visibility pause；
- VueUse effect scope dispose 后无残留 listener、observer 或 timer；
- `useStorage` key allowlist、敏感数据拒绝和默认值恢复；
- CAS conflict 和 stale draft；
- operation state machine；
- URL、token、UUID 和 credential redaction；
- private payload create/append/commit/remove 与失败清理；
- source、node、application、connection ID validator；
- traffic ring buffer 和 Canvas 数据归一化；
- 10,000 节点筛选、1,000 应用搜索；
- `useBoundedVirtualizer` stable key、固定高度、动态测量、overscan 和 cleanup；
- 10,000 节点筛选/排序后 measurement identity 不串用；
- `scrollToIndex(9000)`、返回恢复、列表缩短和当前节点置顶；
- 动态高度 estimate -> measure 后无明显锚点跳动；
- 页面隐藏、`display:none`、路由卸载后无零尺寸污染或残留 observer；
- TDesign theme token 覆盖；
- system/light/dark 到根元素 `theme-mode` 映射；
- Android 系统字体覆盖；
- Tabler 禁止 namespace import 的 lint contract。

### 17.3 组件测试

- 主开关全部 runtime state；
- source 添加、编辑、排序、删除确认；
- secret URL 默认遮蔽和显式查看；
- 节点测试、选择、排除、导出；
- all/blacklist/whitelist；
- capability disabled/conflict/experimental；
- apply impact 和 destructive confirmation；
- 长文本、空状态、loading、partial success、offline；
- 返回键、焦点恢复、Dialog 和 toast。

### 17.4 视觉质量

Playwright 截图至少覆盖：

```text
360x640
393x873
412x915
600x960
```

每个尺寸覆盖 light/dark、正常/降级/错误、中文长文本。检查：

- 无重叠和横向滚动；
- TDesign 下拉、按钮与自有紧凑控件尺寸一致；
- 图标与文字中心线对齐；
- Android safe area；
- Canvas 非空且不遮挡文本。

无障碍、TalkBack、WCAG、键盘导航和 200% 文本缩放不属于 NetHop WebUI 的设计与验收范围，不得以这些指标为理由放大全部按钮或降低信息密度。NetHop 自有组件不增加 `aria-*`、显式 `role`、无障碍专用 props、reduced-motion 分支或对应测试；TDesign 内部自动生成的属性与行为不修改。可见表单标签、按钮文字和正常触控交互仍按产品功能保留。

### 17.5 真机测试

首版现实设备矩阵不要求多品牌商业级覆盖：

- 用户现有 Android arm64 设备作为发布参考机；
- KernelSU 或当前实际 root backend 至少一个真机宿主；
- APatch 在获得设备后作为兼容验证，不阻塞 KernelSU 首版；
- Magisk 验证模块功能和 Action/CLI，不虚构官方 WebUI 支持；
- Android Emulator/host tests 补充布局和故障注入，不能替代 root 真机闭环。

## 18. 分阶段实施

### Phase W0：基础与门禁

1. 建立 Vue/Vite/TypeScript 工程和 hash router。
2. 引入精确锁定的 TDesign Mobile Vue、Tabler Icons Vue、VueUse、TanStack Virtual 和 kernelsu bridge。
3. 对比 VueUse 10.7.0 去重、14.4.0 双版本和 14.4.0 override 三种 production bundle；默认冻结无重复依赖且通过 TDesign 回归的方案。
4. 建立 TanStack Virtual 与 VueUse `useVirtualList` 的 10,000 固定高度对照基准，以及 TanStack 动态高度、深度跳转和锚点稳定性真机基准；达标后删除 VueUse 虚拟列表候选路径。
5. 建立显式按需 import、TDesign token 映射、Android 字体覆盖和 bundle analyzer。
6. 建立 MockHost、protocol fixtures、Playwright 和截图门禁。
7. 接入模块 `webroot` 构建、manifest、checksums、许可证和 ZIP contract。

完成条件：静态四页 shell 可在浏览器和 KernelSU WebView 打开，满足首屏包体预算，无 root 写操作。

### Phase W1：实时日常闭环

1. hello、status 和常驻 event stream。
2. 概览、启停、generation、当前节点、流量。
3. 多 source 列表、添加、编辑、排序、删除和更新。
4. 节点列表、搜索、测速、选择、排除和导出。
5. 应用模式、package picker 和 typed mutation。
6. 基础设置、capability、validate/apply/reload。
7. 日志摘要、最近错误和诊断入口。

完成条件：用户不接触 CLI 即可完成安装后的日常代理管理，所有状态由 daemon 证据确认。

### Phase W2：全部稳定 CLI 能力

逐项覆盖：

- subscription import preview/apply；
- config export/backup/restore；
- connections list/close/close-all；
- logs get/clear；
- diagnostics bundle；
- topology；
- ruleset status/update；
- core version check；
- 完整 schema-driven 高级字段；
- 网络、路由、Wi-Fi scene 和 resource candidate 领域页面。

每项只在 CLI JSON 契约稳定后接入，不为赶 UI 解析 human text。

### Phase W3：Manager APK 复用准备

- 抽离 framework-neutral DTO fixtures；
- 固化 operation IDs、error codes 和 i18n keys；
- 复用视觉 token；
- 保留 KernelSU host adapter 与未来 Android root-shell adapter 的同一 `WebUiBridge` 语义；
- 不要求 APK 嵌入当前 WebView 代码。

## 19. 发布闸门

WebUI 进入模块发布包前必须同时满足：

- production build、typecheck、unit、component、contract 和 screenshot 全部通过；
- 无 `app.use(TDesign)`；
- 无 `import * as ... from '@tabler/icons-vue'`；
- 无重复 `@vueuse/core`/`@vueuse/shared`/`@vueuse/metadata` runtime；
- 无 `useWebSocket`、`useFetch` 或自动轮询绕过 daemon 事件流；
- 无 VueUse `useVirtualList`、自研虚拟滚动或第二个 virtualizer 并存；
- TanStack Virtual 固定/动态高度、深度跳转、筛选重排和 cleanup 真机测试通过；
- 无远程 JS/CSS/font/CDN；
- 无页面直接 import `kernelsu`；
- 无任意 shell API 暴露给页面；
- 无敏感内容进入日志、toast、localStorage 或命令预览；
- bundle 和 WebView 性能预算通过；
- event stream 断线、重连、resync 和页面隐藏测试通过；
- config/source/application 写操作通过 digest CAS 测试；
- KernelSU 真机完成“打开 -> 启停 -> 更新订阅 -> 选节点 -> 改应用范围 -> 查看状态”闭环；
- WebUI 销毁后无残留 `nethopctl events` 进程；
- license、SBOM、build manifest 和 checksums 包含 WebUI 依赖与资产。

## 20. 主要风险与处理

| 风险 | 处理 |
|---|---|
| TDesign 选中组件带入较多内部依赖 | 显式导入、metafile、逐组件 bundle delta，替换具体重型组件 |
| TDesign 与 Tabler 双图标依赖 | Tabler 负责业务图标，TDesign 内部图标视为实现细节，不 fork |
| TDesign 与应用引入不同 VueUse 版本 | 默认对齐 10.7.0，锁文件/metafile 去重门禁；升级需三组 bundle 和组件回归证据 |
| VueUse 被误用为第二控制面 | 允许清单、lint/代码审查禁止 `useFetch`/`useWebSocket`，所有业务操作仍走 bridge |
| `useStorage` 泄漏业务数据 | 仅允许 `nethop.ui.*` 非敏感偏好，增加 key 和 value 契约测试 |
| TanStack 上游基准不能代表 Android WebView | 使用 NetHop Vue DOM、目标 WebView 和真实行组件重跑固定/动态高度基准 |
| 动态测量导致滚动跳动 | 偏大 estimate、稳定 key、默认锚定策略、禁止默认 smooth scroll、截图和高速滚动测试 |
| virtualizer options 分散造成行为漂移 | 页面只能使用 `useBoundedVirtualizer`，集中固定 overscan、key、cleanup 和恢复策略 |
| WebView 版本过旧 | Chrome/WebView >=105 admission，不满足时显示静态说明 |
| 高频流量填满事件 ring | traffic 使用独立 coalesced ephemeral lane，不持久化 |
| WebUI 退出后流进程残留 | host lifecycle 关闭子进程，daemon 有订阅者回收测试 |
| source URL 进入命令行 | 私有 payload 分块 staging、命令预览脱敏、consume-before-apply |
| 页面成为第二配置发布者 | 所有写操作只调用 typed daemon transaction |
| 大节点/应用列表卡顿 | TanStack Virtual、稳定 key、固定高度快路径、有界 overscan 和真机滚动预算 |
| 功能过多破坏简约性 | 四个一级入口，全部 CLI 能力放二级领域页 |
| Magisk 用户误以为官方可打开 WebUI | 文档明确宿主差异，Action/CLI 保持完整 |

## 21. 参考资料

### 21.1 本地参考

```text
refer/NetProxy-Magisk/src/webui
refer/Surfing/webroot/index.html
refer/MagicNet-main/webui
refer/MagicNet-main/crates/magicnet-cli/src/webui_payload.rs
refer/Magisk
refer/KernelSU/website/docs/zh_CN/guide/module-webui.md
refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/webui
refer/tdesign-mobile-vue-develop
refer/tdesign-common-develop
refer/tabler-icons-main/packages/icons-vue
refer/vueuse-main
refer/virtual-main/packages/vue-virtual
refer/virtual-main/packages/virtual-core
refer/virtual-main/benchmarks
```

### 21.2 官方网页

- KernelSU Module WebUI: <https://kernelsu.org/guide/module-webui.html>
- KernelSU JavaScript API: <https://github.com/tiann/KernelSU/blob/main/js/README.md>
- Magisk Developer Guides: <https://topjohnwu.github.io/Magisk/guides.html>
- APatch Module Guide: <https://apatch.dev/apm-guide.html>
- APatch WebUI FAQ: <https://apatch.dev/faq.html>
- Android WebView local content: <https://developer.android.com/develop/ui/views/layout/webapps/load-local-content>
- Android WebView native bridge security: <https://developer.android.com/privacy-and-security/risks/insecure-webview-native-bridges>
- TDesign Mobile Vue: <https://tdesign.tencent.com/mobile-vue/getting-started>
- TDesign Mobile Vue npm: <https://www.npmjs.com/package/tdesign-mobile-vue>
- Tabler Icons Vue npm: <https://www.npmjs.com/package/@tabler/icons-vue>
- VueUse: <https://vueuse.org/>
- VueUse GitHub: <https://github.com/vueuse/vueuse>
- TanStack Virtual: <https://tanstack.com/virtual/latest>
- TanStack Virtual GitHub: <https://github.com/TanStack/virtual>
- Vue performance: <https://vuejs.org/guide/best-practices/performance>
- Vite production build: <https://vite.dev/guide/build>

## 22. 最终结论

NetHop WebUI 不复用 Clash 面板，不直接编辑模块文件，也不增加本地 HTTP 控制面。它以 KernelSU/APatch 本地 WebView 为宿主，以 TDesign Mobile Vue 和 Tabler Icons Vue 构建移动端界面，以 VueUse 复用有边界的浏览器生命周期、事件和尺寸能力，以 TanStack Virtual 统一处理节点与应用长列表，以 `nethopctl` JSON/JSONL 作为唯一 root bridge，并继续由 `nethopd` 完成所有安全校验和事务发布。

实时性由常驻事件流和 coalesced traffic lane 提供，VueUse 只负责可见性与资源消费生命周期，不靠每秒启动 root 命令；高性能由按需组件、依赖去重、路由懒加载、TanStack 单列虚拟化、稳定实体更新和严格 bundle/真机预算保证；轻量由显式依赖边界和不引入 HTTP server、图表库、动画库、通用状态库或其他 TanStack 生态包保证；简约美观由四区导航、TDesign 移动端交互、Tabler 一致图标和与 TDesign token 对齐的 NetHop 语义主题共同完成。
