# NetHop 核心常驻与流量接管热切换重构设计

> 状态：设计中
>
> 日期：2026-08-24
>
> 适用阶段：开发期破坏性重构
>
> 目标：让 sing-box 核心持续运行，概览页开关只控制 NetHop 的流量接管；在不牺牲直连、反泄漏、回滚和故障诊断正确性的前提下，将重复启用代理从“启动核心并等待健康检查”降低为“切换已准备好的接管入口”。

## 1. 决策摘要

NetHop 当前把两个不同的生命周期绑定在 `service.enabled` 和 `service.start/stop` 上：

```text
核心进程生命周期
流量接管生命周期
```

这会导致概览页开关执行以下长路径：

```text
service.start/stop --wait
  -> 配置事务
  -> 启停 sing-box
  -> TPROXY/TUN 数据面准备
  -> 核心健康检查
  -> 网络健康检查
  -> WebUI 全量 refresh
```

本设计改为：

```text
sing-box：常驻并保持 inbound/API/路由运行
NetHop：独立维护流量接管入口
概览开关：只执行 capture.enable/capture.disable
```

首个落地目标是 TPROXY：

```text
core_warm
  -> capture_disabled
  -> capture_enabled
```

TUN 不直接套用 TPROXY 热切换模型。TUN 包含虚拟接口、地址、默认路由和 Android 路由所有权，必须在真机证明“保留 TUN 但关闭系统接管”不会泄漏或残留路由后，才允许进入后续实验批次。

项目处于开发期，不保留旧 API、旧状态含义或旧配置字段兼容层。允许删除 `service.start/stop` 的 WebUI 用法、重命名状态、拆分配置和修改协议；但每个破坏性批次必须有 before/after 测试，证明新能力增加且原有业务功能仍然正确。

### 1.1 现实世界类比：热水器和水龙头

可以把 NetHop 想象成家里的热水系统：

```text
热水器        = sing-box 核心
热水管路      = 核心的 inbound/API/内部路由
总进水阀      = NetHop 的 TPROXY 流量接管入口
水龙头        = 概览页“启用代理”开关
燃气/电源开关  = 设置页“核心服务”开关
```

旧流程相当于每次打开水龙头前都先：

```text
打开燃气
等待热水器点火
等待水温稳定
检查管路
最后才允许水流出来
```

所以用户每次点击概览开关，都会等待 sing-box 创建、配置检查、TUN/TPROXY 准备和健康检查。

重构后的流程是：

```text
设置页打开“核心服务”
  -> 热水器启动并保持待机

概览页打开“启用代理”
  -> 只打开总进水阀
  -> 已经待机的热水器立即接收流量

概览页关闭“启用代理”
  -> 只关闭总进水阀
  -> 流量恢复直连，热水器仍然待机

设置页关闭“核心服务”
  -> 先关闭总进水阀
  -> 确认不再有流量进入
  -> 再关闭热水器
```

这个类比对应三个关键原则：

1. **核心常驻不等于代理接管开启**：热水器开着不代表水龙头正在出水；sing-box 运行不代表系统流量正在经过 NetHop。
2. **概览开关必须是轻操作**：概览页只切换 TPROXY 入口，不重复启动核心。
3. **停止核心必须先撤销接管**：不能在总进水阀仍打开时直接关闭热水器，否则对应网络上就是残留规则、连接异常或泄漏风险。

如果核心尚未启动，点击概览页“启用代理”不能假装是瞬时操作；它应提示用户先到设置页启用核心。这相当于热水器没有通电时，水龙头不能承诺立即有热水。

## 2. 调研结论

### 2.1 Surfing 的可取实现

`refer/Surfing` 是三个参考项目中与目标最接近的一个。

其 `changelog.md` 明确记录：

- 热切换直接插入/删除关键 iptables 链规则；
- 不重启服务；
- 从模块启停切换到 iptables hot switch；
- 网络过滤不再依赖硬性的 service start/stop。

`refer/Surfing/box_bll/scripts/ctr.inotify` 的 `apply_wifi_bypass()` 在已有自有链前插入 `NET_BYPASS -> ACCEPT`，旁路时核心继续运行，但流量不进入代理捕获路径；恢复时删除旁路链，重新进入原有 TPROXY/DNS 路径。

Surfing 还提供了值得保留的工程经验：

- 用 `proxying`/`bypassed` 等状态区分核心状态和网络状态；
- 使用 inotify 处理模块、网络和配置变化；
- 对 Wi-Fi/IP 变化做 debounce 和 lock，避免规则并发重建；
- 规则拥有明确的自有链和清理路径。

不直接照搬的部分：

- `destroy_all_rules` 后 `sleep` 再重建，存在非原子窗口；
- bypass 直接 `ACCEPT` 可能绕过过多后续策略；
- TUN、IPv6 恢复和失败回滚边界较粗；
- 状态主要依赖脚本和标志文件，缺少 NetHop 所需的 generation/owner 校验。

### 2.2 MagicNet 的可取实现

`refer/MagicNet-main` 将耗时服务操作放入后台任务：

- 立即返回 `[accepted]`；
- 记录 operation id 和日志路径；
- 后台执行实际命令；
- 通过日志中的 `[exit]` 和定时状态读取对账；
- UI 区分 `accepted`、`running`、`done`、`error`、`timeout`。

它解决的是“UI 不要等待”，不是“核心启动本身变快”。NetHop 应采用其异步操作和对账模型，但底层操作改为接管热切换后，实际等待路径也会缩短。

### 2.3 NetProxy-Magisk 的可取实现

`refer/NetProxy-Magisk-main` 的服务状态显式区分：

```text
preparing -> starting -> ready -> stopping -> stopped
                                      \-> failed
```

其状态读取优先使用本地状态文件；只有服务 `ready` 时才访问核心 API 合并运行态快照。这说明状态读取不能因为等待完整核心检查而阻塞 UI。

### 2.4 sing-box 1.14.0-rc.1 的边界

本设计审计了 `refer/sing-box-1.14.0-rc.1` 的 Box、route、inbound、TUN platform、DNS、URLTest、cache file、OOM killer 和 libbox 生命周期。

得到以下结论：

1. sing-box 的 `Box.Start()` 会按生命周期阶段启动 network、router、inbound、service 等组件；`Box.Close()` 会按生命周期关闭它们。它没有一个通用的“暂停所有外部流量接管但保持所有组件可用”的公开运行时开关。
2. TUN 的 `OpenInterface` 会创建并注册真实 TUN 接口，构造自动路由范围并交给 platform interface；关闭 TUN 依赖生命周期 close，不应把 TUN 设备当作普通 iptables 开关。
3. sing-box 的 interface/network monitor 负责核心自身的接口变化和出站选择，不替代 NetHop 对 Android 外部 netfilter、policy route、IPv6 guard 和应用 UID 入口的所有权校验。
4. TPROXY 的入站 listener 可以常驻；外部是否把系统流量送入该 listener 是宿主网络控制器的职责。因此 TPROXY 是核心常驻热切换的首选模式。
5. reload、inbound close、TUN close 都可能影响核心内部生命周期，不应被伪装成“只关闭流量接管”。配置拓扑变化继续走 NetHop 的完整 activation 事务。

结论：不修改 sing-box 作为通用代理核心的生命周期语义，NetHop 在其外部增加独立 capture attachment 生命周期。

### 2.5 空闲资源调研结论

本次进一步审计了 `refer/sing-box-1.14.0-rc.1` 与同类项目的空闲行为。

sing-box 1.14.0 RC 的 changelog 明确包含：

- 优化 idle connections 的内存占用；
- URLTest 增加 `idle_timeout`，空闲一段时间后暂停定时测速；
- rule-set、DNS cache 和 cache file 分离并支持独立持久化；
- route/network environment 通过事件和延迟合并更新，而不是每个组件高频轮询；
- debug/managed service 可以读取 Go runtime memory、heap、goroutine 和 OOM 报告。

源码证据包括：

- `protocol/group/urltest.go` 与 `docs/configuration/outbound/urltest.zh.md`：`idle_timeout` 默认 30 分钟，空闲时暂停 scheduled delay test；
- `dns/client.go`：有界 LRU cache、optimistic background refresh、异步持久化 DNS cache；
- `adapter/lifecycle.go`、`box.go`：组件按生命周期阶段启动和关闭，但没有通用的核心 suspend API；
- `option/oom_killer.go`、`debug.go`、`service/oomkiller`：提供 memory limit、GC 和 OOM 观测/控制入口，不能直接当成 NetHop 的默认调优值。

同类项目的边界：

- Surfing 通过 iptables hot bypass 避免反复启动核心，但没有证明 sing-box 永久 warm 的 CPU/RSS 预算；
- MagicNet 通过后台 accepted、日志对账和延迟状态刷新降低 UI 阻塞，并用空闲时预加载页面减少界面等待；
- NetProxy-Magisk 将 Service API、状态文件和 Dashboard 分离，并明确指出 Worker 空闲 RSS、CPU、耗电必须在 Android 真机测量，不能用 Host 数据替代。

因此本设计不承诺“常驻核心没有成本”，也不通过猜测设置 `gc_percent`、`memory_limit`、DNS cache 或 keepalive。NetHop 必须增加资源观测和 Idle Policy，再根据真机证据决定 warm 保持时长。

## 3. 当前问题基线

### 3.1 WebUI 长路径

`webui/src/views/OverviewView.vue` 当前的 `toggle()`：

```text
await runJson(service.start|service.stop, wait: true)
await refresh()
```

`refresh()` 还会读取：

```text
status.get
traffic.get
config.get
node.list
metrics.get
```

Android Companion 的 root command executor 通过全局互斥锁串行执行 root 命令，因此前端看似并行的多个查询在真实 Android 路径上仍可能排队。

### 3.2 daemon 生命周期绑定

当前 `ServiceStart/ServiceStop` 在 `worker_application.rs` 中修改 `service.enabled`，并把 `Start/Stop` 命令送入 worker。启动过程会涉及：

- capability probe；
- generation/config check；
- sing-box process start；
- TPROXY network plan apply 或 TUN 建立；
- core/data-plane health；
- `RunningTproxy`/`RunningTun` 发布。

这条路径适合首次启动、完整配置激活和恢复，不适合作为频繁的用户开关。

### 3.3 当前状态不足

现有 `RuntimeState` 能表达：

```text
running_tproxy
running_tun
fail_open_direct
stopping
```

但无法表达：

```text
核心健康且常驻，但当前没有接管流量
接管正在启用/撤销
核心健康状态与 capture 状态分别是什么
```

如果只把 UI 开关改成乐观更新而不拆分 daemon 状态，界面会出现“显示关闭但规则仍在”“显示开启但 capture 尚未安装”等语义错误。

## 4. 目标架构

### 4.1 三层职责

```text
WebUI
  发送 capture.enable/capture.disable
  显示 core_state、capture_state、operation

nethopd
  持有 CoreRuntime 和 CaptureAttachment
  维护单写 mutation lock
  发布状态事件和回滚结果

sing-box
  持续监听 inbound/API
  处理进入核心的连接和数据包
```

### 4.2 核心与接管对象

新增概念：

```text
CoreRuntime
  generation
  process identity
  API/inbound health
  core state

CaptureAttachment
  mode: tproxy | tun
  generation
  network plan
  apply receipt
  capture state
  owner metadata
```

TPROXY 的 `CaptureAttachment` 包含已有 `NetworkPlan + ApplyReceipt`。关闭 capture 不关闭 `CoreRuntime`，只回滚 attachment。

TUN 的 attachment 在第一阶段保持“与 core 绑定”的保守模型；如果没有经过真机证明，不允许从 `RuntimeAttachment::Tun` 进入 `core_warm`。

### 4.3 状态模型

核心状态：

```text
core_absent
core_starting
core_ready
core_failed
core_stopping
```

接管状态：

```text
capture_disabled
capture_enabling
capture_enabled
capture_disabling
capture_failed
```

组合状态示例：

| core | capture | 对外语义 |
|---|---|---|
| absent | disabled | 核心未运行，直连 |
| ready | disabled | 核心保活，当前直连 |
| ready | enabling | 正在恢复接管 |
| ready | enabled | 代理接管生效 |
| ready | disabling | 正在撤销接管 |
| failed | disabled | 核心失败，直连或降级 |

禁止使用单一布尔值代替这两个维度。

runtime snapshot 另外发布资源档位，不把 `capture_state` 当作资源档位：

```json
{
  "core_state": "ready",
  "capture_state": "disabled",
  "resource_state": "warm"
}
```

例如 `core_ready + capture_disabled + resource_state=idle` 仍然是可恢复的核心待机，而不是核心已经停止。

### 4.4 资源生命周期：Active / Warm / Idle / Cold

Core Warm 不是 24 小时满功率运行策略。NetHop 增加独立的资源档位：

```text
ACTIVE
  core_ready + capture_enabled
  正常接管流量，允许正常 URLTest、DNS、规则和连接活动

WARM
  core_ready + capture_disabled
  保留快速恢复能力，不主动产生新的业务流量

IDLE
  core_ready + capture_disabled + idle policy applied
  核心仍可被快速唤醒，但暂停或放慢 NetHop 自己的非必要工作

COLD
  core_absent + capture_disabled
  核心停止，释放进程、连接、缓存和大部分内存
```

状态转换：

```text
COLD --core.start--> WARM
WARM --capture.enable--> ACTIVE
ACTIVE --capture.disable--> WARM
WARM --idle budget exceeded--> IDLE
IDLE --user/core/capture demand--> WARM or ACTIVE
IDLE --cold budget exceeded--> COLD
```

`IDLE` 不是 sing-box 的假想暂停接口。它由 NetHop 控制：

- 停止 NetHop 自己的高频 status/metrics/diagnostic 轮询；
- 暂停订阅更新、规则集更新和节点测速调度；
- 不主动触发 DNS 查询、健康探测或网络重建；
- 让 sing-box 已有的 event-driven monitor、连接 idle timeout 和 URLTest `idle_timeout` 生效；
- 只有确认没有活动连接、pending operation、capture intent 或网络恢复任务后，才允许进入 COLD。

NetHop 不得通过杀线程、发送未定义信号或修改 sing-box 内部状态来制造 IDLE。对 sing-box 本身无法安全暂停的组件，必须记录为 `warm_required_work`，纳入预算，而不是伪装成零 CPU。

资源策略配置建议：

```toml
[service]
enabled = true                 # 是否允许使用 WARM/IDLE

[service.idle]
enabled = true
warm_timeout = "5m"           # capture disabled 后进入 IDLE 的最短等待
cold_timeout = "30m"          # IDLE 后停止核心的最短等待
```

初始值只作为测试 fixture，不是产品承诺。实现中应使用 `DEFAULT_TEST_WARM_TIMEOUT` 和 `DEFAULT_TEST_COLD_TIMEOUT` 等明确的测试默认值，避免它们被误解为产品 SLA。最终默认值必须由 Android 真机资源预算确定。计时器属于 daemon，WebUI 不得自行停止核心。

WARM 与 IDLE 的严格语义：

```text
WARM = Fast Resume Mode
  保留快速恢复所需的核心/API、必要状态维护、必要连接维持和网络监听；
  不由 NetHop 主动制造新的业务流量或高频诊断工作。

IDLE = Low Resource Mode
  只保留核心必须工作、安全必须工作和恢复必须工作；
  其余任务暂停、降频或延迟，不能以“没有调用 WebUI API”推断 CPU 为零。
```

用户意图和资源策略必须是两条独立轴：

```text
User Intent
  capture.enabled = true | false
  service.enabled = 是否允许核心使用 WARM/IDLE

Resource Policy
  ACTIVE | WARM | IDLE | COLD
```

例如 `capture.enabled=false + resource_state=idle` 表示“用户关闭流量接管，daemon 正在降低后台资源”，不表示用户停止了核心服务。

### 4.5 空闲工作分级

每个可能产生 CPU、内存、唤醒或网络活动的功能必须登记在下表之一：

| 工作 | ACTIVE | WARM | IDLE | COLD |
|---|---:|---:|---:|---:|
| sing-box 核心/API | 必须 | 保留 | 保留 | 停止 |
| TPROXY capture | 开启 | 关闭 | 关闭 | 关闭 |
| 已建立连接 | 按协议 | 只保留真实活动 | 按 idle timeout 清理 | 关闭 |
| DNS memory cache | 有界使用 | 有界保留 | 有界保留或清理 | 释放 |
| DNS background refresh | 允许 | 禁止新触发 | 禁止 | 无 |
| URLTest | 正常 | 允许现有状态 | 使用 `idle_timeout` 暂停 | 无 |
| subscription/rule-set update | 按调度 | 不主动触发 | 暂停 | 无 |
| WebUI metrics polling | 页面需要 | 降频 | 停止 | 无 |
| network reconcile | 事件驱动 | 事件驱动 | 仅处理安全恢复 | 无 |

该表描述 NetHop 的控制策略，不承诺 sing-box 每一项都能运行时暂停。不能暂停的 sing-box 内部任务必须通过实测纳入 `WARM_IDLE_CPU`/`WARM_IDLE_RSS`。

### 4.6 用户意图与实际状态

配置中拆分：

```toml
[service]
enabled = true              # 是否允许 daemon 使用 WARM/IDLE 核心运行策略

[capture]
enabled = true              # 用户期望是否接管流量
mode = "tproxy"
```

开发期允许直接重命名或删除旧字段，不保留旧字段兼容解析。实际状态不写回用户配置，发布在 runtime snapshot/event 中：

```json
{
  "core_state": "ready",
  "capture_state": "enabled",
  "resource_state": "active",
  "capture_mode": "tproxy",
  "core_generation": 12,
  "capture_generation": 12,
  "capture_owner": "nethop",
  "last_operation": "capture.enable"
}
```

## 5. TPROXY 热切换设计

### 5.1 启动阶段

首次启动或设备恢复：

```text
1. 读取有效 generation
2. capability probe
3. compose/check sing-box 配置
4. 启动 sing-box
5. 等待 core/API/inbound 健康
6. 预计算并验证 NetworkPlan
7. 按用户 capture.enabled 决定 apply 或保持 disabled
8. 发布 core_ready + capture_state
```

为保证启用低延迟，推荐在核心健康后预构建完整 TPROXY plan，但不把规则写入活动入口，或安装到 inactive A/B slot。启用时只执行入口切换和必要的规则激活。

### 5.2 启用

```text
capture.enable
  -> 检查 core_ready
  -> 检查 generation/capability/owner
  -> 在 mutation lock 内激活 capture generation
  -> verify active chain/rule/route/IPv6 guard
  -> 发布 capture_enabled
```

目标：

```text
命令 accepted <= 100ms
入口切换 <= 1s
完整健康确认 <= 2s
```

### 5.3 关闭

```text
capture.disable
  -> 在 mutation lock 内切换固定入口到 bypass/direct
  -> 删除或停用 capture generation
  -> 验证自有链、policy route、DNS、IPv6 guard 已撤销
  -> 保留 sing-box core
  -> 发布 capture_disabled
```

关闭必须验证直连，而不是只验证 iptables 命令成功。至少要检查：

- 普通 UID 的 TCP 新连接走直连；
- UDP DNS 不再进入 NetHop 捕获链；
- IPv6 不绕过策略泄漏；
- sing-box 自身的 loopback/API 不被误捕获；
- 既有连接按产品策略 drain 或明确断开。

### 5.4 入口结构

不采用 Surfing 的全链无条件 `ACCEPT` 作为最终模型，采用固定入口与 A/B generation：

```text
OUTPUT
  -> NH_OUT_ENTRY
       -> NH_BYPASS       # capture_disabled
       -> NH_CAPTURE_A    # capture_enabled, slot A
       -> NH_CAPTURE_B    # capture_enabled, slot B

PREROUTING
  -> NH_PRE_ENTRY
       -> NH_BYPASS
       -> NH_CAPTURE_A/B
```

入口只保留一条 owner 可证明的跳转。规则主体和路由表在 inactive slot 准备完成后，通过最小 replace 切换活动 slot。

### 5.5 Bypass 约束

`NH_BYPASS` 不是全局 flush，也不是无条件修改其他模块链：

- 只存在于 NetHop 自有链；
- 只跳过 NetHop 自有捕获入口；
- 不删除其他模块的规则；
- 不修改 Android 全局 IPv6 状态；
- 不绕过必须保留的本地安全规则；
- 每条规则包含 generation/owner 可审计元数据；
- rollback 必须能恢复上一个活动 slot。

## 6. TUN 边界

### 6.1 第一阶段不做 TUN 热停

TUN 包含：

- 真实虚拟接口；
- 地址和 MTU；
- 自动路由/严格路由；
- Android 路由优先级；
- IPv4/IPv6 guard；
- 可能的热点/USB 转发。

因此第一阶段规定：

```text
capture.mode = tproxy -> 支持 core warm
capture.mode = tun    -> 保持受控 stop/start
```

UI 必须明确显示：TUN 模式启停会有数据面切换窗口，不能伪装为与 TPROXY 相同的瞬时切换。

### 6.2 后续 TUN 实验门槛

只有同时满足以下条件，才允许设计 `tun_attachment.detach/attach`：

1. Android 真机验证保留 TUN 接口时关闭所有系统捕获不会泄漏；
2. 关闭和重新启用不残留默认路由、rule priority、DNS 和 IPv6 guard；
3. 其他 VPN、Private DNS、热点和工作资料用户场景通过；
4. sing-box 1.14.0-rc.1 及实际打包版本的 TUN platform 行为有固定 fixture；
5. 异常退出和重启恢复能识别旧 TUN 身份，不相信旧接口名或 PID 文件。

## 7. 协议和 CLI 破坏性改动

### 7.1 新控制方法

新增并替代概览页使用：

```text
capture.enable
capture.disable
capture.status
```

请求参数：

```json
{
  "expected_core_generation": 12,
  "expected_capture_generation": 12,
  "wait": false
}
```

返回必须区分：

```json
{
  "accepted": true,
  "completed": false,
  "operation_id": "cap_...",
  "core_state": "ready",
  "capture_state": "enabling"
}
```

### 7.2 旧方法处置

开发期直接删除或改造：

- WebUI 不再使用 `service.start/stop` 控制概览开关；
- `service.start/stop` 只保留给核心生命周期管理、启动恢复和测试工具，或直接删除并改为 `core.start/core.stop`；
- 不保留旧方法兼容 wrapper；
- 协议 fixture、Kotlin bridge allowlist、CLI parser 和 WebUI operation types 同批修改。

### 7.3 设置页提供核心生命周期配置

概览页和设置页承担不同职责：

```text
概览页：快速控制当前是否接管流量
设置页：控制 sing-box 核心是否允许常驻运行
```

设置页增加“sing-box 核心”配置组，至少包含：

| 设置项 | 类型 | 语义 |
|---|---|---|
| 核心服务 | Switch | `service.enabled`；允许 daemon 启动并保持 sing-box 核心运行 |
| 空闲资源策略 | Switch | `service.idle.enabled`；允许 WARM 超时后进入 IDLE/COLD |
| Warm/Cold 超时 | 数值/时长 | `service.idle.warm_timeout`、`service.idle.cold_timeout`；开发期可调，用于真机资源基线 |
| 当前核心状态 | 只读状态 | `core_state`；显示 stopped/starting/ready/stopping/failed |
| 当前接管状态 | 只读状态 | `capture_state`；显示 disabled/enabling/enabled/disabling/failed |
| 当前资源档位 | 只读状态 | `resource_state`；显示 active/warm/idle/cold |
| 停止核心 | 危险操作 | 停止核心前必须先撤销 capture；停止后概览页只能显示直连 |

推荐交互：

1. 打开“核心服务”时，daemon 异步启动 sing-box；设置页立即显示 `core_starting`，完成后显示 `core_ready`。
2. 关闭“核心服务”时，daemon 先执行 `capture.disable`，确认流量已恢复直连后再停止 sing-box。
3. 核心停止期间，概览页的 capture 开关不可用；重新启用核心后，是否自动恢复 capture 由独立的 `capture.enabled` 用户意图决定。
4. 核心已经停止时，用户点击概览页“启用代理”，UI 应提示先启用核心，不能隐式执行核心启动并伪装成瞬时 capture 切换。
5. 核心启动失败时，设置页保留失败状态、诊断码和重试入口；不得把失败吞掉后显示“代理已关闭”。

设置页的核心开关不是普通字段更新。它必须使用专用命令：

```text
core.start
core.stop
core.status
```

不能通过直接修改配置文件让 WebUI 自己推断核心是否已经停止。配置中的 `service.enabled` 只表达“是否允许核心常驻”，实际状态必须由 daemon 发布。

核心生命周期与 capture 的关系固定为：

```text
core.stop  => capture.disable -> core.stop
core.start => core.start -> core_ready -> 按 capture.enabled 决定是否 capture.enable
```

禁止以下状态：

```text
core_stopped + capture_enabled
core_starting + capture_enabled
core_failed + capture_enabled
```

设置页测试必须覆盖：

- 核心停止前自动撤销 TPROXY capture；
- 核心启动后按用户意图恢复或保持 disabled；
- 核心停止时概览 capture 操作被拒绝；
- 核心启动失败时设置页显示错误并可重试；
- 设置页可查看和调整 idle policy，但不能通过页面 timer 直接停止核心；
- idle policy 调整后由 daemon 重新计算 deadline，并在状态事件中发布 resource_state；
- 设置页和 Quick Settings 同时操作时由 daemon 单写锁串行化；
- 页面刷新、Activity 重建和事件丢失后通过 `core.status` 对账；
- `core.stop` 不残留 TPROXY 链、policy route、DNS capture 或 IPv6 guard。

### 7.4 事件

事件流新增：

```text
core_state
capture_state
capture_operation
network_generation
resource_snapshot
resource_pressure
```

事件 payload 至少包括：

```json
{
  "operation_id": "cap_...",
  "phase": "accepted|running|success|failure|rollback",
  "core_state": "ready",
  "capture_state": "enabled",
  "resource_state": "active",
  "generation": 12,
  "diagnostic_code": null
}
```

事件丢失时，客户端通过 `capture.status` 对账，不依赖固定 sleep。

## 8. daemon 重构步骤

### P0：基线和观测

在修改行为前新增：

- service.start/stop 端到端耗时分段日志；
- core spawn、core health、network apply、network verify 的 monotonic duration；
- capture owner/generation 快照；
- Android root 命令串行等待时间；
- WebUI 点击到 accepted、点击到最终状态的时间。

增加资源基线采样。每个状态至少稳定采样 1、5、15、30、60 分钟。每个时间窗不能只取单点，至少以 1 秒粒度记录原始样本，并计算 baseline、median、p95、最大值和 upper bound：

```text
core_rss_bytes
core_cpu_user_ms
core_cpu_system_ms
core_threads
core_open_fds
core_active_connections
core_dns_cache_entries
core_rule_set_bytes
core_wakeup_count
core_network_bytes
```

预算判定规则：

```text
baseline     = COLD 或设备无 NetHop 进程时的同条件基线
median       = 稳态样本中位数，描述典型成本
p95          = 稳态样本 95 分位，描述尾部成本
upper bound  = 登记的硬上限，任何连续窗口超出都失败
```

RSS/CPU 的单次尖峰不能直接判定失败，但连续 p95 或 upper bound 超标必须触发资源策略降级，并写入诊断事件。

采样矩阵必须覆盖：

```text
ACTIVE：有真实 TCP/UDP/DNS 流量
WARM：capture disabled，无新业务流量
IDLE：idle policy 已应用，无 pending operation
COLD：核心已停止
```

Android 真机基线优先于 Windows/浏览器数据。Host 上的 RSS、CPU、线程和耗电数字不能替代 Android 结论。

保留当前行为作为 before baseline：

- 启动代理；
- 关闭代理；
- TPROXY；
- TUN；
- Android 返回键和 WebUI bridge；
- 异常退出恢复。

### P1：内部对象拆分

不改变外部行为，先把 `ActiveRuntime` 拆成：

```text
CoreRuntime
CaptureAttachment
CaptureOperation
```

要求：

- `RuntimeAttachment::Tproxy` 迁移为独立持有；
- `RuntimeAttachment::Tun` 标记为不可 detach；
- stop 顺序明确为 capture rollback -> core stop；
- 所有失败路径都能判断是 core failure 还是 capture failure。

after 测试：现有 worker activation、rollback、fail-open、TUN cleanup 全部通过，新增对象状态快照测试。

### P2：TPROXY capture controller

新增 `CaptureController`，最小接口：

```rust
trait CaptureController {
    type Receipt;

    fn prepare(&mut self, policy: &CapturePolicy, slot: PlanSlot) -> Result<PreparedCapture, Error>;
    fn enable(&mut self, prepared: PreparedCapture) -> Result<Self::Receipt, Error>;
    fn disable(&mut self, receipt: &mut Self::Receipt) -> Result<(), Error>;
    fn verify_enabled(&mut self, receipt: &Self::Receipt) -> Result<(), Error>;
    fn verify_disabled(&mut self) -> Result<(), Error>;
}
```

实现必须复用 `NetworkPlanner`、`NetworkExecutor` 和现有 owner 检查，不在 worker 中拼接 shell。

after 测试：

- enable/disable 幂等；
- 中途失败自动 rollback；
- 双重 enable 不生成重复链；
- disable 后 core process/API 仍存活；
- 旧 slot 恢复；
- generation 不匹配拒绝操作；
- 规则属于 NetHop 才允许删除。

### P3：核心 warm runtime

启动流程改为：

```text
core start -> core health -> prepare capture -> capture according to intent
```

如果用户 intent 为 disabled：

- 核心进入 `core_ready`；
- capture 保持 disabled；
- 不发布 `running_tproxy`，改发布组合状态。

如果启动时 capture enable 失败：

- 核心可以继续 warm；
- capture 进入 failed；
- 状态明确 fail-open direct；
- 不把 core failure 误报为 capture failure。

### P4：协议、Kotlin bridge、CLI 和 WebUI

同一批改动：

- Rust protocol enums/params/responses；
- nethopctl parser/build/render；
- Companion `BridgeCommandPolicy` allowlist；
- WebUI operation requests、event parser、overview toggle；
- mock host 和 browser/e2e fixtures。

WebUI 行为：

1. 点击立即更新为 `capture enabling/disabling`；
2. 调用不等待的 capture 命令；
3. 监听事件或短路径 `capture.status`；
4. 不在 toggle 完成后立即执行全量 overview refresh；
5. 最终状态确认后低优先级刷新 metrics/traffic/nodes；
6. 失败时回滚开关并显示结构化错误。

设置页同步增加核心生命周期区块：核心服务 Switch、核心状态、capture 状态和停止核心确认操作；设置页不得复用概览页 capture 开关的 loading/完成语义。

### P5：默认策略和保活策略

首版默认策略：

- daemon/sing-box 允许进入 WARM/IDLE，但不承诺无条件永久运行；
- TPROXY capture 由用户意图控制；
- TUN 仍按安全 stop/start；
- 核心 warm/idle 空闲资源纳入性能预算；
- 不以“进程仍存在”作为资源优化完成的证据。

首版资源策略：

```text
capture disabled
  -> WARM：短时保留快速恢复
  -> warm_timeout 到期且无活动需求
  -> IDLE：暂停 NetHop 非必要工作，保留核心
  -> cold_timeout 到期且无活动连接/操作
  -> COLD：停止核心并释放资源
```

该策略必须是 daemon 的单一决策，不能由 WebUI timer 自行停止核心。`warm_timeout` 和 `cold_timeout` 在 Android 真机资源基线完成前只能作为实验配置，不得硬编码成产品承诺。

IDLE 阶段至少执行：

- 停止 WebUI/Companion 发起的高频 metrics、traffic、status 轮询；
- 暂停订阅、rule-set、节点测速和诊断调度；
- 停止新的 DNS background refresh；
- 保留 sing-box API、核心健康和必要 network/interface monitor；
- 通过 sing-box `urltest.idle_timeout` 让空闲 URLTest 暂停定时测试；
- 通过 DNS cache capacity 和 cache file 控制内存，不默认关闭 DNS cache；
- 记录无法暂停的核心内部服务及其资源消耗。

资源策略不得直接修改 sing-box 的 `gc_percent`、Go `memory_limit` 或 OOM killer 参数作为首选优化。只有在真机数据证明存在内存压力，并完成 core health、吞吐、延迟和 OOM recovery 测试后，才允许为特定设备/构建提供受控覆盖。

预算超标决策必须由 daemon 执行：

```text
WARM p95/upper bound 超标
  -> 提前进入 IDLE

IDLE 仍超标或发生内存压力
  -> 提前进入 COLD

ACTIVE 超标
  -> 不得静默进入 IDLE；先保持 capture 安全、发布 resource_pressure 诊断并按故障策略处理
```

设备级或构建级 override 只允许覆盖资源阈值和 timeout，不允许覆盖：

- capture owner/generation 校验；
- TPROXY/TUN 安全回滚；
- IPv6/DNS 防泄漏；
- core/capture 状态合法转换；
- COLD 前先撤销 capture 的顺序。

### P6：完整配置/订阅变更适配

配置变化分为：

```text
热路径：capture enable/disable、selector、规则入口、应用 UID 入口
完整 activation：inbound/outbound 拓扑、监听、DNS server、TUN 结构、核心 build 依赖变化
```

热路径不得重启核心。完整 activation 仍使用：

```text
candidate -> check -> health -> A/B switch 或受控 stop/start -> commit/rollback
```

订阅更新先保持当前核心和 capture，不在下载期间修改活动 generation；发布新 generation 时根据变更分类决定 reload、TPROXY overlap 或受控重启。

## 9. 测试策略

### 9.1 Before 基线

在 P1 前固定：

- 现有 unit/browser/e2e/gate；
- service start/stop 功能；
- TPROXY/TUN 数据面 contract；
- Android bridge command policy；
- WebUI overview operation banner；
- 配置 CAS、generation rollback、fail-open。

记录以下指标：

```text
click_to_accepted_ms
click_to_core_ready_ms
click_to_capture_enabled_ms
click_to_capture_disabled_ms
core_spawn_ms
core_health_ms
network_apply_ms
network_verify_ms
webui_refresh_ms
android_root_queue_ms
warm_idle_rss_bytes
warm_idle_cpu_user_ms_per_min
warm_idle_threads
warm_idle_fds
warm_idle_connections
warm_idle_wakeups_per_min
idle_policy_apply_ms
cold_transition_ms
```

### 9.2 Rust unit/contract

新增：

- `capture_state` transition table；
- core/capture 组合状态序列化；
- `capture.enable/disable/status` protocol contract；
- generation/owner mismatch rejection；
- NetworkPlan A/B activation；
- enable/disable idempotency；
- rollback after each network plan step；
- disable leaves core alive；
- core crash while capture enabled；
- capture failure while core remains warm；
- concurrent capture operations are serialized;
- TUN detach is explicitly rejected in first phase。
- Active/Warm/Idle/Cold transition table；
- pending operation、活动连接、capture intent 会阻止 Cold；
- idle policy 只暂停 NetHop 可控任务，不修改 sing-box 私有线程；
- warm/cold timeout 由 daemon 计时且重启后恢复一致；
- DNS cache capacity、URLTest idle timeout 和 rule-set pause 的配置边界；
- resource snapshot schema and monotonic sampling；
- memory pressure or OOM report changes state to a diagnosable failure, not silent restart。

### 9.3 Android bridge

新增：

- `BridgeCommandPolicy` accepts only new capture commands and exact args；
- unknown/legacy toggle args rejected；
- root command timeout and queue metrics；
- event delivery and status re-query after bridge reply loss；
- Activity recreation does not duplicate capture operation；
- Quick Settings tile and WebUI share the same daemon capture operation，不各自改规则。
- Companion/Android process recreation does not reset daemon idle policy；
- entering IDLE stops WebUI polling and does not create a new root command loop；
- COLD transition releases capture before core process termination；
- foreground/root service policy is measured on the target Android version, not inferred from browser behavior。

### 9.4 WebUI browser/e2e

新增 before/after 场景：

1. 点击关闭后 100ms 内显示 `capture disabling`，不等待全量 refresh；
2. 点击开启后先显示 `capture enabling`，收到事件后显示 `capture enabled`；
3. capture 命令 accepted 但最终失败时，开关回滚并保留错误；
4. event 丢失时通过 `capture.status` 对账；
5. 快速重复点击被 action lock 拒绝或合并；
6. overview 的 traffic/nodes/metrics 不阻塞 capture 状态显示；
7. TUN 模式仍展示受控重启状态，不伪装成瞬时热切换；
8. AppShell、返回键、事件重连和 KeepAlive 不重复提交操作。
9. WARM 不触发高频 WebUI refresh；IDLE 不触发订阅/测速后台请求；
10. 超过 idle budget 后进入 COLD，重新启用核心后按 capture intent 正确恢复；
11. 事件丢失、页面隐藏和 Activity 重建不会重置或绕过 daemon idle policy。

### 9.5 真机数据面

TPROXY 必测：

- IPv4 TCP 新连接；
- IPv4 UDP DNS/QUIC；
- IPv6 已启用和 degraded guard；
- 应用 UID include/exclude；
- 核心自身 bypass；
- 普通应用直连与代理切换；
- 既有 TCP/UDP 连接行为；
- Wi-Fi/蜂窝切换；
- 热点/USB experimental 能力不被误启用；
- 与其他 VPN/Private DNS 冲突；
- 切换期间无泄漏窗口超出预算。

每项记录：

```text
capture_state
iptables/ip6tables owner snapshot
ip rule/route snapshot
DNS path
IPv6 guard state
packet/request result
operation duration
```

### 9.6 空闲资源与压力测试

资源测试必须在同一 Android 设备、同一 ABI、同一 sing-box 构建、同一配置和同一网络条件下执行。每个状态至少运行 60 分钟，短于 60 分钟的结果只能作为开发诊断，不能作为 release gate。

测试组：

1. `ACTIVE`：固定 TCP/UDP/DNS 业务负载，记录吞吐、延迟、RSS、CPU、线程、FD、连接和 wakeup。
2. `WARM`：关闭 capture，不产生新业务流量，确认核心 API 可用且重复 enable 延迟降低。
3. `IDLE`：应用 idle policy，确认 URLTest、订阅、rule-set、DNS background refresh 和 WebUI polling 均按策略暂停。
4. `COLD`：停止核心，确认 RSS、线程、FD、连接和 capture owner 归零或回到基线。
5. `WARM -> ACTIVE -> WARM` 循环 100 次，检查内存是否增长、规则是否重复、连接是否泄漏。
6. `WARM -> IDLE -> COLD -> ACTIVE` 循环 30 次，检查计时器、事件、状态恢复和真实启用延迟。
7. Android 低内存压力、网络切换、屏幕熄灭、应用进程重建和 root command 排队场景。

每次恢复还必须记录：

```text
resume_accepted_ms
resume_core_ready_ms
resume_capture_enabled_ms
resume_first_successful_tcp_ms
```

至少比较：

```text
WARM -> ACTIVE
IDLE -> ACTIVE
COLD -> ACTIVE
```

资源节省和恢复速度必须一起评估，不能只用“核心没有停止”作为 Warm 成功标准。

通过标准：

- `WARM_IDLE_CPU`、`WARM_IDLE_RSS`、线程、FD、连接和 wakeup 有明确设备基线与上限；
- `IDLE` 相比 `WARM` 的 CPU/wakeup/网络活动下降达到登记目标；
- `COLD` 不残留 NetHop capture 规则和路由；
- 内存压力不会导致静默丢失 capture 状态；
- 自动降级到 COLD 不破坏下一次 core/capture 恢复；
- 任何资源优化都不能降低 ACTIVE 的代理正确性、DNS 语义、IPv6 防泄漏和吞吐门槛。

## 10. 性能目标

第一阶段目标：

| 指标 | 目标 |
|---|---:|
| 命令 accepted | <= 100ms |
| TPROXY capture enable 入口切换 | <= 1s |
| TPROXY capture disable 入口切换 | <= 1s |
| enable/disable 完整 verify | <= 2s |
| 核心 warm 后重复启用 | 不重新 spawn sing-box |
| toggle 后 WebUI 首次视觉反馈 | <= 100ms |
| toggle 后全量概览 refresh | 不阻塞 capture 状态 |
| WARM_IDLE_RSS | Android 真机测量并登记上限 |
| WARM_IDLE_CPU | Android 真机测量并登记上限 |
| WARM_IDLE_THREADS/FD | Android 真机测量并登记上限 |
| WARM_IDLE_CONNECTIONS | 无业务流量时应为 0 或有明确 keepalive 归属 |
| WARM_IDLE_WAKEUPS | Android 真机测量并登记上限 |
| IDLE 相对 WARM CPU/wakeup | 必须有下降证据 |
| COLD 核心 RSS/CPU/FD | 回到设备基线 |
| WARM -> ACTIVE resume | 必须记录并作为快速恢复基线 |
| IDLE -> ACTIVE resume | 必须记录资源/速度权衡 |
| COLD -> ACTIVE resume | 必须记录完整冷启动成本 |

性能比较必须同时记录 before/after，不能只报告 UI 感知时间。若热切换减少了等待但引入规则执行、CPU wakeup 或内存超预算，不能判定完成。未完成 Android 真机资源基线前，不得把“核心常驻”标记为默认完成能力。

## 11. 安全与故障语义

### 11.1 fail-open 与 fail-closed

关闭 capture 的目标是明确直连，不是删除所有保护：

- TPROXY disabled：撤销 NetHop 捕获，保留必要的核心/控制流量 bypass 和其他模块规则；
- IPv6 capability 不满足时，不得因为 capture disable/enable 切换而泄漏公网 IPv6；
- TUN 失败必须进入已有 fail-open/degraded 语义，不允许假装 capture enabled；
- 任何 owner/generation 不匹配都拒绝删除并进入诊断状态。

### 11.2 崩溃恢复

核心崩溃时：

```text
capture rollback
core state failed
按 restart budget 重新 warm
根据 capture intent 决定是否重新 enable
```

capture 规则失败但核心健康时：

```text
core 保持 warm
capture failed
恢复直连
等待用户重试或 network reconcile
```

### 11.3 单写原则

以下入口必须进入同一个 daemon mutation lock：

- WebUI capture toggle；
- Quick Settings tile；
- Wi-Fi/network reconcile；
- 应用 UID policy 更新；
- 配置/订阅 generation 发布；
- 卸载和 stop。

Kotlin 不得直接执行 iptables 热切换；Quick Settings 只发送 daemon command，避免出现第二套网络控制器。

### 11.4 Android 进程与 Foreground Service 边界

Android Service/Foreground Service 不是自动降低 CPU 或内存的机制。是否使用 foreground service 必须依据目标 Android 版本、root 模块启动路径、用户可感知的长期任务和真机行为决定。

禁止因为“核心常驻”就无条件增加永久通知或 FGS：

```text
daemon lifecycle policy
  -> 管理 ACTIVE/WARM/IDLE/COLD

Android process/FGS policy
  -> 只负责实际平台要求和进程存活边界
```

Android 真机测试必须记录：

- 屏幕亮灭、后台、Activity 销毁重建后的 core/capture/resource 状态；
- FGS 开启/关闭对通知、CPU、RSS、电量和系统回收的影响；
- root 模块 service、Companion Activity、Quick Settings tile 的进程重建关系；
- Android 版本差异导致的后台启动或长期运行限制。

不能从浏览器、Windows Host 或 sing-box upstream 文档推断 Android 进程一定会保持常驻。

## 12. 不做的事情

- 不开发第二套 Kotlin 业务 UI；
- 不让 WebUI 自己拼 shell 或改 iptables；
- 不用固定 `sleep` 代替 operation/event/status 对账；
- 不在 TUN 未验证前声称支持 TUN 热切换；
- 不通过全表 flush 清理网络；
- 不为了兼容旧配置保留双字段长期同步；
- 不把 sing-box 的 interface monitor 当作 NetHop 网络接管状态；
- 不在 subscription update 下载期间重启核心；
- 不把“核心进程存活”当作“代理接管健康”。

## 13. 完成定义

只有同时满足以下条件，才算本重构完成：

1. 概览开关不再调用 `service.start/stop --wait`；
2. TPROXY 核心在 `capture_disabled` 时保持健康常驻；
3. `capture.enable/disable/status` 协议、CLI、Kotlin bridge 和 WebUI 完成切换；
4. enable/disable 不重复 spawn sing-box；
5. capture 状态与 core 状态在协议和 UI 中分离；
6. TPROXY 网络入口使用 owner/generation/A-B 规则，支持幂等、verify 和 rollback；
7. capture 失败不会误报 core 失败，core 失败不会遗留 capture 规则；
8. TUN 仍遵守保守 stop/start 边界，所有 UI 文案与状态准确；
9. Rust unit/contract、Android bridge、WebUI browser/e2e、真机 TPROXY 数据面测试通过；
10. before/after 证据显示重复启用不再执行核心启动健康长路径；
11. IPv4、IPv6、DNS、UID 策略、Wi-Fi/蜂窝、Quick Settings 和返回键功能无回归；
12. `npm run gate`、Rust workspace 测试、Android Companion 测试和 release-quality 测试通过；
13. 文档、状态枚举、协议 fixture、测试 fixture 和诊断码不再保留旧 toggle 语义；
14. 设置页提供核心服务启用/停止配置，并正确展示 core/capture 两套状态；
15. Active/Warm/Idle/Cold 资源状态、Idle Policy 和 daemon 计时器已实现并可观测；
16. Android 真机已给出 WARM_IDLE_RSS、WARM_IDLE_CPU、线程、FD、连接和 wakeup 基线及上限；
17. IDLE 相比 WARM 的资源下降、COLD 释放资源和状态恢复均有 before/after 证据；
18. 资源预算使用原始样本、median、p95、最大值和 upper bound 判定，不依赖单点读数；
19. WARM/IDLE/COLD 到 ACTIVE 的 resume cost 已测量，并据此决定 timeout/资源策略；
20. 没有兼容 wrapper、第二套网络控制器或绕过 daemon 的规则修改路径。

## 14. 实施顺序总览

```text
P0 基线与耗时观测
  -> P1 CoreRuntime/CaptureAttachment 拆分
  -> P2 TPROXY CaptureController 与 A/B 入口
  -> P3 core warm + capture disabled/enabled 状态机
  -> P4 protocol/CLI/Kotlin/WebUI 切换
  -> P5 真机验证、Quick Settings 合并入口、故障恢复
  -> P6 配置/订阅热路径分类与完整 activation 适配
  -> P7 删除旧 service toggle 语义与 release 产物审计
```

每个阶段必须满足：

```text
before 测试固定
实现一项行为
after 测试通过
性能和安全证据补齐
再进入下一阶段
```

本设计不授权自动执行 Git 提交、推送、设备修改或生产环境操作。真机测试和任何网络规则变更必须在明确的测试设备与可恢复环境中执行。
