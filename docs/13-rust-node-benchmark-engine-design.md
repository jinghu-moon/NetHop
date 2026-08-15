# NetHop Rust 节点测速与自动选优引擎设计

> 状态：主架构、逐节点实时反馈及“2-3 秒快速选优 + 同轮后台续测”已完成 host 实现与自动化门禁；Android 快速切换 SLA 待新模块真机验收
> 日期：2026-08-12
> 目标平台：Android arm64 Root 模块
> 当前核心基线：独立进程 `sing-box v1.13.15` + IPv4 loopback Clash API
> 影响范围：`nethop-core`、`nethopd`、`nethop-protocol`、`nethopctl`、WebUI、模块构建与真机测试
> 上位约束：`01-performance-budget-and-slo.md`、`10-subscription-selection-and-node-optimization-refactor-design.md`
> 被替代契约：D10 9.2 的两阶段 group delay、D12 H014/N006 的 25/30 秒即时测速路径

> D16 覆盖说明：本文测速引擎、cutoff、tolerance 和 selector 事务仍是当前实现；示例中的 Protocol v3 `active_node_id` 仅是当时的输出形状。当前生产输出使用 Protocol v5 与 selection snapshot v2 typed `active_terminal`，详见 D14/D15/D16。

## 1. 文档目的

当前 `node test-all` 对 27 个节点约需 14 秒。用户触发的“全部测速”应在 5 秒内给所有当前自动候选一次探测机会，并在 auto 模式下使用同一轮结果完成选优，而不是等待 sing-box 固定并发 10 的两阶段 group delay。

本文冻结以下边界：

1. Rust 只调度 sing-box 单终端节点 delay API，不实现 VLESS、VMess、Trojan、Shadowsocks、Hysteria2、TUIC、AnyTLS 等协议握手；
2. 所有真实连接和测速请求仍由 sing-box terminal outbound 发出；
3. nethopd 负责有界并发、统一绝对截止时间、结果汇总和 auto 选择；
4. 开发期直接移除 `selector -> urltest -> terminal` 嵌套，改为单一受控 selector，不保留旧配置、旧 IPC 或旧运行时兼容层；
5. auto intent 在证据充分时于 2-3 秒内提前应用一次 selector，未完成 probe 仍在同一 operation 内继续到 4.5 秒；
6. 重构前冻结有效用户行为，重构后同时证明旧功能未退化和新 SLA 生效。

## 2. 当前事实

### 2.1 NetHop 当前链路

```text
WebUI / CLI
  -> nethopd NodeTestAll
  -> GET /group/nethop-select/delay?timeout=10000
  -> GET /group/nethop-auto/delay?timeout=10000
  -> GET /proxies
  -> 返回节点延迟和 active terminal
```

当前上层超时为 CLI 25 秒、WebUI 30 秒。第一阶段给所有 terminal 写共享 URLTest history；第二阶段试图让 `nethop-auto` 执行 tolerance-aware 选举。真机 27 节点完整调用约 13.99 秒，说明行为正确但不满足交互时延要求。

### 2.2 sing-box `v1.13.15` 源码结论

本地固定源码给出以下确定事实：

- selector group delay 与 URLTest group 均固定 `batch.WithConcurrencyNum(10)`；
- selector group delay 的所有 terminal 共用请求 context，能够继承 `timeout=`；
- URLTest group 在每个任务内重新创建 `context.WithTimeout(g.ctx, C.TCPTimeout)`；
- `g.ctx` 是核心长期 context，`C.TCPTimeout` 为 15 秒，因此 URLTest group 不继承 Clash HTTP 请求 deadline；
- URLTest group 的 `performUpdateCheck()` 没有独立 API；
- 单节点 `/proxies/{tag}/delay` 会通过真实 outbound 完成握手，并更新共享 history；
- 单节点测速不会更新 URLTest group 内部的 `selectedOutboundTCP/UDP`。

因此，只把 NetHop 客户端超时降到 5 秒会产生“调用方已经超时，核心仍在后台测速”的半取消状态；只并发调用单节点 API又不能刷新旧 `nethop-auto` 的 active child。两个问题必须一起消除。

### 2.3 参考项目

| 项目 | 实现 | 结论 |
|---|---|---|
| MagicNet | Rust CLI 逐节点串行调用单节点 delay；默认最多 16 个；单节点 5 秒，curl 外层 7 秒 | 简单但不满足整组 5 秒 SLA |
| NetProxy-Magisk CLI | shell 逐节点串行调用单节点 delay | 节点数线性放大总耗时 |
| NetProxy-Magisk WebUI | 每批 5 个并行、批次串行；单节点 5/6 秒超时；批次间等待 100ms | 比串行快，但 27 节点仍可能超过 30 秒 |
| FlClash | Flutter 每批最多 100 个 `Future`；Go/Mihomo bridge 使用并发上限 50 的 batch；单节点结果通过 callback 立即写入 `delayMap` | 网络层并不比 NetHop 的 64 全并发更激进，主观更快来自逐节点即时回填 |
| NetHop 当前实现 | sing-box group delay 固定并发 10，且执行两阶段调用 | 能刷新 auto，但真机约 14 秒 |

上述项目均没有提供“最多 64 个候选、整轮 5 秒内收敛”的实现，可以吸收其单节点 API 边界，不能照搬其调度方式。

### 2.4 FlClash 调用链结论

本地 `refer/FlClash-main` 的实际调用链为：节点页把当前代理组按 100 个分批并 `Future.wait`，单节点请求进入 Go bridge 的 `handleAsyncTestDelay`，再由全局 `batch.WithConcurrencyNum(50)` 调用 Mihomo `proxy.URLTest`。每个 callback 完成后立即更新前端 `delayMap`；慢节点仍可占满单节点 5 秒超时。

因此 FlClash 的“快”主要是 **time-to-first-result** 和 **持续可见进度**，不是整轮网络探测更早结束。NetHop 保留 64 candidates 同时启动、4.5 秒共同 cutoff 和 4.9 秒 daemon deadline，只吸收逐节点完成即发布结果的交互优点。禁止为追求动画速度缩短有效探测窗口、重复测速或恢复批次串行。

### 2.5 Android probe 分布证据

2026-08-15 在同一 generation、64-candidate auto pool 连续采集三轮：

| 指标 | 第 1 轮 | 第 2 轮 | 第 3 轮 |
|---|---:|---:|---:|
| ADB+CLI wall | 5174ms | 5014ms | 4846ms |
| 2 秒内真实完成 | 50 | 46 | 43 |
| 3 秒内真实完成 | 59 | 52 | 49 |
| cutoff 前真实完成 | 59 | 61 | 55 |
| cutoff pending | 5 | 3 | 9 |
| cutoff tail | 1524451us | 306859us | 847385us |

多数候选在 2-3 秒内已有结果，完整 4.5 秒主要用于等待 3-9 个尾部候选。这支持把“可以安全执行 selector 决策”和“全部 probe 已收敛”拆成两个里程碑，但不支持直接把单节点有效探测窗口缩短为 2-3 秒。快速路径必须复用同一批 future 和 progress，不得重新请求、取消尾部候选或把部分结果伪装成完整报告。

## 3. 需求冻结

### 3.1 “全部节点”的精确定义

首版的“全部测速”指当前 generation 中 `auto_pool` 的所有 terminal candidates，默认最多 64 个，由 `proxy.urltest.max_candidates` 约束。它不指：

- parser 接受的 10,000 个 conversion-only 节点；
- generation registry 中最多 2,000 个 active outbounds；
- `direct`、`block`、DNS、selector 等内部 outbound；
- 未进入 fair pool 的节点。

WebUI 必须称为“全部测速”时，在节点页同时明确候选数量；不允许后台静默扫描 2,000 个节点。

### 3.2 5 秒 SLA

从 nethopd 接受 `NodeTestAll` 到产生完整结果：

| 指标 | 门槛 |
|---|---:|
| 本地受控 fake core，64 candidates，p95 | `<= 5.0s` |
| Android 参考真机，三轮独立公网探测，每轮 engine report | `<= 5.0s` |
| daemon 调度、JSON 解析、选优和 selector PUT 的本地开销 p95 | `<= 100ms` |
| 满足快速门槛的 auto 轮次，selector PUT/ACK | `<= 3.0s` |
| 单节点有效探测窗口 | 默认 `4.5s` |
| daemon 内部 operation 硬截止时间 | `4.9s` |
| 设备内完整 CLI wall-clock SLA | `<= 7.0s` |

“5 秒内完成”是 daemon operation 的内部目标：所有候选都已获得探测机会，且探测、汇总、可选 selector PUT 和最终快照在 4.9 秒内部 deadline 前结束。probe cutoff 为 4.5 秒；到达 cutoff 后取消未完成 probe，把 400ms 留给终态汇总和最终快照。新增 3.0 秒指标只约束满足快速门槛的 auto 轮次，不放宽或替代完整终态指标。设备内完整 CLI 还包含进程启动、su、UDS 往返与 25ms operation 轮询，其独立验收上限为 7 秒。Android 13 真机上，38 candidates 的 engine report 为 4.504 秒，设备内完整 CLI 墙钟为 5.12 秒，符合该分层预算。网络不可达时允许返回 timeout；不承诺每个公网节点都在 5 秒内产生有效延迟。

### 3.3 功能不变量

- manual 模式：测速只更新结果，不改变 requested/active node；
- auto 模式：同一轮有效结果可以触发一次 selector 切换；
- 无有效结果：保留当前节点，不切到未测速节点；
- 部分成功：保留成功结果，其他节点返回明确终态；
- 新旧连接：selector 保持 `interrupt_exist_connections = false`，切换只影响后续新连接；
- 同时最多一轮 test-all；并发调用必须合并到在途操作或返回稳定 busy 诊断，不得叠加 128 个探测；
- generation 变化：旧轮结果整体作废，不得写入新 generation。

## 4. 候选方案与选型

### 4.1 方案比较

| 方案 | 性能与取消 | 依赖/体积 | 复杂度 | 结论 |
|---|---|---|---|---|
| 保留 group delay，只降低 timeout | URLTest 内部仍可跑到 15 秒；固定并发 10 | 无新增 | 低但行为错误 | 不采纳 |
| `ureq` + 每节点一个 OS 线程 | 可共享 wall-clock deadline，但 64 个线程有栈、调度与峰值 RSS 成本 | 已有依赖 | 中 | 不采纳 |
| 固定小线程池 + `ureq` | 有界资源，但慢/不可达节点占住 worker，无法保证所有 64 个节点在 5 秒内获得机会 | 已有依赖 | 中 | 不采纳 |
| `mio` 手写 HTTP/1.1 | 最小运行时，可精确取消 | 依赖少 | 需要自行维护编码、解析、部分读写和状态机 | 违反 KISS，不采纳 |
| `smol`/`async-net` + HTTP client | 非阻塞可行 | `smol` 是多个 async crate 的聚合重导出，仍需选择 HTTP 层 | 生态组合更多 | 无明确净收益，不采纳 |
| Tokio current-thread + Hyper HTTP/1 | 单线程 epoll、每节点 future、统一 deadline、成熟 HTTP 状态机 | 新增最小 feature 依赖 | 中且边界清晰 | **采纳** |
| patch sing-box group delay | 可修 deadline，若改并发仍需维护 fork | 不增 Rust 依赖，但增加 GPL 对应源码、patch 构建与升级成本 | 跨语言维护 | 暂不采纳 |
| Rust 自己实现各代理协议测速 | 理论上脱离 Clash API | 巨大且重复 sing-box | 极高，安全风险大 | 禁止 |

### 4.2 最终依赖

2026-08-12 调研快照为 Tokio `1.53.1`、Hyper `1.11.0`、hyper-util `0.1.20`、http-body-util `0.1.4`，项目现有同步客户端为 ureq `3.3.0`。实施时选择当时最新、满足 MSRV 与 feature 约束的兼容版本并由 `Cargo.lock` 固定；任何升级都必须重新运行依赖、包体和 Android 测速门禁。

在 `nethopd` 中仅增加：

```toml
tokio = { version = "1", default-features = false, features = ["rt", "net", "time", "sync", "macros"] }
hyper = { version = "1", default-features = false, features = ["client", "http1"] }
hyper-util = { version = "0.1", default-features = false, features = ["tokio"] }
http-body-util = { version = "0.1", default-features = false }
```

约束：

- 使用 `tokio::runtime::Builder::new_current_thread()`，禁止 `rt-multi-thread`、process、signal、fs 和 `full`；`sync` 只用于取消信号和任务协调，`macros` 只用于 `tokio::select!`；
- 使用 Hyper low-level HTTP/1 connection，不采用 `hyper-util::client::legacy::Client`、连接池、HTTP/2、TLS、DNS、redirect 或 proxy；
- 目标只能是构造时已验证的 `127.0.0.1:<ephemeral-port>`；
- 每个请求创建独立 TCP 连接。最多 64 个短连接，避免在一次性批量操作中引入连接池状态；
- JSON 沿用现有 `serde_json`，tag 编码沿用现有安全 helper；
- 不增加 `futures` 聚合 crate；任务集合使用 Tokio 自带 `JoinSet`。

`nethop-subscription` 继续使用同步 `ureq` 下载订阅。两个场景的并发模型不同，不为“统一 HTTP 库”牺牲任一侧的边界。

### 4.3 选型证据

- Tokio 官方说明同步程序可以把 async runtime 隔离在一个较小、逻辑独立的部分，并通过 `Runtime::block_on` 暴露同步边界；
- Tokio `new_current_thread` 只使用当前线程调度任务，符合 nethopd 不引入多线程 runtime 的要求；
- Tokio `timeout_at` 接受绝对 deadline，超时 future 可通过 drop 取消；
- Tokio `JoinSet::shutdown()` 等价于 `abort_all()` 后持续 `join_next()` 至集合为空，并忽略正在关闭任务的 panic，适合作为截止时的统一回收原语；
- Hyper low-level HTTP/1 `handshake` 返回 `SendRequest` 与 `Connection` future；官方源码明确说明若不 await/poll `Connection`，`SendRequest` 不会工作，因此每个 probe 必须同时驱动 request/body 与 connection；
- Hyper low-level API 不提供 DNS、TLS 或连接池，恰好匹配固定 loopback、一次性 HTTP/1 探测的窄边界；
- `smol` 官方文档说明其本身重导出多个较小 async crate，对本场景没有比最小 Tokio feature 更明确的体积优势；
- `ureq` 的定位是同步 HTTP client，适合现有低频订阅下载，不适合用 64 个 OS 线程模拟异步 I/O。

最终是否“轻量”以 Android arm64 release 实测为准，而不是以直接依赖数量推断。

## 5. 破坏性运行时重构

### 5.1 移除嵌套 URLTest group

旧结构：

```text
route.final -> nethop-select(selector)
                  |- nethop-auto(urltest)
                  `- terminal nodes
```

新结构：

```text
route.final -> nethop-select(selector)
                  `- terminal nodes

nethopd AutoSelectionController
  -> 定时或即时调用 NodeBenchmarkEngine
  -> tolerance-aware 选择
  -> PUT /proxies/nethop-select
```

删除：

- sing-box 配置中的 `nethop-auto` outbound；
- selector members 中的 `nethop-auto`；
- active terminal 的递归 group 解析需求；
- D10/D12 中两阶段 group delay 和 25/30 秒特例；
- TOML 中固定为 `10`、却无法真实控制 sing-box 的 `proxy.urltest.concurrency` 字段。

保留：

- TOML `interval_minutes`、`tolerance_ms`、`max_candidates`；
- runtime auto/manual intent；
- stable node ID 到内部 terminal tag 的 generation registry；
- selector `interrupt_exist_connections = false`；
- fair auto pool。

这是 schema v3 内的开发期破坏性修正；由于项目未发布，不升级 schema、不写兼容迁移，不保留双实现。

### 5.2 自动模式语义

auto 不再表示“selector 选中一个 URLTest child”，而表示：

```text
selection intent = auto
last successful benchmark result + tolerance
  -> daemon 决定 selector 当前 terminal
```

选择算法复刻 sing-box `v1.13.15` 的稳定语义，而不是简单取全局最小值：

1. 当前 terminal 有本轮有效结果时，以其 delay 为基准；
2. 按 auto pool 的确定性顺序扫描有效候选；
3. 当前 terminal 有有效结果时，只有 `current_delay > candidate_delay + tolerance` 才替换；
4. 当前 terminal 无有效结果时，第一个有效候选建立基准，后续也只有改善超过 tolerance 才替换；
5. 不额外按 map/hash 迭代顺序做 tie-break，候选顺序本身就是稳定 tie-break；
6. 无有效结果保持当前 terminal。

上述比较刻意保持 sing-box `v1.13.15 URLTestGroup::Select` 的严格大于语义。它保证最终节点处于本轮最低延迟的 tolerance 区间内，但不承诺总是选中数值绝对最低的节点，避免在近似节点间抖动。

TCP/UDP 共用同一个 selector terminal。NetHop 当前 UI 和 intent 也只有一个 active node；不再保留 sing-box URLTest 内部潜在的 TCP/UDP 分离状态。

### 5.3 周期测速

`interval_minutes = 10` 的移动端省电默认值保持不变。daemon 的低频 scheduler 触发同一 `NodeBenchmarkEngine`，不新增第二套探测实现。

- core 启动后不立刻与 generation 激活健康检查争抢网络；
- auto intent 且 selector 被使用时才执行周期轮次；
- manual intent 不进行周期全量测速；用户即时测速仍可执行；
- suspend/网络切换后按现有 worker 生命周期重新计算下一次触发；
- 同一时刻只能有一轮，定时与用户触发冲突时用户读取/等待同一 operation。

## 6. Rust 模块设计

### 6.1 代码边界

首版不新建 workspace crate。实现放入 `nethopd::node_benchmark`，原因是：

- 只服务 daemon 到本地 sing-box API 的进程边界；
- `nethop-core` 不应依赖 HTTP 或 async runtime；
- CLI/WebUI 只消费 typed protocol；
- 当前没有第二个消费者，独立 crate 会增加公开接口和维护面。

模块对 worker 暴露窄的 job 接口：

```rust
pub struct BenchmarkRequest {
    pub generation: u64,
    pub candidates: Vec<BenchmarkCandidate>,
    pub deadline: std::time::Instant,
}

pub struct BenchmarkReport {
    pub generation: u64,
    pub trigger: BenchmarkTrigger,
    pub bootstrap_ms: u32,
    pub elapsed_ms: u32,
    pub timing: BenchmarkEngineTiming,
    pub probe: BenchmarkProbeSummary,
    pub outcomes: Vec<NodeProbeOutcome>,
}

pub trait NodeBenchmarkJobs {
    fn start(&mut self, request: BenchmarkRequest) -> Result<OperationId, BenchmarkError>;
    fn try_complete(&mut self) -> Option<BenchmarkReport>;
    fn cancel(&mut self);
}
```

当前 worker 在单一 service loop 中同步处理 UDS、核心退出观察和低频 reconcile。禁止在 request handler 内 `block_on` 5 秒，否则测速会暂停这些职责。

具体实现按需启动一个名为 `nethop-node-bench` 的短生命周期线程，线程内部创建 current-thread runtime 并 `block_on`；结果通过容量为 1 的有界 result channel 回到 worker。现有 `SystemWorkerServiceDriver` 已通过 `std::sync::mpsc::Receiver<()>` 接收配置 watcher 的 wake，但 sender 没有作为公共能力暴露。本次把它重构为 worker-owned 共享 wake channel：config watcher 与 benchmark job 各持有一个克隆 `Sender<()>`，driver 继续只持有单一 receiver。benchmark 线程发送结果后再向 wake channel 发送一次 `()`，不能等待主循环最长 1 秒的常规 poll；无需 eventfd、pipe、新线程或高频轮询。

`NodeTestAll` 只验证并启动 operation，立即返回 operation ID；worker 被唤醒后 non-blocking 收割结果、验证 generation/intent、执行选择提交并发布完成事件。该线程不常驻，因此空闲线程和 wakeup 增量为 0；测速期间线程增量为 1。shutdown 必须发出取消信号并 join，不能留下 detached thread。

worker 同时持有 `JoinHandle`、operation deadline 和 result receiver，不把 channel 当作唯一完成信号：

- receiver 有 report：正常收割；
- receiver disconnected 或 JoinHandle 已结束且没有 report：整轮 `internal_error`，active 不变；
- 到达 operation deadline 仍无 report：请求取消，标记 `internal_error`，并在 shutdown/reaper 路径 join；
- benchmark 完成通知丢失：worker 的 deadline 仍是最终兜底，不允许 operation 永久停在 `running`。

worker 观察到 `JoinHandle::is_finished()` 后必须实际调用 `join()` 回收线程，并检查 join result；禁止只丢弃 handle，因为丢弃 `JoinHandle` 会 detach 线程。正常完成、panic、cancel 和 daemon shutdown 四条路径最终都必须汇聚到同一 join/reap helper。

在途 operation 必须把 `operation_deadline - now` 纳入 `WorkerServiceTasks::next_wakeup_in()` 的最小值，不能依赖现有最长 1 秒 idle poll。这样即使完成 wake 丢失，driver 仍会在内部 4.9 秒 deadline 到期时唤醒 worker；正常完成则由显式 wake 立即收割。该设计只在 operation 存在时注册一次定时 deadline，不提高空闲 reconcile 或轮询频率。

线程最外层使用 `std::panic::catch_unwind` 仅作为线程边界的故障封装，将 unwind 转为无敏感信息的 `internal_error` report。常规错误必须继续使用 `Result`，禁止用 panic 表达网络或解析失败；顶层 panic 后不提交可能不完整的部分结果。若 release profile 最终采用 `panic = "abort"`，`catch_unwind` 不提供保护，因此该门禁必须改为进程级崩溃恢复测试，不能虚假声称捕获成功。

若 Android profile 证明每轮创建 runtime/线程的成本可测，再通过 ADR 评估私有常驻 executor；首版不提前增加常驻资源。

### 6.2 结果模型

```text
NodeProbeOutcome
  node_id: stable public ID
  state: success | timeout | unavailable | protocol_error
  latency_ms: u16?          # 仅 success；sing-box URLTest 返回值
  request_elapsed_us: u64   # Rust candidate task 从开始请求到取得结果/被截止
  completed_at_us: u64      # 相对本轮 engine 起点的完成时间
```

不得把内部 tag、API secret、订阅 URL、server、port、UUID、password、Reality key 或原始 core body带入 IPC、日志和事件。

整轮状态：

- `success`：全部候选成功；
- `partial`：至少一个成功且至少一个失败/超时；
- `failed`：没有成功结果；
- `internal_error`：benchmark 线程 panic、channel 异常或内部不变量失败；
- `superseded`：generation 在提交结果前变化；
- `busy`：调用者选择不加入在途 operation。

### 6.3 分段计时与诊断口径

在修改并发、cutoff、probe URL 或 selector 流程前，terminal report 必须先提供微秒级 typed timing。所有 daemon 内部阶段统一使用单调 `std::time::Instant`，不得使用 wall clock，也不得把 URL、internal tag、secret 或原始错误正文写入计时对象。

engine timing：

```text
thread_spawn_us       调用测速线程到线程闭包开始执行
runtime_init_us       current-thread Tokio runtime 构建
candidate_dispatch_us 稳定 ID 顺序表和 <=64 个 JoinSet task 的创建
probe_us              task 开始被 runtime 调度到全部完成或共享 cutoff 后 shutdown
result_assembly_us    outcome 按 daemon candidate 顺序回排及 timeout 补齐
total_us              engine started 到结果组装完成
```

`probe_us` 是整轮并发任务的包络，不等于每个节点都耗时相同。terminal report 同时携带严格的 `BenchmarkProbeSummary`：

```text
first_result_us             首个真实完成结果相对 engine 起点的时间
last_result_us              最后一个真实完成结果相对 engine 起点的时间
last_success_us             最后一个 success 相对 engine 起点的时间
completed_within_500ms      500ms 内累计完成数
completed_within_1s         1s 内累计完成数
completed_within_2s         2s 内累计完成数
completed_within_3s         3s 内累计完成数
completed_before_cutoff     shutdown 前由 JoinSet 实际收割的结果数
cutoff_pending              共同 cutoff/cancel 时仍未返回的候选数
cutoff_tail_us              最后一个真实结果后为等待 pending 消耗的尾部时间
```

时间桶必须单调不减，`completed_before_cutoff + cutoff_pending == tested`；没有真实结果时首/末时间为空，有 success 时 `last_success_us` 必须存在。`cutoff_pending == 0` 时 `cutoff_tail_us` 必须为 0。cutoff 合成的 timeout 仍携带有界的 Rust 请求耗时，但不会被计入时间桶或 `completed_before_cutoff`。

该分段只能解释 Rust 到本机 sing-box Clash API 的完整等待。`TcpStream::connect` 和 Hyper HTTP/1 handshake 均指向 loopback；远端节点的 DNS、TCP、协议握手、TLS 与 HTTP HEAD 由 sing-box `urltest.URLTest` 内部执行。sing-box 1.13.15 只返回合并后的 `delay`，而且会根据 outbound 的 `NeedHandshakeForWrite` 在拨号后重置计时起点，因此 NetHop 不把 `request_elapsed_us - latency_ms` 解释为某个远端网络阶段。若未来需要远端 DNS/TCP/TLS 精确分段，必须由固定版本 core 提供新的 typed trace，不能用外层差值猜测。

control/terminal timing：

```text
admission_us                请求进入 worker 到调用 engine 前的 generation/plan admission
worker_reap_us              engine report 完成到 worker join 并取得 report
control.intent_load_us      读取持久选择意图
control.current_snapshot_us 周期 auto 读取切换前 core 快照；其他路径为 0
control.decision_us         tolerance 纯函数选优
control.target_resolve_us   stable ID 到当前 generation internal tag 的受控解析
control.selector_apply_us   必要时的一次 selector PUT/ACK
control.final_snapshot_us   最终 core 快照读取与 typed selection 生成
control.total_us            完整 control 后处理
operation_total_us          worker 接受新 operation 到 terminal report 可构造
```

快速路径复用同一个 `BenchmarkControlTiming` 类型，terminal timing 破坏性调整为 `fast_control` 与 `terminal_control` 两个语义独立的槽位。没有进入快速控制时 `fast_control` 全零；快速 `switched/kept/control_error` 必须记录真实 intent/snapshot/decision/resolve/apply 时间。fast control 与 benchmark engine 在两个线程上并发，二者的 wall time 会重叠，因此 `operation_total_us` 守恒不能把两者相加：原有 `admission + engine + worker_reap + terminal_control <= operation_total` 保持，另行校验 `fast_control.total_us <= operation_total_us` 且快速 milestone 的 `elapsed_us <= 3_000_000`。禁止把并发阶段强行相加制造虚假超限。

边界必须互不重叠：`admission_us` 在调用 `spawn_benchmark_with_wake` 前截止；engine 从自己的 `started` 开始；`worker_reap_us` 是 job 已经过时间减去 `engine.total_us`；control 在 worker 取得 report 后开始。允许存在未归类的调度和对象构造间隙，因此校验规则是“阶段和不大于 total”，而不是强制相等。

兼容摘要字段保留但只由 typed timing 派生：

```text
bootstrap_ms == floor((thread_spawn_us + runtime_init_us) / 1000)
elapsed_ms   == floor(engine.total_us / 1000)
```

每个 timing 字段上限为 60 秒，用于让异常调度延迟仍可被报告；这不放宽 4.5 秒 probe cutoff、4.9 秒 daemon deadline 或 7 秒完整 CLI 验收上限。`operation_total_us` 是 daemon 内完整 operation 口径，ADB 启动、`su`、nethopctl 进程创建、UDS 往返和 CLI polling 仍只属于外部 wall clock。性能优化必须先基于同一轮 terminal report 判断瓶颈属于 spawn/runtime、probe、worker 调度还是 selector 后处理。

Protocol v5 将 outcome、probe summary、engine、快速选择里程碑和 terminal timing 设为强制 strict DTO：未知字段、字段缺失、越界、计数不守恒、毫秒映射不一致或阶段和超过 total 均拒绝。CLI 原样输出，WebUI 严格解析；首版不增加常驻指标数据库或遥测上传。

### 6.4 并发与 deadline

```text
validate snapshot and candidates
  -> create operation deadline (start + 4.9s)
  -> create probe cutoff (start + 4.5s)
  -> spawn <=64 probe futures immediately
  -> each future:
       connect 127.0.0.1
       HTTP/1 handshake
       build one request/body future:
         GET /proxies/{tag}/delay?timeout=<remaining probe ms>
         read bounded JSON body
       in the same JoinSet task, tokio::select! { biased; ... }:
         request/body completes -> return mapped outcome and drop connection
         Connection completes/errors first -> protocol_error
  -> timeout_at(shared_probe_cutoff, drain JoinSet)
  -> on cutoff, JoinSet::shutdown()
  -> map missing outcomes to timeout
  -> send probe report to worker through bounded channel
  -> send () through worker-owned wake channel
  -> worker discards report if generation changed
  -> worker applies auto selection once
  -> worker fetches one final /proxies snapshot before operation deadline
```

Hyper 的 `Connection` 必须持续 poll，否则 `SendRequest` 不会推进。首版禁止为每个连接再 `tokio::spawn` 一个 detached/第二层 task；request/body 与 `Connection` 在同一个 candidate task 中用 `tokio::select!` 驱动。`select!` 使用 `biased;` 并把 request/body 分支放在前面：当正常的 `Connection: close` 使两边同轮 ready 时，完整响应优先，不能误报 `protocol_error`。request/body 完成后直接 drop connection，符合一次请求、不复用连接的契约；只有 connection 在完整响应尚未 ready 时先结束/报错才映射为 `protocol_error`。因此最多 64 个 candidate tasks、64 个 probe TCP sockets，不是 128 个 probe tasks。

所有 probe future 使用同一绝对 cutoff，禁止每完成一批就重新获得 4.5 秒预算。单节点 API 的 `timeout=` 取 probe cutoff 的剩余毫秒并封顶 4500ms，因此客户端取消后 sing-box 也不会继续到 15 秒。到达 cutoff 后统一调用 `JoinSet::shutdown()`；它由 Tokio 官方实现任务 abort 与排空，不保留自研 drain 循环。启动时先验证 `candidates.len() <= 64`；若未来提高上限，必须重新评估 task、FD、core goroutine、远端连接和网络突发，不能只改常量。

### 6.5 HTTP 限制

- 只允许 IPv4 loopback，禁止 hostname、IPv6、Unix path 和用户 URL；
- Authorization 由现有随机 secret 构造并在 Debug/错误中脱敏；
- 请求 target 只由固定路径和严格编码的 daemon-owned tag 组成；
- probe URL 固定为 `https://www.gstatic.com/generate_204`，配置与订阅均不可覆盖；
- status 仅接受 `200`；`504` 映射 timeout，`503` 映射 unavailable；
- response body 用 `http_body_util::Limited` 强制上限 4 KiB，header 上限 8 KiB；允许 Hyper 正确解帧 chunked，但禁止无界 collect；
- 解析完成立即关闭连接；不读取到 EOF 的异常响应也必须被 deadline 回收。

### 6.6 快速选优检查点与后台续测

“后台续测”不是第二个 job、第二个 runtime 或 detached thread，而是现有 `NodeBenchmarkEngine` 在快速 selector 决策后继续等待同一 `JoinSet` 中尚未完成的 candidate task。线程、socket、远端 probe 次数、4.5 秒 cutoff 和 4.9 秒 operation deadline 均不改变。

固定预算：

```text
fast earliest              2000ms
fast latest decision       2800ms
fast selector ACK deadline 3000ms
probe cutoff               4500ms
operation deadline         4900ms
```

2.8 秒而不是 3.0 秒开始最后一次决策，是为了给 loopback current snapshot、stable ID 解析和 selector PUT/ACK 留出 200ms 硬预算。时间均从同一个 `operation_started: Instant` 计算，禁止按阶段重新获得相对 timeout。

worker 为在途 operation 维护一个按 auto pool 稳定顺序预分配的 `Vec<Option<NodeProbeOutcome>>`。每次 drain progress 时只填对应槽位，不从 hash/map 迭代顺序重建候选。快速策略是纯函数，只读取以下输入：

```rust
FastSelectionPolicyInput {
    elapsed,
    candidate_count,
    completed_count,
    successful_count,
    current_candidate_state,
}
```

门槛如下：

1. `< 2.0s`：只发布 progress，不执行 selector 控制请求；
2. `2.0s..2.8s`：真实完成数达到 `ceil(candidate_count * 2 / 3)` 时允许检查；
3. 到 `2.8s`：真实完成数至少达到 `ceil(candidate_count / 2)` 时执行最后一次快速检查，否则 defer 到完整终态；
4. 任一检查都必须至少有一个 success；
5. 当前 active terminal 属于 auto pool 时，其本轮结果必须已经真实完成。当前仍 pending 时禁止把“未知”当失败并提前切走；当前不属于 auto pool 时按“无当前基准”处理；
6. operation 完成时重新读取的 intent 必须仍为 auto，generation 必须仍与启动 generation 一致；manual intent 只继续测速；
7. 满足门槛后仍复用 5.2 的 `choose_auto_target` 和同一 tolerance，禁止另写“快速最小延迟”算法。

`ceil` 使用整数安全公式，64 candidates 的早期/最晚覆盖门槛分别为 43/32；1 candidate 时均为 1。覆盖率只计算 JoinSet 实际返回的结果，cutoff 后合成的 timeout 永远不能进入快速检查点。

快速决策结果分为：

| 结果 | 快速阶段行为 | 4.5 秒终态行为 |
|---|---|---|
| `switched` | selector PUT/ACK 成功，立即发布 typed milestone 与 `node_active` | 只汇总完整结果和读取最终 snapshot，不再二次选优 |
| `kept` | 当前节点在已知结果中满足 tolerance，不 PUT | 用完整结果再运行一次正常选优；若出现更优晚到节点，最多 PUT 一次 |
| `deferred` | 覆盖不足、当前 pending、无成功或快路径控制失败 | 保留现有完整终态选优 |
| `not_applicable` | checkpoint 时 intent 为 manual | 终态重新读取 intent；仍为 manual 则不选优，已变为 auto 才按完整结果选优 |
| `superseded` | generation 已变化，不 PUT、不发布旧 selection | 取消/收敛为 superseded |

单轮允许 **最多一次成功的 selector mutation**。快速阶段已成功切换后，晚到结果只更新节点观测，不得在同一 operation 内再次切换；这用少量“本轮未采用的晚到最优”换取稳定连接和可解释状态。快速 PUT 失败不算成功 mutation，终态可按完整结果再尝试一次；禁止无界重试。`interrupt_exist_connections = false` 保持不变，因此快速切换只影响后续新连接。

快速控制请求由 worker 执行，benchmark 线程继续 poll 剩余 future。`WorkerServiceTasks::next_wakeup_in()` 必须同时考虑 2.0 秒 earliest、2.8 秒 latest 和 4.9 秒 operation deadline；progress wake 丢失时仍能在检查点被唤醒。worker 只在门槛满足后获取一次 current snapshot，current 尚未完成时缓存 stable ID 并等待后续 progress，不能每个节点完成都重复 GET `/proxies`。

该设计不承诺每轮都在 3 秒内切换：manual intent、当前节点未返回、成功数为 0、覆盖率不足、generation 变化或 core API 异常时必须 fail closed，退回 4.5 秒完整路径。快速选优是受条件约束的低延迟里程碑，不是降低正确性门槛的强制计时器。

## 7. 选优事务

测速与切换仍是一个异步 operation，但包含“快速选择里程碑”和“完整终态”两个提交点，不是 generation 配置事务：

1. 读取同一 generation 的 registry 和 auto pool，启动全部 probe；
2. worker 按 stable ID 槽位收集 progress，并根据 6.6 计算快速检查点；
3. 快速检查点重新验证 generation、读取 intent/current snapshot；
4. auto 且策略返回 switch 时，在 3.0 秒 deadline 前执行一次 PUT/ACK，立即发布 milestone；manual 或证据不足不切换；
5. benchmark 线程不受 selector 提交影响，继续收集剩余 probe 到全部完成或 4.5 秒 cutoff；
6. 完整 report 到达后再次验证 generation 与 intent；
7. 快速阶段未成功 mutation 时，使用完整结果执行原有 tolerance 选优；已经成功 mutation 时禁止本轮第二次 PUT；
8. 读取最终 snapshot，在 4.9 秒内部 operation deadline 前发布唯一 terminal report。

任一提交点发现 generation 变化时不得 PUT。快速阶段之后用户切换 manual 或选择具体节点时，用户操作在 worker 串行控制面中取得最终所有权；终态只读取并报告最终 snapshot，不得覆盖。PUT 失败时保留原 active，完整测速结果仍正常发布，并带稳定的 fast/terminal diagnostic。

worker 主循环在测速期间必须继续响应 status、观察 core 退出并执行安全 reconcile。配置 publish/core restart 与 benchmark 并发时，配置 mutation 优先，benchmark 收到 generation 失效后取消并进入 superseded。禁止 benchmark 线程直接持有或修改 `OperationalControl`、selection store、event hub 或 generation 状态。

## 8. Protocol、CLI 与 WebUI

### 8.1 typed contract

快速里程碑会改变 strict running/terminal shape，因此实施时直接升级到 Protocol v5，删除生产 Protocol v4 fixture 和兼容解析器。项目尚未发布，不增加 optional 字段、双写事件或版本协商分支。

`NodeTestAll` 快速返回：

```json
{
  "operation_id": "opaque",
  "phase": "running",
  "generation": 9,
  "trigger": "manual",
  "joined_existing": false,
  "candidate_count": 64,
  "fast_selection_earliest_ms": 2000,
  "fast_selection_latest_ms": 2800,
  "fast_selection_deadline_ms": 3000,
  "probe_cutoff_ms": 4500,
  "deadline_ms": 4900,
  "fast_selection": { "state": "pending" }
}
```

每个节点完成时，已有 `node_test` 事件流发布一个严格、有限的 progress payload：

```json
{
  "operation_id": "bench_opaque",
  "phase": "progress",
  "generation": 9,
  "completed": 17,
  "candidate_count": 64,
  "outcome": {
    "node_id": "nh1s-0123456789abcdef",
    "state": "success",
    "latency_ms": 117
  }
}
```

`completed` 在同一 operation 内严格递增；progress 只携带 stable node ID 和归类后的 outcome，不包含 internal tag、代理凭据、订阅 URL 或原始错误。generation 已失效时 daemon 不发布旧 progress。

快速检查点收敛后，同一 `node_test` 事件流最多发布一个 selection milestone：

```json
{
  "operation_id": "bench_opaque",
  "phase": "selection",
  "generation": 9,
  "completed": 49,
  "candidate_count": 64,
  "elapsed_us": 2418000,
  "fast_selection": {
    "state": "switched",
    "selection": {
      "version": 2,
      "intent": { "mode": "auto" },
      "active_terminal": { "kind": "node", "node_id": "nh1s-0123456789abcdef" }
    }
  }
}
```

`fast_selection.state` 是严格 tagged union：

- `pending`：尚未到达检查点；
- `switched`：一次 selector mutation 已 ACK，必须携带 selection snapshot；
- `kept`：已用部分结果评估但未 mutation；
- `deferred`：携带稳定 reason，只允许 `insufficient_coverage | current_pending | no_success | control_error`；
- `not_applicable`：checkpoint 时 intent 为 manual；
- `superseded`：generation 已失效。

除 `switched` 外均不得伪造 selection snapshot。`completed/candidate_count/elapsed_us` 让消费者解释决策证据，但不携带完整部分 report。事件丢失时，`NodeTestOperationGet` 的 running ACK 必须返回当前 `fast_selection` 状态；WebUI 不依赖瞬时事件才能恢复。

所有 probe 收敛后，同一事件流发布唯一 terminal report；`NodeTestOperationGet` 也只返回 running ACK 或该 terminal report：

```json
{
  "operation_id": "opaque",
  "phase": "completed",
  "report": {
    "generation": 9,
    "trigger": "manual",
    "status": "partial",
    "bootstrap_ms": 3,
    "elapsed_ms": 4512,
    "timing": {
      "thread_spawn_us": 800,
      "runtime_init_us": 2200,
      "candidate_dispatch_us": 9000,
      "probe_us": 4480000,
      "result_assembly_us": 6000,
      "total_us": 4512000
    },
    "tested": 64,
    "succeeded": 52,
    "timed_out": 10,
    "failed": 2,
    "nodes": []
  },
  "selection": {
    "version": 2,
    "intent": { "mode": "auto" },
    "active_terminal": { "kind": "node", "node_id": "nh1s-0123456789abcdef" }
  },
  "fast_selection": {
    "state": "switched",
    "completed": 49,
    "candidate_count": 64,
    "elapsed_us": 2418000,
    "selection": {
      "version": 2,
      "intent": { "mode": "auto" },
      "active_terminal": { "kind": "node", "node_id": "nh1s-0123456789abcdef" }
    }
  },
  "timing": {
    "admission_us": 1200,
    "worker_reap_us": 8000,
    "fast_control": {
      "intent_load_us": 100,
      "current_snapshot_us": 7000,
      "decision_us": 20,
      "target_resolve_us": 800,
      "selector_apply_us": 9000,
      "final_snapshot_us": 7000,
      "total_us": 25000
    },
    "terminal_control": {
      "intent_load_us": 100,
      "current_snapshot_us": 0,
      "decision_us": 0,
      "target_resolve_us": 0,
      "selector_apply_us": 0,
      "final_snapshot_us": 7000,
      "total_us": 8000
    },
    "operation_total_us": 4550000
  }
}
```

`trigger` 记录创建 operation 的真实来源，只能是 `manual` 或 `periodic`。它只用于审计、single-flight 归因和 UI 状态，不参与 selector 决策；是否选优分别由快速检查点和完整终态当时重新读取的持久 `NodeSelectionIntent::Auto | Manual` 决定。因此用户即时测速与周期测速在 auto intent 下使用同一 tolerance 选优，在 manual intent 下都不得覆盖用户目标。用户请求命中在途 periodic operation 时返回相同 `operation_id`、保留 `trigger = periodic` 并设置 `joined_existing = true`，不得篡改来源，也不得再启动一轮。全部 deadline 字段是只读元数据；消费端可以据此显示“快速选优/后台续测/完整收敛”，但状态必须以 daemon report/event 为准，不能只靠本地计时器伪造里程碑。

启动请求继续使用普通 control timeout。CLI `node test-all` 默认订阅 operation 事件并等待最多 6 秒后输出最终 report；WebUI 启动后通过已有 event stream 接收结果，并设置 7 秒本地 watchdog。两者都不让 daemon 的 UDS request handler 阻塞等待网络，不再保留 25/30 秒特例。断开 CLI/WebUI 不取消 daemon operation。

### 8.2 WebUI

- 点击闪电按钮启动一次 operation，按钮仅在该 operation pending 时禁用；
- 启动后将本轮候选标为“测速中”；每收到一个 progress，只更新对应 stable node ID 的延迟与状态，不等待慢节点；
- 收到 `selection` milestone 后立即刷新顶部当前节点；仍有 pending 时显示轻量的“剩余 N 个测速中”状态，不弹阻塞提示、不清空已完成延迟；
- 快速阶段 `kept/deferred/not_applicable` 不伪装成节点已切换；完整终态仍可能在 auto intent 下完成唯一一次切换；
- terminal report 仍是结果守恒、auto selection 和 operation 完成状态的唯一权威来源；progress 不触发前端选优；
- timeout 显示“超时”，unavailable 显示“不可用”，不能复用上轮延迟冒充本轮结果；
- auto 切换由 daemon report/event 驱动，前端不得选择最低延迟后自行发第二个命令；
- 页面卸载只取消 UI 订阅，不取消 daemon 已开始的整轮测速；
- UI 根据 operation 的 `trigger` 与 `joined_existing` 区分用户新建轮次和加入周期轮次；
- 事件流重连或页面恢复时查询 operation，使用 ACK 中的 `fast_selection` 恢复快速/后台阶段，禁止重复 toast 或重复选择；
- 7 秒 watchdog 后 UI 必须查询 terminal 状态并离开 pending，即使所有节点均失败；
- 节点页提供延迟低到高、延迟高到低两种严格排序；未知、超时和不可用节点在两个方向都置后，选中态只负责高亮，不覆盖排序键。

## 9. 资源预算

选型必须通过 Android arm64 release A/B，而非仅通过 host 测试：

| 指标 | 门禁 |
|---|---:|
| nethopd 文件体积增量 | `<= 750 KiB` |
| 模块 ZIP 增量 | `<= 350 KiB` |
| daemon 空闲线程增量 | `0` |
| 测速期间 daemon 线程增量 | `<= 1` |
| daemon 空闲 RSS 增量 | `<= 256 KiB` |
| 64 节点测速峰值 RSS 增量 | `<= 4 MiB` |
| 64 节点新增 FD 峰值 | `<= 70` |
| 64 节点 probe task 峰值 | `<= 64` |
| 测速结束 1 秒后残留 probe FD/task | `0` |
| 相对旧路径的远端 outbound probe 尝试数 | 不增加 |

loopback HTTP control request 会从两个 group 请求变为每 candidate 一个请求，这是获得独立取消和结果状态的必要成本；远端 outbound probe 每节点最多一次，不允许因选优执行第二轮握手。

快速检查点只增加 worker-owned 的最多 64 个脱敏 outcome 槽位和至多一次额外 loopback current-snapshot GET；不新增远端请求、OS 线程、Tokio task 或 socket。实现后必须用 A/B 证明峰值 RSS/FD/thread 仍满足原表，不能因为对象“看起来很小”跳过真机测量。

FD 和 task 是两个独立指标：额外 connection driver task 本身不会产生第二个 socket FD，但会扩大任务身份、取消和排空状态面。实现同时冻结 `<=64` probe task 与 `<=70` 新增 FD，并由测试确认不存在每 candidate 的第二层 connection task。线程创建与 runtime build 的 `bootstrap_ms` 单独采集 p50/p95/p99；task 创建由 `candidate_dispatch_us` 独立观测，首次网络进展及等待归入 `probe_us`。这些阶段都计入 4.9 秒整体 SLA，但不混入 daemon 外部的 ADB/CLI 启动成本。

若 Tokio/Hyper 不满足包体门禁，下一候选是只抽取 `mio + httparse` 的专用实现，但必须先提交 ADR 和状态机测试，不能直接为体积牺牲正确性。

## 10. 安全与滥用防护

1. WebUI/CLI 不能传入任意 probe URL、host、port、tag 或并发数；
2. endpoint 继续由受控 core generation 产生，只接受 IPv4 loopback；
3. 候选必须来自当前 sealed generation registry；
4. 单轮最多 64、同一时刻最多一轮，并设置最小用户触发冷却时间 3 秒；
5. 响应和日志严格有界并脱敏；
6. 订阅正文无法扩大并发、deadline 或响应限制；
7. daemon 退出或 core 重启必须取消全部 future；
8. 禁止 WebUI 直连 Clash API。
9. 2.0/2.8/3.0 秒、覆盖率和 mutation budget 都是 daemon 常量，配置、订阅和 WebUI 不得覆盖。

## 11. 失败矩阵

| 场景 | 预期行为 |
|---|---|
| core API 未就绪 | 整轮快速失败，active 不变 |
| 部分节点超时 | 返回 partial，成功结果仍可参与 auto |
| 全部节点超时 | failed，active 不变 |
| 单节点返回畸形/超大 JSON | 仅该节点 protocol_error，整轮继续 |
| API secret 错误 | 整轮 unauthorized，禁止逐节点重试 |
| generation 在测速中切换 | superseded，结果不提交、不切换 |
| generation 在快速 PUT 前切换 | 快速检查 fail closed，不 PUT、不发布旧 selection milestone |
| generation 在快速 PUT ACK 后切换 | 新 generation 激活/replay 接管 selector；旧 operation 终态 superseded，不做第二次 PUT |
| 用户在测速中切 manual | 保存延迟报告，不执行 auto PUT |
| 用户在测速中切另一 manual node | 不覆盖用户选择 |
| 当前 active 到 2.8 秒仍 pending | 不把未知当失败，defer 到 4.5 秒完整终态 |
| 2.8 秒完成覆盖低于 1/2或没有 success | 不快速选择，剩余 probe 原样继续 |
| 快速结果为 keep | 不 PUT；完整终态仍可基于晚到结果执行一次切换 |
| 快速 selector PUT 已成功 | 发布 milestone；完整终态只汇总和 snapshot，禁止本轮第二次 PUT |
| 快速 selector PUT 失败 | active 保持；完整终态允许一次有界 fallback，禁止循环重试 |
| 快速 milestone 事件丢失/WebUI 重连 | operation query 从 running ACK 恢复 fast state，不重复 mutation |
| selector PUT 失败 | active 不变，report 标记切换失败 |
| Hyper connection 在响应完成前退出/报错 | 仅该节点 protocol_error，其他 probe 继续 |
| core 在 deadline 后仍处理请求 | Rust 连接已关闭；健康检查确认无持续异常负载 |
| 同时点击两次 | 返回同一 operation ID 或第二次 busy，不启动第二轮 |
| manual 请求加入 periodic 在途轮次 | 复用 operation ID，保留 trigger=periodic，joined_existing=true |
| benchmark 期间查询 status | 正常响应，核心退出监控和 reconcile 不暂停 |
| benchmark 期间 daemon shutdown | 取消 operation，回收 future，join 短生命周期线程 |
| benchmark 线程 panic | catch_unwind 转 internal_error；无 report 时由 channel/JoinHandle/deadline 兜底；active 不变 |
| result channel 断开或完成 wake 丢失 | worker 最迟在 operation deadline 收敛为 internal_error，不永久 running |
| 0 candidates | 稳定 validation error，不启动 runtime |
| 65 candidates | 内部不变量失败，不静默截断 |

## 12. TDD 与重构证明

### 12.1 重构前基线

先保存脱敏 before fixture，并让完整性测试绑定 SHA-256：

- B01：单 source -> compose -> check -> start -> Google 204；
- B02：多 source fair pool、去重与 attribution；
- B03：manual 选择保持；
- B04：auto 按 tolerance 保持/切换；
- B05：旧连接不因 selector 切换中断；
- B06：partial delay 仍返回有界结果；
- B07：generation/LKG/TUN/TPROXY/应用黑白名单/路由行为；
- B08：当前 27 节点两阶段 fake-core 和真机耗时记录。

before fixture 只证明旧行为，不把 `nethop-auto` tag、两阶段请求或 25/30 秒超时冻结成必须兼容的 ABI。

### 12.2 RED -> GREEN -> REFACTOR 顺序

1. **RED：绝对 deadline**。64 个 fake endpoints 中含永不响应、慢 header、慢 body，旧实现超过 5 秒；新 engine 必须整体收敛。
2. **RED：并发启动**。barrier 证明 64 个请求在 250ms 内全部到达 fake core，禁止批次串行。
3. **RED：连接驱动与取消回收**。同一 candidate task 同时驱动 request/body 和 Hyper Connection；connection 提前失败可诊断；deadline 后 task、TCP 连接和 FD 归零。
4. **RED：结果模型**。success/timeout/unavailable/protocol_error 不互相混淆。
5. **RED：纯选优**。覆盖 tolerance、当前失败、tie-break、无成功、manual 不切换。
6. **RED：generation fence**。测速中 publish 新 generation 后旧结果不能 PUT。
7. **RED：单飞**。定时和用户触发重叠只产生一轮请求。
8. **RED：worker 不阻塞**。测速期间 status、核心退出观察和 reconcile 仍推进；完成信号立即唤醒 worker；shutdown 能取消并 join。
9. **RED：线程韧性**。panic、channel disconnect、wake 丢失均在 deadline 内进入 internal_error，operation 不永久 running。
10. **RED：typed timing 观测**。线程创建、runtime 初始化、candidate dispatch、probe、结果组装、worker reap 与 control 后处理可独立报告并计入整体 SLA。
11. **GREEN：最小 Tokio/Hyper loopback client**。只实现上述测试所需能力。
12. **REFACTOR：删除 `nethop-auto`**。composer、registry、active resolution、fake API、DTO 和 WebUI 统一到单 selector。
13. **REFACTOR：删除旧路径**。删除 group delay、两阶段注释、CLI 25 秒、WebUI 30 秒和 `concurrency = 10` 配置。
14. **VERIFY：旧功能回归**。运行全部 workspace、WebUI、模块与 Android 契约。
15. **VERIFY：新能力**。host 受控 SLA、Android 三轮 SLA、资源 A/B 和代理连通性全部通过。
16. **RED：快速策略边界**。fake clock 覆盖 2.0/2.8/3.0 秒、`ceil(2N/3)`、`ceil(N/2)`、当前 pending、无成功和 1/64 candidates。
17. **RED：单次 mutation**。快速 switch 后晚到更优节点不得二次 PUT；快速 keep/defer 后完整终态仍可 PUT 一次。
18. **RED：同轮续测**。selection milestone 发布后未完成 probe 继续，terminal tested 仍等于 candidate_count，远端请求数仍为每 candidate 一次。
19. **GREEN：worker checkpoint**。复用 progress 槽位、wake 和 operation deadline，实现纯 policy + 窄 control adapter。
20. **REFACTOR：Protocol v5**。running query、selection milestone、terminal summary 和 WebUI reducer 使用同一个 strict fast-selection union，删除 v4 兼容分支。

### 12.3 自动测试矩阵

| 层级 | 必测内容 |
|---|---|
| Rust 单元 | outcome、严格 tolerance 纯函数、稳定候选顺序、deadline 预算、快速覆盖率/时间窗/当前基准纯策略 |
| fake HTTP 契约 | 64 并发、Connection 同 task 驱动/提前退出、正常 keep-alive/Connection close、各种响应分片、超大 body、断连、401、503、504 |
| daemon 集成 | registry 映射、trigger/joined、generation fence、single-flight、worker wake/非阻塞、快速 checkpoint、同轮后台续测、mutation budget、panic/channel/deadline、shutdown join、selector ACK/失败 |
| composer golden | 只有一个 selector，无 URLTest group，全部 terminal 可手选 |
| Protocol/CLI | Protocol v5 fast-selection union、trigger、joined、全部 deadline、微秒级 engine/control/terminal timing、新 report、稳定诊断、6 秒 timeout、敏感字段禁入 |
| WebUI Vitest | 单次 pending、逐节点 progress、selection milestone、断线 query 恢复、后台 pending、terminal 收敛、partial/failed、双向延迟排序、auto changed event |
| 回归 | subscription、LKG、generation rollback、TUN/TPROXY、应用范围、路由、DNS |
| 静态依赖 | 只允许 Tokio `rt/net/time/sync/macros`；禁止 `full`/`rt-multi-thread`、Hyper HTTP/2/TLS、smol、reqwest |
| release A/B | 二进制/ZIP、RSS、FD、线程、task、bootstrap p50/p95/p99、任务回收 |

测试不得访问真实订阅 URL或把 token 写入 fixture。公网节点只用于人工兼容验收，正确性和性能门禁使用受控 fake core。

### 12.4 Android 真机门禁

在固定参考设备上：

1. 覆盖安装前记录旧模块单 source、manual/auto、Google/YouTube/Bilibili 和当前节点；
2. 安装新模块后使用脱敏测试订阅生成 27 和 64 candidate 场景；
3. 冷启动后分别执行三轮 `node test-all --json`，每轮 engine report `<= 5.0s`、设备内完整 CLI `<= 7.0s`；
4. 至少一轮包含 timeout/不可达节点，并证明无后台延迟请求继续占用；
5. auto 下验证 tolerance 内保持、超出 tolerance 切换；manual 下验证绝不切换；
6. 验证旧连接保持、新连接走新 terminal；
7. 验证 TPROXY、TUN、应用黑白名单、DNS guard、规则路由和卸载回滚；
8. 采集 nethopd/sing-box CPU、RSS、线程、FD、网络请求数和模块体积 A/B。
9. 构造满足快速门槛的 auto 轮次，三轮 selector ACK 均 `<= 3.0s`；同时证明 milestone 后剩余节点继续回填、terminal tested 完整且每节点只有一次 probe。
10. 构造当前节点 pending、覆盖不足、快速 keep、快速 switch 和快速 PUT 失败，验证 defer/fallback 与单次成功 mutation。

任一轮 engine report 超过 5 秒或完整 CLI 超过 7 秒即不宣称对应 SLA 完成；满足快速门槛却在 3 秒内没有 selector ACK 时不得用 4.5 秒终态替代快速 SLA。不得删除失败样本后只保留最好结果。

## 13. 实施阶段

### 阶段 A：冻结基线与性能 harness

- 冻结 B01-B08 脱敏 fixture；
- 建立可模拟慢 connect/header/body 的 fake Clash API；
- 建立 wall-clock、并发 barrier、FD/task leak 测试；
- 记录当前依赖树和 Android release 体积。

### 阶段 B：纯模型与 async transport

- 实现 outcome、report、deadline budget 和纯选优函数；
- 引入最小 Tokio/Hyper feature；
- 实现 64 个 loopback HTTP/1 candidate task、同 task Connection 驱动和绝对 deadline；
- 实现 panic/channel 失败封装与 bootstrap 观测；
- 通过安全、取消、task/FD 和资源 host 门禁。

### 阶段 C：daemon 事务与运行时简化

- 增加异步 operation、按需 benchmark thread、有界结果 channel、完成 wake、deadline wake、single-flight、generation fence 和 auto controller；
- composer 删除 URLTest group；
- operational control 改为一次 engine + 可选 selector PUT；
- 删除旧 group delay 和递归 active 解析。

### 阶段 D：消费层

- 更新 Protocol progress/report/event；
- CLI/WebUI timeout 收紧为 6/7 秒；
- WebUI 逐节点回填结果，terminal 统一收敛，展示 partial/timeout；
- 删除旧 tag、旧 DTO、旧 CSS/文案和兼容分支。

### 阶段 E：回归与真机

- 执行 workspace/WebUI/module 全回归；
- Android release A/B 资源门禁；
- 三轮 5 秒真机测速并报告 bootstrap p50/p95/p99；
- manual/auto、旧连接保持与全代理流程回归；
- 用实测结果更新 D10、D12 和性能报告。

### 阶段 F：快速选优与同轮后台续测

- 先实现纯 `FastSelectionPolicy`、fake clock、覆盖率和 mutation-budget RED，不直接在 progress reducer 中堆条件；
- worker 增加稳定 outcome 槽位、2.0/2.8 秒 wake checkpoint 和窄的 fast control 调用；benchmark engine 与 HTTP transport 不改；
- 抽取快速/终态共用的 tolerance decision 和 selector apply helper，避免两套选优语义；
- Protocol 破坏性升级 v5，WebUI 增加 selection milestone、后台 pending 和 query 恢复；CLI 保持等待完整 terminal，便于脚本获得守恒报告；
- 完成 host fake-core、WebUI、模块门禁和 Android 三轮 3 秒 selector / 5 秒 engine / 7 秒 CLI A/B 后，才签署快速路径完成。

## 14. 不采纳与重新评估

### 14.1 为什么不直接 patch sing-box

修复 URLTest group 的 context 是合理的上游改动，但不能单独满足本需求：固定并发 10 仍可能让 64 个慢节点排队；提高到 64 又需要评估核心通用行为。NetHop 已有一次性操作、stable ID、generation fence 和 WebUI report 需求，控制面接管选优更直接，也移除了嵌套 group。

只有以下条件出现才重新评估 patch：上游接受并发布可配置并发、严格继承请求 deadline、提供“只按现有 history 重新选举”的稳定 API，且保留 URLTest group 比 daemon 选优显著更简单。

### 14.2 为什么不等待 gRPC API service

sing-box 1.14 API service 仍属于未来 pin 评审。其 `URLTest` 实现同样以核心 group 调度为中心，不自动证明 64 节点 5 秒 SLA。控制 transport 将来可从 Clash HTTP 换为 gRPC，但 deadline、single-flight、report 和选优事务仍可复用。

### 14.3 为什么不保持两套 auto 实现

项目未发布，不需要用 feature flag 同时保留 `nethop-auto` 与 daemon auto。双路径会造成 intent、active、tolerance、周期调度和测试矩阵翻倍，违反 KISS/YAGNI。新实现通过前后回归后立即删除旧路径。

## 15. 完成定义

只有同时满足以下条件，节点测速重构才算完成：

1. Rust 不实现任何代理协议，仅调用受控 sing-box terminal API；
2. 64 个当前 auto candidates 都在单一绝对 deadline 下获得探测机会；
3. host fake core p95 与 Android 三轮均 `<= 5.0s`；
4. 每个 candidate 只有一个同时驱动 request/body 和 Hyper Connection 的 probe task；
5. deadline 后无残留 task、FD 或持续核心探测负载；
6. benchmark panic、channel disconnect 和完成 wake 丢失均在 deadline 内收敛，不永久 running；
7. auto tolerance、manual 保持、generation fence 和 single-flight 均有自动测试；
8. `nethop-auto` group、两阶段 group delay、25/30 秒 timeout 和伪 concurrency 配置已删除；
9. nethopd/ZIP/RSS/FD/线程/task/bootstrapping 资源门禁通过；
10. 重构前 B01-B08 与重构后新增能力测试全部通过；
11. TPROXY、TUN、应用范围、路由、DNS、LKG、generation rollback 和旧连接保持无回归；
12. 日志、IPC、fixture 和报告不含订阅 token、节点凭据、API secret 或内部 tag。
13. 节点完成后无需等待整轮即可在 WebUI 显示；progress 单调、受 generation fence 约束，terminal report 仍保持唯一终态权威。
14. 满足快速门槛的 auto 轮次在 3.0 秒内产生 selector ACK；不满足门槛时 fail closed 到完整终态，不把未知节点当失败。
15. 快速 selection 后剩余 candidate 使用同一批 task 继续，远端 probe 次数、线程、FD 和 4.5/4.9 秒预算不增加。
16. 单轮最多一次成功 selector mutation；快速 switch 后不因晚到结果二次切换，快速 keep/defer 仍保留完整终态 fallback。
17. Protocol v5、CLI、WebUI 对 pending/selection/background/completed 的 strict contract、断线恢复和敏感字段禁入均通过自动测试。

## 16. 参考资料

### 16.1 官方网页

1. Tokio, Bridging with sync code: <https://tokio.rs/tokio/topics/bridging>
2. Tokio runtime Builder: <https://docs.rs/tokio/latest/tokio/runtime/struct.Builder.html>
3. Tokio `timeout_at`: <https://docs.rs/tokio/latest/tokio/time/fn.timeout_at.html>
4. Tokio `JoinSet`: <https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html>
5. Tokio `select!`: <https://docs.rs/tokio/latest/tokio/macro.select.html>
6. Hyper HTTP/1 client handshake: <https://docs.rs/hyper/latest/hyper/client/conn/http1/fn.handshake.html>
7. Hyper HTTP/1 client `Connection`: <https://docs.rs/hyper/latest/hyper/client/conn/http1/struct.Connection.html>
8. hyper-util `TokioIo`: <https://docs.rs/hyper-util/latest/hyper_util/rt/tokio/struct.TokioIo.html>
9. Rust `std::panic::catch_unwind`: <https://doc.rust-lang.org/std/panic/fn.catch_unwind.html>
10. Rust `std::thread::JoinHandle`: <https://doc.rust-lang.org/std/thread/struct.JoinHandle.html>
11. ureq 3.3 documentation: <https://docs.rs/ureq/3.3.0/ureq/>
12. smol documentation: <https://docs.rs/smol/latest/smol/>
13. sing-box URLTest outbound: <https://sing-box.sagernet.org/configuration/outbound/urltest/>
14. sing-box Selector outbound: <https://sing-box.sagernet.org/configuration/outbound/selector/>

### 16.2 本地源码证据

- `refer/sing-box-v1.13.15/experimental/clashapi/api_meta_group.go`
- `refer/sing-box-v1.13.15/experimental/clashapi/proxies.go`
- `refer/FlClash-main/lib/views/proxies/common.dart`
- `refer/FlClash-main/core/common.go`
- `refer/FlClash-main/core/hub.go`
- `refer/sing-box-v1.13.15/protocol/group/urltest.go`
- `refer/sing-box-v1.13.15/common/urltest/urltest.go`
- `refer/sing-box-1.14.0-beta.13/protocol/group/urltest.go`
- `refer/MagicNet-main/crates/magicnet-cli/src/nodes.rs`
- `refer/MagicNet-main/crates/magicnet-cli/src/node_delay.rs`
- `refer/NetProxy-Magisk/src/module/scripts/utils/api.sh`
- `refer/NetProxy-Magisk/src/webui/src/components/NodesScreen.vue`
