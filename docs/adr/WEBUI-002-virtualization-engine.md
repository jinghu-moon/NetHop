# WEBUI-002：虚拟列表引擎

## 状态

Accepted，2026-08-08。

## 问题

节点、连接和日志可能达到 10,000 行。引擎必须支持固定/动态高度、按索引跳转、过滤重排和显式 cleanup，同时保持依赖面最小。

## 数据与测量

| 候选 | DOM 上界 | 动态测量 | 深度跳转 | 依赖结果 | 结论 |
|---|---:|---|---|---|---|
| TanStack Vue Virtual 3.13.35 | viewport + overscan | `measureElement` | `scrollToIndex` | 仅 `virtual-core 3.17.7` | 采用 |
| VueUse `useVirtualList` 10.7.0 | viewport + overscan | 以固定 itemHeight 为主 | 可滚动但动态测量能力较弱 | 无新增包 | 不作为通用引擎 |
| 自研窗口算法 | 可控 | 需自行实现 | 需自行实现 | 无依赖 | 测试与维护成本最高，拒绝 |

TanStack 发布包 unpacked size 为 18,928 B；当前 production 未消费列表页面，因此被 tree-shake，基线 JS gzip 仍为 47,441 B。阶段 G 首个 10,000 行页面必须重新记录实际 delta、DOM 数与交互时延。

## 选择

唯一允许的通用虚拟化依赖是 `@tanstack/vue-virtual 3.13.35` 及其精确传递依赖 `@tanstack/virtual-core 3.17.7`。

## 删除的旧路径

不引入 `@vueuse/components`、`vue-virtual-scroller` 或自研窗口引擎。

## before evidence

阶段 A 没有 WebUI 列表实现，10,000 行若直接渲染将产生 10,000 个行节点。

## RED evidence

`check-dependencies.mjs` 对第二个虚拟列表包、TanStack 版本漂移返回非零。

## after evidence

lockfile 只有 TanStack Vue adapter 与 virtual-core；生产构建不存在未使用的虚拟化 chunk。

## regression evidence

`npm run check:dependencies` 与 production bundle budget 通过。

## 回滚条件

阶段 G 的真机 WebView 10,000 行基准无法满足设计文档时，重新执行候选对照，不在页面内混用两个引擎。

## 复测命令

`cd webui && npm run check:dependencies && npm run build && npm run check:bundle`
