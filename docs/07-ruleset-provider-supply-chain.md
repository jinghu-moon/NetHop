# NetHop 规则集 Provider 与供应链决策

## 1. 状态与范围

- 状态：`Conditional Accept`
- 审核日期：2026-08-07
- 数据面版本：sing-box `1.13.15`
- 机器可读契约：`crates/nethopd/manifests/ruleset-providers-v1.json`

本文只冻结中国域名与中国 IP 两个托管规则集的来源、安全边界和准入流程。广告规则、多国家规则、用户自定义远端 provider、订阅内 provider 和 sing-box `remote` rule-set 均不在本次范围。

## 2. 决策

NetHop 自己下载并验证两个固定 HTTPS provider，sing-box 只读取 daemon-owned 本地 SRS：

| ID | 用途 | 固定来源 | 仓库声明许可证 | 默认上限 |
|---|---|---|---|---:|
| `cn-domain` | 中国域名直连 | `https://raw.githubusercontent.com/CHIZI-0618/v2ray-rules-dat/release/singbox_rule_set/geosite-cn.srs` | GPL-3.0 | 5 MiB |
| `cn-ip` | 中国 IP 直连 | `https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/cn.srs` | GPL-3.0 | 5 MiB |

不采用 sing-box `remote` rule-set。否则下载生命周期、缓存、重试和网络出口会进入数据面，绕过 NetHop 已有的 SSRF、持久调度、状态报告和事务发布边界。

## 3. 许可证与来源证据

2026-08-07 通过 GitHub 官方仓库元数据和仓库 `LICENSE` 核对：

- `CHIZI-0618/v2ray-rules-dat`：GitHub 标识 `GPL-3.0`，许可证正文为 GNU GPL version 3；
- `MetaCubeX/meta-rules-dat`：GitHub 标识 `GPL-3.0`，许可证正文为 GNU GPL version 3；
- 两份许可证文件当日 SHA-256 均为 `3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986`。

官方核验入口：

- https://github.com/CHIZI-0618/v2ray-rules-dat
- https://github.com/CHIZI-0618/v2ray-rules-dat/blob/master/LICENSE
- https://github.com/MetaCubeX/meta-rules-dat
- https://github.com/MetaCubeX/meta-rules-dat/blob/master/LICENSE

当前模块基线来自 `Fanju6/NetProxy-Magisk` commit `75864788707853aa7b3e206d401f649d65c5c081`，产物摘要记录在 `module/rulesets/PROVENANCE.md`。该快照尚未记录生成时两个数据仓库各自的精确 commit 和生成命令，因此首次公开发布前必须二选一：补齐原快照的完整对应源码链，或用已冻结 commit 和可复现生成命令重建两个 SRS。此项未完成前，允许开发测试，不把规则集产物标记为 release-ready。

## 4. 运行时安全边界

以下约束不可由 TOML、订阅或 Manager 放宽：

1. provider 列表编译进 daemon，用户不能增加 URL、镜像或规则集用途；
2. 只接受 HTTPS，固定 `raw.githubusercontent.com`，禁止 user-info 和 fragment；
3. 下载复用 `nethop-subscription` 的受控 fetch：DNS 与 connect-time 地址检查、peer address 校验、每次 redirect 重验、禁用环境代理和连接池；
4. 每份解压后 body 最多 5 MiB，只接受 `application/octet-stream` 和 SRS binary；
5. 两个候选都必须通过 SRS magic 预检和引用本地文件的真实 `sing-box check`；
6. 任一下载或校验失败时保留完整旧 pair；订阅更新、核心版本检查和规则集更新使用互不冒充的 schedule key；
7. 订阅内容不能覆盖 provider manifest、规则集文件或 composer 路径；
8. 日志、事件和状态只记录 provider ID、摘要、大小和稳定诊断码，不记录响应正文。

## 5. sing-box 1.13.15 Reload 证据

固定源码 `route/rule/rule_set_local.go` 对 local rule-set 创建 `fswatch.Watcher`，回调执行 `reloadFile`；解析和构建新规则成功后才在写锁下替换 `rules` 与 metadata。依赖 `github.com/sagernet/fswatch@v0.1.2` 默认监视目标文件的父目录，并对目标路径的 `Create`/`Write` 事件做 100 ms debounce。其上游 `TestFileWatcher` 覆盖“移除目标、将临时文件 rename 到目标路径”并要求触发 callback。

2026-08-07 已将 `fswatch@v0.1.2` 上游测试交叉编译为 Android arm64，并在 alioth/Android 13 执行 `TestFileWatcher`；create/write/remove/rename fixture 在 1.21 秒内通过。这证明目标设备的 rename callback 可达，但 sing-box 的 local rule-set 回调没有成功 ACK，仍不能单独证明业务规则已经完成切换。因此实现不以 watcher 作为提交条件，而是停止旧核心、发布候选、受控启动当前 generation，并以启动和数据面健康结果决定 commit 或 rollback。公开发布前仍须在真机执行：

1. 用真实 SRS 原子替换两个文件；
2. 观察两个 watcher callback 和 reload error；
3. 验证替换后域名/IP 命中发生变化；
4. 注入损坏 SRS，验证 admission 阻止发布；
5. 注入 reload 失败，验证旧 pair 恢复且当前代理保持可用。

## 6. 当前实现与剩余闸门

已完成：

- 安装包 SHA-256 校验与持久基线原子发布；
- composer 固定引用 `/data/adb/nethop/rulesets/`；
- 严格、内嵌、两项封闭的 provider manifest；
- 5 MiB/文件预算、私有 staging、真实 checker 接口、校验失败保留旧 pair；
- 逐文件原子 rename 与进程内失败回滚；
- 专用受控 fetch、响应 Content-Type/SRS 检查、成对获取和内容不变 no-op；
- 独立 `resource:rulesets` 持久 schedule 契约；
- 两阶段 `prepare/publish/commit/rollback` 与持久 journal，daemon 重启会恢复未提交旧 pair；
- 私有、摘要绑定的跨重启 body/ETag/Last-Modified 缓存，损坏缓存按 miss 处理；
- worker 到期消费、受控核心重启、健康确认和失败恢复旧 generation；
- `nethopctl ruleset status|update`、摘要状态和稳定诊断；
- alioth/Android 13 的 fswatch rename fixture；
- manifest 与打包基线 digest 的自动契约测试。

尚未完成，因而开发构建可以执行自动更新，但不得标记为 release-ready：

- 对应源码/生成工具的可复现发布链；
- 在 alioth 上用两个真实 provider 完成下载、候选 check、核心重启、规则命中变化和 `ruleset.status` 证据；
- 在真机注入候选启动失败与进程中断，确认旧 pair、旧 generation 和 journal 恢复；
- 将上述真机证据纳入模块发布 gate。

只有上述闸门全部通过，才能把本文状态改为 `Accepted` 并在 release notes 中声明自动规则集更新。
