# NetHop 订阅解析器 Android 范围声明

> 状态：Alpha scope fragment
> 日期：2026-08-02
> 约束：[`02-subscription-import-and-parser-design.md`](./02-subscription-import-and-parser-design.md)

## 1. 兼容原则

NetHop 只面向 Android，但解析能力按输入内容和固定 sing-box 能力矩阵判定，不按客户端品牌猜测。客户端使用某个内核，不代表 NetHop 自动支持该内核的全部协议、配置字段或运行时语义。

格式兼容与协议兼容是两条独立门禁：输入必须先属于受支持容器，其中的节点还必须落入 NetHop 九协议白名单，并通过 sing-box 1.13.15 mapping、安全语义和 Android 验证。

## 2. 输入范围

| 机场/客户端入口 | 内容容器 | NetHop 路径 | Alpha 状态 |
|---|---|---|---|
| Mihomo / Clash Meta | Clash YAML | `ClashYamlAdapter`，只读顶层 inline `proxies` | stable parser |
| Clash Standard | Clash YAML | 与 Mihomo 共享 adapter，保留独立请求 profile | stable parser |
| SFA / sing-box for Android | sing-box JSON | `SingBoxJsonAdapter`，只读白名单终端 `outbounds` | stable parser |
| 通用 sing-box | sing-box JSON | 与 SFA 共享 adapter，保留独立请求 profile | stable parser |
| Surfboard Android | Surfboard INI | `SurfboardIniAdapter`，只读 `[Proxy]` | experimental；library 默认关闭，Android daemon 的 `subscription-update` 构建显式编译 |
| 任意 Android 客户端导出的 URI/Base64 | URI list / Base64 URI list | 客户端无关 fast path | stable parser |

Karing、Hiddify、NekoBox、FlClash、v2rayNG 等客户端若导出上表中的通用容器，NetHop 按内容解析；当前不建立品牌专用 adapter 或“完整兼容该客户端”的承诺。Stash、Surge、Shadowrocket、Quantumult X 和 tvOS 专用配置不在 Android Alpha 范围内。

`Mihomo` 与 `ClashStandard` 请求 profile 只提供不同的 `User-Agent`/`Accept`，共同进入 Clash YAML adapter。`SingBox` 与 `SingBoxAndroid` 同理共同进入 sing-box JSON adapter。profile 不决定格式，响应仍必须通过结构探测。

## 3. 协议范围

parser 白名单为 VLESS、VMess、Shadowsocks、Trojan、Hysteria2、TUIC、AnyTLS、HTTP 和 SOCKS。HTTP/SOCKS 当前只在 Clash/Mihomo YAML 与 sing-box JSON 中启用受控映射；`http(s)://` 与订阅 URL 载体有歧义，SOCKS 分享 URI 又没有统一字段契约，因此两者暂不进入 URI/Base64 scheme 白名单。SFA 或其他 sing-box 客户端还能运行 Naive、Mieru 等 outbound，不构成 NetHop 导入支持证据；这些节点当前返回 `unsupported_protocol`。

WireGuard 在 sing-box 1.13.15 中属于顶层 `endpoints`，不是 terminal outbound。它要求本地地址、私钥和 peer/allowed IP 等 L3 语义，不能无损放入当前 `ProxyNode -> TerminalOutbound` 模型。NetHop 会识别含 `endpoints` 的 sing-box JSON，并用 `non_node_section_ignored` 明确报告未导入；endpoint-only 配置得到零节点，混合配置只导入普通 terminal outbounds。支持 WireGuard 需要独立的 managed endpoint、secret 和路由所有权设计，不能通过扩大协议枚举直接启用。

Naive 的 sing-box 1.13.15 outbound 受 `with_naive_outbound` build tag 控制；当前模块内官方 Android arm64 CLI 二进制没有该 tag。alioth 真机最小配置检查明确返回 `naive outbound is not included in this build`，因此 Naive 保持 `unsupported`。Mieru 只在本轮对照的 Mihomo 源码中存在，sing-box 1.13.15 没有对应 outbound，NetHop 也保持 `unsupported`。两者都不能因其他 Android 客户端可运行而进入 parser 白名单。

HTTP 只支持 TCP CONNECT，可选成对 username/password、TLS、server name、path 和有界 headers；SOCKS 只支持 sing-box 原生 SOCKS 4/4a/5、可选成对认证、TCP/UDP network 与原生 UDP-over-TCP。Mihomo 的 SOCKS-over-TLS、certificate/private-key、dialer-proxy 等无法等价映射的语义必须返回 `unsupported_semantics`，不得静默丢弃。

parser 能生成节点也不等于发布数据面已经稳定支持。M004 及后续 release gate 必须为计划启用的每项协议绑定 sing-box 1.13.15 mapping manifest、`sing-box check`、Android 连通性和资源证据。HTTP/SOCKS 已在 alioth 上通过官方 Android arm64 sing-box 1.13.15 `check`，但尚未用受控远端完成协议级连通测试，因此 Android data-plane 仍是 `best_effort`。证据不足的协议保持 `experimental` 或 `unsupported`。

## 4. 设备与回退

当前性能证据仅来自 `alioth / Android 13 / arm64-v8a / Magisk 30.6`，不外推为所有 Android 设备。Surfboard 在该设备达到 p95 `142.892ms`、进程 `VmHWM=45,316KiB`，但因细粒度阶段分配证据未完成，library feature 继续默认关闭，支持等级保持 `experimental`。

实现状态补充：`nethop-subscription` 的默认 feature 集仍不包含 Surfboard，保持稳定 library 边界；`nethopd` 的默认 Android `subscription-update` feature 显式编译 `format-surfboard`，允许配置 `format_hint = "surfboard_ini"` 或由强结构证据自动探测。该接线已通过 `TOML -> source registry -> nodes-only adapter -> composer -> candidate check` 端到端测试，但支持等级仍为 `experimental`，不升级为跨设备稳定承诺。

不受支持的专用方言可以改用机场提供的 URI/Base64、Clash/Mihomo YAML 或 sing-box JSON。NetHop 不通过轮换客户端请求头猜格式，不递归下载 Clash provider，也不执行规则、策略组、脚本、远程 include 或客户端控制字段。
