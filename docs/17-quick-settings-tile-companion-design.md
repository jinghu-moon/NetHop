# NetHop 快捷设置磁贴 Companion APK 设计方案

> 状态：Design Baseline v0.3
>
> 日期：2026-08-15
>
> 适用范围：Android 13+ arm64 Root 设备、NetHop Alpha、KernelSU Module WebUI
>
> 上位文档：[`00-nethop-system-design.md`](./00-nethop-system-design.md)
>
> 配置契约：[`06-configuration-toml-refactor-design.md`](./06-configuration-toml-refactor-design.md)
>
> WebUI 契约：[`08-webui-design.md`](./08-webui-design.md)
>
> TDD 任务清单：[`18-quick-settings-tile-companion-tdd-task-list.md`](./18-quick-settings-tile-companion-tdd-task-list.md)

## 1. 决策摘要

NetHop 增加一个可选安装的轻量 Companion APK。它不是第二套 Manager，也不重新实现 WebUI，只提供两个系统入口：

1. 在 Android 快捷设置面板点按 NetHop 磁贴，明确启动或停止代理；
2. 长按磁贴，通过受控 Root 资源加载器直接进入模块现有的 NetHop WebUI。

| 项目 | 决策 |
|---|---|
| 产品定位 | NetHop 系统入口 APK，不是独立业务控制面 |
| 首版功能 | 磁贴点按启停；长按进入 WebUI |
| 最低系统 | Android API 33，与 NetHop 模块一致 |
| APK 形态 | 普通用户 APK，不 systemize，不创建第二个 Root 模块 |
| UI | 不编写原生管理界面；WebUI Activity 流式加载模块唯一的 Vue 构建产物 |
| Android 实现 | Kotlin + framework-first；不使用 Compose、Material、Firebase、Xposed 或网络 SDK |
| Root 入口 | 固定绝对路径 `su -c /data/adb/modules/nethop/bin/nethopctl ...` |
| 控制协议 | `nethopctl` JSON + daemon typed IPC；不直接读 TOML，不连接 Clash API |
| 状态源 | `status.get` 返回的持久 intent、有效 intent、runtime state 和 capture state |
| WebUI 宿主 | APK 内受限 Android HostAdapter + Root WebRoot PathHandler；KernelSU Module WebUI 继续使用现有 host adapter |
| APK 资产 | 不重复内嵌完整 WebUI；只内嵌无 Root bridge 的最小错误页 |
| Magisk | 不纳入 Companion/WebUI 首版支持和验收范围 |
| APatch | 其 WebUI 与安装行为完成真机验证后再加入支持矩阵 |
| 分发 | APK 随模块 ZIP 分发，校验后由安装脚本询问是否安装 |
| 默认选择 | 首次安装 10 秒无输入时跳过，不以超时替代用户同意 |
| 安装升级 | 已安装且签名匹配时随模块执行 `pm install -r` 更新 |

推荐总链路：

```text
Android SystemUI
  |
  +-- tap --> NetHopTileService
  |              |
  |              +--> fixed root command --> nethopctl
  |                                         |
  |                                         +--> root-only UDS --> nethopd
  |                                                                  |
  |                                                                  +--> service.enabled
  |                                                                  +--> health check
  |                                                                  +--> rollback
  |
  +-- long press --> WebUiEntryActivity
                         |
                         +--> WebViewAssetLoader
                         |       |
                         |       +--> RootWebRootPathHandler
                         |               |
                         |               +--> lifecycle-owned global-mount Root shell
                         |                       |
                         |                       +--> /data/adb/modules/nethop/webroot
                         +--> AndroidHostAdapter --> nethopctl --> nethopd
```

## 2. 调研结论

### 2.1 SurfingTile

`refer/Surfing` 中没有 SurfingTile 的 Kotlin/Java 源码，只有发布 APK、Manifest 可见信息和混淆后的字节码，因此调研结论来自安装脚本、Manifest、smali 行为和 Surfing shell 链路的交叉验证。

SurfingTile 的主要流程为：

```text
TileService
  -> root touch/rm /data/adb/modules/Surfing/disable
  -> inotifyd 观察 disable 文件
  -> box.inotify
  -> box.service start/stop
  -> box.iptables enable/disable
  -> Clash API 轮询确认
  -> 更新 Tile
```

可吸收的部分：

- 快捷设置磁贴是高频启停的有效系统入口；
- `onStartListening()` 时重新查询真实状态；
- 命令在后台执行，不阻塞 SystemUI 主线程；
- 操作期间阻止重复点击；
- 命令完成后再次查询实际状态，不盲信旧 Tile state；
- 启停失败时回滚，并显示不可用状态。

不吸收的部分：

- 不以模块 `disable` 文件建立第二套控制面；
- 不使用 `inotifyd + shell + Clash API` 拼接代理事实；
- 不让 APK 直接控制核心、iptables 或 Clash API secret；
- 不强制 system app，不要求额外 Magisk/KernelSU 模块；
- 不复制 SurfingTile 中与磁贴无关的 Compose、Firebase、Xposed 和完整管理功能；
- 不在 APK 内重做 daemon 已有的健康检查、轮询、事务和回滚。

### 2.2 NetProxy-Magisk 安装器

`refer/NetProxy-Magisk/src/module/customize.sh` 使用音量键询问是否安装内置 Manager APK，并通过 `pm install -r` 安装，最后删除模块展开目录中的 APK。

NetHop 吸收以下思路：

- APK 与模块使用同一个发布 ZIP；
- 刷入阶段明确询问；
- 用户跳过后不保留无用 APK；
- APK 安装失败只警告，不破坏模块主体安装。

NetHop 不照搬以下行为：

- NetProxy 在超时后默认安装；NetHop 首次安装必须明确选择，超时默认跳过；
- NetProxy 的 `getevent -lqc 1` 不构成可靠硬超时；NetHop 使用每秒有界读取；
- NetHop 不提供第二个安装来源选择，不在安装时打开应用商店或网络 URL。

### 2.3 KernelSU WebUI 深链边界

当前 KernelSU 源码注册了 `ksu://webui`，但外部 deep link 必须同时携带 Manager 私有的随机 `intentToken`。`parseValidatedDeepLink()` 会拒绝缺少 token 或 token 不匹配的 URI；实际 WebUI Activity 又是 `exported=false`。

因此以下方案不成立：

```text
长按 Tile -> ksu://webui?id=nethop
```

NetHop 不采用以下绕过方式：

- 不读取 KernelSU 私有 SharedPreferences 获取 token；
- 不以 root 强行启动未导出的 KernelSU Activity；
- 不硬编码 KernelSU Manager 的包名、Activity 类名或 fork 差异；
- 不要求用户预先创建并暴露一个 Launcher shortcut。

这些方案都把 NetHop 的入口绑定到 KernelSU 私有实现，无法形成稳定契约。为满足“长按直接进入 WebUI”，Companion 使用自有 Activity、受限 Android HostAdapter 和经用户授权的 Root 资源加载器，不调用 KernelSU 私有入口。

### 2.4 KernelSU 模块 WebUI 的 Root 读取链路

“普通 APK 不能读取模块 Root 目录”只对未提权的普通文件 API 成立，不应写成绝对能力边界。KernelSU Manager 自身已经给出可验证的实现：

```text
WebView
  -> WebViewAssetLoader
  -> SuFilePathHandler
  -> SuFileInputStream
  -> lifecycle-owned global-mount Root shell
  -> /data/adb/modules/<module-id>/webroot/*
```

源码事实如下：

- `WebViewHelper.kt` 将模块目录固定为 `/data/adb/modules/${moduleId}`，创建 `createRootShell(true)`，关闭 `allowFileAccess`，再将模块 `webroot` 注册给 `WebViewAssetLoader`；
- `SuFilePathHandler.java` 先做 canonical child 校验，再将绑定 Root shell 的 `SuFile` 交给 `SuFileInputStream.open()`；
- `WebUIState.dispose()` 同时销毁 WebView 并关闭 Root shell；
- `KsuCli.createRootShell(true)` 请求 global mount namespace；KernelSU `su` 的 `-M/--mount-master` 也明确定义为切换到全局挂载命名空间；
- KernelSU Manager 精确锁定 libsu `6.0.0`，其 WebUI 路径实际依赖 `core` 与 `io`；NetHop 不需要 KernelSU 同时使用的 `service` 模块；
- KernelSU 安装器默认将模块目录设为 `0755`、普通文件设为 `0644`，并使用 `system_file` SELinux context。权限位本身不等于普通应用具备访问能力，实际读取仍应经过 Root shell。

因此 NetHop 可以复用模块唯一的 `webroot`，不需要在 APK 中再打包一份完整副本。该结论不意味着向 WebView 暴露任意 Root 文件：Companion 的路径根必须编译期固定为 `/data/adb/modules/nethop/webroot`，并施加比 KernelSU 通用宿主更窄的校验，见第 7 节。

### 2.5 Android Quick Settings 契约

Android 快捷设置磁贴由 APK 内的 `TileService` 提供，纯 Magisk/KernelSU shell 模块无法注册系统磁贴。service 必须：

- 声明 `android.permission.BIND_QUICK_SETTINGS_TILE`；
- 响应 `android.service.quicksettings.action.QS_TILE`；
- 声明 `android.service.quicksettings.TOGGLEABLE_TILE=true`；
- 由系统绑定并驱动 `onStartListening()`、`onStopListening()` 和 `onClick()`。

长按磁贴由系统启动处理 `android.service.quicksettings.action.QS_TILE_PREFERENCES` 的 Activity。该 Activity 是 NetHop 自有入口，不能依赖 Root Manager 的私有 Activity。

## 3. 目标

1. 用户下拉快捷设置后能在一次点按内启用或关闭 NetHop。
2. Tile 展示 daemon 的真实持久 intent 和运行结果，不使用本地缓存推断成功。
3. 长按 Tile 直接打开现有 NetHop WebUI，不经过模块列表或第二个管理界面。
4. APK 不成为第二个状态发布者；所有写操作继续经过 daemon typed IPC。
5. APK 未安装、被停用或安装失败时，不影响 NetHop 模块、daemon 和 WebUI。
6. APK 不常驻后台，不建立轮询 service，不增加 daemon 的稳态 wakeup。
7. APK、模块 WebUI 和 CLI 对同一状态给出一致结论。
8. 安装过程明确、可取消、可超时，并且不因可选 APK 失败而回滚模块主体。
9. 构建、签名、checksum、SBOM 和许可证链覆盖 APK、Root 读取依赖及最小 fallback 资产。

## 4. 非目标

- 不开发独立原生 Manager 页面；
- 不增加 Launcher 首页、Dashboard、设置页或后台常驻通知；
- 不在 Tile 上实现双击切换节点、重启、订阅更新或测速；
- 不在 Tile 中显示当前节点、流量或订阅余额；
- 不直接修改 `nethop.toml`、generation、SQLite、iptables 或模块 `disable` 文件；
- 不直连 sing-box Clash API、gRPC API 或本地 HTTP server；
- 不读取 Root Manager 的私有配置、token 或数据库；
- 不在 APK 中复制完整 WebUI，也不把模块 WebUI 解压或复制到应用私有目录；
- 不把 APK systemize；
- 不请求网络、安装其他应用、读取电话状态、广告标识或悬浮窗权限；
- 不承诺 Magisk 官方 Manager 中打开 WebUI；
- 不为尚未验证的 APatch 跳转契约预留分支或反射代码。

## 5. 工程结构

2026-08-17 实施补充：Companion 增加 `NetHopCompanionApplication -> CompanionServices` 应用组合根。`RootCommandExecutor` 由组合根唯一创建并供 Tile/WebUI 使用，`AndroidPackageRepository` 仍保持 Activity scope；不引入 DI 框架、ViewModel 或第二套 Manager。

建议新增独立 Android 工程目录：

```text
companion/
  settings.gradle.kts
  build.gradle.kts
  gradle.properties
  gradle/wrapper/
  gradle/libs.versions.toml
  app/
    build.gradle.kts
    proguard-rules.pro
    src/main/
      AndroidManifest.xml
      kotlin/com/jinghumoon/nethop/companion/
        NetHopTileService.kt
        WebUiEntryActivity.kt
        RootCommandExecutor.kt
        StatusDecoder.kt
        TileStateMapper.kt
        webui/AndroidWebUiBridge.kt
        webui/EventProcess.kt
        webui/RootShellSession.kt
        webui/RootWebRootPathHandler.kt
      res/
        drawable/ic_nethop_tile.xml
        values/strings.xml
        values/styles.xml
      assets/fallback/error.html
```

首版使用 Kotlin 和 Android framework API。Kotlin 只负责类型、协程和序列化，不因此引入 Compose、AppCompat、ViewModel、依赖注入或第二套 UI 框架。安全承载模块 WebUI 使用 `androidx.webkit` 和 libsu；release 构建必须通过 R8 移除未使用代码，并记录 Kotlin、协程、serialization、WebKit、libsu 及其传递依赖的实际包体贡献。

包名在首轮实现前冻结为一个唯一值。本文建议：

```text
applicationId = com.jinghumoon.nethop.companion
```

若仓库发布身份最终不同，应在首次构建前一次性修改；项目尚未发布，不维护旧包名兼容或双包迁移。

### 5.1 YingLi-Player 工具链基线

Companion Android 工程参考 `D:/100_Projects/110_Daily/YingLi-Player` 当前冻结方式，而不是复制播放器依赖树：

| 项目 | Companion 基线 | 来源/理由 |
|---|---:|---|
| Gradle Wrapper | `9.5.0` + distribution SHA-256 | 复用 YingLi 可复现 wrapper 纪律 |
| Android Gradle Plugin | `9.3.1` | Version Catalog 精确锁定 |
| Kotlin | `2.4.0` | 使用 AGP 9 built-in Kotlin |
| JDK / JVM target | `21` | 构建入口 fail-fast 校验 |
| `compileSdk` | `36` | 与参考工程一致 |
| `targetSdk` | `36` | 使用当前 Android 行为基线 |
| `minSdk` | `33` | 与 NetHop Android 13+ 产品范围一致 |
| Coroutines | `1.11.0` | Root I/O、超时、取消和主线程切换 |
| kotlinx.serialization | `1.11.0` | 版本化 JSON envelope，拒绝 human text parser |
| AndroidX WebKit | `1.17.0` | Google Maven 2026-08-12 最新稳定版；本地可信 origin 和 bridge |
| libsu core | `6.0.0` | 专用 Root shell、生命周期和 mount-master 构造 |
| libsu io | `6.0.0` | `SuFile`、`SuFileInputStream` Root 流式读取；传递依赖 libsu nio |
| JUnit | `4.13.2` | 纯函数和 command mapping 测试 |
| AndroidX Test Ext | `1.3.0` | Android 组件测试 |
| AndroidX Test Core | `1.7.0` | 宿主和 Activity 测试 |
| Espresso | `3.7.0` | WebUiEntryActivity 基础交互测试 |

AGP 9 已提供 built-in Kotlin，工程不得再应用 `org.jetbrains.kotlin.android`。`settings.gradle.kts` 应像 YingLi-Player 一样在检测到旧 Kotlin Android plugin 时直接失败；serialization 只应用与 Kotlin `2.4.0` 同版本的 `org.jetbrains.kotlin.plugin.serialization`。

构建纪律同步吸收：

- 常规依赖只从 `google()`、`mavenCentral()` 和 `gradlePluginPortal()` 解析；
- JitPack 只通过 Gradle `exclusiveContent` 解析 `com.github.topjohnwu.libsu`，不得成为通用 fallback 仓库；
- `RepositoriesMode.FAIL_ON_PROJECT_REPOS`；
- 提交并校验 Gradle dependency verification metadata，固定 libsu `core/io/nio` 及其 POM/AAR checksum；
- JDK、Gradle 和关键依赖不匹配时 fail-fast；
- `org.gradle.configuration-cache=true`、build cache 和并行构建；
- Kotlin `allWarningsAsErrors=true`；
- release lint `abortOnError=true`、`warningsAsErrors=true`；
- release 启用 R8 与 resource shrink；
- signing config 只从 `NETHOP_COMPANION_*` 环境变量读取，私钥和密码不写入仓库。

Companion 不包含 native library，因此不复制 YingLi-Player 的 ABI splits。APK 使用一个 platform-neutral release 产物；它随 NetHop arm64 模块分发，不等于 APK 自身需要 `arm64-v8a` split。

### 5.2 最小依赖集合

生产依赖只允许：

```text
org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0
org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0
androidx.webkit:webkit:1.17.0
com.github.topjohnwu.libsu:core:6.0.0
com.github.topjohnwu.libsu:io:6.0.0
```

`androidx.webkit` 不在 YingLi-Player 当前依赖中，是 Companion 为 `WebViewAssetLoader`、可信 origin 和受限 message bridge 增加的 AndroidX 生产依赖。Google Maven 官方 metadata 在 2026-08-12 发布 `1.17.0` 为最新 release，本文固定该版本。libsu `io` 的源码依赖 `core` 和 `nio`，因此 release dependency tree、SBOM、许可证和体积核算必须包含三者；NetHop 不使用 `libsu-service`，因为本方案不创建 Root Binder service。

`refer/libsu-master` 对 `6.0.0` 的源码核对结果：

- `Shell.FLAG_MOUNT_MASTER` 请求以 `su --mount-master` 创建 Root shell；
- 默认 Builder 在 mount-master 失败后会回退到普通 `su`，再回退到 `sh`，因此不能只凭 Builder 成功就认为获得了正确能力；
- `SuFile.setShell()` 可将文件元数据查询和 `SuFileInputStream` 绑定到指定 shell，避免隐式使用进程级 main shell；
- `SuFileInputStream` 对 `SuFile` 使用临时 FIFO 和后台 `cat` 传输，属于流式读取但不是零复制，也会产生短生命周期线程、FD 和 shell 子任务；
- libsu 文档说明每个 Shell 实例至少需要 3 个 worker thread 才能正常工作，因此不得为每个资源请求创建 Shell。

实现必须创建一个 Activity 会话专用 shell。由于 `setFlags(FLAG_MOUNT_MASTER)` 允许内部降级，KernelSU 首版应使用 `Shell.Builder.create().build("su", "--mount-master")` 明确构造，并验证 `getStatus() == Shell.ROOT_SHELL`、`isAlive()` 且固定入口文件可读；任一步失败即进入 fallback 错误页，不能退化为普通 `su` 或 `sh`。所有 `SuFile` 必须显式绑定该 shell，禁止调用会隐式获取全局 main shell 的快捷 API。Activity 销毁时先关闭仍在使用的资源流，再有界 `waitAndClose`/`close` shell。

实现阶段必须完成：

1. `WebViewAssetLoader` / origin allowlist 能力核对；
2. minSdk 33 release 构建；
3. R8 后 APK size breakdown；
4. 无 WebKit 依赖的 framework-only 原型对比；
5. WebView bridge 安全测试。
6. libsu `core/io/nio` 的 R8、线程、FD、FIFO 和首屏性能核算。

版本必须写入 `gradle/libs.versions.toml`，不允许 `+`、dynamic range 或 snapshot。libsu `6.0.0` 当前通过 JitPack 发布；若 CI 无法稳定、可验证地解析该 artifact，应采用审计后固定 AAR 或源码 vendoring，并保留 upstream revision、checksum、许可证和更新流程，不能放宽仓库白名单。若 framework API 能在不自研 URL loader/bridge 的前提下满足同等安全边界，可以在独立体积/安全对比后删除 `androidx.webkit`；不能为少量体积手写一套更难审计的资源服务器。

明确不吸收 YingLi-Player 的以下依赖：

- Compose BOM、Activity Compose、Material3 和 Compose icons；
- Lifecycle ViewModel、Navigation Compose；
- Room、DataStore、KSP；
- Media3、Coil、DocumentFile；
- benchmark module 和多 ABI splits。

Tile 和 WebUI Activity 使用结构化并发：每个 Android component 持有自己的 `CoroutineScope(SupervisorJob() + dispatcher)`，在销毁时取消；Root I/O 固定在 `Dispatchers.IO`，Tile 更新和 WebView 访问回到主线程。不得使用 `GlobalScope`、cached thread pool 或无法归属生命周期的后台任务。

libsu 自身的 worker threads 是该依赖正常驱动 Shell 所需资源，不计作业务 executor，但必须纳入线程预算。Companion 只能在 WebUI Activity 生命周期内创建一个 WebRoot shell；不得按资源、页面或 operation 创建新 Shell，也不得把 Activity shell 提升为进程级常驻 main shell。

## 6. Android 组件

### 6.1 `NetHopTileService`

职责限定为：

- `onStartListening()`：后台执行一次只读状态查询并刷新 Tile；
- `onClick()`：在解锁后执行一次显式 start 或 stop 事务；
- `onStopListening()`：取消尚未开始的只读刷新，不启动额外轮询；
- service 销毁时终止属于本实例的子进程并关闭 executor。

不得承担：

- WebUI 宿主；
- TOML 解析；
- 状态持久化；
- retry scheduler；
- daemon 健康检查；
- 节点切换或订阅逻辑。

### 6.2 `WebUiEntryActivity`

该 Activity 同时作为长按 Tile 的 `QS_TILE_PREFERENCES` 入口，职责为：

1. 创建受控 WebView；
2. 在后台创建会话专用的 mount-master Root shell，并验证固定模块入口；
3. 通过 `WebViewAssetLoader` 和 `RootWebRootPathHandler` 加载模块 `webroot/index.html`；
4. 安装 Android HostAdapter 对应的受限 native bridge；
5. 将资源流、Root shell、事件子进程、WebView 和 Activity 生命周期正确配对；
6. 禁止外部导航在拥有 Root bridge 的同一个 WebView 中加载；
7. Root 被拒绝、模块缺失、入口损坏或资源校验失败时，只显示 APK 内无 bridge 的最小错误页。

它不增加原生导航或第二套视觉设计。页面、路由、组件和业务逻辑仍来自 `webui/`。

### 6.3 `RootCommandExecutor`

统一拥有所有 `su` 子进程，职责为：

- 固定 executable 为 `/data/adb/modules/nethop/bin/nethopctl`；
- 将封闭 operation ID 映射到固定参数列表；
- 为 status、start/stop 和 WebUI operation 设置有界超时；
- 限制 stdout/stderr 大小；
- 终止超时或宿主销毁后的子进程；
- 将退出码、stdout 和受控错误映射为 typed result。

不得暴露：

```text
exec(command: String)
```

允许的接口必须类似：

```text
run(StatusGet)
run(ServiceStart(wait = true))
run(ServiceStop(wait = true))
spawn(EventsSubscribe(allowedKinds))
```

所有参数必须由应用内白名单生成。Tile 路径没有用户输入，因此其 `su -c` 命令可以是三条固定常量。WebUI payload 继续使用 D08 已定义的受控 staging 协议，不把任意文本拼入 shell。

### 6.4 `StatusDecoder` 与 `TileStateMapper`

`StatusDecoder` 只解析版本化 JSON envelope，不解析 human 文本。`TileStateMapper` 是纯函数，将 daemon fact 映射为 Android Tile state，便于单元测试覆盖全部状态组合。

## 7. 模块 WebUI 的双宿主

### 7.1 APK 是否内嵌完整 WebUI

否。Companion APK 不内嵌完整生产 WebUI，只内嵌一个不可执行 Root operation 的最小 fallback 错误页。生产页面唯一位于模块 `webroot`：

```text
webui/ Vue source
       |
       +--> 一次 production build + immutable staging
                        |
                        +--> module/webroot（唯一生产副本）
                                  |
                 +----------------+----------------+
                 |                                 |
                 v                                 v
        KernelSU Module WebUI             Companion Root PathHandler
```

这同时满足单一源码、单一构建产物和单一物理副本。Companion 不复制、解压、缓存整套 WebUI，不启动 localhost server，也不依赖 KernelSU 私有 token。APK 中的 fallback 页面不包含 Vue bundle、业务路由或 Android HostAdapter，只用于说明 Root 拒绝、模块缺失、webroot 损坏和资源读取失败。

### 7.2 Root WebRoot 会话

`RootShellSession` 与 Activity 一一对应：

```text
WebUiEntryActivity.onCreate
  -> Dispatchers.IO
  -> Shell.Builder.create()
       -> mount-master Root shell
       -> verify root status
       -> verify /data/adb/modules/nethop/webroot/index.html
  -> install RootWebRootPathHandler
  -> load trusted local origin

WebUiEntryActivity.onDestroy
  -> disable/detach bridge
  -> close event processes
  -> close active asset streams
  -> destroy WebView
  -> bounded waitAndClose/close Root shell
```

不得使用 libsu 的静态 main shell，因为它是进程级缓存，所有权与 Activity 生命周期不一致。不得为每个 `shouldInterceptRequest` 创建 shell。Root 授权提示只允许在用户长按主动打开 Activity 后出现，Tile 被动刷新不能为了 WebUI 资源预热触发授权。

`RootWebRootPathHandler` 的根路径是编译期常量：

```text
/data/adb/modules/nethop/webroot
```

每个请求必须依次执行：

1. URL decode 恰好一次，拒绝 NUL、反斜杠、绝对路径、空段以及 `.`/`..` 段；
2. canonical root 与 child 校验，child 必须严格位于固定 root 内；
3. 通过绑定同一专用 shell 的 `SuFile` 检查目标；
4. 拒绝 symlink、目录、设备、FIFO、socket 和非普通文件；
5. 只允许构建 manifest 中出现的相对路径，并核对声明长度；
6. MIME 取自构建 manifest 的封闭白名单，拒绝从任意扩展名猜测可执行内容；
7. 设置单文件大小、并发 stream、单会话请求数和总读取字节上限；
8. 任一校验失败返回受控 404/错误响应，不回退网络或任意文件访问。

逐文件 release manifest 由模块现有 WebUI 构建生成，并随 APK 只携带路径、长度、digest 和 MIME 元数据，不携带文件正文。模块安装期继续用 release checksum 校验实际资产；Activity 启动时比较模块 manifest 与 APK 内预期 manifest 的整体 digest，请求期使用其中的路径、长度和 MIME allowlist。若 manifest 身份不匹配，Activity fail-closed 到最小错误页；Tile 控制链仍可使用。

请求期不对每个文件重复执行 `sha256sum`：这会让 Root 文件至少读取两次，而边读边校验又无法在脚本内容交给 WebView 前得出结果。NetHop 威胁模型不对抗已经取得同级 Root、并能同时篡改模块文件的攻击者；安装期完整校验 + 启动期 manifest 身份校验足以防止版本漂移和普通损坏。入口 `index.html` 小且关键，可以在安装 bridge 前完整读取并核对 digest；其他资产按 allowlist 流式提供，读取异常由 WebView 加载失败和受控错误处理收敛。

### 7.3 单一前端与 HostAdapter

Vue WebUI 仍只有一个源码目录：

```text
webui/
  src/
    bridge/
      host.ts
      kernelsu-host.ts
      android-host.ts
```

Android HostAdapter 应实现 D08 已有 `HostAdapter` 语义，而不是让页面增加 `if (android)` 业务分支：

- `run()`：固定 typed operation；
- `spawn()`：事件 JSONL 子进程；
- `listPackages()` / `getPackagesInfo()`：使用 Android PackageManager，并维持既有脱敏和有界语义；
- `toast()`、edge-to-edge、exit：映射到宿主能力；
- host capability 新增封闭值 `android`。

### 7.4 WebView 安全边界

1. 只加载固定模块 `webroot` 中且匹配 APK release manifest 的资产；
2. 使用受信任的本地 origin，不使用 `file://` 开放式跨文件访问；
3. 禁用不需要的文件访问、content access、明文网络和第三方 cookie；
4. `shouldOverrideUrlLoading` 默认拒绝非本地 origin；
5. 外部 URL 只能离开 Root WebView 后交给系统浏览器，并继续服从 WebUI 的固定 allowlist；
6. native bridge 校验 origin、operation ID、参数 shape 和长度；
7. `RootWebRootPathHandler` 只暴露固定 root、manifest allowlist 和只读 stream，不暴露任意 shell、路径、Intent 或写文件能力；
8. fallback 页面不安装 Root bridge，也不允许跳回不可信页面后继续持有 bridge；
9. WebView 销毁时关闭所有资源流、事件子进程和专用 Root shell，不能遗留 `cat`、FIFO 或 `nethopctl events`；
10. release 禁用 WebView debugging；
11. 页面和 bridge 日志不得包含订阅 URL、节点凭据或 Root 命令 payload。

## 8. Tile 状态契约

### 8.1 后端缺口

当前 `status.get` 已返回 runtime state 和 `capture.active`，但没有稳定、窄化地暴露持久 `service.enabled` intent。Tile 不能通过 `capture.active=false` 猜测用户关闭，因为同一事实也可能由以下情况产生：

- 首次订阅尚未成功；
- 启动中或停止中；
- fail-open direct；
- backoff 或 circuit open；
- Wi-Fi scene 暂时禁用代理；
- daemon/core 故障。

Tile 也不能调用 `config.get`，因为完整配置超出磁贴职责并包含订阅等敏感信息。

`status.get` 应增加脱敏的 service 摘要：

```json
{
  "service": {
    "configured_enabled": true,
    "effective_enabled": true,
    "override": null
  },
  "state": "running_tproxy",
  "capture": {
    "active": true
  },
  "diagnostic_code": null
}
```

字段语义：

- `configured_enabled`：持久 `service.enabled`，Tile 点按只根据它决定 start 或 stop；
- `effective_enabled`：应用 Wi-Fi scene 等瞬态覆盖后的有效 intent；
- `override`：封闭、脱敏枚举，例如 `wifi_scene`，不能包含 SSID/BSSID；
- `state`：现有 runtime state；
- `capture.active`：数据面当前是否接管流量；
- `diagnostic_code`：可选稳定诊断码，不携带自由文本或敏感值。

该契约进入实现时应按当前开发期规则直接更新 protocol golden、CLI、WebUI mock 和版本协商，不增加旧字段兼容层。

### 8.2 状态映射

| daemon fact | Tile state | subtitle | 点按行为 |
|---|---|---|---|
| `configured_enabled=false` | `STATE_INACTIVE` | 已关闭 | `service.start --wait` |
| enabled + `running_tproxy` | `STATE_ACTIVE` | TPROXY | `service.stop --wait` |
| enabled + `running_tun` | `STATE_ACTIVE` | TUN | `service.stop --wait` |
| enabled + scene override disabled | `STATE_ACTIVE` | 场景暂停 | `service.stop --wait` |
| `probing/starting_core/starting_tun` | `STATE_UNAVAILABLE` | 启动中 | 暂不接受点击 |
| `stopping` | `STATE_UNAVAILABLE` | 停止中 | 暂不接受点击 |
| `degraded/backoff/circuit_open` | `STATE_UNAVAILABLE` | 异常 | 重新查询后拒绝含糊 toggle |
| daemon 不可达、Root 被拒绝、JSON 非法 | `STATE_UNAVAILABLE` | 不可用 | 不执行写操作 |

Wi-Fi scene 暂停时仍显示 `STATE_ACTIVE`，因为持久主开关保持启用；subtitle 负责表达当前数据面暂停。否则一个视觉上 inactive 的 Tile 点击后却执行 stop，会违反 toggle 的可预测性。

## 9. 点按事务

### 9.1 正常流程

```text
onClick
  -> 检查并解除锁屏
  -> 原子设置本地 busy
  -> Tile 临时 STATE_UNAVAILABLE / 正在处理
  -> status --json
  -> configured_enabled ? stop --wait --json : start --wait --json
  -> status --json
  -> TileStateMapper
  -> updateTile
  -> 清除 busy
```

关键规则：

1. 不根据 `qsTile.state` 直接反转；Tile state 可能陈旧；
2. 不提供 daemon `toggle` 方法；先读 intent，再发明确 start 或 stop；
3. 同一进程同时最多一个写操作；重复点击直接忽略；
4. `start/stop --wait` 返回成功后仍执行一次 status，确认最终事实；
5. APK 不自行轮询 sing-box，不重复 daemon 的健康检查；
6. 失败后查询一次 status；仍不可用时显示稳定错误状态；
7. 不因 Activity/WebUI 正在运行而绕过同一写操作锁。

### 9.2 锁屏

Root 启停不得在安全锁屏下静默执行。设备锁定时应使用系统提供的解锁后执行语义；用户取消解锁则不改变服务状态。APK 不申请设备管理权限，也不自行展示密码或生物识别界面。

### 9.3 超时和资源预算

建议初始预算：

| 操作 | 硬超时 | stdout 上限 | stderr 上限 |
|---|---:|---:|---:|
| status | 3 s | 256 KiB | 16 KiB |
| start/stop `--wait` | 20 s | 256 KiB | 32 KiB |
| WebUI 单次读操作 | 沿用 D08 operation 预算 | 沿用 D08 | 沿用 D08 |
| WebUI event process | 页面生命周期 | JSONL 有界解析 | 16 KiB 单事件诊断 |

超时只终止 APK 拥有的 CLI 子进程，不直接 kill daemon/core。最终状态仍由 daemon 事务和恢复逻辑决定。

## 10. 并发与生命周期

- `TileService` 使用一个单线程 executor；不创建固定四线程池或 cached thread pool；
- read refresh 可以被更新的 click transaction 取代，不能与写操作并发发布 Tile state；
- 每次操作分配单调本地 sequence，旧结果不得覆盖新结果；
- `onStartListening()` 只触发一次 snapshot，不建立周期轮询；
- SystemUI 不可见后不继续刷新流量或节点；
- WebUI 使用自己的有界 event process，但与 Tile 共用同一 RootCommandExecutor 约束；
- 进程重建后不恢复本地 busy 状态，直接重新查询 daemon；
- 不把服务 intent、运行状态或错误写入 SharedPreferences 作为事实缓存。

## 11. Manifest 与权限

核心 Manifest 结构：

```xml
<service
    android:name=".NetHopTileService"
    android:exported="true"
    android:icon="@drawable/ic_nethop_tile"
    android:label="@string/app_name"
    android:permission="android.permission.BIND_QUICK_SETTINGS_TILE">
    <intent-filter>
        <action android:name="android.service.quicksettings.action.QS_TILE" />
    </intent-filter>
    <meta-data
        android:name="android.service.quicksettings.TOGGLEABLE_TILE"
        android:value="true" />
</service>

<activity
    android:name=".WebUiEntryActivity"
    android:exported="true"
    android:excludeFromRecents="true">
    <intent-filter>
        <action android:name="android.service.quicksettings.action.QS_TILE_PREFERENCES" />
    </intent-filter>
</activity>
```

最终 Manifest 只允许任务所需权限。预期不声明：

- `INTERNET`；
- `REQUEST_INSTALL_PACKAGES`；
- 广告 ID、电话、位置、通知、悬浮窗、VPN 或辅助功能权限。

完整 WebUI 已明确包含分应用代理和应用选择，因此 AndroidHostAdapter 需要枚举设备应用并读取 label/icon。普通用户 APK 无法复用 KernelSU Manager 的特权 PackageManager bridge；首版允许声明 `QUERY_ALL_PACKAGES`，但必须满足以下边界：

- 该权限只服务本地应用选择，不通过网络上传或写入日志；
- APK 不声明 `INTERNET`，应用元数据只在当前 WebUI 会话内使用；
- PackageManager 查询有界分批执行，页面退出后释放 icon 和临时对象；
- `lastUsedTimeMs`、`storageBytes` 等需要额外 app-op 的指标保持可选，未授权时返回缺失值，不强迫用户开启 Usage Access；
- 不为可选排序指标扩大为辅助功能、设备管理或常驻 usage observer；
- release Manifest 和权限测试必须拒绝除此以外的广泛数据权限。

## 12. 模块安装集成

### 12.1 发布布局

模块 ZIP 增加：

```text
companion/nethop-companion.apk
```

APK 必须同时进入：

- `checksums.sha256`；
- `build-manifest.json`；
- 发布 SBOM；
- license/provenance 清单；
- 模块 ZIP allowlist 和 checksum contract 测试。

安装脚本必须先验证 APK 是非 symlink 普通文件且 digest 唯一匹配，再允许调用 PackageManager。

### 12.2 首次安装交互

模块主体完成校验和持久目录发布后，若 Companion 尚未安装，则显示：

```text
Install NetHop Quick Settings Tile?

[Volume +] Install
[Volume -] Skip

Waiting for input: 10 seconds
```

倒计时从 10 到 0，每秒有界读取一次 `KEY_VOLUMEUP/KEY_VOLUMEDOWN` 的按下事件：

- 音量加：明确同意安装；
- 音量减：跳过；
- 10 秒无输入：跳过；
- 其他 key event：忽略，继续当前倒计时；
- 不把 key release 当成第二次输入。

### 12.3 倒计时渲染

优先使用 `\r` 原地刷新：

```sh
printf '\r- Waiting for input: %2d seconds' "$remaining"
```

选择完成或到 0 后清除当前行并补换行。安装器输出不是 TTY、只支持 `ui_print` 协议或实测不渲染回车时，降级为逐行 `ui_print`。不得向 `$OUTFD` 的命令协议写入未经封装的控制字符，以免破坏 Root Manager 安装通信。

倒计时和按键读取必须共用同一个 10 秒预算，不允许先等待 10 秒再播放视觉倒计时。每轮使用一秒硬超时读取，保证任意按键响应延迟不超过约一秒。

### 12.4 安装和更新

首次明确同意后执行：

```text
pm install -r --user 0 companion/nethop-companion.apk
```

规则：

- `pm` 不可用时打印警告并跳过，不中止模块安装；
- Companion 已安装且签名匹配时，随模块执行 `pm install -r` 更新，避免 protocol mismatch；
- 签名冲突、downgrade 或 PackageManager 失败时不自动卸载旧应用；
- 安装成功后验证包名和版本，不把完整 PackageManager 输出写入公开日志；
- 无论安装、跳过或失败，最终从模块展开目录删除 APK，减少已安装模块占用；
- ZIP 中仍保留 APK，用户可以从原始发布包手动安装；
- 模块安装成功不依赖 Companion 安装成功。

### 12.5 卸载

NetHop 模块卸载脚本默认不执行 `pm uninstall`：

- 避免模块卸载脚本删除应用数据；
- 避免 Companion 更新/卸载与模块清理形成跨产品事务；
- 模块不存在时 Tile 自动映射为不可用；
- 用户可在 Android 应用设置中显式卸载 Companion。

## 13. 签名、版本和供应链

1. release APK 必须使用稳定发布签名；私钥不进入仓库或模块 ZIP；
2. CI 从受控 secret 注入签名，本地未配置 release key 时只能生成不可发布构建；
3. Android `versionCode` 单调递增，`versionName` 与 NetHop 发布版本关联；
4. APK 声明兼容的 daemon protocol min/max；首次 Root 操作先完成版本协商；
5. 不兼容时 Tile 显示不可用，WebUI 显示固定升级提示，不尝试旧协议 fallback；
6. APK 内预期 WebUI release manifest 的整体 digest 必须与模块 manifest 匹配；模块安装期验证实际 `webroot` 的逐文件 digest；
7. Gradle Wrapper、AGP、JDK、Android SDK platform 和直接/传递依赖全部固定并记录；
8. JitPack 只能通过 `exclusiveContent` 提供 `com.github.topjohnwu.libsu`，dependency verification 固定 `core/io/nio` 的 POM/AAR checksum；
9. release 执行 R8、resource shrink、zipalign 和签名校验；
10. 不使用动态下载代码、远程配置或 CDN 资产；
11. APK provenance 与模块 build manifest 建立双向引用，并记录 libsu upstream revision、许可证和解析来源。

## 14. 性能与体积预算

初始 release 门槛：

| 指标 | 目标 | 硬门禁 |
|---|---:|---:|
| APK（不重复含 WebUI） | <= 1.5 MiB | <= 2.5 MiB |
| Companion 导致的模块 ZIP 增量 | <= 2 MiB | <= 3 MiB |
| 无 WebUI 打开时常驻进程 | 0 | 0 |
| 展开快捷设置到 status 发布 p95 | <= 250 ms | <= 500 ms |
| 点击到 CLI 请求发出 p95 | <= 100 ms | <= 250 ms |
| Tile 主线程阻塞 | 0 network/root I/O | 严禁 root I/O |
| WebUI 首屏 | 沿用 D08 | 不放宽 D08 门禁 |
| WebRoot shell | Activity 会话 1 个 | 严禁按资源创建 |
| WebRoot 资源流 | 按 manifest 有界 | 无 Activity 销毁后残留 |
| 后台定时器 | 0 | 0 |
| 空闲 wakeup | 0 | 0 |

APK 体积统计包含 fallback 页面、WebUI release manifest、Kotlin、协程、serialization、`androidx.webkit`、libsu `core/io/nio` 和所有传递依赖，但不重复计算模块已有的唯一 `webroot`。模块 ZIP 增量按加入 Companion 前后的真实 release ZIP 大小计算。若超出目标，必须提交 R8 mapping、dependency tree 和 size breakdown，不直接提高门槛。

libsu `SuFileInputStream` 使用 FIFO 和后台 `cat`，实现阶段必须单独记录 Root 授权、shell bootstrap、manifest 校验、首个 HTML、静态资源完成和 Vue 可交互五个时间点，并测量冷/热启动 p50/p95。若这一链路不能满足 D08 首屏门禁，应先减少入口 chunk 和资源请求数量；只有证据证明 libsu I/O 是主瓶颈后，才评估审计后的更窄只读传输实现，不能先引入 localhost server、整包复制或 Root service。

## 15. 失败矩阵

| 失败 | Tile/WebUI 行为 | 模块行为 |
|---|---|---|
| 用户拒绝 Root | Tile 不可用，显示 Root 未授权 | 不受影响 |
| `nethopctl` 不存在 | Tile 不可用；WebUI 显示模块不可用 | 不尝试修改文件 |
| daemon socket 不可达 | Tile 不可用 | supervisor 继续自行恢复 |
| protocol 不兼容 | Tile 不可用；WebUI 固定升级提示 | 不做 fallback mutation |
| status JSON 非法/超限 | 拒绝 toggle，Tile 不可用 | 保留 daemon 当前状态 |
| start/stop 返回失败 | 再查一次 status，展示最终事实 | daemon 负责回滚 |
| CLI 超时 | 终止 CLI 子进程并重查一次 | 不 kill daemon/core |
| mount-master Root shell 创建失败或退化为非 Root | Activity 显示本地固定错误页 | Tile 按自身 Root 契约处理 |
| 模块 `webroot/index.html` 缺失 | Activity 显示本地固定错误页 | Tile 启停仍可用 |
| 模块 WebUI release manifest 与 APK 预期 digest 不匹配 | Activity fail-closed，不加载部分页面 | KernelSU Module WebUI 不受 Companion 影响 |
| 路径逃逸、symlink 或非普通文件 | 当前资源返回受控失败；不安装 bridge | 不访问 root 外路径 |
| Root 资源流超限或读取失败 | Activity 显示受控错误并关闭会话 | daemon 不受影响 |
| APK fallback asset 损坏 | Activity 结束并显示原生最小错误文本 | Tile 启停仍可用 |
| event process 异常退出 | WebUI resync 或显示断开 | daemon 不受影响 |
| APK 安装失败 | 安装器警告 | 模块主体继续成功 |
| APK 签名冲突 | 保留旧 APK，提示手动处理 | 不自动卸载 |
| 用户跳过 APK | 不提供 Tile | 模块和 KSU WebUI 正常 |
| 模块已卸载但 APK 保留 | Tile/WebUI 显示模块不可用 | 不创建模块目录 |
| 设备锁定 | 等待解锁或取消 | 不静默执行 Root 写操作 |

## 16. 测试策略

### 16.1 JVM/纯函数测试

- status envelope 正常、缺字段、未知字段、版本错误、超大输出；
- `configured_enabled` 与所有 runtime state 的映射；
- Wi-Fi scene override 不产生反向 toggle；
- busy/sequence 防止旧查询覆盖新写操作；
- 固定 operation 到 argv 的白名单映射；
- 任意文本、换行、shell meta character 无法进入命令；
- timeout、非零退出码和截断输出映射；
- protocol mismatch fail-closed；
- WebUI manifest 拒绝重复路径、绝对路径、`..`、非法 MIME、超大文件和 digest 格式错误；
- URL decode、canonical child、symlink/类型和 manifest allowlist 的组合矩阵；
- Root shell status、入口探测和非 Root fallback 均 fail-closed。

### 16.2 Android 组件测试

- `onStartListening()` 只触发一次 snapshot；
- click 不在主线程执行 Root I/O；
- 连续点击最多产生一个写事务；
- 锁屏取消后不执行 start/stop；
- Tile ACTIVE/INACTIVE/UNAVAILABLE、label 和 subtitle 正确；
- `QS_TILE_PREFERENCES` 直接打开 WebUiEntryActivity；
- WebView 只加载本地 origin；
- 每个 Activity 最多创建一个专用 WebRoot shell，资源请求不会创建新 shell；
- `SuFile` 显式绑定会话 shell，不触发 libsu 全局 main shell；
- manifest 身份匹配且 `index.html` digest 正确时，CSS/JS/SVG/字体按路径、大小和 MIME allowlist 流式加载；
- `../`、双重编码、反斜杠、NUL、symlink、目录、FIFO、设备和未列入 manifest 的文件全部拒绝；
- Root 拒绝、模块缺失、digest 不匹配时只加载无 bridge fallback；
- 外部导航不能留在 Root WebView；
- Activity 销毁后 stream、event process、libsu shell、后台 `cat` 和 FIFO 全部退出；
- release 禁用 WebView debugging。

### 16.3 构建与供应链测试

- Version Catalog 精确锁定 libsu `6.0.0`，release dependency tree 恰好包含所需 `core/io/nio`，不含 `service`；
- JitPack repository 使用 `exclusiveContent`，除 `com.github.topjohnwu.libsu` 外的 group 不能从该仓库解析；
- Gradle dependency verification 在 POM/AAR checksum 变化、缺失或多出 artifact 时 fail-closed；
- 离线复现使用已验证缓存或审计后的固定 artifact/source，不临时放宽仓库策略；
- SBOM、license 和 provenance 包含 libsu `core/io/nio`、Apache-2.0、上游 tag/revision 与 artifact checksum；
- R8 后 APK 不含 `libsu-service`、未使用的 nio API 或第二套 WebUI bundle；
- APK 内预期 WebUI manifest digest 与同一模块 ZIP 中的模块 manifest 一致。

### 16.4 安装器契约测试

- APK 在 checksum/manifest/SBOM/allowlist 中恰好出现一次；
- 音量加安装、音量减跳过、10 秒超时跳过；
- 倒计时按 10..0 顺序且总预算有界；
- TTY 使用 `\r`，非 TTY 不破坏 `$OUTFD`；
- `pm` 缺失、安装失败、签名冲突不使模块失败；
- 已安装同签名 APK 可以 `-r` 更新；
- 跳过或结束后模块目录不保留 APK；
- uninstall 不调用 `pm uninstall`。

### 16.5 前后行为对比

开发期允许破坏性更新 protocol，但必须冻结并对比以下旧行为：

- `nethopctl status/start/stop` 原有 CLI 行为继续通过；
- WebUI 启停、订阅、节点、应用、设置和事件流继续通过；
- `service.enabled=false` 仍只停止数据面，daemon 控制面保持可用；
- TPROXY/TUN 健康检查和回滚不因 APK 出现而改变；
- 未安装 APK 的模块 ZIP 行为与当前基线一致；
- 安装 APK 后，同一操作只产生一个 daemon mutation。

### 16.6 真机矩阵

首版必须在目标 KernelSU 真机完成：

1. 首次刷入选择跳过；
2. 再次刷入选择安装；
3. 编辑快捷设置并添加 NetHop Tile；
4. 代理关闭时点按启动并验证 Google/YouTube；
5. 代理运行时点按停止并确认 capture 撤销；
6. TPROXY 和 TUN 分别验证 ACTIVE 状态；
7. 启动中重复点击不会产生第二个事务；
8. 长按直接打开 WebUI；
9. WebUI 完成现有日常闭环；
10. 冷/热启动分别记录 shell bootstrap、首个 HTML、静态资源完成和可交互耗时；
11. Root 拒绝、daemon 不可达、模块卸载、webroot 缺失/digest 不匹配后的失败状态；
12. 快速反复打开/关闭后无 Root shell、`cat`、FIFO、stream 或 event process 残留；
13. 覆盖安装模块后 APK 同签名升级且 release manifest 同步；
14. APK/WebUI/daemon 进程、线程、FD、RSS、CPU 和 wakeup 预算。

APatch 只有完成同等真机矩阵后才能从“待验证”改为支持。Magisk 不进入本设计首版矩阵。

## 17. 分阶段实施

### Phase T0：契约基线

- 冻结现有 CLI/WebUI 启停 baseline；
- 为 `status.get.service` 编写 RED protocol 和 worker tests；
- 实现 configured/effective intent 与脱敏 override；
- 更新 CLI、WebUI mock、golden 和版本协商；
- 不创建 APK 空壳或未来接口。

### Phase T1：Tile 最小纵切

- 按 YingLi-Player 基线建立 Kotlin + AGP 9 built-in Kotlin 工程；
- 冻结 Version Catalog、Gradle/JDK 校验和最小依赖树；
- 实现固定 RootCommandExecutor、StatusDecoder、TileStateMapper；
- 实现 `onStartListening`、点按 start/stop、busy 和最终 status；
- 完成 Root 拒绝、daemon 不可达和锁屏行为；
- 不接入 WebUI。

### Phase T2：同源 WebUI 宿主

- 为现有 WebUI 增加 Android HostAdapter；
- 验证精确锁定的 `androidx.webkit:webkit:1.17.0` 与 libsu `core/io:6.0.0`，冻结 `nio` 传递依赖和供应链校验；
- 先写路径逃逸、symlink、文件类型、manifest 和 shell 生命周期 RED tests；
- 实现 Activity-owned `RootShellSession`、`RootWebRootPathHandler`、受限 local-origin bridge 和事件生命周期；
- 一次 Vite production build 生成模块 `webroot` 和逐文件 release manifest；APK 只打包 manifest 与无 bridge fallback；
- 长按通过 `QS_TILE_PREFERENCES` 直接进入 WebUI；
- 运行完整 D08/D09、Root 流式读取、资源残留和性能回归，不复制页面。

### Phase T3：模块安装集成

- 将签名 APK 纳入 ZIP、checksum、manifest、SBOM 和 license；
- 实现音量键 10..0 倒计时、`\r` 优先和逐行降级；
- 实现首次 opt-in、同签名更新、失败非阻断和 APK 清理；
- 扩展 fake Magisk/KernelSU module contract 测试。

### Phase T4：发布门禁

- 完成 KernelSU 真机矩阵；
- 校准体积、首个 status、点击延迟、RSS 和 wakeup；
- 完成签名升级与 protocol mismatch 演练；
- 所有门禁通过后才把 APK 放入公开模块产物。

## 18. 完成定义

同时满足以下条件才视为完成：

1. Tile 点按只通过 typed IPC 修改 `service.enabled`；
2. Tile 不依据本地旧状态盲目 toggle；
3. 长按直接打开同源 NetHop WebUI；
4. APK 通过固定根、manifest allowlist、只读 Root PathHandler 加载模块唯一 WebUI，不内嵌完整副本；
5. APK 无 system app、Clash API、TOML 写入和常驻轮询；
6. 当前 KernelSU 私有 token 不进入 NetHop 代码或配置；
7. 首次安装必须音量加明确同意，超时默认跳过；
8. 倒计时优先 `\r` 原地刷新，并具有可靠输出降级；
9. APK 失败不影响模块主体，模块卸载不静默卸载 APK；
10. Activity 销毁后无 Root shell、资源流、`cat`、FIFO 或事件进程残留；
11. protocol、WebUI、CLI、安装器和真机测试全部通过；
12. APK、fallback、manifest 和 Root 读取依赖满足体积、性能、签名和供应链门禁。

## 19. 参考资料

本地源码与文档：

- `refer/Surfing/build.sh`
- `refer/Surfing/customize.sh`
- `refer/Surfing/box_bll/scripts/box.inotify`
- `refer/Surfing/SurfingTile/uninstall.sh`
- `refer/NetProxy-Magisk/src/module/customize.sh`
- `refer/KernelSU/manager/app/src/main/AndroidManifest.xml`
- `refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/navigation3/IntentDispatcher.kt`
- `refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/util/module/Shortcut.kt`
- `refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/util/KsuCli.kt`
- `refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/webui/WebUIActivity.kt`
- `refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/webui/WebViewHelper.kt`
- `refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/webui/SuFilePathHandler.java`
- `refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/webui/WebUIState.kt`
- `refer/KernelSU/manager/gradle/libs.versions.toml`
- `refer/KernelSU/userspace/ksud/src/su.rs`
- `refer/KernelSU/userspace/ksud/src/installer.sh`
- `refer/KernelSU/userspace/ksud/src/restorecon.rs`
- `refer/libsu-master/README.md`
- `refer/libsu-master/CHANGELOG.md`
- `refer/libsu-master/LICENSE`
- `refer/libsu-master/io/build.gradle.kts`
- `refer/libsu-master/core/src/main/java/com/topjohnwu/superuser/Shell.java`
- `refer/libsu-master/core/src/main/java/com/topjohnwu/superuser/internal/BuilderImpl.java`
- `refer/libsu-master/io/src/main/java/com/topjohnwu/superuser/io/SuFile.java`
- `refer/libsu-master/io/src/main/java/com/topjohnwu/superuser/io/SuFileInputStream.java`
- `refer/libsu-master/io/src/main/java/com/topjohnwu/superuser/internal/ShellPipeStream.java`
- `refer/sing-box-for-android-dev/app/src/main/AndroidManifest.xml`
- `refer/FlClash-main/android/app/src/main/AndroidManifest.xml`

Android 官方资料：

- Quick Settings tiles：<https://developer.android.com/develop/ui/views/quicksettings-tiles>
- `TileService`：<https://developer.android.com/reference/android/service/quicksettings/TileService>
- `Tile`：<https://developer.android.com/reference/android/service/quicksettings/Tile>
- WebView native bridge 风险：<https://developer.android.com/privacy-and-security/risks/insecure-webview-native-bridges>
- 加载本地 Web 内容：<https://developer.android.com/develop/ui/views/layout/webapps/load-local-content>
- AndroidX WebKit Google Maven metadata：<https://dl.google.com/dl/android/maven2/androidx/webkit/webkit/maven-metadata.xml>
- libsu 上游：<https://github.com/topjohnwu/libsu>
- libsu JitPack 发布：<https://jitpack.io/#topjohnwu/libsu>

## 20. 最终结论

NetHop 应吸收 SurfingTile 的系统入口体验，但不能吸收其 system app、模块文件 toggle、inotify 和 Clash API 控制链。Companion APK 只负责把 Android SystemUI 和现有 NetHop typed control plane 连接起来；点按发出明确 start/stop，长按承载同源 WebUI。

当前 KernelSU 已用私有随机 token 保护 WebUI deep link，依赖无 token URI 不具备正确性和长期稳定性。但这不代表 Root APK 无法读取模块目录：KernelSU 与 libsu 源码共同证明，可以通过生命周期受控的 mount-master Root shell、`SuFileInputStream` 和 `WebViewAssetLoader.PathHandler` 流式提供模块资源。

因此最小完整方案是：模块保留唯一 WebUI 物理副本，Companion 只增加受限 Android HostAdapter、固定根只读 PathHandler、逐文件 release manifest 和无 bridge fallback。它比 APK 重复打包完整 WebUI 更轻，也消除了双副本漂移；相应代价是必须严格验证 Root shell 生命周期、FIFO/FD 残留和首屏性能。
