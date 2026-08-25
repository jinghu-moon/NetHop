# NetHop WebUI UI 基础层与 TDesign 迁移设计索引

> 本文件是拆分后的索引，不再重复维护组件设计和迁移计划正文。
>
> 组件分层、设计变量、Overlay、基础组件 API、列表和组合组件设计见 [`26a-webui-native-ui-foundation-design.md`](./26a-webui-native-ui-foundation-design.md)。
>
> 当前组件基线、现有组件处置矩阵、迁移批次、PullRefresh 迁移、测试、性能、安全和 TDesign 删除完成定义见 [`26b-webui-tdesign-removal-migration-plan.md`](./26b-webui-tdesign-removal-migration-plan.md)。

## 阅读顺序

```text
26a：目标架构和组件契约
  ↓
26b：从当前代码迁移到目标架构的执行计划
```

## 适用范围

- 项目处于开发期，允许破坏性重构；
- 不保留 TDesign 兼容 wrapper、别名或假 DOM；
- 现有真实功能必须通过迁移前后测试保持正常；
- `tdesign-mobile-vue` 只有在所有页面、Gesture Feature 和产物审计完成后才允许删除；
- Git 提交、推送、模块构建和设备安装不属于本文档授权范围。

## 维护规则

1. 新增组件设计先更新 `26a`，不得把目标 API 写入 `26b`；
2. 新增迁移批次、调用点、测试和完成条件先更新 `26b`，不得在 `26a` 中写迁移进度；
3. 两份文档的交叉引用必须保持有效；
4. 旧版完整文档内容不再在本文件恢复，避免三份规范产生分歧。
