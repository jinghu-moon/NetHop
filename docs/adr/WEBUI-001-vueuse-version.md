# WEBUI-001：VueUse 版本选择

## 状态

Accepted，2026-08-08。

## 问题

TDesign Mobile Vue 1.16.1 将 `@vueuse/core` 10.7.0 声明为精确运行时依赖。WebUI 需要在复用 VueUse 的同时避免双 runtime，并且不能以 override 强迫 TDesign 运行在未经其发布契约验证的版本上。

## 数据与测量

| 方案 | lockfile 结果 | TDesign 契约 | 构建结果 | 结论 |
|---|---|---|---|---|
| 直接依赖 10.7.0 | `npm ls` 只有一份 core/shared 10.7.0 | 精确匹配 | 基线应用 JS gzip 47,441 B | 采用 |
| 直接依赖 14.4.0 | 10.7.0 与 14.4.0 双 runtime | TDesign 仍使用 10.7.0 | 必然增加运行时与审计面 | 拒绝 |
| override 到 14.4.0 | 单 runtime，但修改第三方解析结果 | 超出 TDesign 1.16.1 已验证范围 | 组件回归责任转移到 NetHop | 拒绝 |

测量命令：`npm ls @vueuse/core @vueuse/shared`、`npm run build`、`npm run check:dependencies`、`npm run check:bundle`。

## 选择

锁定 `@vueuse/core = 10.7.0`。升级只允许与 TDesign 依赖契约共同评审，不使用 npm override。

## 删除的旧路径

不保留 VueUse 14.4 双版本 lockfile、override 或兼容 adapter。

## before evidence

阶段 A 无 WebUI runtime 依赖。

## RED evidence

`check-dependencies.mjs` 对重复 VueUse 路径、非 10.7.0 版本和 override 返回非零。

## after evidence

当前 lockfile 仅含 `node_modules/@vueuse/core@10.7.0` 与 `@vueuse/shared@10.7.0`。

## regression evidence

Vitest Browser 的 TDesign Button 真实浏览器用例和 Playwright production smoke 通过。

## 回滚条件

TDesign 发布新稳定版并改变精确 VueUse 依赖，且 Android WebView 回归和 bundle 预算全部通过。

## 复测命令

`cd webui && npm ci --ignore-scripts && npm run gate`
