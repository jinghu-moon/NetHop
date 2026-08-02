# NetHop 性能预算与 SLO

> 状态：Draft v0.4  
> 日期：2026-08-02  
> 适用范围：NetHop Phase 0-A 至首个稳定版  
> 上位文档：[`00-nethop-system-design.md`](./00-nethop-system-design.md)

## 1. 文档定位

本文档定义 NetHop 的性能预算、服务级目标（SLO）、测量口径、测试资产、阶段闸门和回归判定。它细化系统设计第 24、25、27 章，不改变产品边界、功能承诺或安全不变量。

发生冲突时按以下优先级处理：

1. 已确认的用户需求；
2. `00-nethop-system-design.md`；
3. 已批准的架构决策记录（ADR）；
4. 本文档；
5. 测试脚本、报告模板和历史基线。

参考稿 `refer/NetHop性能预算与SLO.md` 提供了较完整的指标候选集，但其中的设备分级、YAML 400 ms、性能 profile、全局 sysctl、UDP/QUIC/DNS 固定阈值等内容不能直接成为首版承诺。本文档只保留可复现、可解释且与上位设计一致的部分。

NetHop 的性能原则是：

> 性能必须可测量，预算必须可归属，基线必须可复现，未实测的数据不得伪装成发布承诺。

当前项目没有常驻 Pixel/GKI、Qualcomm、MediaTek 和 Magisk/KernelSU 多设备实验室。性能工作以一台实际可用的 `reference_verified` arm64 Android 13+ 真机为主；具体型号、ROM、内核和 Root 管理器不在设计文档中虚构，随测试报告登记。本文的绝对 SLO 首先约束该精确组合和后续逐台验证的组合，不构成“所有 Android 13+ 设备均已验证”的声明。

## 2. 规范词与 SLO 等级

文中的“必须”“不得”“应”“可以”分别表示强制要求、禁止要求、推荐要求和可选能力。

每个性能指标只能属于以下一种等级：

| 等级 | 含义 | 未达到时的处理 |
|---|---|---|
| `release_gate` | 已由上位设计冻结、作用于已验证设备/能力组合的发布硬门槛 | 阻断该组合的稳定支持声明；不阻断缩小范围的 Alpha |
| `feature_gate` | 某个可选模式、补丁或实现方案的启用门槛 | 关闭该能力并回退到已验证路径 |
| `budget_gate` | 体积、依赖或资源分配预算 | 阻断合并或提交 ADR；不得静默提高预算 |
| `challenge` | 工程挑战目标 | 记录差距，不单独阻断发布 |
| `report_only` | 从 Phase 0-B 或该能力首次启用阶段开始采集，尚无足够数据冻结阈值 | 对声明包含该能力的发布，缺少数据会阻断；数值本身暂不判失败 |
| `diagnostic` | 用于定位问题，不参与自动判定 | 写入报告并关联异常分析 |

只有需要作出跨设备或广泛稳定支持声明时，才要求：

1. 至少一台固定参考 arm64 runner 完成三轮独立稳定基线；
2. Pixel/GKI、Qualcomm、MediaTek 等不同内核/厂商组合取得足够实机或社区复核；
3. 指标的方差、热状态和网络漂移可解释；
4. 通过性能 ADR 冻结 fixture、统计量和阈值；
5. 不与上位设计冲突。

设备不足时不降低阈值，也不阻塞 reference-device Alpha；应缩小 support matrix 和 release notes 中的承诺范围。社区结果只有包含完整设备 fixture、原始报告和脱敏诊断时才进入 `community_verified`。

## 3. 首版 SLO 总表

### 3.1 发布硬门槛

| 指标 | SLO | 等级 | 适用条件 |
|---|---:|---|---|
| TPROXY TCP 四流吞吐保持率 | `>= 85%` | `release_gate` | 同设备、同路径、同服务端、稳定温度，与相邻 direct baseline 比较 |
| TUN `system` TCP 四流吞吐保持率 | `>= 75%` | `release_gate` | 在固定 runner 的 capability probe 支持该路径时判定，单独报告 |
| 空闲 CPU | `<= 3%` | `release_gate` | sing-box、worker、supervisor 全部常驻进程合计，空闲 10 分钟窗口 |
| 稳态 RSS | `<= 80 MiB` | `release_gate` | 500 active outbounds、规则/DNS/stats 全启用，预热后空闲 10 分钟 |
| 更新峰值 RSS | `<= 110 MiB` | `release_gate` | 5 MiB/10,000 节点转换、候选校验、激活或回滚完整窗口 |
| 订阅转换 | `<= 300 ms` | `release_gate` | 目标 arm64 release build；每种标准 URI、JSON、YAML fixture 分别满足 |
| 运行时轻量切换不可用窗口 | `<= 1 s` | `release_gate` | selector、Clash mode、SRS reload、netfilter A/B 入口切换 |
| 完整配置激活或回滚 | `<= 3 s` | `release_gate` | 从活动路径开始受影响到新路径健康或旧路径恢复 |
| stats patch 吞吐/事务率开销 | `<= 3%` | `feature_gate` | 相同 core、tags、config 下，开启与关闭 patch 的配对测试 |
| stats patch 额外 RSS | `<= 5 MiB` | `feature_gate` | 500 active outbounds |

`TUN gvisor` 是兼容性最后回退。它只需通过功能、IPv4/IPv6 泄漏、崩溃恢复和状态报告，不设置吞吐硬门槛，也不得用其结果替代 `TUN system`。

### 3.2 挑战目标

| 指标 | 挑战目标 | 说明 |
|---|---:|---|
| TPROXY TCP 四流吞吐保持率 | `>= 90%` | 不降低 85% 硬门槛的适用范围 |
| 空闲 CPU | `<= 2%` | 不通过增加轮询周期掩盖泄漏窗口 |
| 稳态 RSS | `<= 70 MiB` | 500 active outbounds |
| 更新峰值 RSS | `<= 100 MiB` | 包含候选 core 或 parser 峰值 |
| stats patch 开销 | `<= 1%` | TCP streaming 或 TCP_RR 配对结果 |
| DNS cache hit p95 | `<= 10 ms` | 受控 fixture，不含客户端排队时间 |
| 节点选择 API 确认 p95 | `< 200 ms` | 不等同于新连接已走新节点 |

### 3.3 首版仅报告项

以下项目从 Phase 0-B 起按已启用能力进入报告，并进入 release full 报告；Phase 0-A 只采集 19.1 节规定的路线 smoke。在取得三轮稳定真机基线前，不为这些项目设置跨设备硬阈值：

- TCP_RR、TCP_CRR 的 transaction rate、p50、p95、p99；
- UDP goodput、offered load、丢包率、jitter、PPS；
- QUIC/HTTP/3 首次握手、恢复握手和下载 goodput；
- Hysteria2、TUIC 的连接建立、吞吐、CPU 与 RSS；
- DNS cache miss、国内 direct DoH、国外 proxy DoH 的 p50/p95/错误率；
- supervisor 拉起 worker、core 崩溃恢复、NETLINK_ROUTE reconcile；
- SQLite 写入、查询、WAL checkpoint 和 24 小时 soak；
- 2,000 active outbounds 兼容边界；
- wakeup、调度、功耗和热节流特征。

## 4. 测试对象与边界

### 4.1 计入模块总量的进程

资源总量至少包含：

- NetHop 固定版本的 `sing-box`；
- `nethopd` worker；
- `nethopd` supervisor；
- 测试场景中由模块保持常驻的其他进程。

`nethopctl` 是短生命周期客户端，其峰值单独记录，不计入空闲稳态；如果 CLI 在后台留下进程，则该进程立即计入模块总量。

### 4.2 active outbounds 边界

| 规模 | 含义 | 承诺 |
|---:|---|---|
| 500 | 发布性能基线 | 必须满足 80 MiB、3% CPU 和核心功能门槛 |
| 2,000 | 托管模式兼容上限 | 必须可加载和操作；性能退化须报告，不继承 500 节点资源 SLO |
| 10,000 | conversion-only 输入边界 | 只承诺转换、校验、去重和诊断，不承诺全部发布给 sing-box |

合并结果超过 2,000 active outbounds 时必须拒绝托管发布并返回稳定错误码，不得静默截断。10,000 节点结果不得用于证明 500 active outbounds 的稳态资源指标。

### 4.3 不计入转换时间的阶段

300 ms 转换指标只覆盖：

```text
detect -> decode -> parse -> normalize -> validate -> dedupe -> compose -> serialize
```

不包括：

- 网络下载；
- TLS 和 DNS；
- `sing-box check`；
- 写盘、`fsync` 和 rename；
- core 启动、健康检查和发布。

这些阶段必须另行计时，不得把慢阶段移出 span 后宣称转换达标。

## 5. 可复现测试环境

### 5.1 设备 fixture

不用“高端机”“中端机”替代可复现设备记录。每份报告必须包含：

- 厂商与完整型号；
- SoC、RAM、存储类型；
- Android 版本、build fingerprint、安全补丁级别；
- 内核 release、GKI 状态、SELinux 模式；
- Magisk 或 KernelSU 版本；
- netfilter backend：iptables legacy、iptables-nft wrapper 或其他；
- NetHop commit、构建配置、Rust/Go/NDK 版本；
- sing-box tag、commit、build tags 和 patch digest；
- 电池电量、充电状态、屏幕状态和电源模式；
- 测试前后 thermal status 与可获得的传感器读数。

至少固定一台当前实际可用的 arm64 真机作为参考 runner，不要求必须是 Pixel/GKI。Host/AVD 可以验证流程和报告 schema，不能证明 TPROXY、SELinux、vendor netfilter、功耗或最终性能。其他设备按可获得性和社区贡献增量加入，不把缺少设备记为指标失败。

### 5.2 网络 fixture

硬门槛测试使用受控局域网：

- 客户端与服务端固定位置和链路；
- 服务端 CPU、NIC 和链路容量不能成为瓶颈；
- 禁止使用公网代理节点证明吞吐硬门槛；
- Wi-Fi 测试记录频段、信道、带宽、RSSI、协商速率和重传；
- 条件允许时增加 USB Ethernet，区分无线波动和数据面开销；
- IPv4 与 IPv6 分开测试；
- 上传与下载分开测试。

受控代理端点必须支持首版协议所需的 TCP、UDP 和 QUIC 场景。端点配置、证书、服务端版本和限速均纳入 fixture manifest。

### 5.3 运行条件

每组测量前必须：

1. 使用 release 构建，关闭额外 debug/trace 日志；
2. 保持同一 CPU、电源和网络配置；
3. 完成规定 warmup；
4. 确认不存在系统更新、备份、应用安装等显著后台负载；
5. 记录热状态，出现 thermal throttling 时将结果标记为 invalid；
6. 确认 direct baseline 未被其他 VPN、代理或防火墙模块接管。

Android Thermal HAL 的 severity 比绝对温度更适合跨设备判定。测试开始与结束的 thermal status 不一致、或任一阶段进入会限制性能的 severity 时，整组吞吐/时延结果无效。

## 6. 统计规则

### 6.1 重复次数

- 吞吐、RR/CRR、DNS、切换和 IPC：每个场景至少 5 次有效重复；
- 转换 microbenchmark：至少 20 次测量，另有不少于 5 次 warmup；
- 稳态 CPU/RSS：至少 3 个独立 10 分钟窗口；
- release full：在不同启动周期完成至少 3 组重复。

报告原始样本，并计算 median、P10、P90；时延类增加 p95，样本量允许时增加 p99。不得只保留最好结果。

### 6.2 direct baseline

吞吐保持率定义为：

```text
retention_percent = proxied_value / paired_direct_value * 100
```

每个代理模式前后各运行一次 direct，使用两者中位数作为 paired direct。若前后 direct 相差超过 5%，说明链路或热状态漂移，整组结果 invalid。测试顺序采用 direct/A/B/direct 的配对块并在不同重复中交换 A/B，减少固定顺序偏差。

吞吐 gate 对上传、下载分别判定；只有在状态明确声明 IPv6 数据面健康时才对 IPv6 判定同一门槛，否则必须报告对应 degraded/unsupported 原因。

### 6.3 回归基线

历史回归只比较以下条件完全匹配的结果：

- device fixture；
- Android build 与内核；
- 网络 fixture；
- 测试工具及版本；
- sing-box tag/tags/patch set；
- NetHop 功能开关；
- active outbounds、规则集和 DNS 配置。

任一条件变化时建立新 baseline，不把环境迁移误判为代码回归。

## 7. TCP 吞吐

### 7.1 工具与参数

使用固定版本 iperf3。官方文档说明 `-J` 输出 JSON，`-P` 设置并行流；从 3.16 起并行流由独立线程执行，因此工具版本本身属于 fixture。

基准命令语义固定为：

```text
iperf3 -c <server> -P 4 -t 30 -O 5 -J
iperf3 -c <server> -P 4 -t 30 -O 5 -R -J
```

`-O 5` 的 omission 区间只用于预热，不进入最终吞吐。上传和下载都以接收端 `sum_received.bits_per_second` 作为 primary goodput，发送端 bitrate 与 retransmits 作为诊断；报告必须保存完整 JSON，而非解析终端文本。

### 7.2 场景矩阵

| 捕获模式 | IP | 方向 | 判定 |
|---|---|---|---|
| direct | IPv4/IPv6 | upload/download | paired baseline |
| TPROXY | IPv4 | upload/download | 每项保持率 `>=85%` |
| TPROXY | IPv6 | upload/download | 声明 healthy 时每项 `>=85%` |
| TUN system | IPv4/IPv6 | upload/download | 声明 supported 时每项 `>=75%` |
| TUN gvisor | IPv4/IPv6 | upload/download | `report_only` |

每次同时记录：

- bits per second；
- retransmits；
- sender/receiver 差异；
- 模块进程 CPU、RSS/PSS；
- thermal status；
- Wi-Fi RSSI、协商速率或有线链路速率。

单流结果作为诊断项，用于识别单连接瓶颈，不替代四流硬门槛。

## 8. 短连接与请求响应

iperf3 长流吞吐不能代表交互延迟。使用 netperf 的：

- `TCP_RR` 测量复用连接上的 request/response；
- `TCP_CRR` 测量每次 transaction 建立新 TCP 连接的 connect/request/response；
- IPv4、IPv6、direct、TPROXY、TUN system 分开执行。

固定 request/response size、持续时间、并发数和 netperf 版本。Phase 0-B 报告：

- transactions/s；
- p50/p95/p99 latency；
- 相对 direct 的 transaction-rate 保持率；
- 相对 direct 的绝对和百分比时延开销；
- CPU、context switch 和 wakeup。

参考稿提出的 `p95 <= direct +20%` 作为 Phase 0-B 候选挑战目标，不是首版 release gate。三轮基线后由 ADR 决定是否冻结以及使用相对值还是绝对毫秒值。

## 9. UDP 与 QUIC

### 9.1 UDP 测量口径

iperf3 UDP 需要通过 `-b` 指定目标 bitrate；默认值不能代表链路上限。测试按 direct 稳定 goodput 的 25%、50%、75%、90% 形成 offered-load 阶梯，固定 datagram length、socket buffer 和持续时间。

每个阶梯报告：

- offered load；
- sender bitrate 与 receiver goodput；
- lost packets、loss percent；
- jitter；
- packets per second；
- CPU、RSS、socket error/drop counter；
- IPv4/IPv6 与捕获模式。

UDP 不与 TCP 百分比混算。参考稿中的 UDP4 40%、UDP6 35% 下限没有真机证据，首版不采用为硬门槛。Phase 0-B 可将 TPROXY UDP goodput 保持率 70% 作为实验挑战值，但不能用降低 offered load 或容忍高丢包伪造达标。

### 9.2 QUIC/HTTP3

受控端点分别测量：

- 首次 QUIC 握手；
- session resumption；
- HTTP/3 小对象 TTFB；
- HTTP/3 10 MiB 下载 goodput；
- Hysteria2、TUIC 首次连接和稳定传输；
- 网络切换后的 UDP session 行为。

首次与恢复握手不得混合统计。参考稿提出的首次握手 `direct +250 ms`、HTTP/3 goodput 60% 等数值保留为 Phase 0-B 校准候选，不是发布硬门槛。

任何默认参数优化必须是 sing-box v1.13.15 已发布字段，并通过相同矩阵验证。首版不修改全局 socket buffer、BBR 或其他 sysctl。

## 10. DNS 性能

NetHop 使用 sing-box 内建、遵循 TTL 的 DNS cache，不重复实现 Rust cache。测试至少覆盖：

| 场景 | 路径 | 等级 |
|---|---|---|
| cache hit | sing-box cache | `challenge: p95 <=10 ms` |
| cache miss, 国内域名 | direct DoH | `report_only` |
| cache miss, 国外域名 | proxy DoH | `report_only` |
| NXDOMAIN/失败 | 对应策略路径 | `report_only` |
| cache capacity 变化 | 1024/4096/校准值 | `diagnostic` |
| strict Private DNS | degraded 路径 | 功能与状态测试，不纳入健康 split DNS 延迟 |

DNS span 从请求进入 NetHop DNS inbound 开始，到响应写回客户端结束。客户端排队、应用自身 resolver cache 和网络首次连接必须单独标记。

每次报告：

- cache hit/miss；
- query type；
- upstream transport；
- direct/proxy detour；
- connection reused；
- p50/p95；
- timeout、SERVFAIL、NXDOMAIN 和其他错误计数；
- DNS cache 对 sing-box RSS 的增量。

DoH 首次建连与已复用连接分组，不能用大量 warm cache 请求掩盖 cold path。

## 11. 切换、激活与恢复

### 11.1 时间定义

| 指标 | 起点 | 终点 |
|---|---|---|
| selector API 确认 | CLI 写入完整请求 | daemon 返回已由 core 接受的响应 |
| selector 数据面生效 | CLI 写入完整请求 | 首个新连接经目标 terminal outbound 成功 |
| SRS reload | 文件原子替换完成 | 新规则对探针生效 |
| netfilter A/B | 开始 replace 入口规则 | 新连接经候选链成功 |
| 完整激活 | 活动数据面首次受影响 | 候选 generation 健康并 commit |
| 完整回滚 | 判定候选失败 | last-known-good 数据面恢复健康 |
| worker 恢复 | supervisor 观察到 worker 退出 | 新 worker 完成 reconcile 并报告 healthy/degraded |
| core 恢复 | worker 观察到 core 不健康 | core 或旧 generation 恢复并通过数据面探针 |

只记录命令返回时间不足以证明数据面恢复。切换测试必须连续运行短连接探针，并用成功/失败时间序列计算不可用窗口。

### 11.2 硬门槛

- selector、Clash mode、SRS reload、netfilter A/B：不可用窗口 `<=1 s`；
- 完整配置激活或回滚：不可用窗口 `<=3 s`；
- TPROXY 双实例原型只有同时满足 `<=1 s`、更新峰值 `<=110 MiB`、统计隔离、UDP drain 和回滚正确性时才能启用；
- TUN 不运行双实例，仍使用完整配置 `<=3 s` 门槛。

旧连接是否保持、何时 drain、如何归因必须单独报告。既有连接存活不能替代新连接可用性。

### 11.3 网络 reconcile

对 Wi-Fi/蜂窝切换、默认路由变化、IPv6 地址变化、休眠唤醒、规则被外部删除分别测量：

- NETLINK_ROUTE 事件到达时间；
- debounce/coalesce 时间；
- reconcile 完成时间；
- IPv6 guard 暴露窗口；
- 是否发生重复全表扫描或规则抖动；
- CPU/wakeup 增量。

这些时延从 Phase 0-B 起为 `report_only`，但任何流量泄漏、死循环或无界重试均为功能失败，而非性能降级。

## 12. CPU 与 wakeup

### 12.1 空闲 CPU 硬门槛

测试配置固定为 500 active outbounds、规则、DNS cache 和 stats 全启用，日志级别 `warn`，无订阅下载、测速、前台 CLI 或活动代理连接。

启动并预热 5 分钟后，连续采样 10 分钟。对每个模块进程读取 `/proc/<pid>/stat` 的 `utime + stime` 增量，按系统 clock tick 换算：

```text
process_cpu_percent = cpu_seconds_delta / wall_seconds_delta * 100
module_cpu_percent  = sum(process_cpu_percent)
```

该口径中单个 CPU core 满载为 100%。三个独立窗口的最大值均须 `<=3%`，同时报告 median；进程在窗口内重启则该窗口 invalid，并另记稳定性失败。

`dumpsys cpuinfo` 只作为交叉检查，不作为硬门槛数据源，因为它的采样窗口和系统负载归因不够稳定。

### 12.2 wakeup 与功耗

使用 Perfetto/ftrace 记录 scheduler wakeup、timer、CPU frequency/idle 和线程活动。Phase 0-B 至少报告：

- 各模块线程 wakeup/s；
- 默认 60 秒规则核验 ticker 的实际唤醒；
- stats 每分钟采集的 CPU 与 wakeup 增量；
- urltest idle timeout 后是否停止 ticker；
- supervisor 无崩溃时是否保持事件驱动；
- 10 分钟空闲窗口的 CPU frequency/idle 分布。

在固定支持设备上可以使用 Android 官方基于 Perfetto 的 Wattson 做 A/B 功耗估算，但它属于 `diagnostic`，不能在不同 SoC 间直接比较绝对值。

## 13. 内存预算

### 13.1 测量口径

硬门槛使用 `/proc/<pid>/status` 的 `VmRSS`。Linux 文档将其定义为：

```text
VmRSS = RssAnon + RssFile + RssShmem
```

同时读取 `/proc/<pid>/smaps_rollup` 的 Rss、Pss、Private 和 Swap 作为诊断。PSS 用于解释共享文件页，不能替换已经确认的 RSS 硬门槛。

每个样本分别记录：

- sing-box；
- nethopd worker；
- nethopd supervisor；
- 其他模块常驻进程；
- 各项总和。

### 13.2 稳态预算

加载 500 active outbounds、默认 SRS、DNS cache、Clash API 和 stats，完成预热后空闲 10 分钟。每秒采样一次，10 分钟内模块进程 VmRSS 总和的最大值必须 `<=80 MiB`。

设计子预算为：

| 组件 | 子预算 | 性质 |
|---|---:|---|
| sing-box | `<=55 MiB` | 归因预算 |
| worker + supervisor | `<=20 MiB` | 归因预算 |
| SQLite page cache 与其他余量 | `<=5 MiB` | 归因预算 |
| 模块总量 | `<=80 MiB` | `release_gate` |

子预算用于定位，不把共享页重复计算后当作独立发布门槛。某个组件超出子预算但总量仍达标时必须分析，不得忽略增长趋势。

### 13.3 更新峰值

更新场景从读取本地 5 MiB/10,000 节点 fixture 前开始，到候选 generation commit 或 rollback、临时对象释放并完成一次 allocator 稳定采样后结束。

在整个窗口以不低于 10 Hz 采集所有模块进程 VmRSS，总和最大值必须 `<=110 MiB`。测试分别覆盖：

- Base64 URI；
- 明文 URI；
- sing-box outbounds JSON；
- Clash YAML；
- 多 source 合并；
- 候选校验失败与回滚；
- TPROXY 双实例原型（若启用）。

双实例不能依据“设备剩余内存看起来足够”动态绕开门槛。只有完整 fixture 实测达标才可启用。

## 14. 订阅转换

### 14.1 标准 fixture

每种格式各自提供接近 5 MiB、最多 10,000 节点的冻结 fixture，覆盖：

- VLESS、VMess、Shadowsocks、Trojan、Hysteria2、TUIC、AnyTLS；
- IPv4、IPv6、域名地址；
- TLS/uTLS/Reality 和首版允许的 transports；
- 重复节点；
- 10% 有界非法节点；
- 超长字段、非法 Base64、YAML 深度/alias 限制；
- 稳定 node ID 和脱敏 golden output。

fixture manifest 记录生成器版本、seed、字节数、节点数、格式分布和 SHA-256。不得只用简单 Shadowsocks URI 证明 300 ms。

### 14.2 时间与结果

在目标 arm64 Android 真机的 release build 上，预热后测量：

```text
detect + decode + parse + normalize + validate + dedupe + compose + serialize
```

Base64 URI、明文 URI、sing-box JSON、Clash YAML 和多 source 合并的标准 fixture 必须分别 `<=300 ms`。不因 YAML 实现较慢自动放宽到 400 ms；若在安全复杂度限制下仍不达标，应收紧受支持的 YAML 复杂度并返回明确诊断，而不是修改计时边界。

报告至少包含：

- end-to-end duration；
- 每阶段 duration；
- peak VmRSS/PSS；
- allocations 或可获得的 allocator 诊断；
- 输入、有效、无效、去重后、输出节点数；
- 输出字节数和 digest；
- cold/warm run 标记。

Host benchmark 可用于 PR 快速回归，不能替代目标设备 300 ms gate。

## 15. stats patch 与传输

### 15.1 归因正确性优先

性能通过不能弥补错误归因。以下场景必须先通过功能测试：

- selector -> terminal node；
- selector -> urltest -> terminal node；
- 运行时节点切换；
- TCP、UDP、TUN flow；
- 旧连接保持原节点归因；
- core_instance_id 变化；
- group 循环或深度保护；
- SQLite counter 与 bucket 同事务提交。

不能使用 `inbound bytes = direct + proxy + block bytes` 作为严格总量守恒公式。代理封装、DNS/sniff、重试和拒绝路径处在不同统计层。正确性验证必须在同一 counter 层、已知 payload fixture 下证明无重复和无漏计，并明确允许误差来源。

### 15.2 patch 性能闸门

在相同 sing-box v1.13.15、build tags、配置和设备上，对 patched build 分别关闭/开启 attribution：

- TPROXY TCP streaming 保持率下降 `<=3%`，挑战 `<=1%`；或
- TCP_RR transaction rate 下降 `<=3%`，挑战 `<=1%`；
- 500 active outbounds 额外 VmRSS `<=5 MiB`；
- idle 不得新增高频 wakeup。

采用配对、交错顺序，至少 5 次有效重复。若 streaming 与 RR 结论相反，两项都保留，不能选择更好的一项通过。

### 15.3 stats transport ADR

历史统计进入实现后，从 Phase 0-B 或对应功能阶段比较：

- A：V2Ray gRPC over UDS，socket mode `0600`；
- B：认证 Clash API `/nethop/stats` JSON exporter。

比较维度：

| 维度 | 计量方式 |
|---|---|
| 发布体积 | sing-box、nethopd、模块 ZIP 差值 |
| 稳态内存 | 500 active outbounds VmRSS/PSS 差值 |
| 采集 CPU | 60 秒周期采集的 `/proc` CPU 增量 |
| wakeup | Perfetto 线程 wakeup/s |
| 延迟 | counter request p50/p95 |
| 安全 | 普通 App、错误凭据、socket mode 绕过测试 |
| 维护成本 | patch 行数、依赖树、上游升级冲突 |

Android 官方安全指南明确指出 localhost 端口可被设备上的其他应用访问，不能作为敏感 IPC 的身份边界。因此禁止未鉴权的 V2Ray TCP listener。只有 B 实测降低总资源且不扩大安全边界时才选择 B，否则使用 A。

## 16. IPC、SQLite 与存储

### 16.1 UDS IPC

Phase 0-B/1 报告：

- `status.get` p50/p95；
- `node.select` p50/p95；
- `node.list` 在 500 与 2,000 节点下的 p50/p95、响应字节数和 peak RSS；
- 并发只读与单写发布时的延迟；
- 超时、断连、worker 重启和 generation conflict。

测试必须走真实 UDS framing、权限检查和序列化，不能直接调用 Rust 函数代替 IPC。

### 16.2 SQLite

SQLite 使用 WAL 时，普通写入与 checkpoint 的 I/O 特征不同；官方文档指出 checkpoint 需要同步和页搬运，可能比提交更慢。测试必须分开报告：

- 每分钟 counter delta 聚合事务；
- hourly/daily bucket upsert；
- 当前日、30 日、90 日查询；
- PASSIVE checkpoint 持续时间；
- WAL 大小与 page count；
- checkpoint 与配置发布重叠时的 p95/p99；
- 模拟掉电/进程崩溃后的完整性与允许的数据回退。

不得为了降低延迟静默将 durability 改为未审核模式。`journal_mode`、`synchronous`、auto-checkpoint 阈值和 retention 必须进入配置 manifest；任何调整通过故障注入和 ADR。

SQLite 延迟在首版为 `report_only`。如果 checkpoint 影响 1 秒/3 秒数据面 gate，应调整 checkpoint 调度或事务边界，不能放宽数据面 SLO。

## 17. 体积与依赖预算

| 产物 | 预算 |
|---|---:|
| 模块 ZIP | `<=60 MiB` |
| sing-box | `<=30 MiB` |
| nethopd | `<=12 MiB` |
| nethopctl | `<=4 MiB` |
| 默认 SRS、许可证、manifest | `<=10 MiB` |

以上属于 `budget_gate`。Phase 0-B 使用真实 arm64 strip 后产物校准；超出时必须提交符号/依赖分析和 ADR，不能直接修改数字。

报告至少记录：

- compressed/uncompressed bytes；
- SHA-256；
- Rust `cargo tree` 重复依赖；
- `cargo bloat` 主要符号/Crate；
- Go build tags 和主要符号；
- stats A/B 方案体积差；
- SRS 各数据源体积和许可证。

Go 固定工具链并使用上游 release flags、`-trimpath` 和最小 build tags。Rust release 性能基线固定为 `opt-level=3`、`lto="thin"`、`codegen-units=1`、`strip="symbols"` 和 `incremental=false`；报告记录完整 profile 及其 digest，避免不同构建参数混入回归比较。`panic="abort"` 只有在 supervisor/worker 崩溃恢复、诊断和 FFI 边界验证后采用；测试、fuzz、Miri/ASan 构建保留 unwind 和调试符号。禁止 UPX。

## 18. 稳定性与 soak

24 小时 soak 是参考设备 release candidate 的稳定性门槛，不属于 Phase 0-A 技术尖刺，也不要求在所有 SoC 上重复后才能发布范围受限的 Alpha。

24 小时真机 soak 至少包含：

- 500 active outbounds；
- 每分钟 stats 聚合；
- 周期性 DNS、TCP、UDP 探针；
- 每小时一次 selector 切换；
- 受控网络断开/恢复；
- 至少一次订阅更新和候选回滚；
- 屏幕亮灭与系统休眠周期。

报告：

- RSS/PSS 时间序列与线性趋势；
- CPU、wakeup 与 thermal status；
- goroutine/thread/fd 数；
- WAL、数据库和日志体积；
- worker/core restart 次数；
- stats delta 与负差事件；
- DNS/连接错误率。

参考稿中的“RSS 增长 `<=5%` 且 `<=5 MiB`”保留为候选稳定性门槛，在参考设备取得三轮数据后再决定是否冻结。出现无界增长、OOM、FD 泄漏、死循环或数据损坏时直接判功能失败，不等待数值门槛。

## 19. 阶段闸门

### 19.1 Phase 0-A：核心安全可行性

必须在一台当前可用参考真机完成：

1. 固定 sing-box v1.13.15、最小 tags 和目标 arm64 构建；
2. TPROXY IPv4/IPv6 最小数据路径、mark 防环路、IPv6 guard、精确清理和失败恢复直连；
3. 相邻 direct baseline 与 TPROXY 短时 smoke，记录吞吐保持率、RSS、CPU 和不可用窗口；
4. 安装、启动、核心崩溃和卸载后不遗留导致断网的规则；
5. Host/fake module 的网络计划所有权和回滚测试。

Phase 0-A 的性能结果是路线诊断：明显无界资源、死循环、泄漏或无法恢复属于失败；85%/80 MiB/3% 等完整 SLO 留到 Phase 0-B。此阶段不要求 TUN system/gvisor 全矩阵、500/2,000/10,000 全边界、stats A/B、双实例、UDP/QUIC/DNS 完整 characterization 或多设备复核。

### 19.2 Phase 0-B：参考设备 Alpha

必须完成：

1. 对实际启用的 TPROXY/TUN 路径执行功能、IPv4/IPv6 泄漏和崩溃恢复测试；
2. TPROXY TCP 四流 `>=85%`；启用且 capability 支持 TUN system 时达到 `>=75%`；
3. 500 active outbounds 下总 RSS `<=80 MiB`、空闲 CPU `<=3%`；
4. 5 MiB/10,000 节点稳定格式转换 `<=300 ms`、更新峰值 `<=110 MiB`；
5. 模块和二进制体积预算；
6. stats patch、双实例或其他可选能力只有在各自 feature gate 通过后才启用，未启用时不阻断安全 Alpha；
7. 生成精确到设备、ROM、内核和 Root 管理器的 support/performance manifest。

未通过安全与回滚门槛时不得继续堆叠 UI 或扩大格式；未通过可选 feature gate 时关闭对应能力，不把整个基础 Alpha 判死。

### 19.3 Phase 1：控制面与订阅

必须完成：

- 版本化 UDS 的 IPC 延迟与权限测试；
- 各格式标准 fixture 和 report schema；
- 500/2,000/10,000 边界行为；
- SQLite counter/bucket 事务和 checkpoint 测量；
- 候选失败不影响 last-known-good；
- 订阅转换和更新峰值硬门槛持续通过。

### 19.4 Phase 2：数据面

必须完成：

- 在参考设备能力允许范围内完成 TCP upload/download、IPv4/IPv6 数据面矩阵；不以缺少其他 SoC 设备判失败；
- selector/SRS/netfilter A/B 的 1 秒 gate；
- 完整激活/回滚的 3 秒 gate；
- UDP/QUIC/Hysteria2/TUIC full characterization；
- DNS cache、DoH direct/proxy characterization；
- core crash、worker crash、网络切换和 IPv6 guard reconcile。

### 19.5 Phase 3：发布

必须完成：

- 当前参考 runner 的至少三轮稳定 baseline 和 24 小时 soak；
- 发布实际声明为稳定支持的每个设备/Root 组合都有完整报告；
- Pixel/GKI、Qualcomm、MediaTek 和 Magisk/KernelSU 双平台是广泛稳定支持的覆盖目标；设备不足时保持 reference/community verified 的窄支持声明，不阻断范围受限发布；
- 性能报告、SBOM、provenance、二进制 digest；
- 当前发布实际启用并声明稳定的能力通过全部适用 hard gate；
- 所有 report-only 项有原始数据或明确 invalid 原因；
- 未解决回归有批准的 waiver，且不涉及安全、泄漏、数据损坏或已冻结硬门槛。

## 20. 回归、失效与 waiver

### 20.1 回归判定

硬门槛始终按绝对 SLO 判定。对于尚未冻结的指标，使用以下默认 triage 触发器，不自动等同于 release failure：

| 变化 | 处理 |
|---|---|
| 吞吐/transaction rate 相对匹配 baseline 下降 `>=5%` | `regression_suspected`，要求复测和分析 |
| p95 latency 上升 `>=10%` | `regression_suspected` |
| 稳态或峰值 RSS 增加 `>=5 MiB` | 依赖/heap/配置归因 |
| 空闲 CPU 增加 `>=0.5` 个百分点 | wakeup/采集周期归因 |
| 二进制增加 `>=5%` | 符号和依赖差异报告 |

这些触发器只适用于完全匹配的环境。统计噪声超过变化量时，结果标记 inconclusive，不允许选择性重跑直到通过。

### 20.2 invalid 条件

以下任一条件使对应测试组无效：

- thermal severity 进入节流状态或前后不一致；
- 前后 direct baseline 漂移超过 5%；
- 服务端 CPU/NIC/链路饱和；
- Wi-Fi 频段、信道或协商速率发生显著变化；
- 系统出现更新、备份或其他不可控高负载；
- release/debug 构建混用；
- fixture、工具版本、配置或 patch digest 缺失；
- 采样器失败、时钟回退或样本数不足；
- 模块进程意外重启，除非场景本身是恢复测试。

invalid 结果必须保留原始记录和原因，不参与中位数，也不得计为通过或失败。

### 20.3 waiver

waiver 必须包含：

- 指标和受影响设备/能力；
- 原始报告和复现步骤；
- 根因或已验证的范围；
- 用户可见影响；
- 回退/禁用方案；
- 到期版本与负责人；
- ADR 或 issue 链接。

waiver 不能覆盖流量泄漏、权限绕过、数据损坏、错误统计归因或上位文档冻结的 release gate。可选能力未过 feature gate 时应禁用该能力，而不是 waiver 后默认开启。

## 21. 报告格式

### 21.1 文件组织

每次运行至少产生：

```text
performance-report/
  manifest.json
  samples.jsonl
  summary.json
  artifacts/
    iperf3-*.json
    perfetto-*.pftrace
    logs-redacted.txt
```

`samples.jsonl` 每行一个不可变样本，`summary.json` 由样本生成，不能手工录入摘要数值。

### 21.2 manifest 最小字段

```json
{
  "schema": "nethop.performance-manifest.v1",
  "status": "measured",
  "started_at": "2026-08-01T00:00:00Z",
  "device": {
    "model": "fixture-model",
    "soc": "fixture-soc",
    "android_build": "fixture-build",
    "kernel": "fixture-kernel",
    "selinux": "enforcing",
    "netfilter_backend": "iptables-nft"
  },
  "build": {
    "nethop_revision": "fixture-revision",
    "profile": "release",
    "rust_toolchain": "fixture-rust-version",
    "rust_profile": {
      "opt_level": "3",
      "lto": "thin",
      "codegen_units": 1,
      "strip": "symbols",
      "incremental": false,
      "panic": "unwind",
      "sha256": "fixture-profile-sha256"
    },
    "sing_box_version": "1.13.15",
    "sing_box_commit": "fixture-commit",
    "build_tags": [
      "with_quic",
      "with_utls",
      "with_clash_api",
      "with_v2ray_api",
      "with_gvisor",
      "badlinkname",
      "tfogo_checklinkname0"
    ],
    "patch_manifest_sha256": "fixture-sha256"
  },
  "fixture": {
    "id": "tcp4-four-stream-v1",
    "sha256": "fixture-sha256"
  },
  "tools": {
    "iperf3": "fixture-version",
    "netperf": "fixture-version"
  }
}
```

尚未测量的字段使用显式 `null` 和 `status: not_measured`，不得填入估计值。

### 21.3 sample 最小字段

```json
{
  "schema": "nethop.performance-sample.v1",
  "metric": "tcp_throughput_bps",
  "mode": "tproxy",
  "ip_family": "ipv4",
  "direction": "download",
  "run": 1,
  "value": 850000000,
  "unit": "bit/s",
  "paired_direct_value": 1000000000,
  "retention_percent": 85.0,
  "validity": "valid",
  "thermal_status_start": "none",
  "thermal_status_end": "none",
  "artifact": "artifacts/iperf3-tproxy-tcp4-download-1.json"
}
```

### 21.4 自动判定

summary 必须为每项输出：

- SLO 等级；
- 适用或不适用原因；
- 有效/无效样本数；
- 统计量；
- threshold 和单位；
- `pass`、`fail`、`inconclusive`、`not_applicable`；
- degraded/unsupported 状态；
- 对应原始 artifact。

缺少 release gate 样本时结果是 `inconclusive`，发布仍被阻断，不能当作 `not_applicable`。只有未声明支持的可选能力才可 `not_applicable`。

## 22. 安全与隐私

性能 fixture、日志和报告不得包含：

- 真实订阅 URL 或 token；
- 节点密码、UUID、Reality key 等凭据；
- 用户真实域名访问历史；
- Android ID、IMEI、序列号、MAC 地址；
- 未经脱敏的 build property 全量转储；
- Clash/V2Ray API secret；
- 可关联用户身份的数据库内容。

设备标识使用发布前审核过的 allowlist；域名和节点使用受控 fixture。support bundle 和性能报告共用脱敏规则。debug 性能日志必须有超时恢复，不能为获得指标长期提高日志级别。

性能测试不得关闭 TLS 验证、放宽 UDS 权限或把控制 API 监听到非 loopback。为了跑分快而绕过安全边界的结果一律无效。

## 23. 参考工程吸收原则

从参考工程吸收以下可复用实践，不复制其产品边界：

- `MagicNet-main`：将 HTTP timing 拆为 DNS、TCP、TLS、server、transfer，并拒绝非单调或非有限时间值；NetHop 对 direct/proxy 字段使用不同语义，避免误导；
- `PathGuard-Next/tests/perf`：release build guard、稳定 JSONL schema、p95 和 peak memory 字段、benchmark schema 测试；
- `NetGuard/sub-parser`：fixture 驱动、超时边界、VmRSS 读取、稳定输出和 host/device 分层；
- `sing-box-testing`：只用来理解上游实现和迁移方向，不能把开发线 API 当作 v1.13.15 已发布契约。

这些参考只能影响测试组织和工程实现，不能覆盖本文档的 hard gate、Android 13+ 范围或安全约束。

## 24. 官方资料

以下资料用于校准工具语义和测量口径；实际测试固定具体版本与访问日期：

1. ESnet iperf3 manual：<https://github.com/esnet/iperf/blob/master/src/iperf3.1>
2. ESnet iperf3 FAQ：<https://software.es.net/iperf/faq.html>
3. Netperf 官方手册：<https://hewlettpackard.github.io/netperf/doc/netperf.html>
4. Linux `/proc` 文档：<https://docs.kernel.org/filesystems/proc.html>
5. Linux `smaps_rollup` ABI：<https://www.kernel.org/doc/Documentation/ABI/testing/procfs-smaps_rollup>
6. Android 性能评估：<https://source.android.com/docs/core/tests/debug/eval_perf>
7. Android thermal mitigation：<https://source.android.com/docs/core/power/thermal-mitigation>
8. Android Wattson/Perfetto 功耗分析：<https://source.android.com/docs/core/power/wattson/wattson>
9. Android 安全检查清单：<https://developer.android.com/privacy-and-security/security-tips>
10. SQLite WAL：<https://www.sqlite.org/wal.html>
11. SQLite checkpoint API：<https://www.sqlite.org/c3ref/wal_checkpoint_v2.html>
12. sing-box TUN：<https://sing-box.sagernet.org/configuration/inbound/tun/>
13. sing-box DNS：<https://sing-box.sagernet.org/configuration/dns/>
14. sing-box rule-set：<https://sing-box.sagernet.org/configuration/rule-set/>
15. sing-box Clash API：<https://sing-box.sagernet.org/configuration/experimental/clash-api/>
16. sing-box V2Ray API：<https://sing-box.sagernet.org/configuration/experimental/v2ray-api/>

## 25. 冻结结论

首版实现和评审必须遵守以下结论：

1. 85% TPROXY 吞吐、75% TUN system 吞吐、3% 空闲 CPU、80 MiB 稳态 RSS、110 MiB 更新峰值、300 ms 转换、1 秒轻量切换和 3 秒完整激活/回滚是发布门槛；
2. 300 ms 对每种标准 URI、JSON、YAML fixture 分别判定，不给 YAML 额外宽限；
3. 500 active outbounds 是运行性能基线，2,000 是兼容边界，10,000 只用于转换；
4. UDP、QUIC、DNS miss、RR/CRR 和恢复时延必须测量，但在稳定基线与 ADR 前不伪造跨设备硬阈值；
5. gvisor 是兼容回退，不用吞吐数字冒充 system；
6. RSS 使用 VmRSS 判 gate，PSS 用于诊断；CPU 使用 `/proc` 时间增量，热节流结果无效；
7. stats patch 必须同时通过正确性、`<=3%` 开销和 `<=5 MiB` 增量闸门；
8. 首版不提供修改全局 sysctl 的 performance profile；
9. 任何硬门槛变更必须先修改上位设计或批准 ADR，不能从报告脚本中静默变更；
10. release 缺少原始性能报告时不得发布。
