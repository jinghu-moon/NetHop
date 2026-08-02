# NetHop 订阅导入与解析库设计

> 状态：Draft v0.7
> 日期：2026-08-02  
> 适用范围：NetHop Phase 0-A 至首个稳定版  
> 上位文档：[`00-nethop-system-design.md`](./00-nethop-system-design.md)  
> 性能约束：[`01-performance-budget-and-slo.md`](./01-performance-budget-and-slo.md)

## 1. 设计目标

NetHop 是 Android 13+ 的 root 透明代理模块。订阅解析库负责把二维码、文件、文本或 URL 得到的订阅内容转换为受约束的统一节点模型，再由配置 composer 生成 sing-box 1.13.15 可验证的 outbound fragment。

本库的核心目标：

1. 支持主流机场面向 Android 或跨平台客户端提供的订阅内容；
2. 将“输入载体”“容器格式”“代理协议”分离，避免为每个客户端名称复制解析器；
3. 对可映射节点逐节点转换，对不可映射节点给出稳定、可脱敏的诊断；
4. 不执行订阅中的路由、脚本、重写、策略组、远程 provider 或文件路径；
5. 保持稳定的 `node_id`、来源追踪、去重和 last-known-good 事务语义；
6. 在 5 MiB、最多 10,000 节点标准 fixture 上满足 `detect..serialize <=300 ms` 和更新峰值 RSS `<=110 MiB`；
7. 以 Rust 实现，支持 host 单元测试、Android 真机 benchmark 和未来 Kotlin App/CLI 复用。

当前没有多设备实验室。解析器的 Android 性能门槛先在 `00` 所定义的一台 `reference_verified` arm64 真机上验证；该结果不外推为所有 Android 13+ 设备的性能或格式兼容承诺。Host/AVD 负责确定性解析、资源限制和 IPC 流程，不能替代真机 RSS/CPU 证据。

非目标：

- 不实现完整 Mihomo、Surfboard 或 sing-box 运行时；
- 不适配只服务于 iOS/macOS 客户端的 Stash、Surge、Shadowrocket、Quantumult X 专用配置方言；其中通用 URI/Base64 节点仍按稳定容器导入；
- 不把客户端的完整策略配置迁移为 NetHop 路由配置；
- 不在 parser 中下载嵌套订阅、执行脚本或访问本地文件；
- 不为了“格式支持数量”伪造无法映射到 sing-box 的协议；
- 不让订阅内容修改 inbound、控制 API、netfilter、DNS 入口或模块脚本。

## 2. 上位约束与术语

### 2.1 规范优先级

冲突时按以下顺序处理：

1. 用户已确认的产品需求；
2. `00-nethop-system-design.md`；
3. 已批准的 ADR；
4. `01-performance-budget-and-slo.md`；
5. 本文档；
6. parser 实现与测试 fixture。

本文档可以细化实现，但不得放宽上位文档的安全、资源、协议和事务边界。

### 2.2 术语

| 术语 | 定义 |
|---|---|
| 输入载体 | 二维码解码结果、文件字节、用户粘贴文本或 HTTP 响应 |
| payload | 带来源元数据和有界字节的待解析输入 |
| 容器格式 | URI 列表、Base64 URI 列表、YAML、JSON、INI/snippet 等包装形式 |
| 客户端方言 | Android 相关的 Mihomo/Clash、Surfboard、sing-box 字段和语法变体 |
| 协议 | Android sing-box 数据面可验证的代理协议；首版导入白名单为 VLESS、VMess、Shadowsocks、Trojan、Hysteria2、TUIC、AnyTLS |
| `UnvalidatedNode` | 格式适配器提取但尚未完成语义校验的节点 |
| `ProxyNode` | 通过统一语义校验、可生成 sing-box outbound 的可信节点 |
| source | 用户配置的一条订阅来源，拥有独立缓存和状态 |
| generation | 多 source 合并后的候选或活动配置版本 |
| nodes-only | 只导入终端代理节点，不导入客户端规则和控制面语义 |

## 3. 总体架构

```text
QR / file / text / URL
          |
          v
Input acquisition and bounded payload
          |
          v
Content normalization and format sniffing
          |
          v
Format adapter / client dialect
          |
          v
UnvalidatedNode + NodeDiagnostic
          |
          v
Semantic validation and protocol capability check
          |
          v
Canonicalization -> fingerprint -> stable dedupe
          |
          v
ProxyNode + ConversionReport
          |
          v
sing-box outbound composer
```

首版保持一个 `nethop-subscription` crate，内部按职责分模块。只有后续出现明确的编译边界、复用需求或依赖隔离收益时才拆 crate，避免为格式数量制造工程复杂度：

| 模块 | 职责 | 禁止职责 |
|---|---|---|
| `model` | payload、诊断、IR、来源和报告类型 | 读取网络、执行命令 |
| `fetch` | HTTPS 获取、重定向、大小、SSRF 和缓存输入 | 解析节点语义 |
| `detect` | 有界格式探测、候选排序、强制 hint 校验 | 下载或自动猜测失败后执行危险回退 |
| `parser` | URI、Base64、YAML、JSON、INI/snippet 方言适配器 | 生成完整 sing-box 配置 |
| `normalize` | 字段规范化、语义校验、fingerprint、去重 | 修改活动 generation |
| `compose` | 从可信 `ProxyNode` 生成 outbound fragment | 读取原始客户端策略 |
| `nethop-core` | 多 source 事务、last-known-good、active limit | 复制格式解析逻辑 |

解析库可以在 CLI、`nethopd` 和未来 Kotlin App 的 native bridge 中复用。Android App 的二维码相机不进入 Rust parser crate。

### 3.1 依赖选型与轻量化原则

订阅库同时承担不可信输入解析、协议语义校验和有界转换，不能用“依赖数量最少”替代安全和可维护性；也不能为了方便把完整异步网络栈、数据库、二维码识别或客户端配置框架放进 parser。

选型原则：

1. 核心 parser 不依赖网络、异步 runtime、SQLite、Android UI 或 sing-box 进程；
2. 每个运行时依赖必须对应一个不可由简单标准库实现安全替代的职责；
3. 可选能力使用 Cargo feature 隔离，关闭后不得出现在纯 parser 构建的依赖树和二进制中；
4. 首选纯 Rust、无 C/系统库链接、Android arm64 可复现构建的 crate；
5. 不启用与本项目无关的默认 feature，例如 cookies、brotli、charset、TLS native backend 或 JSON map 顺序保持；
6. 依赖版本、许可证、features、源码 digest 和构建结果写入 SBOM/provenance；
7. 最终以 Android arm64 release 的体积、RSS、转换耗时和 fuzz 稳定性决定是否保留，而不是凭主观印象判断轻量。

推荐的单 crate feature 形态：

```toml
[features]
default = [
  "parser",
  "format-uri",
  "format-base64",
  "format-clash-yaml",
  "format-singbox-json",
]
parser = []
format-uri = ["parser"]
format-base64 = ["parser", "format-uri"]
format-clash-yaml = ["parser", "dep:serde-saphyr"]
format-singbox-json = ["parser"]
format-surfboard = ["parser"]
experimental-formats = ["format-surfboard"]
fetch = ["parser", "dep:flate2", "dep:ureq", "dep:url"]

[dependencies]
serde = { version = "1", default-features = false, features = ["derive", "std"] }
serde_json = { version = "1", default-features = false, features = ["std", "raw_value"] }
serde-saphyr = { version = "1", optional = true, default-features = false, features = ["deserialize"] }
base64 = { version = "0.23", default-features = false, features = ["std"] }
url = { version = "2", optional = true, default-features = false, features = ["std"] }
percent-encoding = { version = "2", default-features = false, features = ["std"] }
sha2 = { version = "0.11", default-features = false }
uuid = { version = "1", default-features = false }
thiserror = "2"
zeroize = { version = "1", default-features = false, features = ["alloc", "derive"] }

flate2 = { version = "1", optional = true, default-features = false, features = ["rust_backend"] }
ureq = { version = "=3.3.0", optional = true, default-features = false, features = ["rustls"] }

[dev-dependencies]
criterion = { version = "0.8", default-features = false, features = ["cargo_bench_support"] }
proptest = { version = "1", default-features = false, features = ["std"] }
rustls = { version = "=0.23.43", default-features = false, features = ["ring", "std", "tls12"] }
tempfile = "3"

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = "symbols"
incremental = false
```

版本号是选型基线，不是未经审计的最终锁定值。实现时必须解析 `Cargo.lock`，选择通过许可证、Android arm64、MSRV、fuzz 和性能门禁的精确版本；不得使用 `*` 或在发布构建中漂移 lockfile。

首版明确不启用 `base64 0.23` 的 `simd-unsafe`，也不保留 `base64-simd` Cargo feature 或 Phase 0 对比任务。订阅输入上限只有 5 MiB，Base64 decode 预算仅占整条转换的 20 ms；在没有真实端到端 profile 证明它是瓶颈时，为 NEON 快路径增加 unsafe、Miri/ASan 和专项 fuzz 成本不符合 YAGNI。未来只有已发布版本的 profile 证明 decode 持续主导总时延时，才允许通过独立 ADR 重新立项，不能把 dormant feature 留在首版代码中。

`[profile.release]` 必须由 workspace 根统一定义，避免 CLI、daemon 和 bench 使用不同参数。`opt-level=3` 优先满足 300 ms CPU 密集转换门槛，Thin LTO 与单 codegen unit 在可接受构建时间内兼顾跨 crate 优化和体积，strip 只作用于发布产物。`panic="abort"` 不写入初始基线：只有 supervisor/worker 崩溃后能够可靠撤销网络接管、恢复 last-known-good，且不存在 panic 跨 FFI 边界时才启用；测试、fuzz、Miri/ASan 构建保留 unwind 和调试符号。release 报告必须记录完整 profile、Rust/Cargo 版本和 profile digest。

#### 依赖分层

| 依赖 | 所属层 | 运行时用途 | 轻量化判断 |
|---|---|---|---|
| `serde` | model/parser/compose | typed IR、诊断、配置 fragment | 必需；derive 仅用于明确模型 |
| `serde_json` | detect/parser/compose | sing-box JSON、报告和输出 | 必需；不启用 `preserve_order` |
| `serde-saphyr` | YAML adapter | 带 budget、alias limit 的 typed YAML 解析 | 只启用 `deserialize`；禁止默认的 YAML serialize 及 include/property 扩展 |
| `base64` | detect/parser | 标准/URL-safe Base64 | 必需；首版固定无 unsafe 的通用 engine，不启用或评估 `simd-unsafe` |
| `url` | optional `fetch` | source URL、重定向 URL、host/port/IDNA 规范化 | 只进入 fetch；不用于逐节点 URI fast path |
| `percent-encoding` | URI parser | 协议字段的一次性百分号解码 | 保留；小型无运行时框架 |
| `sha2` | normalize/report | source/fixture/发布摘要；canonical fingerprint 默认基线 | 关闭 `alloc`/OID 默认 feature，使用 no_std `Sha256` core；即使 fingerprint 改用 BLAKE3，供应链摘要仍需要 SHA-256 |
| `uuid` | protocol validation | VLESS/VMess/TUIC UUID 解析和格式校验 | 仅解析，不启用 RNG、版本生成、serde 或 `std` feature |
| `thiserror` | all | 稳定错误类型和匹配码 | 必需；无常驻运行时框架 |
| `zeroize` | model/normalization | 敏感临时缓冲清零 | 条件保留；只包裹明确的 secret 类型 |
| `flate2` | optional `fetch` | 手动 gzip 解压，并分别限制压缩输入和解压输出 | 只启用 Rust backend；替代不可审计双层大小的透明解压 |
| `ureq` | optional `fetch` | 低频 HTTPS 订阅下载 | 可选；关闭 feature 时不进入 parser binary |

#### 为什么不使用 `reqwest`/`tokio`

订阅下载是 worker 的低频、受控任务，不是高并发代理数据面。`ureq` 提供同步 HTTP 和 Rustls，`flate2` 负责可独立计数的 gzip 解压，足以覆盖 source fetch；引入 `reqwest + tokio + hyper` 会扩大依赖树、编译时间和 Android 交叉编译面，并在常驻 worker 中引入不必要的异步 runtime。

如果未来出现并发下载、多 source 取消或 daemon 已经统一使用 Tokio 的事实，应先做依赖 ADR，比较：

- fetch feature 关闭后的纯 parser 二进制；
- fetch feature 开启后的二进制增量；
- 10 个 source 并发更新的 CPU/RSS；
- 取消、超时、重定向和 SSRF 行为；
- 与 nethopd 既有 runtime 的重复开销。

在这些数据出来之前，保持同步 `ureq`。

`ureq 3.3.0` 的普通配置已经提供 `https_only`、分阶段/全局 timeout、response header 上限和连接池上限；`BodyWithConfig` 提供读取上限。但 NetHop 的 SSRF 地址固定需要 `Agent::with_parts`、自定义 `Resolver`/`Connector`，这些类型位于 `ureq::unversioned`，上游明确说明它们不遵循通常 semver，minor release 即可能破坏。fetch 安全适配层因此必须固定精确 `ureq` 版本；任何 patch/minor 升级都重新编译并运行 resolver、redirect、peer、TLS SNI、gzip bomb 和连接池测试，不能只凭 `cargo update` 通过。

#### 为什么使用 `serde-saphyr`

Clash/Mihomo YAML 是不可信输入，解析器必须能限制：

- reader bytes、events、nodes、documents 和 scalar bytes；
- nesting depth；
- anchor/alias 数量、展开次数和 replay depth；
- merge keys、重复 key 和 YAML schema 行为。

`serde-saphyr` 的 `Options`、`Budget` 和 `AliasLimits` 可以把这些约束绑定到一次解析。源码审计确认其默认 feature 同时启用 `serialize` 与 `deserialize`；NetHop 只需要后者，必须使用 `default-features = false, features = ["deserialize"]`。禁止启用 `serialize`、`include`、`include_fs`、`properties`、`robotics`、`figment`、`miette`、`garde`、`validator` 和 `serde_derived_types`。这既阻断文件/外部解释语义，也移除 YAML serialize 路径的 `base64`、`zmij` 和 `nohash-hasher`。

`deserialize` 本身仍固定引入 `granit-parser`、`smallvec`、`encoding_rs_io`、`num-traits` 和 `annotate-snippets`。即使运行时关闭 snippet 渲染，当前版本也不能通过 feature 移除 `annotate-snippets`，所以不得把它描述为零成本依赖。Phase 0-A 先完成源码、feature tree 与 Host 安全行为审计；Phase 0-B 再以 Android arm64 产物和 peak RSS 判断是否接受。当前选型基线为 `1.x`，两个阶段合计必须验证：

1. Clash/Mihomo 的实际 YAML 语法、merge key 和重复 key 行为；
2. 行列位置是否足够支持 `NodeDiagnostic`；
3. 5 MiB/10,000 节点的转换时间和 peak RSS；
4. alias bomb、深度和超长 scalar 的拒绝时间；
5. Android arm64 release 构建、许可证和传递依赖。

只有在这些测试失败时，才评估 `yaml-rust2` event parser 等备选方案。首版不同时保留两个 YAML 库。

#### `url` 的体积取舍

源码确认 `url 2.5.8` 即使关闭默认 feature、只启用 `std`，仍固定依赖 `form_urlencoded`、`idna`、`percent-encoding`；`idna` 又启用 compiled data 并带入 ICU4X 数据与类型链。HTTP URL 的 scheme、host、port、IPv6、重定向和 SSRF 判断不能用字符串拼接替代，因此完整 `url` 保留在可选 `fetch` feature 中。代理节点 URI 只有七个冻结 scheme，走有界协议语法解析和 `percent-encoding`，不为每行构造 `url::Url`。

Phase 0-A 记录依赖与 feature tree，Phase 0-B 补充 Android 产物数据：

- `url` 的 feature tree 和 transitive crate；
- fetch 开关前后的体积差；
- ASCII/Unicode host 的解析行为；
- Android arm64 RSS 与转换耗时。

parser-only 依赖树出现 `url`、`idna` 或 ICU4X 即视为 feature 泄漏并阻断发布。fetch 仍必须使用完整 URL parser；不能为节省体积而手写或放宽 HTTP URL 安全检查。任何替代方案必须保持等价的 WHATWG/IDNA、IPv6、userinfo、重定向和 SSRF 测试。

#### `zeroize` 的边界

`zeroize` 源码使用 volatile write/set 和 optimization barrier，并为 `Zeroizing<T>` 在 `Drop` 时调用 `zeroize`；`Vec` 会清理当前长度和剩余 capacity，但明确不能清理历史 realloc 留下的旧副本。NetHop 使用公开 feature 名 `derive`，不直接依赖内部包名 `zeroize_derive`。它只保证被包裹对象在明确生命周期结束时尽力清零，不能保证 serde 临时值、格式化字符串或其他副本不存在。因此：

- parser 不把 secret 写入日志、错误和 fingerprint debug；
- 用专用 `SecretString`/协议凭据类型限制复制；
- 在完成 source conversion 后尽早释放原始 body 和临时解码 buffer；
- 不宣称“使用 zeroize 即可消除所有内存残留”。

#### 不引入通用 INI/正则/二维码库

- Surfboard 需要保留行号、逗号边界、转义和方言语义；使用小型本地 tokenizer 比通用 INI crate 更可控；
- 首版不使用 `regex` 处理不可信用户表达式，避免 ReDoS 和额外代码体积；过滤和节点名称策略由受审核的简单匹配实现；
- 二维码由未来 Kotlin App 的 ML Kit/平台能力完成，Rust 只接收 barcode raw value；
- 不引入完整 Clash/Surfboard 配置框架，因为它们会把策略、脚本、provider 和外部资源带入 root 控制面。

#### 纯 parser 不应携带的依赖

以下依赖属于其他模块，不进入 `nethop-subscription` 默认构建：

| 依赖/类别 | 所属模块 | 原因 |
|---|---|---|
| `rusqlite`/SQLite binding | `nethopd` storage | 统计和 source 状态存储，不属于纯转换 |
| UDS/IPC crate | `nethopd`/CLI | parser 接收结构化 request，不监听 socket |
| `clap`/命令行框架 | `nethopctl` | CLI 解析不应污染库依赖 |
| `tokio`/async runtime | daemon（未来如有需要） | 不能因 fetch 引入常驻 runtime |
| `ring`/平台 TLS 绑定 | 仅由 `ureq` 的 rustls feature 间接使用 | 含 C/汇编和 `build.rs`；只进 fetch，并验证 NDK/clang/arm64 |
| QR/图像识别库 | Kotlin App | Android UI 和相机职责 |
| `simd-json` | 暂不引入 | 需要可变 buffer/额外平台评估，当前 300 ms 目标不要求 |
| 直接 `smallvec`/`indexmap` | 暂不引入 | `smallvec` 已由 YAML 间接使用；业务代码只有 benchmark 证明收益后才直接依赖 |

### 3.2 源码快照审计结论

本次设计依据的源码快照为 `refer/subscription-import-and-parser-refer/snapshots/20260802-014712`。它包含 14 个直接 crate、完整 `Cargo.lock` 和 `vendor/` 源码；快照用于审计候选最新版，不是最终发布 manifest。

当前快照不包含 BLAKE3。它只在 16.1 节条件满足时作为一次性 benchmark 候选加入测试闭包；未经源码、feature、unsafe/build script、许可证和 Android arm64 审计，不得直接进入正常依赖或发布构建。该候选不改变本节对现有 14 个直接 crate 的事实统计。

| 类别 | 快照结果 | 设计结论 |
|---|---:|---|
| 直接依赖 | 16 | 12 个运行时候选，4 个 dev-only；测试用 `rustls` 与 fetch 传递版本一致 |
| `Cargo.lock` package | 181 | 包含 target/dev/build 候选，不能等同 Android 运行时依赖数 |
| 当前未裁剪 normal closure | 88 个唯一 package/version | 同时包含 fetch、ICU、Rustls、YAML serialize 等，不是 parser-only 目标 |
| normal + build + dev closure | 146 个唯一 package/version | criterion/proptest/tempfile 及其依赖不进入 release binary |
| vendor 源码体积 | 310,969,912 bytes | 只反映离线源码闭包，不作为 APK/模块体积指标 |

快照解析到的直接版本为：`serde 1.0.229`、`serde_json 1.0.151`、`serde-saphyr 1.0.0`、`base64 0.23.0`、`url 2.5.8`、`percent-encoding 2.3.2`、`sha2 0.11.0`、`uuid 1.24.0`、`thiserror 2.0.19`、`zeroize 1.9.0`、`flate2 1.1.9`、`ureq 3.3.0`；dev-only 为 `criterion 0.8.2`、`proptest 1.11.0`、`rustls 0.23.43`、`tempfile 3.27.0`。许可证均为 `MIT OR Apache-2.0` 或等价顺序，但最终仍以完整 SBOM 的传递许可证为准。

源码闭包揭示以下必须量化的重复版本：

- NetHop 直接使用 `base64 0.23.0`，`ureq/ureq-proto` 固定使用 `base64 0.22.1`；fetch-enabled 构建会包含两版。不得为了表面去重回退直接依赖，先测二进制增量并关注上游升级；
- TLS normal closure 使用 `getrandom 0.2.17`，dev-only 的 proptest/tempfile 还带入 `0.3.x/0.4.x`；后两者不计入 release；
- derive/ICU 链同时存在 `syn 2.x` 与 `syn 3.x`，均为编译期 proc-macro 闭包，不直接构成 Android 常驻 RSS，但影响离线源码和构建时间。

`ureq` 的 `rustls` feature 选择 Rustls + WebPKI roots + `ring` provider。`ring 0.17.14` 包含 C、汇编、pregenerated source 和 `build.rs`，因此“首选纯 Rust”不是对 fetch 闭包的事实描述。最终构建必须记录 Rust、Cargo、Android NDK、clang、target triple 和最低 API level，并在 arm64-v8a 上做可复现构建；不得静默切换到 native-tls、平台 verifier 或 AWS-LC。

当前快照的 `.cargo/config.toml` 将 vendored directory 写为 `../vendor`，而源码实际位于快照内的 `vendor/`。本次树统计使用一次性 Cargo config override 完成；后续快照生成器必须修正路径，并以不带 override 的 `cargo metadata/tree --locked --offline` 成功作为完整性门禁。失败的离线构建不能作为已归档、可复现的供应链证据。

### 3.3 依赖预算与验收门禁

依赖是否“轻量”必须由可复现数据判定。Phase 0-A 为 parser 核心和 fetch feature 分别生成 Host 依赖闭包报告，Phase 0-B 补齐 Android arm64 产物、RSS 和时延报告：

| 项目 | 要求 |
|---|---|
| lockfile | `Cargo.lock` 固定；未审计漂移阻断发布 |
| 直接依赖 | 每项必须在本文档或 ADR 中有用途 |
| 传递依赖 | 记录数量、许可证、来源和 feature 路径 |
| parser-only 产物 | 不包含 `ureq`、Rustls、压缩和 daemon 依赖 |
| fetch 增量 | 单独报告 `ureq + rustls + gzip` 的二进制差值 |
| feature 泄漏 | parser-only 不出现 `url`/IDNA/ICU4X；YAML 不出现 serialize/include/property 依赖 |
| native build | fetch 的 `ring` 在固定 NDK/clang/arm64-v8a 上可复现 |
| Android arm64 | release 构建可重复，记录 Rust/NDK/API level |
| release profile | 固定 `opt-level=3`、Thin LTO、单 codegen unit、strip 和 profile digest；回归样本不得混用 profile |
| parser 性能 | 标准 fixture `detect..serialize <=300 ms` |
| 更新峰值 | parser/fetch 候选窗口总 RSS `<=110 MiB` |
| 许可证 | SPDX 清单完整，无未知或不允许许可证 |
| 安全 | `cargo deny`/等价审计无未处理 advisory |

建议的验证命令：

```text
cargo tree --locked -e normal
cargo tree --locked -e normal,features
cargo metadata --locked --format-version 1
cargo deny check advisories bans licenses sources
cargo bloat --release --crates
```

CI 必须至少运行以下构建组合：

```text
--no-default-features --features parser,format-uri,format-base64,format-clash-yaml,format-singbox-json
--no-default-features --features parser,experimental-formats
--no-default-features --features parser,format-uri,format-base64,format-clash-yaml,format-singbox-json,fetch
```

第一组是首版稳定 parser 基线，第二组验证扩展格式，第三组测量 fetch 增量。`fetch` 显式依赖 `parser`，但不得通过默认 feature 或其他隐藏 feature 规避体积、许可证和依赖树检查。

生产、fetch、dev/test 三种闭包必须分开归档。统一对整个 workspace 执行 `cargo tree -e all` 会把 proc-macro、host build、target-specific 和 dev-only package 混为一谈，不能据此判断模块运行时是否轻量。“选择最新版本”只定义候选发现策略；最新版仍须通过上述门禁，失败时固定最后一个已验证版本并记录 ADR，而不是降低性能或安全门槛。

### 3.4 依赖变更流程

新增或升级依赖必须提交依赖变更记录，包含：

1. 功能缺口和为什么标准库/现有依赖不能满足；
2. crate 许可证、维护状态、MSRV、unsafe/FFI 和传递依赖；
3. parser-only/fetch/daemon 三种构建的体积、RSS 和 benchmark 差异；
4. Android arm64、host、fuzz 和故障注入结果；
5. 回退方案和上游升级影响；
6. `Cargo.lock`、SBOM 和 provenance 更新。

不接受仅以“API 更方便”“社区常用”或“本地测试通过”为理由的依赖增加。

## 4. 输入载体模型

### 4.1 载体与格式正交

同一个内容可以通过二维码、文件或 URL 进入；同一 URL 也可能返回 URI 列表、Base64、Clash YAML 或 sing-box JSON。因而不得根据输入载体直接指定 parser。

```text
carrier: qr | file | text | url
content_format: auto | uri_list | base64_list | clash_yaml | singbox_json | ini_profile | quantumultx
```

`carrier` 只影响获取、权限和审计；`content_format` 影响探测和解析；`protocol` 由节点字段决定。

### 4.2 `ImportPayload`

逻辑模型如下：

```text
ImportPayload {
  origin: PayloadOrigin,
  bytes: bounded byte buffer,
  declared_content_type: optional MIME,
  expected_format: optional FormatHint,
  source_id: optional SourceId,
  source_url_digest: optional Digest,
  received_at: monotonic and wall-clock metadata,
  transport: optional FetchMetadata
}
```

`PayloadOrigin` 至少包含：

- `QrRawValue`：二维码解码得到的原始字符串；
- `LocalFile`：文件名只用于诊断，路径不进入 parser；
- `PastedText`：用户粘贴内容；
- `HttpResponse`：URL digest、状态码、Content-Type、响应长度和最终 scheme。

原始 URL、Authorization、Cookie、token、二维码内容和文件路径不得出现在普通日志或错误消息中。报告只保留 `source_id`、URL digest 和脱敏的 origin 类型。

### 4.3 二维码边界

二维码扫描由未来 Kotlin App 完成：相机模块得到 barcode raw value 后，将 UTF-8 文本通过版本化本地 IPC 交给 daemon。Android 官方 Barcode API 区分 `rawValue` 与显示文本；NetHop 必须使用原始值，不使用经过 UI 格式化的显示文本。

Rust 侧只处理：

1. 去除 UTF-8 BOM 和首尾空白；
2. 判断 raw value 是 URL、单节点 URI、URI 列表还是其他受支持内容；
3. 按与文件/文本相同的规则进入 parser。

二维码不得触发：

- Android Intent；
- 浏览器打开；
- 自动下载多个 URL；
- 解析 QR 中嵌入的脚本、HTML 或客户端专用动作；
- 未经用户确认的 source 持久化。

首版 CLI 可接受 `--text` 或文件输入，二维码 UI 属于未来 Kotlin App。

## 5. URL 获取层

URL 获取由 `nethop-subscription` 的可选 `fetch` 模块完成，解析器只接收已经有界的 bytes。首版不为 fetch 单独拆 crate；关闭 `fetch` feature 后，HTTP、TLS 和压缩相关依赖不得进入纯 parser 构建。

### 5.1 默认限制

| 项目 | 默认值 |
|---|---:|
| 压缩响应读取上限 | 5 MiB |
| 解压后响应大小 | 5 MiB |
| HTTPS 重定向 | NetHop 手动最多 5 次；`ureq.max_redirects = 0` |
| connect timeout | 5 秒 |
| total timeout | 30 秒 |
| 单次读取块 | 有界，具体值由实现 benchmark 校准 |
| response header 总大小 | 64 KiB |
| Content-Encoding | 只允许 identity/gzip；压缩输入和解压输出各自受上限，首版不为 deflate/brotli 增加依赖 |

### 5.2 安全策略

- 只允许 HTTPS；不得提供关闭 TLS 验证、允许明文 HTTP 或从 HTTPS 降级到 HTTP 的配置开关；
- TLS 证书验证不可关闭；节点字段中的 `skip-cert-verify` 不影响订阅下载器；
- 每次重定向重新执行 scheme、DNS/IP、端口和 SSRF 校验；
- loopback、link-local、RFC1918、ULA、未指定地址和 metadata 地址默认拒绝；
- 允许 LAN source 时必须显式标记、记录目标范围，并仍限制端口和重定向；
- 不执行 URL 中的用户名、密码或路径脚本；
- URL query token 只在请求层存在，进入日志、报告或错误前必须脱敏；
- 订阅下载可经 NetHop loopback proxy inbound，直连回退必须显式记录；
- response Content-Type 只是探测提示，不能绕过内容安全校验。

`ureq/gzip` 的透明解压会在调用方取得响应前移除原始 `Content-Encoding` 和压缩态 `Content-Length`；只对返回的 body 设置 `.limit(5 MiB)` 不能证明压缩输入也受限。NetHop 因此不启用 `ureq/gzip`，改用同一 fetch feature 下的 `flate2` 手动解压：

1. 在响应头阶段拒绝超过压缩输入上限的已知 `Content-Length`；
2. 用 bounded read loop 读取原始 body，发现第 `compressed_limit + 1` 个字节立即失败；
3. 仅接受 identity/gzip；gzip 输入交给 `flate2::MultiGzDecoder`，在 decoder 输出外再次执行 bounded read loop；
4. 对 identity 响应仍执行相同的原始输入上限，不创建无界副本；
5. 禁止 `read_to_string()`/`read_to_vec()` 的无参默认上限路径，禁止依赖 ureq 默认 10 MiB 读取限制作为 NetHop 5 MiB 策略。

`ureq` 默认最多保留 10 个 idle connections、每 host 3 个，订阅更新是低频任务，fetch Agent 必须将连接池总数和 per-host 数量降到最小，并在一次 source/mirror 事务结束时释放 Agent 或显式清理连接。重定向由 NetHop 逐跳执行并重新创建受审核 request，不能把 ureq 默认的自动重定向当作 SSRF 策略。

首版直连 fetch Agent 冻结 `https_only = true`、`max_redirects = 0`、`redirect_auth_headers = Never`、`save_redirect_history = false`、`proxy = None`、`max_idle_connections = 0`、`max_idle_connections_per_host = 0`，并显式设置 global/resolve/connect/recv-response/recv-body timeout 和 64 KiB response header 上限。输入/输出 buffer 大小以 Android arm64 benchmark 校准，不继承 128 KiB 默认值后直接宣称轻量。经 NetHop loopback proxy 的 fetch 必须提供等价的目标 IP 固定、TLS SNI/证书验证和 peer 证据；做不到时标记 `proxy_fetch_security_unsupported`，不能绕过 SSRF 约束。

连接安全不能只依赖 hostname 的一次解析。实现必须对每次请求和重定向执行：

1. 自定义 `Resolver` 解析 hostname；每个候选 IP 都通过 loopback、link-local、私网、ULA、metadata 和未指定地址检查，且限制在 ureq 的最多 16 个 `SocketAddr` 槽位内；
2. 通过 `Agent::with_parts` 接入受审核 `Connector`，只向通过检查的地址建立连接，并在 TCP/TLS 连接建立后核对实际 peer address；
3. DNS 结果、连接地址和重定向 URL 不得跨请求无限复用；
4. `ureq::unversioned` API 不提供通常 semver 保证；若固定版本的 connector 适配层不能完成 peer 校验，fetch feature 必须拒绝该请求，不能退化为只检查字符串 hostname。

这组约束用于防止 DNS rebinding 和重定向绕过 SSRF 策略，不能由 source 配置关闭。

### 5.3 缓存与事务

下载器先写 source 临时 body，完成大小、digest、格式探测和解析后才提交缓存。失败时保留 source 的 last-known-good body/config，不覆盖活动 generation。

```text
download temp
  -> bounded read/decode
  -> digest
  -> parse/validate
  -> candidate source cache
  -> multi-source compose
  -> sing-box check
  -> publish or rollback
```

### 5.4 主地址与 Backup 镜像

机场标注的 `Mihomo (Backup)`、`Clash (Backup)` 是下载端点镜像，不是新容器格式。一个逻辑 source 可以配置：

```text
SourceEndpointSet {
  primary,
  mirrors[] { url, priority },
  expected_format,
  request_profile
}
```

默认 `mirrors` 最多 3 个；单个 endpoint 使用 30 秒总超时，整个 source 的端点尝试窗口默认不超过 90 秒。主端点一旦得到一个或多个可接受节点，不再尝试镜像；只有网络/HTTP 错误、超限、格式错误或 `accepted == 0` 才进入下一个镜像。可用的 `ETag`/`Last-Modified` 仅用于条件请求，不能改变 source digest、解析和事务语义。

主端点发生网络错误、HTTP 错误、响应超限、格式错误或全节点失败时，才按顺序尝试镜像；最多端点数量必须有界。成功镜像与主地址共享同一 `source_id`、缓存和 last-known-good，不把两份相同节点作为两个 source 合并。

镜像必须与主端点声明相同的 expected format 和用户授权范围。NetHop 不根据 URL 中的 `/mihomo/`、`/clash/` 等路径自动推导或改写备用地址，避免把供应商命名约定写进 parser。

### 5.5 客户端请求 profile

部分机场根据 URL 路径、User-Agent 或 `Accept` 返回不同表示。请求 profile 与检测格式必须分离：

```text
RequestProfile =
  NetHopGeneric
  | Mihomo
  | ClashStandard
  | Surfboard
  | SingBox
  | SingBoxAndroid
```

请求 profile 只决定内建的 User-Agent/Accept 组合，不决定 parser，也不证明响应格式正确。`Mihomo` 与 `ClashStandard` 都返回 Clash YAML，但保留独立 UA/Accept 以适配机场的 Android 客户端分发策略；两者共享 `ClashYamlAdapter`。`SingBox` 是通用 sing-box 请求，`SingBoxAndroid` 对应 SFA/Android 请求，二者共享 `SingBoxJsonAdapter`。自动模式默认只请求一次 `NetHopGeneric`；不通过轮换多个客户端 UA 猜测内容，以免重复请求或触发供应商风控。

用户自定义 header 采用 allowlist。`Authorization`/Cookie 作为 secret 字段独立存储；禁止覆盖 `Host`、`Connection`、`Content-Length`、`Transfer-Encoding`、`Proxy-Authorization` 等 hop-by-hop 或安全敏感 header。

## 6. 内容规范化与格式探测

### 6.1 规范化

在探测前只做无语义的安全处理：

- 去除 UTF-8 BOM；
- 识别并统一 `CRLF`/`CR`/`LF` 行尾；
- 保留原始字节 digest；
- 计算 trimmed view，不修改原始 body；
- 检查 UTF-8；必要时按明确的 Base64/URL-safe Base64 规则处理 ASCII 输入。

不得在探测阶段把任意文本转换为小写、替换 `+`/`/`、删除所有标点或解码 URL 百分号，因为这些操作可能破坏凭据和路径。

### 6.2 探测顺序

自动探测按“结构证据优先、昂贵操作最后”执行：

1. 读取显式 `expected_format`；
2. 空输入、超限输入、非 UTF-8 直接拒绝；
3. URI scheme 行首证据；
4. JSON 顶层结构证据；
5. YAML/Clash `proxies` 结构证据；
6. Surfboard INI section/header 证据；
7. 最后尝试有界 Base64 解码，并对解码结果重新探测；
8. 仍无法确定时返回 `unknown_format`，不静默尝试所有 parser。

### 6.3 证据规则

| 证据 | 候选格式 |
|---|---|
| 第一条有效内容以 `ss://`、`vmess://`、`vless://`、`trojan://`、`hysteria2://`、`hy2://`、`tuic://`、`anytls://` 开始 | URI list |
| 顶层 JSON object 且存在 `outbounds` 数组 | sing-box JSON |
| 顶层 JSON array 且元素具有 `type`/`tag` 等 outbound 字段 | sing-box outbound array |
| YAML map 的 `proxies` 为 sequence | Clash/Mihomo YAML |
| `[Proxy]` 与 Surfboard profile section 结构 | Surfboard INI |
| Base64 解码后可重复识别为 URI list/JSON/YAML | Base64 wrapped format |

包含 `proxies:` 的字符串但 YAML 解析失败，必须返回 YAML 诊断；不能退回 Base64。JSON 解析失败但首字符为 `{` 时，也必须报告 JSON 结构错误。这样可以避免恶意输入通过失败回退消耗大量 CPU。

多个候选同时拥有强结构证据时返回 `ambiguous_format`。INI 自动探测只面向 Surfboard；仅出现通用 `[Proxy]` 而没有足够 Surfboard 证据时，保持 `ini_profile` 候选并要求 `surfboard_ini` hint，不尝试猜测 iOS/macOS 方言。

### 6.4 强制格式 hint

用户或 source 可指定：

```text
uri_list
base64_list
clash_yaml
singbox_json
surfboard_ini
```

强制 hint 只减少歧义，不绕过安全限制。内容与 hint 不匹配时返回 `format_hint_mismatch`，不自动切换其他 parser，除非 CLI 明确使用 `--format auto`。

## 7. 格式适配器总览

机场常见展示名称不是 parser 类型。首版归类如下：

| 展示名称 | 真实容器/方言 | 适配器 | 节点提取范围 |
|---|---|---|---|
| Mihomo / Clash Meta | Clash YAML | `ClashYamlAdapter` | 顶层 `proxies` |
| Clash Standard / Backup / CFA | Clash YAML | `ClashYamlAdapter` | 顶层 `proxies` |
| Surfboard | Surfboard INI profile | `SurfboardIniAdapter` | `[Proxy]` |
| sing-box / SFA | sing-box JSON | `SingBoxJsonAdapter` | 白名单终端 outbounds |
| 任意客户端导出的通用节点 | URI list 或 Base64 URI list | `UriListAdapter` | 白名单 URI entries |

适配器共享 tokenizer、Base64、URI query、TLS、transport 和诊断工具，但不共享未经验证的字段语义。

### 7.1 首版稳定核心与兼容扩展

`00` 中冻结的三类输入是首版性能与安全的核心容器，而不是否定机场面向其他客户端导出的订阅。两类范围分别处理：

| 分类 | 格式/方言 | 发布要求 |
|---|---|---|
| 稳定核心 | URI/Base64 URI、Clash/Mihomo YAML、sing-box outbounds JSON | 默认构建；每种标准 5 MiB/10,000 节点 fixture 都必须满足 300 ms 和 110 MiB 总峰值门槛 |
| Android 兼容扩展 | Surfboard INI | 独立 Cargo feature 和 fixture；默认不扩大核心依赖；完成能力矩阵、fuzz、脱敏 golden、Android arm64 报告后才能在发布包启用 |

不为 Stash、Surge、Shadowrocket、Quantumult X 增加专用 adapter、format hint、request profile、fixture gate 或发布承诺。它们若输出标准 URI/Base64，仍由客户端无关的稳定容器解析；若只输出专用配置，返回 `unsupported_format`/`unknown_format`。兼容扩展的目的仅是覆盖 Android 上实际使用的 Surfboard，同时隔离 INI 方言长尾。启用扩展后同样不得放宽 nodes-only、协议白名单、资源限制、SSRF、脱敏、active limit 或事务发布规则。

### 7.2 `FormatAdapter` 与共享能力矩阵

每个适配器实现相同的有界接口；格式探测、语义映射和错误报告不得散落在调用方：

```rust
trait FormatAdapter {
    fn id(&self) -> FormatId;
    fn evidence(&self, payload: &NormalizedPayload) -> FormatEvidence;
    fn parse(
        &self,
        payload: &NormalizedPayload,
        limits: &ParserLimits,
        capabilities: &CapabilityMatrix,
    ) -> Result<ParseOutcome, ParserError>;
}
```

`FormatEvidence` 只有 `strong`、`weak`、`none`；registry 按显式 hint、强结构证据、弱证据、有界 Base64 重探测的顺序选择。`CapabilityMatrix` 是 sing-box `1.13.15`、下游 build tags 和已验证 transport/TLS/UDP 组合的唯一事实来源；adapter 只能查询它，不能自行假定协议字段可用。

## 8. nodes-only 安全边界

### 8.1 统一规则

NetHop 只导入可作为终端代理 outbound 的节点。以下字段永远不从外部订阅进入 NetHop 活动配置：

> 订阅只能贡献终端 proxy node；不能贡献任何执行语义、控制面语义、路径语义或网络入口语义。

- inbound/listen/address/port；
- Clash/Mihomo `rules`、`proxy-groups`、`proxy-providers`、`rule-providers`；
- Surfboard `[General]`、`[Proxy Group]`、`[Rule]`、`[Script]`、`[MITM]`、`[URL Rewrite]`；
- sing-box `route`、`dns`、`inbounds`、`services`、`experimental`、`certificate`、本地 path；
- 脚本、正则执行体、模板、外部 include、provider URL 和文件路径；
- 客户端 UI、图标、测速 URL、更新周期和策略组成员。

### 8.2 策略组处理

外部策略组不导入。NetHop 自己生成有限且可审计的 selector/urltest 组：

- 节点 tag 来自稳定 ID 和清洗后的展示名；
- auto 组成员数量遵守系统设计的有界候选集；
- 用户黑名单/白名单在 composer 层应用；
- 外部 group 名称只可作为来源元数据或建议分组，不能直接成为执行规则；
- 组内引用不存在的节点只报告 warning，不创建隐式 outbound。

## 9. URI 列表与 Base64

### 9.1 支持的 URI scheme

Android sing-box 数据面可能还支持 Naive、WireGuard、HTTP/SOCKS、Mieru 等其他 outbound；这些能力不自动进入 NetHop parser。首版协议白名单：

| Scheme | 协议 |
|---|---|
| `vless://` | VLESS |
| `vmess://` | VMess |
| `ss://` | Shadowsocks |
| `trojan://` | Trojan |
| `hysteria2://`、`hy2://` | Hysteria2 |
| `tuic://` | TUIC |
| `anytls://` | AnyTLS |

`socks://`、`socks5://`、`http://`、`wireguard://`、`naive+https://`、`mieru://` 等可作为诊断识别，但如果未纳入首版协议白名单，不得生成活动 outbound。是否加入首版必须通过独立协议 ADR，至少证明 sing-box 版本能力、Android arm64 连通性、统计归因、资源预算和安全边界；不能因为 SFA 或其他客户端能导入就默认支持。

### 9.2 行处理

- 空行、空白行和明确的注释行忽略；
- 保留 1-based 原始行号；
- 单行最大 16 KiB；
- 不按逗号拆分 URI；
- URI fragment 只作为 display name，不参与凭据 fingerprint；
- percent-decoding 只按协议字段规则进行一次；
- query 参数重复、未知或互相冲突时产生 warning 或 error，不能静默选择任意一个；
- 单节点失败不阻断其他节点；全部失败才使 source 失败。

### 9.3 Base64

支持标准和 URL-safe Base64，带 padding 和不带 padding。处理流程：

```text
ASCII candidate
  -> remove only permitted whitespace between Base64 lines
  -> bounded decode
  -> UTF-8 validation
  -> re-run format detection
  -> parse decoded payload
```

不得对已识别的 URI 再尝试 Base64。Base64 解码输出受 5 MiB 总限制；不能因为输入很小就接受无界展开。Base64 重探测深度固定为 1：原始输入最多解码一次，不接受 `Base64 -> Base64 -> ...` 递归。

URI fast path 还必须限制：query 参数最多 64 个、fragment 最多 256 bytes、VMess 内嵌 JSON 最多 64 KiB。行切分和 scheme 分发优先使用 byte slice；不得为了方便给每行构造完整 `url::Url`。完整 `url` crate 保留给 HTTPS source URL 和确实需要通用 URL 语义的字段。

## 10. Clash/Mihomo YAML

### 10.1 解析范围

Mihomo 与 Clash Standard 使用 YAML，并通常将终端节点放在顶层 `proxies`。适配器只接受：

```yaml
proxies:
  - name: node-1
    type: vless
    server: example.com
    port: 443
```

`proxy-groups`、`rules`、`proxy-providers`、`rule-providers`、`script` 和外部路径不进入 IR。

当 `proxies` 缺失或为空而存在 `proxy-providers` 时，source 必须以 `clash_inline_proxies_missing` 和 `clash_proxy_providers_not_imported` 说明失败原因，并建议用户选择包含 inline proxies 的 Clash/Mihomo、sing-box JSON 或 URI 订阅。NetHop 不递归下载 provider，也不把 provider URL 当成普通 source 自动添加。存在 `rule-providers`/`proxy-groups` 但 inline `proxies` 可用时，只生成一次 source-level boundary warning，不为每个节点重复报告。

### 10.2 YAML 资源与重复 key 策略

YAML adapter 的默认上限由 `ParserLimits` 冻结。`serde-saphyr 1.0.0` 的真实 `Budget` 字段必须按下表显式赋值，不能依赖其偏宽松默认值：

| `Budget` 字段 | NetHop 首版值 | 说明 |
|---|---:|---|
| `max_reader_input_bytes` | `Some(5 MiB)` | 只约束 reader 输入；对已在内存的 `str` 仍先做外层 5 MiB 检查 |
| `max_events` | 200,000 | 所有 parser event 的总量 |
| `max_nodes` | 200,000 | scalar、sequence start、mapping start 总量 |
| `max_documents` | 1 | 机场订阅不是 YAML document stream |
| `max_depth` | 64 | sequence + mapping 结构深度 |
| `max_aliases` / `max_anchors` | 各 1,024 | 绝对数量上限 |
| `max_inclusion_depth` | 0 | include feature 本身也不编译 |
| `max_total_scalar_bytes` | 5 MiB | scalar 内容和显式 tag 拼写累计值 |
| `max_total_comment_bytes` | 1 MiB | 拒绝以注释制造诊断/扫描开销 |
| `max_merge_keys` | 0 | 稳定核心首版拒绝 merge key |
| alias/anchor ratio | 启用，最少 64 aliases、倍率 64 | 在绝对上限之外阻断异常放大 |

`AliasLimits` 冻结 `max_total_replayed_events = 200,000`、`max_replay_stack_depth = 32`、`max_alias_expansions_per_anchor = 1,024`。sequence/map entry 数量由 event/node 总预算和最终候选节点 `<=10,000` 共同约束；若实现增加独立 collection 计数，值不得超过 200,000。

`Options` 冻结 `duplicate_keys = Error`、`merge_keys = Error`、`legacy_octal_numbers = false`、`strict_booleans = true`、`reject_non_finite_typeless_float = true`、`with_snippet = false`。关闭 snippet 是为了防止 YAML 错误对象把带凭据的原文片段带入日志/报告；诊断只保留位置、稳定 code 和脱敏字段路径。budget report 可用于 compact metrics，但不能保留 scalar、comment 或原始 snippet。

YAML tag 不触发对象构造、命令、网络或文件读取；`!include`/自定义 tag 一律拒绝。源码虽支持有界 merge replay，稳定核心仍拒绝 `<<` 并返回 `yaml_merge_key_unsupported`，不以手写递归 merge 作为首版兼容手段。只有真实脱敏 fixture 证明机场兼容性收益，并通过 duplicate-after-merge、alias replay、300 ms、45 MiB 和 fuzz 闸门后，ADR 才能将策略改为 `Merge` 且设置非零 `max_merge_keys`。

重复 key 统一使用库的 `DuplicateKeyPolicy::Error`，不采用 first/last-value-wins，也不因字段当前被忽略而降低为 warning。一个重复 key 使该 YAML source 作为不可信歧义输入失败；这是允许的 source-level 安全失败，不适用逐节点部分成功。alias、anchor、replay、event、node、depth、scalar、comment 或输入预算超限分别映射稳定资源诊断，不能统一伪装成普通语法错误。

### 10.3 支持字段原则

字段映射按协议和 sing-box v1.13.15 能力白名单执行：

| 类别 | 例子 | 行为 |
|---|---|---|
| 通用 | `name`、`type`、`server`、`port` | 必须验证并映射 |
| 凭据 | `uuid`、`password`、`cipher` | 使用协议专用类型 |
| TLS | `tls`、`servername`/`sni`、`alpn`、`skip-cert-verify` | 映射到 TLS；安全语义独立记录 |
| Reality | `reality-opts.public-key/short-id`、fingerprint | 仅 VLESS/已验证协议映射 |
| transport | `network`、`ws-opts`、`grpc-opts`、`h2-opts`、`http-opts` | 只接受首版 transport |
| UDP | `udp`、协议专用 UDP 字段 | 转为 capability，不改变 root 捕获策略 |
| 客户端控制 | `proxy-groups`、`rules`、`script` | 忽略并产生一次 source warning |

未知字段本身不失败；未知但会改变连接语义的 `type`、transport、TLS mode 或协议枚举必须拒绝节点。

## 11. sing-box JSON

### 11.1 接受的顶层形态

接受三种输入：

1. 完整配置 object，读取 `outbounds`；
2. outbound object array；
3. 单个终端 outbound object。

只提取终端代理类型。`direct`、`block`、`dns`、`selector`、`urltest`、`urltest` group、`shadowtls` 等无法作为首版独立终端节点的对象按能力处理：

- group、direct、block、dns：跳过并给 summary warning；
- 支持的嵌套 dialer 只有在协议 ADR 明确允许时映射；
- 需要引用本地文件、certificate provider、endpoint、service 或 route 的 outbound 拒绝；
- 不执行 sing-box `merge`，不读取 `config_directory`。

### 11.2 JSON 安全解析

- 顶层 bytes、深度、字符串和数组长度受全局限制；
- unknown fields 默认忽略并记录计数；
- `type` 必须是字符串且命中协议白名单；
- `tag` 只作为 display name 候选，不直接信任为全局唯一标识；
- endpoint、path、certificate、external controller 等字段不得改变 NetHop 目录或监听；
- 使用 serde 类型/访问器生成 IR，不保留完整 JSON AST 与多个输出副本。

JSON object 不能依赖 `serde_json::Value` 的重复 key 覆盖行为。typed visitor 必须检测 key 是否已出现：安全或连接关键字段重复时拒绝节点并返回 `duplicate_credential_key`/`duplicate_key`；无连接语义字段重复时记录 warning，并使用冻结的确定性策略。group、`direct`、`block`、`dns`、`selector`、`urltest`、`shadowtls` 等被跳过对象只生成汇总计数，避免 10,000 节点输入制造重复长消息。

sing-box 官方配置本身包含 `log`、`dns`、`inbounds`、`outbounds`、`route`、`services` 和 `experimental` 等顶层区域；NetHop 只读取经过白名单审计的终端 outbounds。

## 12. Surfboard INI

### 12.1 语法层

Surfboard 配置使用 section-based INI 风格。实现分两层：

```text
INI tokenizer
  -> section/key/value + source location
  -> dialect adapter
  -> node line parser
```

Tokenizer 负责：

- section 名称大小写策略；
- 注释行；
- 等号和逗号分隔；
- quoted/escaped value；
- 原始行号和列号；
- 有界行长度。

它不决定 `https` 是 HTTP over TLS 还是一个订阅 URL，也不把任意逗号字段自动当密码。

### 12.2 Surfboard `[Proxy]`

Surfboard 的 `ProxyVMess`、`ProxySS`、`ProxyTrojan` 等参数名和布尔字段有自己的约定。`SurfboardIniAdapter` 只复用本地 tokenizer 和公共协议语义校验器，不引入或复用其他客户端方言映射。

首版 tokenizer 应能识别机场常见的 `http`/`https`、`socks5`/`socks5-tls`、`ss`、`vmess`、`trojan` 等行式类型，但只把七协议白名单的交集转换为 `ProxyNode`。因此首版实际映射重点是 Shadowsocks、VMess、Trojan 以及经 fixture 验证的 Hysteria2、TUIC、AnyTLS；HTTP/SOCKS 节点返回 `unsupported_protocol`，不能因为 Surfboard 能运行就绕开 NetHop 的协议范围。

Surfboard 的 Shadowsocks `obfs`/`obfs-host` 只允许窄映射为 sing-box 内置 `obfs-local`：`obfs` 必须为 `http` 或 `tls`，`obfs-host` 可选且有界，composer 生成 `plugin="obfs-local"` 与确定性的 `plugin_opts`。任意其他 plugin、plugin option 或缺少 `obfs` 的组合继续返回 `unsupported_semantics`。此白名单由 sing-box 1.13.15 的 `transport/sip003/obfs.go`、`test/ss_plugin_test.go` 和官方 Shadowsocks outbound 文档共同证明，不代表开放任意 SIP003 透传。

`[Proxy]` 中的 `direct` 是客户端内置策略，不是终端代理节点；adapter 只生成一次 `non_node_section_ignored` 汇总 warning，不把它计入 accepted/rejected，也不生成 direct outbound。

`[Proxy Group]`、`[Rule]`、`#!MANAGED-CONFIG`、`policy-path` 和远程策略资源一律不导入。

## 13. Android-only 方言范围

NetHop 只面向 Android。客户端专用配置适配范围冻结为 Mihomo/Clash YAML、Clash Standard YAML、sing-box Android JSON 和 Surfboard INI；URI/Base64 是客户端无关的稳定容器。Stash、Surge、Shadowrocket、Quantumult X 的专用配置语法属于 `out_of_scope`，不会获得 tokenizer、adapter、format hint、request profile 或 release fixture。

此范围限制不等于按客户端来源拒绝内容：只要输入本身是稳定 URI/Base64、Clash/Mihomo YAML 或 sing-box JSON，就按内容结构解析。解析器不读取 URL 路径或客户端品牌名称来绕过格式与协议白名单。

## 14. 统一 `ProxyNode` IR

### 14.1 设计原则

IR 以“连接语义”而不是某一客户端字段为中心：

- 必填字段显式建模；
- 协议专用字段使用 enum/struct；
- 不使用无限制 `HashMap<String, Value>` 存放凭据；
- 未支持字段进入诊断，不透传到 sing-box；
- IR 不包含规则、策略组、脚本、文件路径或下载 URL；
- `ProxyNode` 通过校验后才能构造，不能由外部 parser 直接伪造。

### 14.2 逻辑结构

```text
ProxyNode {
  node_id: NodeId,
  canonical_fingerprint: Digest,
  display_name: DisplayName,
  protocol: ProxyProtocol,
  endpoint: Endpoint,
  credentials: Credentials,
  tls: TlsOptions,
  transport: TransportOptions,
  protocol_options: ProtocolOptions,
  capabilities: Capabilities,
  source_refs: Vec<SourceRef>,
  warnings: Vec<DiagnosticCode>
}
```

公共字段：

| 字段 | 规则 |
|---|---|
| `display_name` | 清洗控制字符、长度有界；原始名称只存脱敏/受控报告 |
| `endpoint.server` | 规范化域名或 IP；拒绝空值、控制字符和非法地址 |
| `endpoint.port` | `1..65535` |
| `protocol` | 首版七协议白名单 |
| `source_refs` | source ID、原始索引、检测格式、原始行号 |
| `capabilities` | UDP、IPv6、QUIC、TLS 等可证明能力，不由名称猜测 |
| `warnings` | 不影响连接的字段损失或方言差异 |

### 14.3 协议枚举

```text
ProxyProtocol =
  Vless
  | Vmess
  | Shadowsocks
  | Trojan
  | Hysteria2
  | Tuic
  | AnyTls
```

每个协议使用独立配置：

```text
Credentials =
  Vless { uuid }
  | Vmess { uuid, alter_id, security }
  | Shadowsocks { method, password, plugin }
  | Trojan { password }
  | Hysteria2 { password, obfs }
  | Tuic { uuid, password }
  | AnyTls { password }
```

字段的敏感性由类型决定；日志、Debug 输出和错误字符串必须使用 redact view。

### 14.4 TLS 与 transport

```text
TlsOptions {
  enabled,
  server_name,
  insecure,
  alpn,
  client_fingerprint,
  reality { public_key, short_id, fingerprint },
  certificate_pin
}

TransportOptions =
  Tcp
  | WebSocket { path, headers }
  | Http { path, hosts, method }
  | HttpUpgrade { path, headers }
  | Grpc { service_name }
  | Quic { ... }
```

只有 sing-box v1.13.15 已实现且 fixture 证明的组合才可构造成功。`insecure` 只表示代理节点 TLS 校验策略；它不能影响订阅下载 TLS。

### 14.5 `CapabilityMatrix`

能力矩阵随 parser mapping 版本化，并至少记录：协议、transport、TLS、Reality、uTLS、UDP/QUIC、flow、SIP003 plugin、sing-box 版本、build tag、源码证据、`sing-box check` fixture 和连通 fixture。矩阵中的 `supported` 必须同时满足：

1. sing-box `v1.13.15` 固定源码存在正式实现；
2. NetHop 的实际 build tags 包含该实现；
3. 最小 JSON 可通过相同发布二进制的 `sing-box check`；
4. parser 与 composer golden 一致；
5. TCP/UDP/QUIC、统计归因和资源闸门中适用项已验证。

任何 adapter 遇到矩阵未列出的组合都返回 `unsupported_transport` 或 `unsupported_semantics`，不得根据客户端名称猜测。首版 Shadowsocks 只接受矩阵明确列出的 `obfs-local` 组合；其余 SIP003 plugin 默认拒绝。每个启用项必须冻结 plugin name、options 语法和 sing-box 映射，不能透传任意 `plugin` 字符串。

## 15. 校验与规范化

### 15.1 分层校验

```text
syntax validation
  -> required-field validation
  -> protocol capability validation
  -> transport/TLS cross-field validation
  -> security policy validation
  -> canonicalization
  -> ProxyNode construction
```

语法错误属于当前 item；安全边界错误可能使整个 source 失败，例如输入超限或 YAML 资源攻击。

### 15.2 典型规则

- VLESS/VMess/TUIC UUID 输入只接受 36-byte hyphenated 或 32-byte simple 形式，先检查形态再调用 `uuid` 解析并规范化为小写 hyphenated；拒绝 nil UUID、URN 和 `{...}`。`uuid::parse_str` 本身还接受 URN/braced 等宽松形式，不能把库“可解析”直接等同协议“允许”；
- Shadowsocks method 必须在 sing-box 允许集合，密码不可为空；
- Trojan/Hysteria2/AnyTLS 密码不可为空；
- TUIC 必须同时具备 UUID 和 password；
- TLS/Reality/transport 组合必须满足协议约束；
- WebSocket path 必须是有界 UTF-8 路径，headers 名和值不能含控制字符；
- gRPC service name、SNI、ALPN 和 host 数量有界；
- Hysteria2/TUIC 的端口跳跃、obfs 和带宽字段按协议类型校验；
- SIP003 plugin 默认拒绝；首版仅 `CapabilityMatrix` 明确列出的 `obfs-local` 受审核组合可以转换，XHTTP、ShadowTLS 或客户端私有 transport 返回稳定拒绝码；
- server 是 IP literal 时仍保留原始 endpoint 语义，不为了 fingerprint 进行 DNS 解析；
- percent-encoded 字段先验证每个 `%` 后恰有两个十六进制字符，再精确解码一次；拒绝非法 UTF-8、NUL/控制字符和二次 percent decode，不依赖 `percent-encoding` 对畸形 `%` 的宽松透传；
- `skip-cert-verify` 只映射到节点 TLS 选项，并在诊断中标记安全影响。

### 15.3 未知字段策略

| 类型 | 行为 |
|---|---|
| 不影响连接的未知字段 | 忽略 + `unknown_field` warning |
| 影响连接但有明确别名 | 规范化 + `field_alias_used` warning |
| 影响连接且无法判断 | 拒绝节点 `unsupported_semantics` |
| 发现脚本/路径/provider | 不执行；按容器格式产生一次 boundary warning |
| 必填字段缺失 | 拒绝节点 `missing_required_field` |

## 16. Fingerprint、去重与来源

### 16.1 Canonical fingerprint

fingerprint 包含影响连接的规范化字段：

```text
protocol
endpoint.server
endpoint.port
credentials
tls identity and verification options
transport and protocol options
```

不包含：

- display name；
- source ID、URL、原始索引；
- 客户端策略组名；
- 无连接语义的未知字段；
- 诊断信息。

fingerprint digest 与 canonical encoding 分开版本化。首版默认基线是 SHA-256：

```text
fingerprint_schema = "nethop-node-v1"
fingerprint_algorithm = "sha256"
fingerprint = digest(fingerprint_algorithm, fingerprint_schema + "\0" + canonical_fields)
node_id = algorithm_tag(fingerprint_algorithm) + truncated_display_id(fingerprint)
```

Phase 0-B 先 profile SHA-256 基线。只有 `fingerprint + dedupe` 在标准 10,000 节点 fixture 中超过 30 ms 阶段预算或持续占 `detect..serialize` 总时延 10% 以上，才对 BLAKE3 做一次有界候选比较。比较使用实际 canonical 字段长度分布和同一 Android arm64 release profile，同时记录阶段/端到端 p50/p95、分配、产物体积、RSS、unsafe/build script 和传递依赖。BLAKE3 只有在三轮配对结果中端到端时延均改善至少 3%，且不破坏体积、RSS、可复现构建和安全门禁时才可被选中；否则保留 SHA-256 并从测试闭包删除 BLAKE3。

该门禁刻意高于单独哈希微基准：`sha2` 无论如何仍用于 source、fixture、发布资产和供应链 manifest 的 SHA-256，选择 BLAKE3 作为节点 fingerprint 会新增而不是替换这一依赖。首版发布构建只能包含一种节点 fingerprint 实现，不提供运行时算法开关，也不为了未来迁移预留双写。算法必须在首次持久化 source cache、节点选择和统计关联前冻结；后续变化需要提升 fingerprint schema，显式迁移或重建派生状态，旧新算法不得无提示混用。

完整 fingerprint 不作为公开接口或凭据保护机制；日志、报告和 UI 只暴露有界截断 ID，不得输出 canonical bytes 或凭据字段。

### 16.2 去重

相同 fingerprint 的节点合并为一个 `ProxyNode`：

- 保留全部 `source_refs`；
- 保留来源顺序和每个来源的别名；
- display tag 使用稳定清洗名称 + 短 node ID；
- 同名不同凭据不得去重；
- 同凭据不同名称不得生成多个 active outbound，除非用户显式要求保留别名。

去重必须稳定于 source 输入顺序变化。合并后排序使用 source 配置顺序、首次出现索引和 node ID，不能按不稳定 HashMap 遍历顺序。

## 17. 诊断与部分成功

### 17.1 诊断模型

```text
NodeDiagnostic {
  severity: info | warning | error,
  code: stable code,
  source_id,
  source_item_index,
  line,
  column,
  protocol,
  node_id?,
  message: redacted human text
}
```

错误消息不得包含完整 URL、token、密码、UUID、private key、Reality key 或原始整行。`line`/`column` 只在本地诊断或用户明确请求时展示。

### 17.2 建议错误码

| 错误码 | 语义 |
|---|---|
| `empty_input` | 输入为空 |
| `input_too_large` | 超过 body/行/字符串限制 |
| `invalid_utf8` | 非法 UTF-8 |
| `unknown_format` | 无法确定容器格式 |
| `ambiguous_format` | 多个容器格式均有强证据 |
| `ambiguous_dialect` | INI/snippet 方言不足以安全判定 |
| `format_hint_mismatch` | 强制格式与内容不符 |
| `invalid_base64` | Base64 解码失败或展开超限 |
| `invalid_json` | JSON 语法/结构错误 |
| `invalid_yaml` | YAML 语法/资源限制错误 |
| `invalid_ini` | INI/snippet 语法错误 |
| `duplicate_key` | 字段重复；YAML source 拒绝，JSON/INI 按对应 adapter 的冻结策略处理 |
| `duplicate_credential_key` | 凭据或连接关键字段重复，拒绝节点 |
| `yaml_alias_limit_exceeded` | alias/anchor/replay 超过预算 |
| `yaml_node_limit_exceeded` | YAML event/node/collection 超过预算 |
| `yaml_merge_key_unsupported` | 稳定核心检测到 `<<` merge key |
| `missing_required_field` | 必填节点字段缺失 |
| `invalid_endpoint` | server/port 不合法 |
| `unsupported_protocol` | 协议不在首版白名单 |
| `unsupported_transport` | transport 未实现或未验证 |
| `unsupported_semantics` | 关键字段语义无法安全映射 |
| `invalid_tls_combination` | TLS/Reality/transport 冲突 |
| `invalid_credential` | 凭据格式或约束错误 |
| `duplicate_node` | 与已接受节点 fingerprint 相同 |
| `source_all_failed` | source 内无可用节点 |
| `active_limit_exceeded` | active outbounds 超过 2,000 |
| `ssrf_blocked` | URL 目标违反下载安全策略 |
| `ssrf_peer_mismatch` | 实际连接 peer 不在本次已审核地址集合 |
| `response_too_large` | 压缩输入或解压输出超过各自上限 |
| `unsupported_content_encoding` | 响应不是 identity/gzip |
| `proxy_fetch_security_unsupported` | loopback proxy 路径不能提供等价地址固定与 peer 证据 |
| `nested_resource_blocked` | 外部 provider/include 未执行 |
| `clash_inline_proxies_missing` | Clash/Mihomo 缺少可导入的 inline `proxies` |
| `clash_proxy_providers_not_imported` | 检测到 `proxy-providers`，但按 nodes-only 边界未递归下载 |
| `last_known_good_used` | source 失败，沿用旧缓存 |

### 17.3 部分成功规则

一个 source 的结果分为：

```text
accepted + duplicate > 0 => source conversion success, even if some items fail
accepted + duplicate == 0 => source conversion failure, keep last-known-good
```

重复节点不计为 rejected；全是跨 source 重复节点的 source 仍然有效，必须把它的 `source_ref` 合并到现有节点。报告同时给出 `accepted`、`rejected`、`duplicate`、`warnings`。一个 source 成功不代表整个多 source generation 成功；最终 generation 仍需通过 active limit、composer、sing-box check、健康探针和事务发布。

## 18. `ConversionReport`

报告必须能够在不包含秘密的情况下重放诊断：

```text
ConversionReport {
  schema_version,
  source_id,
  origin_kind,
  detected_format,
  input_bytes,
  input_digest,
  elapsed_ms,
  phase_timings,
  accepted,
  rejected,
  duplicate,
  warning_count,
  diagnostic_counts: Map<DiagnosticCode, u32>,
  nodes: Vec<CompactNodeReport>,
  detailed_diagnostics: Vec<NodeDiagnostic>,
  diagnostics_truncated,
  source_status,
  resource_limits,
  degraded_reasons
}
```

`phase_timings` 至少包含：

```text
detect, decode, parse, normalize, validate, dedupe, compose, serialize
```

订阅下载、`sing-box check`、写盘、fsync、core 启动和健康检查不计入 parser 300 ms，但必须在 source update report 中单独记录。

`CompactNodeReport` 使用枚举/短 ID 和诊断码，不复制 message、display name、凭据或原始行。逻辑字段至少包含：

```text
source_item_index,
line?,
node_id?,
protocol?,
status: accepted | rejected | duplicate,
diagnostic_codes[]
```

10,000 节点场景仍可保留每个 item 的 compact 状态，但详细 message 默认最多 1,000 条、每节点最多 16 个 warning、每个去重节点最多 64 个 `source_ref`，序列化后的报告默认最多 8 MiB。达到上限时保留计数和首批详细诊断，设置 `diagnostics_truncated=true`；不能丢失 accepted/rejected/duplicate 总数，也不能让截断改变 source success 语义。

人类可读 message 由 CLI/App 根据稳定 diagnostic code 和本地化模板渲染。daemon 只在 root-only 受控报告中保存有上限的详细位置和脱敏参数，避免为 10,000 个节点常驻重复字符串。

## 19. Compose 边界

parser 输出 `ProxyNode` 或只含节点的中间 fragment，不输出完整运行配置。composer 负责：

1. 生成 sing-box 1.13.15 outbound JSON；
2. 注入稳定 tag、source metadata 和受审核 protocol options；
3. 创建 NetHop 自己的 selector/urltest 组；
4. 应用用户黑名单/白名单和 active limit；
5. 与固定 inbound、DNS、route、API、stats 和规则模板合并；
6. 运行 `sing-box check`；
7. 交给 generation transaction 发布。

订阅节点不能提供：

- outbound tag 覆盖 NetHop 保留 tag；
- inbound 或 service；
- route rule 和 DNS server；
- 本地 certificate/provider path；
- Clash API/V2Ray API listener；
- mark/table/端口/iptables 对象。

## 20. 性能与内存实现约束

### 20.1 300 ms 计时范围

标准 fixture 必须对 Base64 URI、明文 URI、sing-box JSON、Clash YAML 和多 source 合并分别测量：

```text
detect + decode + parse + normalize + validate + dedupe + compose + serialize <= 300 ms
```

不得用只含简单 Shadowsocks URI 的 fixture；必须覆盖七种协议、重复节点和 10% 非法节点。Clash YAML 不获得额外 400 ms 宽限。

### 20.2 有界分配

- Base64 按块解码，输出不超过 5 MiB；
- URI parser 尽量借用输入 slice，必须在跨线程/写盘前明确所有权；
- YAML/JSON 不同时长期保留原始 body、完整 AST、IR 和输出四份副本；
- alias、深度、节点、字符串、数组和 map 数量有界；
- 协议字段使用专用小结构，拒绝无限扩展 map；
- 凭据临时 buffer 在所有权明确且无额外副本时使用 zeroize；
- 不以“零拷贝”作为跳过校验或生命周期管理的理由。

### 20.3 active limit

- 10,000 节点只保证 conversion/diagnostic；
- 500 active outbounds 是 80 MiB/3% 发布性能基线；
- 2,000 是首版绝对发布硬上限，Expert 模式也不能提高；
- 超过 2,000 返回 `active_limit_exceeded`，不得静默截断；
- auto/urltest 只使用有界候选集，不对所有节点高频测速。

未来如需提高上限，必须通过独立性能 ADR，重新验证 sing-box RSS、selector/urltest、stats counter、完整重载、报告和回滚预算；不能在首版预留未验证的 5,000 节点开关。

### 20.4 阶段时延预算

300 ms 仍是每种稳定核心标准 fixture 的唯一发布硬门槛，不给 YAML 400 ms 兼容门槛。为定位回归，Phase 0-B 使用以下工程分配；它们在取得 Android arm64 三轮基线前属于诊断预算，不是可用来互相规避总门槛的独立承诺：

| 阶段 | 初始预算 | 说明 |
|---|---:|---|
| detect + 输入规范化 | 10 ms | BOM、UTF-8、行尾、结构证据 |
| decode | 20 ms | Base64；无 decode 的格式不人为等待或记入收益 |
| parse | 120 ms | URI/JSON/YAML typed/event parsing |
| normalize + validate | 70 ms | 协议、TLS、transport 和安全校验 |
| fingerprint + dedupe | 30 ms | 已冻结的单一 digest、集合与稳定排序；默认 SHA-256 |
| compose + serialize | 50 ms | outbound fragment，不含完整配置和 `sing-box check` |
| 合计 | 300 ms | 稳定核心 release gate |

每种格式分别输出 p50/p95、阶段耗时和分配次数。某阶段超过总时间 50%、相对冻结基线回退超过 10%，或连续三次超出初始预算时必须做 profile；是否调整阶段分配由性能 ADR 决定，但总计 300 ms 不变。Surfboard INI 扩展在启用前也必须提交相同报告，不能用“experimental”绕过输入资源上限。

### 20.5 parser workspace 子预算

更新峰值 `<=110 MiB` 是模块所有进程合计的发布门槛。parser 在此总量内采用下列设计预算，防止报告或 DOM 吞掉 core 所需余量：

| 对象 | 设计预算 |
|---|---:|
| 原始 body | `<=5 MiB` |
| normalized view/line index | `<=5 MiB` |
| Base64 decoded output | `<=5 MiB`，与不再需要的原始临时缓冲尽早错峰 |
| YAML/JSON workspace | `<=10 MiB` |
| `UnvalidatedNode` 临时对象 | `<=10 MiB` |
| `ProxyNode` IR | `<=20 MiB` |
| outbound/serialize buffer | `<=8 MiB` |
| diagnostics/report | `<=5 MiB`，最终 JSON 另受 8 MiB 上限 |
| parser workspace 合计 | `<=45 MiB`，挑战 `<=35 MiB` |

各对象预算不能简单相加成允许同时常驻的 68 MiB；workspace 合计 45 MiB 才是实现约束。测量窗口和进程汇总方式沿用 `01`，优先通过 event/typed parsing、buffer 所有权转移、compact diagnostics 和流式序列化降低峰值，不通过提高 110 MiB 门槛解决。

### 20.6 YAML parser 选型闸门

`serde-saphyr` 是首选而非无条件锁定。Phase 0-A 先用 Host fixture 验证 merge key、duplicate key、alias/anchor、深度、超长 scalar、未知字段和拒绝语义；Phase 0-B 再用同一组 Android arm64 release fixture 记录 5 MiB/10,000 节点标准 Clash YAML、真实复杂脱敏 YAML、peak RSS、拒绝时延和短时 fuzz 结果。

只有 `serde-saphyr` 同时满足 300 ms 总门槛、45 MiB parser workspace、资源限制、定位诊断、许可证和维护性要求才冻结精确版本。失败时先评估 `yaml-rust2` event path；C/libyaml 或 unsafe-heavy 方案只有纯 Rust 候选不能满足硬门槛且 ADR 证明净收益时才允许。首版不自研完整 YAML parser，也不同时发布两套 YAML 实现。

## 21. 测试策略

### 21.1 Fixture 分层

每种格式至少有：

1. 最小有效节点 fixture；
2. 每个首版协议的 golden fixture；
3. TLS/Reality/WS/HTTPUpgrade/gRPC/QUIC 组合 fixture；
4. malformed/unknown field fixture；
5. 资源限制 fixture；
6. 多 source、重复和顺序变化 fixture；
7. 脱敏诊断 fixture；
8. 5 MiB/10,000 节点性能 fixture。

fixture manifest 记录：格式、方言、协议分布、节点数、字节数、seed、生成器版本、SHA-256 和预期 accepted/rejected/duplicate。

### 21.2 Golden 输出

Golden 不保存真实凭据。采用固定假域名、测试 UUID、测试密码和 placeholder key。Golden 比较：

- detected format；
- accepted/rejected/duplicate 计数；
- canonical node fields；
- fingerprint/node ID；
- sing-box outbound fragment；
- diagnostic code 和位置；
- source refs。

display name 的原始语言或顺序变化不应改变 fingerprint；canonical serialization 的字段顺序必须稳定。

### 21.3 Fuzz 与资源攻击

Fuzz 目标包括：

- URI percent encoding、query 重复和超长字段；
- Base64 padding、URL-safe 字符和随机二进制；
- YAML alias、深度、锚点、重复 key、超长 scalar；
- JSON 深度、指数型数组、重复字段和巨大字符串；
- INI quoting、逗号、等号、section 切换；
- Surfboard line parser 的转义和未知参数。

Fuzz 必须保证：

- 不 panic；
- 不读取输入以外的文件；
- 不发网络请求；
- 不执行脚本；
- 在全局超时和内存上限内结束；
- 错误消息不泄露输入秘密。

### 21.4 参考项目吸收

- `NetGuard/sub-parser`：采用 `detect -> parser -> model -> converter` 分层、固定语料、稳定诊断和 VmRSS 观测；
- `Proxylink-main`：参考协议字段覆盖和格式兼容行为，不复制 Go 源码或其许可证不明部分；
- `PathGuard-Next`：采用 release benchmark、JSONL schema、p95/peak memory 和 build guard；
- `MagicNet-main`：采用分阶段 HTTP timing 和非法时间值拒绝思路，适用于 fetch/probe 诊断。

## 22. 版本化与兼容性

### 22.1 parser schema

以下内容必须有独立版本：

- `ProxyNode` canonical fingerprint schema；
- `ConversionReport` schema；
- format hint 枚举；
- diagnostic code；
- sing-box output mapping；
- protocol capability matrix。

旧报告必须可读取；旧 fingerprint 不能无提示地与新 fingerprint 混合去重。升级 parser 后，source generation 必须记录 parser version 和 mapping digest。

### 22.2 sing-box 版本

首版固定 sing-box v1.13.15。parser 不因为开发线或 beta 版本出现新字段就放入白名单。协议或字段要进入首版，必须同时满足：

1. v1.13.15 源码有正式实现；
2. 下游构建 tags 包含实现；
3. `sing-box check` 通过；
4. 最小连通 fixture 通过；
5. 性能、RSS、回滚和统计测试通过。

AnyTLS 的“版本存在性”已有三类证据：官方配置页标注自 sing-box 1.12.0 起支持；参考源码包含 `protocol/anytls/outbound.go`、`option/anytls.go`、`include/registry.go` 注册和 `github.com/anytls/sing-anytls` 依赖；本地 changelog 包含 1.13.15 条目与 AnyTLS 引入记录。因此它保留在首版协议白名单，不因审核稿的无证据怀疑降级。

但“源码存在”不等于 NetHop 已完成支持。AnyTLS 若计划在 Alpha 启用，Phase 0-B 必须归档适用证据：固定 `v1.13.15` tag/commit、上述源码位置、实际 build tags、最小 outbound JSON、发布二进制 `sing-box check` 输出、URI/YAML/JSON mapping golden、连通性和性能/RSS 报告；只有启用历史统计时才要求 stats attribution 证据。缺少任何适用项时，发布状态为 `unsupported` 或 `experimental`，不能仅凭 parser 能读字段宣称稳定。其他未来协议同样走独立 ADR。

## 23. CLI 与未来 Kotlin App 接口

### 23.1 CLI

建议命令：

```text
nethopctl subscription import --file <path> [--format auto|...]
nethopctl subscription import --text <stdin> [--format auto|...]
nethopctl subscription import --dry-run --file <path> [--format auto|...]
nethopctl subscription inspect --source <source-id>
nethopctl subscription update [<source-id>]
nethopctl subscription diagnose --source <source-id>
```

CLI 输出默认只显示摘要：格式、节点计数、协议计数、warning/error 计数、digest 和状态。详细报告写到 root-only 受控目录，凭据永不输出。

`nethopctl` 在调用方上下文中读取并限制文件大小，再把 bytes 交给 daemon；daemon 不接受普通客户端提供的任意绝对路径。`--dry-run` 执行完整 detect/parse/validate/dedupe/compose 和报告生成，但不保存 source、不写 candidate、不调用发布事务。未来 App 使用 Android 文件选择器读取 content URI 后采用相同 bytes/stream IPC。

### 23.2 Kotlin App

未来 App 负责：

- QR 相机、文件选择器、剪贴板和用户确认；
- 展示格式探测、节点计数、逐节点诊断和安全警告；
- 通过版本化 UDS/本地 IPC 调用 daemon；
- 不直接写活动 generation；
- 不直接持有 Clash API secret；
- 不把完整订阅内容上传云端。

App 可发送：

```text
ImportRequest {
  schema_version,
  source_id?,
  carrier: qr|file|text,
  payload: bounded bytes or root-only file handle,
  expected_format?,
  user_confirmed_network_fetch,
  expected_source_digest?
}
```

daemon 返回 `ConversionReport` 摘要和候选 generation 状态；导入请求不能绕过 active limit、SSRF、nodes-only 或 sing-box check。

## 24. Phase 闸门

### Phase 0-A：解析安全可行性

- URI/Base64、Clash YAML、sing-box JSON 三类稳定格式完成最小 golden；
- 建立统一 IR、fingerprint、去重和脱敏诊断模型；
- `ParserLimits`、YAML `Budget/Options/AliasLimits`、重复 key、merge-key reject、Base64 深度和 nodes-only 边界有确定性攻击回归；
- 七协议各有字段模型与最小有效/拒绝 host fixture，但未完成真实连通的协议不得标为参考设备稳定支持；
- 不执行外部 rules/script/provider/path；
- parser-only 依赖闭包可复核，且不包含 URL/ICU、HTTP/TLS/gzip；YAML 只启用 `deserialize`；
- parser schema、diagnostic code 和 mapping digest 写入 manifest。

Phase 0-A 不要求 5 MiB/10,000 节点 Android 性能、110 MiB 总峰值、任何 SIMD 对比、长时间 fuzz、实验格式、完整 fetch 或多设备验证。资源超限、凭据泄露、错误格式执行语义或不确定的节点映射仍是阻断项。

### Phase 0-B：参考设备解析 Alpha

- 在当前 `reference_verified` arm64 真机 release build 上，稳定三格式的 5 MiB/10,000 节点标准 fixture 满足 `detect..serialize <=300 ms`；
- parser/fetch 候选窗口与模块合计更新峰值满足 `<=110 MiB`；
- 七协议中计划在 Alpha 启用的每项完成 mapping、`sing-box check`、最小连通和资源报告；未完成者保持 `experimental`/`unsupported`；
- parser-only、fetch-enabled 和 dev/test 依赖闭包分别归档；确认发布 feature tree 不含 `base64/simd-unsafe`；
- 使用冻结的 workspace release profile 采集阶段 profile；fingerprint 阶段只有触发 16.1 节阈值时才比较 BLAKE3，并在冻结 schema 前删除落选实现；
- 输出精确到设备、ROM、内核、Root 管理器和 fixture digest 的解析性能报告。

### Phase 1：格式兼容与多 source

- 每个兼容扩展适配器完成官方结构与真实脱敏样本、golden/fuzz、重复 key/转义/资源攻击和 Android arm64 性能报告；逐 feature 通过后才在发布构建启用，未通过的扩展保持关闭并在能力清单中明确；
- Surfboard INI 的最小可行性样本和依赖/体积评估从 Phase 0-A/0-B 移入本阶段；稳定核心未达标前不得投入方言长尾；
- source 独立缓存、部分成功、last-known-good 和多 source 去重通过；
- active 500/2,000/10,000 边界行为通过；
- composer、sing-box check 和 generation transaction 集成；
- CLI inspect/diagnose 输出稳定。

### Phase 2：Android 集成

- URL fetch、SSRF、手动重定向、地址固定/peer mismatch、TLS SNI、压缩输入和解压输出双层上限、gzip bomb、连接池和磁盘错误注入通过；
- 固定 `ureq` 精确版本的 `unversioned` resolver/connector 适配测试通过；升级 ureq 时必须重新执行；
- root-only IPC、普通 App 权限绕过和超时恢复通过；
- 在参考真机完成 release build 性能、RSS、CPU 和集成 smoke；24 小时 soak 是模块 release candidate 门槛，见 `01`，不作为 parser Phase 2 的前置；
- Kotlin App 只负责载体获取，解析仍由 daemon 统一完成。

### Phase 3：发布冻结

- 所有默认稳定格式及发布包实际启用的扩展格式 fixture 有 manifest 和 SHA-256；
- 所有 hard gate 有原始报告；
- 每项稳定支持声明绑定至少一个 `reference_verified` 或 `community_verified` 设备组合；没有多设备证据时保持范围受限声明；
- 许可证、SBOM、parser mapping 和下游 patch 已归档；
- 未支持协议、字段和客户端策略有明确文档，不以“兼容”名义静默丢失。

## 25. 安全不变量

以下不变量不允许通过配置关闭：

1. 输入大小、深度、节点、字符串和行长度有界；
2. parser 不发网络请求、不执行脚本、不读取外部路径；
3. URL fetch 只允许 HTTPS，TLS 验证不可关闭，也不能降级到 HTTP；
4. SSRF 必须校验 DNS 解析结果、重定向和实际 peer address；LAN 例外必须显式且不能关闭其余限制；
5. 订阅只能贡献终端 proxy node，不能贡献 inbound、route、DNS、service、API 或 netfilter；
6. 非法或全失败 source 永不覆盖 last-known-good；
7. 诊断和日志不包含凭据、token、私钥和原始订阅内容；
8. active outbounds 超过 2,000 时拒绝发布，不静默截断；
9. parser 不因未知 beta 字段自动升级 sing-box 能力；
10. composer 是唯一生成完整活动配置的组件。
11. 压缩响应读取上限与解压后输出上限分别执行，不能只依赖 HTTP 客户端的单层 body limit。

## 26. 参考资料

1. NetHop 系统设计：[`00-nethop-system-design.md`](./00-nethop-system-design.md)
2. NetHop 性能预算与 SLO：[`01-performance-budget-and-slo.md`](./01-performance-budget-and-slo.md)
3. Mihomo configuration syntax：<https://wiki.metacubex.one/en/handbook/syntax/>
4. Mihomo proxy providers：<https://wiki.metacubex.one/en/config/proxy-providers/>
5. Surfboard configuration template：<https://manual.getsurfboard.com/config-template>
6. sing-box configuration：<https://sing-box.sagernet.org/configuration/>
7. Android ML Kit barcode scanning：<https://developers.google.com/ml-kit/vision/barcode-scanning/android>
8. Android Barcode raw value API：<https://developers.google.com/android/reference/com/google/mlkit/vision/barcode/common/Barcode>
9. `NetGuard/sub-parser`：`D:/100_Projects/110_Daily/NetGuard/sub-parser/`
10. `Proxylink-main`：`refer/Proxylink-main/`
11. Serde：<https://serde.rs/>
12. `serde_json`：<https://docs.rs/serde_json/>
13. `serde-saphyr`：<https://docs.rs/serde-saphyr/>
14. `ureq`：<https://docs.rs/ureq/>
15. `url`：<https://docs.rs/url/>
16. `zeroize`：<https://docs.rs/zeroize/>
17. sing-box AnyTLS outbound（自 1.12.0）：<https://sing-box.sagernet.org/configuration/outbound/anytls/>
18. OWASP SSRF Prevention Cheat Sheet：<https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html>
19. 本地 sing-box AnyTLS 证据：`refer/sing-box-testing/protocol/anytls/outbound.go`、`refer/sing-box-testing/option/anytls.go`、`refer/sing-box-testing/include/registry.go`、`refer/sing-box-testing/docs/changelog.md`
20. 本地依赖源码快照：`refer/subscription-import-and-parser-refer/snapshots/20260802-014712/`
21. `serde-saphyr` budget/options 源码：`refer/subscription-import-and-parser-refer/snapshots/20260802-014712/vendor/serde-saphyr/src/de/budget.rs`、`de/options.rs`
22. `ureq` resolver/transport/body 源码：`refer/subscription-import-and-parser-refer/snapshots/20260802-014712/vendor/ureq/src/unversioned/`、`src/body/mod.rs`
23. BLAKE3 官方仓库：<https://github.com/BLAKE3-team/BLAKE3>；Rust crate：<https://docs.rs/blake3/>（仅为条件 benchmark 候选，不代表已引入）

## 27. 冻结结论

1. 二维码、文件、文本和 URL 是输入载体，不是四套 parser；
2. 客户端展示名称归并到 Clash YAML、sing-box JSON、Surfboard INI 和 URI/Base64 四类容器；
3. 默认稳定核心为 URI/Base64、Clash/Mihomo YAML 和 sing-box JSON；Surfboard INI 是唯一 Android 兼容扩展，Stash、Surge、Shadowrocket、Quantumult X 专用配置明确不在范围内；
4. 所有适配器最终生成统一 `ProxyNode`，再由 composer 生成 sing-box outbound；
5. 首版协议能力仍冻结为 VLESS、VMess、Shadowsocks、Trojan、Hysteria2、TUIC、AnyTLS；Android sing-box 的额外 outbound 不在首版 parser 白名单内，格式支持不等于协议支持；
6. 未知关键语义、未支持 transport 和协议必须逐节点拒绝并给稳定诊断码；
7. 10,000 节点是转换边界，2,000 是首版绝对发布上限，500 是运行性能基线；Expert 模式不能提高首版上限；
8. parser 不下载嵌套资源、不执行脚本、不信任外部路径、不生成完整配置；
9. parser schema、fingerprint、diagnostic code 和 sing-box mapping 必须版本化；
10. 首版保持一个 `nethop-subscription` crate；默认 parser 构建不携带 HTTP/TLS/压缩依赖，fetch 仅通过显式 Cargo feature 启用；
11. parser 核心依赖为 `serde`、`serde_json`、仅反序列化的 `serde-saphyr`、明确禁用 `simd-unsafe` 的 `base64`、`percent-encoding`、裁剪 feature 的 `sha2`/`uuid`、`thiserror` 和受限使用的 `zeroize`；完整 `url` 与 `ureq + rustls + gzip` 只属于 fetch feature；精确版本以 Phase 0-A 依赖审计和 Phase 0-B Android 产物验证后的 `Cargo.lock` 为准；
12. 首版不引入 `reqwest`/`tokio`、通用 INI、`regex`、二维码识别、SQLite、IPC、CLI 框架或 `simd-json`；只有 benchmark 和 ADR 证明净收益后才可改变；
13. 依赖轻量性按 Android arm64 release 的产物体积、转换耗时、总 RSS、传递依赖和许可证共同判定，不能以依赖数量或 host debug 结果代替；
14. URI、JSON、Clash/Mihomo YAML 的标准 fixture 分别满足 `detect..serialize <=300 ms`；不采纳审核稿提出的 YAML 400 ms 兼容门槛；
15. 节点 fingerprint 默认使用 SHA-256；BLAKE3 不是默认依赖，只在真实阶段 profile 触发阈值时进行一次性比较，发布构建不得保留双算法；source、fixture 和发布资产摘要始终使用 SHA-256；
16. workspace release profile 固定速度优先的 `opt-level=3`、Thin LTO、单 codegen unit、strip 和关闭 incremental；`panic="abort"` 仍受崩溃恢复与 FFI 门禁约束；
17. parser workspace 设计上限为 45 MiB，ConversionReport 使用 compact item、详细诊断有界和 8 MiB 序列化上限，且模块更新峰值仍必须 `<=110 MiB`；
18. AnyTLS 保留在协议白名单，但最终稳定声明依赖固定 `v1.13.15` 发布二进制的源码、build、check、mapping、连通、统计和资源证据包；
19. URL fetch 必须防御 DNS rebinding，校验解析地址、每次手动重定向和实际 peer address，并分别限制压缩输入与解压输出；
20. `ureq` 的 SSRF 适配依赖 `unversioned` API，必须固定精确版本并在每次升级时重验；
21. 任何新增格式、协议、字段或依赖都必须先添加 fixture、能力矩阵、资源差异和 ADR，再进入实现。
