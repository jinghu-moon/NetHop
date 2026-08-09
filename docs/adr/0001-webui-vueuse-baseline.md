# ADR-0001: WebUI VueUse Baseline

## 状态

首版 WebUI 接受。只有取得包体和 Android WebView 实测证据后才重新评审。

## 问题

TDesign Mobile Vue 1.16.1 精确依赖 `@vueuse/core` 10.7.0，而已审计的独立参考源码版本更高。同时打入两个版本或强制执行未经验证的 override，会增加包体和兼容风险。

## 数据与测量

依赖关系和允许使用的 composable 已记录在 `docs/08-webui-design.md` 第 5.6 节。阶段 C 必须记录生产依赖图、重复包数量、组件回归结果和 Android WebView 行为。

## 选择

首版将 `@vueuse/core` 10.7.0 声明为显式直接依赖，使应用代码与 TDesign 共享同一版本。导入必须保持显式，并受设计文档的允许清单约束。

## 删除的旧路径

首版不发布第二套 VueUse、不使用 npm override、不自建生命周期监听封装，也不引入 VueUse integrations。

## before evidence

`docs/08-webui-design.md` 已在 WebUI workspace 建立前记录审计后的依赖关系。

## RED evidence

阶段 C 的依赖图测试必须在出现重复 `@vueuse/core`、`@vueuse/shared` 或 `@vueuse/metadata` 版本时失败。

## after evidence

阶段 C 将补充锁定的 `package-lock.json` 和生产构建报告。

## regression evidence

已选 TDesign 组件契约、截图和 Android WebView 测试必须保持通过。

## 回滚条件

若生产依赖图出现重复 VueUse、违反包体预算，或生命周期清理及已选组件测试失败，则回滚该依赖决定。

## 复测命令

```powershell
pwsh -NoProfile -File "scripts/test-webui.ps1" -Suite Frontend
```
