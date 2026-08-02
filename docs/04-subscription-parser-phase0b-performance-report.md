# NetHop 订阅解析库 Phase 0-B 性能报告

> 状态：Measured  
> 日期：2026-08-02  
> 范围：稳定格式 parser-only，单台 `reference_verified` arm64 真机  
> 关联设计：[`01-performance-budget-and-slo.md`](./01-performance-budget-and-slo.md)、[`02-subscription-import-and-parser-design.md`](./02-subscription-import-and-parser-design.md)

## 1. 结论

当前实现通过本设备 Phase 0-B parser gate：

- 五个 10,000 项 fixture 的三轮 p95 均小于 `300ms`，最差值为 Clash YAML `245.54ms`；
- 单 case parser workspace 保守上界最大为 Base64 URI `39.69MiB`，小于 `45MiB`；
- 完整五 case runner 的进程 `VmHWM` 最大为 `57,648KiB`，小于 `110MiB`；
- 9,000 个有效节点的 SHA-256 fingerprint 单 pass 为 `3.59-4.21ms`，未达到 `30ms` 或总时延 `10%` 的 BLAKE3 触发条件；
- parser-only feature tree 不含 HTTP/TLS/gzip/URL/IDNA/ICU；fetch 增量使用 `ureq + rustls + gzip + url`，未出现 `native-tls`。

这只证明下列精确设备、系统、toolchain 和 fixture 组合，不代表中端设备、多厂商或所有 Android 版本。

## 2. 设备与构建

| 项目 | 值 |
|---|---|
| ADB serial | `dc39c31d` |
| product/device | `alioth / M2012K11AC` |
| SoC | Qualcomm `SM8250` |
| 内存 | `11,877,588KiB` |
| Android | 13 / API 33 |
| build fingerprint | `Redmi/alioth/alioth:13/TKQ1.220829.002/V14.0.8.0.TKHCNXM:user/release-keys` |
| ABI | `arm64-v8a` |
| kernel | `4.19.157-perf-g9607d8651312` |
| root / SELinux | Magisk 30.6 / Enforcing |
| 测试温度 | 29.5°C 至 32.5°C |
| Rust / Cargo | 1.97.1 |
| Android target/API | `aarch64-linux-android` / 23 |
| cargo-ndk | 4.1.2 |
| release profile SHA-256 | `B7BE5F9C2038C7781EABE02FD14BA16EB008570D3A66E373555EB019AC6FCD91` |
| runner SHA-256 | `C0261B969400C12D1D921BAABA09B771D0B1B5B444CE8890F7C11CDA26982C6C` |
| runner size | 1,624,136 bytes；gzip 829,968 bytes |

## 3. Fixture

所有 fixture 均为 runner 在设备端确定性生成，不读取真实订阅，不包含真实凭据。每个 case 有 10,000 项，其中 9,000 个合法节点、1,000 个非法节点；JSON/YAML 合计覆盖 VLESS、VMess、Shadowsocks、Trojan、Hysteria2、TUIC 和 AnyTLS。

| Case | 输入大小 | SHA-256 |
|---|---:|---|
| URI list | 4,626,891 bytes | `00849b2c503cedd1fffa28e02ba7ed2073cfb6bfd29ed57f22725f4eb11f9565` |
| Base64 URI list | 4,609,188 bytes | `5eb44d96a24c416ab1abd6dfe2ee2bd4a81f0889a4bebccd1e85a07ad2370ae3` |
| sing-box JSON | 4,550,448 bytes | `a91de5f51c32c9da64efbc67faef16852fd9f8b04f0cafed127f62ef96328d9f` |
| Clash YAML | 4,520,590 bytes | `cef056d7c9504eb36d9c3bffd2ed166808250dd82854ec71fcd7f9dfd10bf91a` |
| multi source | 4,626,888 bytes | `09e3830adbd641e5c1ccf1b88b3aaadf56ce9fdb3edb0cf7e099ab111e147b6c` |

## 4. 三轮性能

每轮先 warmup 5 次，再记录 20 个 `detect -> parse -> normalize -> validate -> dedupe -> compose -> report -> serialize` 样本。下表给出每轮 p95，单位为毫秒。

| Case | Round 1 | Round 2 | Round 3 | 最大值 |
|---|---:|---:|---:|---:|
| URI list | 120.61 | 123.77 | 122.18 | 123.77 |
| Base64 URI list | 140.03 | 144.17 | 144.97 | 144.97 |
| sing-box JSON | 122.60 | 125.13 | 128.79 | 128.79 |
| Clash YAML | 243.94 | 245.54 | 245.17 | 245.54 |
| multi source | 115.84 | 115.83 | 123.03 | 123.03 |

三轮完整 runner `VmHWM` 分别为 `57,648KiB`、`57,404KiB`、`57,524KiB`。

## 5. Workspace 与算法判定

单 case 独立进程以空进程 `VmHWM=2,764KiB` 为基线。差值包含 fixture、parser、IR、report、serialize buffer 和 allocator 保留，是 parser workspace 的保守上界。

| Case | VmHWM | 差值 |
|---|---:|---:|
| URI list | 36,712KiB | 33.15MiB |
| Base64 URI list | 43,404KiB | 39.69MiB |
| sing-box JSON | 37,112KiB | 33.54MiB |
| Clash YAML | 40,548KiB | 36.90MiB |
| multi source | 33,192KiB | 29.71MiB |

首次测量曾出现 `48-51MiB` workspace。profile 定位到 `convert_stable_sources` 同时保留 AdapterOutput 和克隆后的 `ProxyNode`；改为所有权转移并同步聚合 compact report 后，峰值降至上述范围。

fingerprint 单 pass 最大 `4.21ms`，占对应 p95 最大约 `3.6%`。因此 J011 为 `not_triggered`：不引入 BLAKE3，不增加算法双栈、build script 或额外审计面。

## 6. 复现命令

```powershell
cargo ndk -t arm64-v8a -P 23 build --locked --release --example subscription_parser_bench
adb push "target/aarch64-linux-android/release/examples/subscription_parser_bench" "/data/local/tmp/nethop-parser-bench"
adb shell chmod 700 /data/local/tmp/nethop-parser-bench
adb shell /data/local/tmp/nethop-parser-bench
```

独立内存 case 使用：

```powershell
adb shell /data/local/tmp/nethop-parser-bench baseline
adb shell /data/local/tmp/nethop-parser-bench base64_uri_list
```

## 7. Surfboard Android 扩展

Surfboard 是默认关闭的 Android 兼容扩展，不属于稳定核心五 case。使用同一设备、toolchain、release profile 和 benchmark runner 对 10,000 项确定性 fixture 做扩展测量：

| 项目 | 结果 | 状态 |
|---|---:|---|
| 输入大小 | 4,120,837 bytes | measured |
| fixture SHA-256 | `7cc1355b2744ec32039f21796219d1e5019272441dcdf784e2786d2cf47f2b4d` | measured |
| accepted / rejected | 9,000 / 1,000 | measured |
| p50 / p95 | 139.457ms / 142.892ms | measured，满足 300ms |
| fingerprint 单 pass | 8.972ms | measured |
| 进程 VmHWM | 45,316KiB | measured，低于 45MiB（46,080KiB） |
| 直接依赖增量 | 0 | 本地 tokenizer，无 INI/regex crate |
| 细粒度阶段分配计数 | - | unsupported，runner 尚未提供分配采样 |

该结果证明 Surfboard 路径在参考设备上的端到端时延和进程峰值满足当前目标，但没有补齐细粒度阶段分配证据，也不能代表其他 Android 设备或任意 Surge-compatible 方言。因此 `format-surfboard` 维持 `experimental` 且默认关闭。

复现单 case：

```powershell
cargo ndk -t arm64-v8a -P 23 build --locked --release --example subscription_parser_bench --features experimental-formats
adb push "target/aarch64-linux-android/release/examples/subscription_parser_bench" "/data/local/tmp/nethop-parser-bench"
adb shell chmod 700 /data/local/tmp/nethop-parser-bench
adb shell /data/local/tmp/nethop-parser-bench surfboard_ini
```
