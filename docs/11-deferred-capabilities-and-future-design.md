# NetHop 延期能力与未来设计候选登记

> 状态：持续维护的设计登记，不是发布路线图
> 适用范围：NetHop 全部 Rust crate、sing-box 集成、Android Root 数据面、CLI、WebUI 与 Companion APK
> 当前基线：Android arm64 Root 模块，sing-box `v1.13.15`，同步 Rust 控制面
> 关联文档：`00-nethop-system-design.md`、`01-performance-budget-and-slo.md`、`02-subscription-import-and-parser-design.md`、`06-configuration-toml-refactor-design.md`、`08-webui-design.md`、`10-subscription-selection-and-node-optimization-refactor-design.md`
> 初始调研时间：2026-08-10

## 1. 文档目的

本文集中记录“技术上可行、当前没有必要实现、未来在明确条件下可以重新评估”的能力，解决以下问题：

1. 避免 gRPC、异步 runtime、eBPF、插件等候选散落在多份设计文档中；
2. 区分“已经拒绝”“暂缓观察”“满足条件后可立项”和“已经进入实施”的状态；
3. 防止为了未来可能性提前增加配置字段、兼容层、空接口、feature、依赖或监听端口；
4. 为重新评估提供可验证的触发条件，而不是按个人偏好反复讨论；
5. 保留源码证据、性能门槛和替换范围，降低未来重新调研的成本。

本文不是任务清单，也不承诺任何候选会进入产品。候选只有完成独立 ADR、TDD 任务分解和真机门禁后，才可以从本文迁出并进入实施文档。

## 2. 管理原则

### 2.1 YAGNI

- 不因候选被登记而修改当前 TOML schema、Protocol、CLI 或 WebUI；
- 不预留 `[plugins]`、`[extensions]`、`feature_set` 或无行为枚举；
- 不保留未启用的 Cargo feature、生成代码、兼容 adapter 或 dormant dependency；
- 不为未来 transport 提前抽象只有一个实现且没有降低当前复杂度的接口。

### 2.2 KISS

- 当前已满足需求的方案继续作为基线；
- 新方案必须替换或明显简化旧方案，不能默认双栈长期共存；
- 功能更多不是采用理由，必须对应 NetHop 已确认的能力缺口；
- 原型只实现验证假设所需的最小纵切，不复制完整产品。

### 2.3 证据优先

重新评估必须至少包含：

- 对应上游稳定版本、tag、commit 和源码路径；
- Android arm64 release 构建结果；
- 与当前方案同机、同 fixture、同 release profile 的对比；
- 依赖树、二进制体积、RSS、CPU、唤醒和失败恢复数据；
- 新监听面、认证、权限和日志泄密审计；
- 保留行为 baseline 与新增行为测试。

### 2.4 开发期破坏性替换

项目尚未正式发布。候选获准实施时：

- 优先直接替换旧内部契约，不维护开发期兼容层；
- 先冻结旧功能 baseline，再编写新能力 RED 测试；
- GREEN 后运行新旧行为映射测试，证明旧有效功能仍正常；
- 删除落选实现、旧依赖、旧测试 fixture 和旧配置字段；
- 不以“可能有人使用”为由保留双读、双写或双监听。

## 3. 状态模型

| 状态 | 含义 | 当前代码动作 |
|---|---|---|
| `observe` | 上游或需求尚不稳定，只收集证据 | 不改代码和 ABI |
| `deferred` | 技术可行，但当前收益不足以覆盖成本 | 不改代码和 ABI |
| `prototype-approved` | 触发条件已满足，允许建立有界原型 | 原型不得进入发布构建 |
| `accepted` | ADR 与门禁通过，进入独立 TDD 实施文档 | 允许破坏性替换 |
| `rejected` | 与安全、范围或架构原则根本冲突 | 除非前提变化，否则不再评估 |
| `superseded` | 已被其他方案或正式设计取代 | 保留历史结论和替代链接 |

状态只能在证据发生变化时更新，不按固定日历制造维护工作。以下事件应触发相关条目复核：

- sing-box pin 升级；
- 当前实现出现无法绕开的能力缺口；
- Android 真机 profile 发现明确瓶颈；
- Companion APK 进入对应功能开发；
- 上游弃用、移除或改变当前依赖的 API；
- 安全边界或 Android 平台能力发生变化。

## 4. 候选总表

| ID | 候选 | 状态 | 当前结论 | 主要触发条件 |
|---|---|---|---|---|
| F001 | sing-box `services.api` gRPC 控制面 | `observe` | 保留 Clash API | sing-box 1.14 稳定并完成 pin 评审，或 Clash API 出现能力缺口 |
| F002 | 历史统计的 V2Ray gRPC over UDS / JSON exporter | `deferred` | 不阻断基础代理闭环 | 产品确认需要持久历史统计且现有 runtime snapshot 不足 |
| F003 | 异步 runtime 与并发订阅下载 | `deferred` | 继续同步、有界、低频 fetch | 多 source 更新时延成为真实瓶颈，且需要取消/并发控制 |
| F004 | 配置 section digest、typed patch 与动态数据 store | `deferred` | 继续全量 typed validate/apply | 配置接近 256 KiB 或 profile 证明 diff/apply 成为瓶颈 |
| F005 | `toml_edit` 注释与格式 round-trip | `deferred` | canonical TOML，可丢自定义注释 | 手工维护用户把注释保真作为明确硬需求 |
| F006 | BLAKE3、Base64 SIMD 等 parser 热路径加速 | `deferred` | SHA-256 + safe Base64 | 10,000 节点端到端 profile 证明对应阶段持续主导 |
| F007 | 每 source group、分层 urltest 或权重调度 | `deferred` | single/merge + fair pool | 公平轮询无法满足已确认的来源级策略需求 |
| F008 | native nft、eBPF/XDP 或硬件 offload | `observe` | 继续受控 TPROXY/TUN | Android 设备覆盖、可回滚性和收益证据成熟 |
| F009 | 多 profile、Expert 配置或插件扩展 | `deferred` | 单一托管配置，无插件入口 | 出现无法由 typed schema 表达的稳定用户需求 |

## 5. F001：sing-box API service gRPC 控制面

### 5.1 当前事实

NetHop 当前通过以下边界控制核心：

```text
nethopd
  -> 启动独立 bin/sing-box run -c generation/config.json
  -> config.experimental.clash_api.external_controller
  -> 127.0.0.1:9090 + 随机 secret
  -> 同步 ClashApiClient
```

NetHop 不链接 libbox，不使用 Android AAR，也不允许 WebUI 直接连接 sing-box 控制端口。

sing-box `v1.13.15` 已含图形客户端本地使用的 `StartedService` protobuf，其中包括 group 订阅、URLTest、selector 切换、状态、日志和连接接口。sing-box `v1.14.0` 新增普通配置可启用的 `services.api`，使独立 `sing-box run` 实例可以通过 gRPC、gRPC-Web 和 WebSocket bridge 暴露这套接口。

截至 2026-08-10：

- NetHop pin：`v1.13.15`；
- sing-box 最新稳定版：`v1.13.18`；
- sing-box 1.14 最新已核对版本：`v1.14.0-beta.13`，仍为 prerelease。

### 5.2 能力对比

| 能力 | 当前 Clash API | `services.api` gRPC | NetHop 当前需要 |
|---|---|---|---|
| 查询 selector/urltest group | 支持 | `SubscribeGroups` | 是 |
| 测试 group/节点 | 支持 | `URLTest` | 是 |
| 切换 selector | 支持 | `SelectOutbound` | 是 |
| 连接列表与关闭 | 支持 | streaming connections | 是 |
| 流量与运行状态 | 支持轮询/现有 adapter | `SubscribeStatus` | 是 |
| 日志流 | 可通过现有进程日志/接口 | `SubscribeLog` | 可选 |
| Clash mode | 支持 | 支持，但部分行为仍依赖 Clash service | 非核心 |
| 网络质量、STUN、Tailscale 工具 | 不完整 | 支持 | 当前不需要 |
| 订阅下载、解析、合并 | 不支持 | 不支持 | 由 NetHop 负责 |

gRPC 能减少部分轮询并提供 typed streaming，但不会替代：

- NetHop 的订阅下载与 parser；
- source registry、last-known-good 和 generation 事务；
- stable node ID 与 sing-box tag 的映射；
- 嵌套 group 的递归 terminal 解析、深度限制和环检测；
- nethopd 对 WebUI/CLI 暴露的稳定 typed Protocol。

### 5.3 当前不采用的理由

1. 上游 `1.14` 尚未成为 NetHop 已审核的稳定 pin；
2. 当前 Clash API 已覆盖节点选择重构所需的最小能力；
3. Rust 标准 gRPC client 通常引入 protobuf codegen、HTTP/2、连接管理和 Tokio/异步依赖；
4. 双向或服务端流需要新的取消、重连、背压和 supervisor 生命周期语义；
5. API service 同时提供 gRPC-Web、CORS 和可选 Dashboard，扩大监听面审计范围；
6. NetHop 已有自己的 WebUI 和 typed IPC，不需要引入 sing-box Dashboard；
7. 同时保留 Clash API 与 gRPC 会形成双控制面，不符合 KISS。

### 5.4 明确不采用的形态

- WebUI 直接连接 sing-box API service；
- 对局域网开放 gRPC 或 Dashboard；
- 无 secret 的 loopback listener；
- 为兼容开发期构建长期同时运行 Clash API 与 gRPC；
- 把上游 protobuf 直接暴露成 NetHop Protocol；
- 因 gRPC 功能更多而顺带开放 STUN、Tailscale 或系统代理控制。

### 5.5 重新评估触发条件

至少满足第一项，并满足后续任一项：

1. sing-box `1.14.x` 已发布稳定版，NetHop 完成 core pin、协议矩阵和真机回归评审；
2. Clash API 被上游弃用、出现 Android 独立进程不可修复的问题，或无法提供已确认的必要状态；
3. WebUI/CLI 对实时 group、连接和日志流的需求使当前轮询超过 CPU、唤醒或延迟预算；
4. gRPC 原型证明可以替换 Clash API，而不是长期双栈；
5. Rust client 能在不显著扩大 runtime 和二进制的条件下通过 Android arm64 门禁。

### 5.6 原型范围

原型只允许验证以下纵切：

```text
services.api(loopback + secret, dashboard disabled)
  -> GetVersion/API version negotiation
  -> SubscribeGroups
  -> URLTest
  -> SelectOutbound
  -> SubscribeStatus
  -> 断线重连与 core restart
  -> 转换成现有 NetHop typed snapshot
```

原型不得修改用户 TOML。API service 配置只能由 composer 生成，并继续把监听地址、端口和 secret 视为 daemon 私有资源。

### 5.7 采用门槛

| 项目 | 门槛 |
|---|---|
| 上游版本 | 稳定 tag、commit 和源码证据冻结 |
| 功能 | group、测速、选择、状态和连接行为不低于当前实现 |
| 迁移 | 发布构建最终只保留一个 core control transport |
| 安全 | IPv4 loopback、强 secret、Dashboard 关闭、浏览器来源收紧、日志脱敏 |
| 二进制 | 增量必须记录并由 `01` 总预算接纳 |
| 内存与 CPU | Android 真机不突破稳态 RSS、空闲 CPU 与 wakeup 门槛 |
| 恢复 | core restart、API restart、半开连接和超时均能回到可观测状态 |
| 测试 | 旧 Clash baseline 与新 gRPC 行为映射通过，新增流式失败测试通过 |

### 5.8 源码证据

- `refer/sing-box-v1.13.15/daemon/started_service.proto`
- `refer/sing-box-1.14.0-beta.13/docs/configuration/service/api.zh.md`
- `refer/sing-box-1.14.0-beta.13/service/api/server.go`
- `refer/sing-box-1.14.0-beta.13/service/api/web_bridge.go`
- `refer/sing-box-1.14.0-beta.13/daemon/attached_service.go`
- `refer/sing-box-1.14.0-beta.13/daemon/started_service.proto`
- `crates/nethopd/src/process.rs`
- `crates/nethopd/src/clash_api.rs`
- `crates/nethop-core/src/composer.rs`

## 6. F002：历史统计控制面

`00` 已登记 V2Ray stats gRPC over UDS 与受控 JSON exporter 两种候选。基础代理、当前流量和节点选择不依赖持久历史统计，因此该能力继续延期。

重新评估条件：

- 产品明确需要跨重启的按应用、节点或时间段历史统计；
- 已冻结数据保留、隐私、上限和清理语义；
- 当前 Clash runtime snapshot 无法满足需求；
- 两个原型以相同 fixture 比较体积、RSS、CPU、补丁面积和权限边界。

最终必须二选一，不能同时保留无消费者的 V2Ray gRPC server 和新增 JSON exporter。

## 7. F003：异步 runtime 与并发订阅下载

当前订阅更新低频、有界，使用同步 fetch 能减少常驻线程、reactor 和取消语义。只有出现以下事实才允许重新评估：

- 多 source 顺序更新显著超过用户可接受时间；
- 需要并发镜像竞争、取消 superseded fetch 或统一异步 I/O；
- daemon 已因其他已采纳能力引入同一 async runtime；
- Android 真机证明并发减少总时延且没有突破 RSS、CPU、连接数和供应商风控门槛。

采用时必须统一 runtime 所有权，不能让 parser、fetcher 和 daemon 各自创建 runtime。

## 8. F004：配置增量化与动态数据 store

当前配置上限内，全量 typed parse、validate、diff 和 canonical apply 不在数据面热路径。暂不引入：

- section digest；
- RFC 6902 JSON Patch；
- 通用数据库；
- 为数组项目设计的细粒度 CRUD 协议；
- 为未来客户端预留未知字段 passthrough。

重新评估条件：配置接近 256 KiB、应用/规则数据达到数千项，或 Android profile 证明全量处理影响交互。届时优先采用 typed patch 和独立有界 store，不把 TOML 扩大成数据库。

## 9. F005：TOML 注释与格式保真

当前 daemon 输出 canonical TOML，并明确不承诺保留用户任意注释位置。`toml_edit` 只有在真实手工维护场景证明注释保真是硬需求时才评估。

原型必须比较：

- crate 与二进制增量；
- 解析和写回内存；
- 注释、数组表、字段删除和 schema 升级行为；
- Manager 与手工编辑并发时的 CAS 语义；
- malformed/unknown field 是否仍严格拒绝。

## 10. F006：parser 热路径加速

SHA-256、safe Base64 和当前 YAML/JSON parser 是已验证基线。BLAKE3、Base64 SIMD 或 unsafe 加速只有在端到端 profile 证明对应阶段持续主导时才允许一次性对比。

规则：

- 不保留双算法或 dormant SIMD feature；
- 不用微基准替代 10,000 节点端到端 fixture；
- unsafe 路径必须增加 Miri/ASan/fuzz 和 Android arm64 验证；
- 落选实现和依赖必须删除。

## 11. F007：分层 group 与来源权重

当前 single/merge 和确定性 fair pool 已解决订阅语义与大来源挤占问题。以下能力继续延期：

- 每 source 独立 urltest 后再套全局 selector/urltest；
- 用户可配置来源权重；
- 按套餐流量、价格、国家或运营商自动调度；
- 根据历史质量自行训练评分模型。

重新评估必须从明确用户规则开始，并证明 fair pool 无法表达。不能仅因为 sing-box 支持嵌套 group 就增加产品复杂度。

## 12. F008：native nft、eBPF/XDP 与硬件 offload

当前受控 TPROXY/TUN、能力探测、事务执行和回滚是稳定基线。offload 候选受 Android kernel、vendor、SELinux、Magisk/KernelSU 和设备差异影响，继续保持 `observe`。

采用前必须证明：

- 至少覆盖当前可获得的代表设备，而不是单一开发机；
- 缺失能力时可以无残留回退；
- 收益在真实代理流量下显著，而非合成包转发微基准；
- 不扩大 root 攻击面或引入不可审计的预编译对象；
- 安装、升级、卸载和崩溃恢复均可撤销。

## 13. F009：多 profile、Expert 配置与插件

当前产品坚持单一托管配置、nodes-only 订阅和受控 sing-box composer。多 profile、Expert JSON 与插件会扩大配置真相源和安全边界，因此只有明确需求出现后才分别立项。

不可接受的实现方式：

- 在当前 TOML 预留空 `[plugins]` 或 `[extensions]`；
- 允许订阅注入 inbound、route、service、script 或 arbitrary JSON；
- 用一个 `expert = true` 绕过 capability matrix 和安全校验；
- Manager 与 daemon 分别维护不同 profile 格式。

## 14. 候选晋级流程

候选从 `observe/deferred` 进入实施必须依次完成：

1. 更新本条目的事实、触发条件和源码证据；
2. 编写独立 ADR，列出当前方案、候选方案和不采用方案；
3. 冻结旧功能 baseline 与资源基线；
4. 编写最小原型，不进入发布构建；
5. 运行 host、Android arm64、失败注入、安全和性能比较；
6. 作出 `accepted` 或继续 `deferred/rejected` 的单一决定；
7. 若接受，新增独立设计与 TDD 任务清单；
8. 破坏性替换旧实现并删除落选路径；
9. 同步 `00/01` 及受影响的领域文档；
10. 将本条目标记为 `superseded` 并链接正式设计。

## 15. 候选条目模板

新增候选必须使用以下最小模板：

```markdown
## Fxxx：候选名称

- 状态：observe | deferred | prototype-approved | accepted | rejected | superseded
- 当前基线：
- 用户价值：
- 当前缺口：
- 为什么现在不做：
- 触发条件：
- 最小原型：
- 安全影响：
- 性能与体积门槛：
- 保留行为 baseline：
- 源码与官方证据：
- 最终替换范围：
```

缺少用户价值、触发条件或可测门槛的想法不进入本文。

## 16. 最终原则

NetHop 不通过提前实现未来能力体现前瞻性，而通过清楚记录边界、证据和重新评估条件保持可演进性。

当前方案只要仍满足安全、性能和产品需求，就继续保持简单。未来候选只有在前提发生变化、收益可测且能够替换旧复杂度时才进入实现；否则只保留记录，不进入配置 ABI、依赖树和发布产物。
