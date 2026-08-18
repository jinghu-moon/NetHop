# NetHop 借鉴 NetProxy 改进 TDD 任务清单

> 状态：A-H 完成；真机结果为非配对公网观察性基线，不替代发布级受控端点基线
>
> 日期：2026-08-17
>
> 设计来源：[`23-netproxy-borrowed-improvements-design.md`](./23-netproxy-borrowed-improvements-design.md)

## A. Provider/Catalog 热重载

- [x] RED：订阅内容变化不得无条件停止正在运行的核心。
- [x] GREEN：稳定活动配置、SIGHUP、健康检查后 generation commit。
- [x] GREEN：reload 不支持或策略变化回退完整激活。
- [x] REFACTOR：失败恢复旧配置并再次 reload，保留原 generation。

## B. 统一 CI

- [x] Rust workspace test/fmt/clippy。
- [x] WebUI typecheck/unit/browser/build gate。
- [x] Companion JVM/lint/instrumentation compile。
- [x] 模块与 Android build provenance contracts。
- [x] 单一 required 聚合 job。

## C. 订阅与配置工作流

- [x] 高级订阅字段 typed model、daemon mutation、WebUI 编辑。
- [x] SQLite source update history 与 WebUI 最近记录。
- [x] 受控 JSON 编辑拒绝重复 key、非 object 和超过 1 MiB 文档。
- [x] JSON 保存继续经过 schema/auditor/CAS/generation transaction。
- [x] 修正 `config/config-mutate` namespace-operation 配对，三层 fail closed。

## D. daemon-owned 节点编辑

- [x] `NodeOverrideStore` 私有、有界、严格 schema。
- [x] candidate replay 保持 stable ID 与 source attribution。
- [x] typed IPC：get/apply/remove。
- [x] 私有 payload：`node/node-override-apply`。
- [x] WebUI 编辑、保存、恢复订阅原值。
- [x] Companion 精确命令白名单。
- [x] 浏览器 mock 完整生命周期。
- [x] apply 幂等，不重复 rebuild。
- [x] core check 失败不写 registry、不推进 generation。
- [x] Worker generation 2/3 apply/remove 端到端测试。
- [x] 凭据不进入 Debug、argv 和 mutation response。

## E. Companion 轻量分层

- [x] 保留 `control / tile / webui / packages` 功能边界。
- [x] 增加 `NetHopCompanionApplication -> CompanionServices` 组合根。
- [x] RootCommandExecutor 只由组合根创建。
- [x] Activity-scoped PackageRepository，不提升为常驻全局快照。
- [x] TileService/WebUiEntryActivity 只负责生命周期和呈现。
- [x] JVM、lint、assembleDebug 通过。

## F. eBPF PoC admission

- [x] RED：缺少 BPF/cgroup/core/TC 任一事实必须拒绝。
- [x] GREEN：本机与共享网络 scope 分离。
- [x] GREEN：稳定、无运行态误导的诊断码。
- [x] REFACTOR：不接入配置、generation、自动回退或发布声明。

## G. 数据面真机基线

- [x] 冻结 `nethop-data-plane-benchmark-v1`。
- [x] 验证 TPROXY/TUN 精确模式集合与每模式至少 5 轮。
- [x] 验证 ready、P95、吞吐、CPU、RSS、功耗、更新中断和 raw digest。
- [x] CI 执行正反 fixture contracts。
- [x] 获得启停代理与切换 capture mode 的明确授权。
- [x] 在同一 alioth 构建上采集 TPROXY 五轮。
- [x] 在同一 alioth 构建上采集 TUN/gvisor 五轮。
- [x] 生成真实 evidence、10 份 raw artifact 和 summary，冻结 validator 通过。
- [x] 根据证据保持 TPROXY 默认、热点/USB experimental、eBPF 仅 PoC admission；公网非配对 workload 不用于宣称性能优胜。

## H. 完成门槛

- [x] `cargo test -p nethop-protocol --tests`
- [x] `cargo test -p nethopctl --tests`
- [x] `cargo test -p nethopd --tests --features subscription-update`
- [x] `cargo clippy -p nethopd --all-targets --features subscription-update -- -D warnings`
- [x] WebUI 109 unit、12 browser、production build。
- [x] Companion JVM/lint/debug APK。
- [x] eBPF PoC contracts 与 data-plane evidence contracts。
- [x] `cargo fmt --all -- --check`。
- [x] M010-M014 发布证据摘要链与当前 `Cargo.lock`、订阅源码一致，14 项契约通过。
- [x] 真机数据面 evidence 完成并归档；发布级 paired direct 基线作为后续独立性能阶段。
