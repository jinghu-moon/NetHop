# NetHop WebUI 应用图标获取与展示设计

> 状态：主题图标方案已于 2026-08-16 经用户确认取消。本文件中的 MIUI/HyperOS 主题 ZIP、主题 capability/batch 和双模式设置均为历史设计记录，不属于当前生产实现；当前仅保留 Android Framework/Root Manager 原始图标 provider 与首字符回退。
>
> 日期：2026-08-16
>
> 适用范围：NetHop Alpha、Android 13+ arm64 Root 设备、KernelSU/APatch Module WebUI、NetHop Companion WebView
>
> 上位文档：[`08-webui-design.md`](./08-webui-design.md)、[`17-quick-settings-tile-companion-design.md`](./17-quick-settings-tile-companion-design.md)
>
> TDD 实施清单：[`20-webui-application-icon-tdd-task-list.md`](./20-webui-application-icon-tdd-task-list.md)
>
> 当前实现基线：Companion `PackageIconPathHandler`、KernelSU/APatch `ksu://icon/<package>`、WebUI 虚拟应用列表

## 1. 决策摘要

NetHop 在设置界面增加“应用图标”展示偏好，提供两个明确选项：

1. `手机主题图标`；
2. `应用自带图标`。

该偏好只影响 WebUI 应用列表的视觉展示，不进入代理配置，不改变应用发现、UID 合并、分应用代理、搜索、排序或策略保存语义。

| 项目 | 决策 |
|---|---|
| 设置名称 | 应用图标 |
| 可选值 | `theme`、`original` |
| UI 文案 | 手机主题图标、应用自带图标 |
| 默认值 | 宿主支持主题 provider 时为 `theme`，否则为 `original` |
| 保存位置 | `localStorage` 中的 `nethop.ui.application-icon-style` |
| 生效方式 | 实时生效，无保存按钮，不触发 daemon 配置 apply |
| 主题图标首版范围 | 已验证的 MIUI/HyperOS `/data/system/theme/icons` ZIP provider |
| 原始图标来源 | Android `ApplicationInfo`/`PackageManager` 或 Root Manager 的受控图标 scheme |
| 单应用回退 | 主题缺失、损坏或超限时自动回退应用自带图标 |
| 最终回退 | 应用名称首字符占位，不留空白框 |
| Rust 职责 | Root 下检测、索引和有界读取厂商主题 ZIP；不解析 APK 资源 |
| Kotlin/Manager 职责 | 通过 Android Framework 获取应用自带图标并转为 WebView 可消费的 PNG |
| WebUI 职责 | 保存偏好、按可见项加载、缓存、回退和稳定布局 |
| 网络 | 不访问网络，不增加 `INTERNET` 权限，不使用远程图标服务 |

核心链路：

```text
SettingsView
  |
  +-- nethop.ui.application-icon-style = theme | original
  |
  v
ApplicationIcon
  |
  +-- theme
  |     |
  |     +-- typed read-only batch request
  |     +-- nethopd ThemeIconProvider
  |     +-- fixed MIUI/HyperOS theme ZIP
  |     +-- found ------> PNG data URL
  |     +-- miss/error --+
  |                      |
  +-- original <---------+
        |
        +-- Companion --> WebViewAssetLoader --> PackageManager
        +-- KernelSU/APatch --> ksu://icon/<package>
        +-- error ------------------------------> initial placeholder
```

## 2. 目标与非目标

### 2.1 目标

1. 用户能够明确选择手机主题图标或应用自带图标。
2. 手机主题图标存在时，展示结果应与厂商主题包中的 PNG 一致，不由 NetHop 二次着色或裁剪。
3. 主题包覆盖不完整时逐应用回退，不让整个应用页失败。
4. 原始图标覆盖全部代理候选应用，包括没有桌面入口的应用和系统组件。
5. 图标加载不得阻塞应用列表、搜索、滚动和代理策略保存。
6. 同一包名的重复请求必须命中有界缓存或进行中的共享请求。
7. 厂商私有路径必须被封装为可选 provider，不能写成 Android 通用事实。
8. Root 侧只能读取固定文件和固定 ZIP entry，不能接受来自 WebUI 的自由路径。
9. 构建和运行时均不下载图标或主题数据。

### 2.2 非目标

- 不复刻任意厂商 Launcher 的全部蒙版、阴影、角标、动效和文件夹渲染器。
- 不读取 Launcher 数据库决定 NetHop 应用列表。
- 不把“有 Launcher Activity”误认为“正在桌面顶层显示”。
- 不遍历 `/data/app`，不自行解析 APK `resources.arsc`、Split APK 或 Vector Drawable。
- 不把应用图标写入代理 TOML、daemon 配置数据库或订阅数据。
- 不引入 Coil、Glide、Fresco 或远程图标服务。
- 不在首版适配未经真机或源码验证的其他 OEM 主题目录。
- 不为图标功能引入常驻 Android Service、轮询或文件系统递归监听。
- 不承诺主题图标在 KernelSU、APatch、Companion 和所有 OEM 上像素级一致。

## 3. 术语和语义边界

### 3.1 应用自带图标

“应用自带图标”指 Android 根据应用 `AndroidManifest.xml`、`ApplicationInfo` 和资源配置解析出的应用图标。它可能是：

- 普通 PNG/WebP；
- Vector Drawable；
- Adaptive Icon 的前景和背景层；
- `android:roundIcon` 对应资源；
- 系统为未声明图标的应用提供的默认图标。

它不是“APK 内某个固定路径的 PNG”。Android 官方文档确认 Adaptive Icon 可以由前景、背景和可选 `monochrome` 层组成，并由设备 OEM 蒙版和 Launcher 场景共同决定最终形状。

### 3.2 手机主题图标

“手机主题图标”是当前设备主题系统提供或生成的图标资产。首版仅把以下已验证来源定义为一个 provider：

```text
provider: miui_theme_archive
archive:  /data/system/theme/icons
entry:    res/drawable-<density>/<packageName>.png
```

这个路径是 MIUI/HyperOS 私有实现，不是 Android SDK 契约。其他系统缺少该 provider 时必须报告 `unsupported` 或 `unavailable`，然后使用应用自带图标。

### 3.3 Android 主题化图标

Android 13 起，支持的 Launcher 可以在用户开启主题图标后，对 Adaptive Icon 的 `monochrome` 层应用壁纸和主题色。官方文档同时指出：

- Android 13 至 Android 16 QPR1，应用是否提供 `monochrome` 会影响结果；
- Android 16 QPR2 起，系统可为未提供 `monochrome` 的应用自动生成主题化结果；
- 用户是否开启主题图标会影响结果；
- Launcher 是否支持会影响结果；
- OEM 蒙版和 Launcher 动效会影响结果。

Android SDK 没有向普通第三方应用公开“读取当前 Launcher 最终渲染位图”的通用 API。AOSP Launcher3 源码也表明主题图标选择、着色、缓存和失效由 Launcher 自身的 `IconProvider`、`IconCache` 和主题控制器完成。

因此 NetHop 首版不把 `ApplicationInfo.loadIcon()` 或 `LauncherActivityInfo.getIcon()` 宣称为“当前桌面主题图标”。厂商主题 ZIP 命中时才把结果标记为 `theme`。

### 3.4 应用列表、可启动应用和桌面布局

三者必须分开：

```text
已安装/可见应用
  != 声明 ACTION_MAIN + CATEGORY_LAUNCHER 的 Activity
  != 当前 Launcher 数据库中实际放置的图标
  != 当前桌面顶层直接可见图标
```

NetHop 的分应用代理对象来自安装包、UID 和共享 UID 语义，不来自桌面布局。图标 provider 只为已有应用行提供视觉资产。

## 4. 官方资料结论

### 4.1 PackageManager 与 ApplicationInfo

Android 官方将 `PackageManager` 定义为查询设备当前安装包信息的系统 API。`ApplicationInfo.loadIcon(PackageManager)` 用于加载该应用关联的当前图形图标。

NetHop 应用列表需要覆盖没有 Launcher Activity 的后台应用和系统组件，因此应用自带图标的通用来源应为：

```kotlin
applicationInfo.loadIcon(packageManager)
```

当已有 `ApplicationInfo` 时，不再通过包名执行一次额外查找：

```kotlin
packageManager.getApplicationIcon(packageName)
```

### 4.2 LauncherApps 的适用边界

`LauncherApps.getActivityList()` 和 `LauncherActivityInfo.getIcon(density)` 面向 Launcher Activity、用户和工作资料。它适合启动器或应用抽屉，但不能成为 NetHop 完整代理应用列表的事实源。

首版不依赖 `LauncherApps` 获取原始图标，原因是：

- 同一包可能存在多个 Launcher Activity；
- 无桌面入口的应用仍可能产生网络流量；
- “应用自带图标”应保持包级语义，而不是任意 Activity 图标；
- Launcher 主题最终渲染仍不由该 API 保证。

### 4.3 Android 11+ 包可见性

Android 11 起，`getInstalledApplications()`、`getPackageInfo()`、`queryIntentActivities()` 等查询默认受 package visibility 过滤。NetHop Companion 的核心功能就是枚举应用并配置分应用代理，因此需要完整可见性。

当前 Companion 声明：

```xml
<uses-permission android:name="android.permission.QUERY_ALL_PACKAGES" />
```

该权限只用于本地应用选择和图标解析。应用清单、包名和图标不得上传或写入遥测。若未来通过 Google Play 分发 Companion，必须重新审查 Play 政策；当前模块随附安装路径不以 Play 审核结论替代隐私约束。

### 4.4 WebViewAssetLoader

Android 官方推荐使用 `WebViewAssetLoader` 以 HTTPS-like URL 加载本地 WebView 内容和子资源，并保持同源策略。`PathHandler.handle()` 在后台线程调用，Android KitKat 及以上可能并发调用，处理时间必须尽可能短且共享状态必须同步。

这支持当前 Companion 的原始图标地址：

```text
https://appassets.androidplatform.net/package-icons/original/<revision>/<packageName>
```

非法、缺失或失败资源必须返回显式 404，不能返回 `null`，否则 WebView 可能继续其他 handler 或尝试网络回退。

## 5. 当前项目基线

### 5.1 WebUI

当前 `ApplicationsView.vue`：

- 从 HostAdapter 批量读取包信息；
- 以 package name 作为稳定 key；
- 使用 `VirtualListViewport`，只挂载可见范围及 overscan 行；
- Android host 使用 `/package-icons/<package>`；
- KernelSU/APatch 使用 `ksu://icon/<package>`；
- 图片错误后隐藏 `<img>`，保留首字符占位。

当前 `packageIconSource()` 没有图标类型参数，也没有异步主题图标 provider。

### 5.2 Companion

当前 `PackageIconPathHandler`：

- 调用 `PackageManager.getApplicationIcon(packageName)`；
- 把 Drawable 绘制为 128 x 128 ARGB_8888 Bitmap；
- 编码为 PNG；
- 单图上限 256 KiB；
- PNG 字节 LRU 上限 2 MiB；
- 返回 `Cache-Control: private, max-age=300`；
- Activity 销毁时释放 handler 和 LRU。

该实现已经是按需加载，不会启动时一次性 Bitmap 化所有应用图标。需要改进的是让应用元数据和图标共用 `ApplicationInfo` 快照，并把 URL/cache key 纳入包更新时间。

### 5.3 KernelSU

参考源码 `refer/KernelSU/.../WebViewHelper.kt` 对 `ksu://icon/<package>` 的处理为：

```text
packageName
  -> SuperUserViewModel.apps 中的 ApplicationInfo
  -> AppIconCache.loadIconSync(..., 512)
  -> PNG WebResourceResponse
```

`AppIconCache` 使用 LRU，并把 package name、UID、sourceDir 和尺寸放入 key。它解决 Root Manager WebView 的应用自带图标加载，但没有读取 MIUI/HyperOS `/data/system/theme/icons`。因此 NetHop 不能把 `ksu://icon` 等同于手机主题图标。

### 5.4 当前设备只读审计

设备：`M2012K11AC / alioth`。审计只读取系统状态，未修改主题或 Launcher 数据。

`/data/system/theme/icons` 实际为 ZIP 文件：

| 指标 | 结果 |
|---|---:|
| ZIP 文件大小 | 28,051,931 bytes |
| ZIP 文件条目 | 3,034 |
| PNG 条目 | 2,669 |
| `res/drawable-*/<package>.png` 条目 | 2,053 |
| 唯一包名主题图标 | 2,053 |
| 合法 PNG | 2,053 |
| 主题图标尺寸 | 全部 168 x 168 |
| 最大 PNG entry | 47,849 bytes |

应用和 Launcher 交集：

| 指标 | 结果 |
|---|---:|
| 当前已安装唯一包 | 568 |
| Launcher Activity 唯一包 | 233 |
| 主题包与当前已安装包交集 | 109 |
| Launcher 数据库应用唯一包 | 230 |
| 主题包与 Launcher 数据库交集 | 103 |
| 桌面顶层和底部栏应用唯一包 | 52 |
| 其中命中主题包 | 12 |

结论：主题 ZIP 是预置或历史主题图标目录，不是当前安装应用清单，更不是桌面布局数据库。运行时只允许对已经由 NetHop 应用列表确认存在的包名查询主题图标。

Launcher 数据库只用于本次设计审计，产品实现不得读取：

```text
/data/user_de/0/com.miui.home/databases/launcher*.db
```

## 6. 用户体验设计

### 6.1 设置项

设置页的界面偏好区域增加一行：

```text
应用图标                         手机主题图标  >
```

点击后使用现有 `OptionDropdown` 展示：

| value | 文案 | 含义 |
|---|---|---|
| `theme` | 手机主题图标 | 优先主题 provider，逐应用回退原始图标 |
| `original` | 应用自带图标 | 始终使用 Android/Root Manager 原始图标 provider |

该设置不是内容区域切换，不使用 Segmented。选择后立即写入 UI preference，并通知已挂载的应用图标组件重新解析。

### 6.2 可用性状态

主题 provider 状态分为：

| 状态 | UI 行为 |
|---|---|
| `supported` | 可选择“手机主题图标” |
| `unavailable` | 保留用户偏好，当前有效结果自动回退原始图标 |
| `unsupported` | 选项不可用，当前有效结果为原始图标 |
| `degraded` | provider 可用但部分条目损坏，逐应用回退 |

设置页不展示厂商文件路径、ZIP entry 数、Root 错误或包名差集。详细原因只进入受控诊断代码。

### 6.3 默认值

首次打开时：

```text
theme provider supported -> theme
otherwise                -> original
```

一旦用户明确选择，保存用户值。provider 暂时不可用时不覆盖已保存的 `theme`；主题恢复后自动重新生效。

### 6.4 应用行展示

图标框尺寸固定，不允许图标加载改变行高、文本位置或 Switch 位置：

```text
+--------+  应用名称                           [switch]
|  icon  |  package.name
+--------+
```

规则：

- 使用 `object-fit: contain`；
- 不对厂商主题 PNG 再做圆形裁剪、蒙版或背景着色；
- 图标未返回前显示首字符占位；
- 主题图标失败后无闪烁地替换为原始图标；
- 原始图标失败后保留首字符；
- 切换偏好时旧请求结果不得覆盖新选择；
- 虚拟行复用时不得把上一个包的图标写入新包。

## 7. 总体架构

### 7.1 分层

```text
WebUI presentation
  ApplicationIcon.vue
  useApplicationIconPreference()
  ThemeIconBatchLoader
           |
           v
HostAdapter typed operation
  browser | kernelsu | apatch | android
           |
           v
nethopctl thin JSON client
           |
           v
nethopd read-only control method
           |
           v
nethop-android ThemeIconProvider
  MiuiThemeArchiveProvider
```

应用自带图标不经过 Rust：

```text
android  -> PackageIconPathHandler -> ApplicationInfo.loadIcon()
kernelsu -> ksu://icon/<package>
apatch   -> manager-provided icon scheme when capability confirms support
browser  -> deterministic mock/placeholder
```

### 7.2 为什么只把厂商主题 ZIP 交给 Rust

Rust daemon 以 Root 身份长期运行，适合：

- 只读打开固定 Root 文件；
- 解析 ZIP central directory；
- 对 entry、尺寸和输出建立统一硬限制；
- 跨 WebUI 宿主复用主题 provider；
- 保持有界索引和缓存；
- 在主题文件版本变化时统一失效。

Rust 不适合替代 Android Resources：

- 独立 daemon 没有 Android `Context`；
- APK 图标可能是 Adaptive Icon、Vector Drawable 或 Split APK 资源；
- 自行解析 `resources.arsc` 会重复 Android Framework 已完成的工作；
- 厂商资源覆盖和默认图标语义难以可靠复刻。

因此边界固定为：Rust 只读取已经是 PNG 的厂商主题 entry，Android Framework 负责应用自带 Drawable。

## 8. Rust 主题图标 provider

### 8.1 Provider 接口

```rust
pub enum ThemeIconProviderStatus {
    Supported,
    Unavailable,
    Unsupported,
    Degraded,
}

pub struct ThemeIconCapability {
    pub provider: Option<ThemeIconProviderKind>,
    pub status: ThemeIconProviderStatus,
    pub revision: Option<String>,
    pub reason_code: Option<ThemeIconReasonCode>,
}

pub trait ThemeIconProvider {
    fn capability(&mut self) -> ThemeIconCapability;
    fn load_batch(&mut self, packages: &[PackageName]) -> ThemeIconBatch;
}
```

首版只有：

```rust
ThemeIconProviderKind::MiuiThemeArchive
```

不为未知厂商创建空壳 provider。

### 8.2 固定路径与文件检查

只允许：

```text
/data/system/theme/icons
```

打开前检查：

1. 路径存在；
2. 是普通文件；
3. 不是符号链接；
4. 文件大小在 `1..=64 MiB`；
5. ZIP central directory 可解析；
6. entry 总数不超过 10,000；
7. 不加密；
8. 只接受 Stored 或 Deflate；
9. 不执行任何解压到磁盘操作。

WebUI、CLI 和配置文件都不能覆盖该路径。

### 8.3 Entry grammar

只索引满足以下逻辑的 entry：

```text
res/drawable-<density>/<packageName>.png
```

包名沿用项目现有严格边界：

```regex
^[A-Za-z0-9_.-]{1,256}$
```

不允许 `/`、`\`、`..`、百分号编码、NUL 或控制字符。provider 根据经过验证的 package name 构造候选 entry，不允许调用方传入 entry path。

支持的 density token 首版固定为：

```text
ldpi mdpi hdpi xhdpi xxhdpi xxxhdpi nodpi
```

同一包出现多个 density 时，选择不超过限制的最高分辨率 entry。首版不把 WebView `devicePixelRatio` 作为 Root 文件选择参数，避免把可伪造前端值放入安全边界。

### 8.4 PNG 验证

读取 entry 前后检查：

- uncompressed size `1..=48 KiB`；
- compressed size 不超过 48 KiB；
- PNG 8-byte signature 正确；
- 首个关键 chunk 为 `IHDR`；
- width/height 均在 `1..=512`；
- 不接受动画、SVG、WebP、JPEG 或任意声明 MIME；
- 实际读取字节数必须等于 ZIP metadata 的 uncompressed size；
- 单个 batch 的原始图标总量不超过 384 KiB。

48 KiB 上限来自两个事实：

1. 当前真机最大主题 PNG 为 47,849 bytes；
2. 48 KiB 的标准 Base64 正好不超过 64 KiB，符合现有 `MAX_WEBUI_STRING_BYTES`。

超限条目返回 `too_large`，然后由前端回退原始图标。

### 8.5 ZIP 依赖选型

使用结构化 ZIP parser，不手写 central directory，不调用 shell `unzip`，不整包释放到临时目录。

候选依赖：

```toml
zip = { version = "=7.2.0", default-features = false, features = ["deflate-flate2"] }
flate2 = { version = "1", default-features = false, features = ["rust_backend"] }
lru = { version = "=0.18.0", default-features = false }
```

选型理由：

- `zip 7.2.0` MSRV 为 Rust 1.83，兼容项目当前 Rust 1.86；
- `zip 8.6.0` 当前 MSRV 为 Rust 1.88，不应在未升级 workspace MSRV 前引入；
- 关闭默认特性，避免 AES、bzip2、deflate64、LZMA、PPMd、time、zstd 和 xz；
- 只启用读取当前主题包所需的 Deflate；
- `lru 0.18.0` MSRV 为 Rust 1.85，兼容当前 workspace；
- 所有新增依赖必须进入 Cargo.lock、许可证、SBOM 和二进制体积审计。

禁止调用 `ZipArchive::extract()`。NetHop 只使用 central directory 和精确 `by_name`/entry index 读取，从设计上排除 Zip Slip 和符号链接写出路径。

### 8.6 Revision 与失效

provider revision 由以下只读 metadata 规范化后计算摘要：

```text
device + inode + size + mtime_ns
```

revision 只用于缓存失效，不作为文件完整性的安全证明。每次 capability 或 batch 请求先执行廉价 metadata 比较；变化时：

1. 关闭旧 `ZipArchive<File>`；
2. 清空 entry index；
3. 清空 PNG LRU；
4. 重新校验并建立索引；
5. 发布新 revision。

不建立 inotify 常驻监听，不轮询 Launcher，不读取主题应用数据库。

### 8.7 Rust 缓存

Rust provider 缓存最多 32 个经过验证的 PNG entry：

```text
key   = revision + packageName
value = immutable PNG bytes
```

最坏上限：

```text
32 * 48 KiB = 1.5 MiB
```

同一 batch 内包名先去重。缓存 miss 才访问 ZIP，missing 结果可做短生命周期 negative cache，但最多 128 个 package，revision 改变时全部失效。

## 9. Read-only 控制协议

### 9.1 Capability

新增只读方法：

```text
webui.icon.capability.get
```

结果示例：

```json
{
  "schema_version": 1,
  "theme": {
    "status": "supported",
    "provider": "miui_theme_archive",
    "revision": "64-lowercase-hex",
    "reason_code": null
  },
  "original": {
    "status": "host_provided"
  }
}
```

Root 路径、文件 owner、ZIP entry 数和解析错误文本不进入 WebUI DTO。

### 9.2 Theme batch

新增只读方法：

```text
webui.icon.theme.batch
```

请求参数：

```json
{
  "packages": ["com.example.one", "com.example.two"]
}
```

约束：

- 1 到 12 个包名；
- 每个包名通过相同 validator；
- 去重后保持首次出现顺序；
- 不接受路径、density、MIME、archive、size 或 output file 参数；
- 方法只读，不要求 expected digest。

结果：

```json
{
  "schema_version": 1,
  "provider": "miui_theme_archive",
  "revision": "64-lowercase-hex",
  "items": [
    {
      "package_name": "com.example.one",
      "status": "found",
      "mime": "image/png",
      "payload_base64": "iVBORw0KGgo..."
    },
    {
      "package_name": "com.example.two",
      "status": "missing"
    }
  ]
}
```

item 状态固定为：

```text
found | missing | invalid | too_large | unavailable
```

`payload_base64` 只允许出现在 `found`，解码后必须再次检查 PNG signature 和大小。整个控制帧继续受 1 MiB 上限约束。

### 9.3 CLI 与 Host allowlist

CLI 只提供 typed command：

```text
nethopctl webui icon capability --json
nethopctl webui icon theme <package>... --json
```

KernelSU、APatch 和 Android bridge 必须为这两个 operation 建立精确 argv 规则。禁止开放：

```text
unzip
cat /data/system/theme/icons
任意 nethopctl 参数
任意文件读取
```

主题图标批量查询应由一次 `nethopctl`/UDS 往返完成，不能为每个可见图标创建一个 Root shell 进程。

## 10. 应用自带图标 provider

### 10.1 Companion PackageRepository

新增 Activity 会话级 `AndroidPackageRepository`，供应用列表、包详情和图标 handler 共用：

```text
PackageManager snapshot
  -> packageName -> PackageInfo/ApplicationInfo
  -> listPackages()
  -> packageInfo()
  -> originalIcon()
```

图标路径使用已有 `ApplicationInfo`：

```kotlin
val drawable = applicationInfo.loadIcon(packageManager)
```

不在每次图标请求中重新通过包名查询 `ApplicationInfo`。

### 10.2 Companion URL

使用固定本地域名：

```text
https://appassets.androidplatform.net/package-icons/original/<lastUpdateTimeMs>/<packageName>
```

`lastUpdateTimeMs` 只作为缓存版本，handler 必须和当前 `PackageInfo.lastUpdateTime` 比对。不匹配返回 404，前端重新读取应用列表后得到新 URL。

响应：

```http
Content-Type: image/png
Cache-Control: private, max-age=300
X-Content-Type-Options: nosniff
```

### 10.3 Drawable 渲染

- 输出固定 128 x 128 px；
- `Bitmap.Config.ARGB_8888`；
- 保留透明通道；
- PNG 编码；
- 单图最大 256 KiB；
- 编码失败、包消失或 Drawable 异常返回 404；
- Bitmap 在 finally 中回收；
- 处理发生在 `PathHandler` worker thread，不进入主线程。

### 10.4 KernelSU/APatch

`original` 优先使用 Root Manager 已提供的图标 scheme：

```text
ksu://icon/<packageName>
```

APatch 只有在 capability 明确确认兼容 scheme 后使用；否则回退首字符。NetHop 不复制 Manager 私有图标缓存，也不读取 Manager 应用私有目录。

## 11. WebUI 实现设计

### 11.1 UI preference

扩展：

```ts
export type UiPreferenceKey =
  | "theme"
  | "last-route"
  | "application-sort"
  | "application-selected-first"
  | "application-icon-style"
  | "node-sort";

export type ApplicationIconStyle = "theme" | "original";
```

只把枚举值写入：

```text
nethop.ui.application-icon-style
```

禁止写入：

- package name；
- Base64 图标；
- data URL；
- provider revision；
- Root 路径；
- capability 报告。

### 11.2 ApplicationIcon 组件

从 `ApplicationsView.vue` 提取单一职责组件：

```ts
interface ApplicationIconProps {
  packageName: string;
  label: string;
  lastUpdateTimeMs?: number;
}
```

状态机：

```text
placeholder
  |
  +-- style=theme --> theme_loading
  |                    |
  |                    +-- found --> theme_ready
  |                    +-- miss/error --> original_loading
  |
  +-- style=original ------------------> original_loading
                                            |
                                            +-- load --> original_ready
                                            +-- error -> placeholder
```

每次 package、style 或 provider revision 变化时递增 request token。异步结果提交前必须匹配当前 token，防止虚拟行复用和快速切换造成陈旧写入。

### 11.3 ThemeIconBatchLoader

同一 animation frame/microtask 窗口内收集可见组件请求：

- 16 ms 内合并；
- 每批最多 12 个 package；
- 同一 package 共享 Promise；
- 最多 2 个 batch 并行；
- 页面离开后取消未发送 batch；
- 已发送只读请求允许完成，但结果无消费者时不写状态。

WebUI 主题图标缓存：

```text
key: revision + packageName
cap: 32 entries
value: data:image/png;base64,... | missing
```

provider revision 变化时清空 found 和 missing cache。

### 11.4 原始 URL resolver

```ts
function originalPackageIconSource(
  host: HostKind,
  packageName: string,
  lastUpdateTimeMs?: number,
): string | undefined;
```

该函数只构造 URL，不执行异步请求。主题加载和原始 URL 构造不得继续混在当前 `packageIconSource()` 中。

### 11.5 设置页集成

在 `SettingsView.vue` 的 `settings-utilities` 增加与“界面主题”一致的设置行和 `OptionDropdown`。它不进入 `config.schema`、draft、validate 或 apply 流程。

切换行为：

1. 校验值只允许 `theme|original`；
2. 更新 `useUiPreference`；
3. 当前应用页组件响应式切换；
4. 不显示“配置已保存”业务消息；
5. 不重启 daemon；
6. 不重载应用列表。

## 12. 缓存设计

### 12.1 缓存层级

```text
Theme
  WebUI data URL LRU (32)
      -> daemon PNG LRU (32 / <= 1.5 MiB)
          -> theme ZIP persistent source

Original on Companion
  WebView HTTP cache (5 min)
      -> PackageIconPathHandler PNG byte LRU (2 MiB)
          -> PackageRepository ApplicationInfo snapshot
              -> Android PackageManager
```

### 12.2 为什么首版不增加独立磁盘图标缓存

- 主题 ZIP 本身已经是持久图标存储，不需要复制为第二套磁盘目录；
- 原始图标来自本机 APK/PackageManager，当前只对虚拟列表可见项编码；
- WebView 和 native handler 已覆盖会话内高频重用；
- 持久磁盘缓存需要处理包更新、主题切换、用户/profile、density 和容量清理；
- 在真机证据显示重新打开页面仍有明显延迟前，不增加这套状态。

如果后续实测跨会话首屏不达标，再单独评审磁盘缓存，不在首版提前实现。

### 12.3 缓存 key

| 来源 | key |
|---|---|
| Theme Rust | `providerRevision + packageName` |
| Theme WebUI | `providerRevision + packageName` |
| Original Companion | `packageName + lastUpdateTimeMs + sizePx` |
| Original KernelSU | 由 Manager 管理；当前参考实现含 package、UID、sourceDir、size |

不得只用 package name 作为跨版本持久 key。

## 13. 安全与隐私边界

### 13.1 文件安全

- 只读固定 `/data/system/theme/icons`；
- 拒绝 symlink、目录、过大文件和未知压缩算法；
- 不递归搜索 `/data/system`；
- 不读取 Launcher 数据库；
- 不释放 ZIP entry 到磁盘；
- 不接受前端路径；
- 不执行厂商 ZIP 内脚本、XML 或 fancy icon 数据；
- `fancy_icons/` 和 `transform_config.xml` 首版全部忽略。

### 13.2 WebView 安全

- Companion 使用 `https://appassets.androidplatform.net`；
- PathHandler 仅注册 `/package-icons/original/`；
- 非法和 missing 返回显式 404；
- 不允许网络 fallback；
- 保持 `X-Content-Type-Options: nosniff`；
- 主题 Base64 只允许 PNG，受 CSP `img-src 'self' data:` 约束；
- 不增加 `blob:`、远程 origin 或通用 `file://`；
- native bridge 继续要求精确 source origin 和 main frame。

### 13.3 数据隐私

- 安装应用清单属于敏感本地数据；
- package name 不写日志、诊断包或遥测；
- 图标字节不持久化到 localStorage；
- 图标响应不包含 APK 路径、UID 或 sourceDir；
- theme capability 不暴露 Root 文件 metadata；
- mock/e2e 使用固定虚构包名，不使用真机应用列表。

## 14. 性能与资源预算

| 指标 | 硬门槛 | 目标 |
|---|---:|---:|
| 应用列表元数据首屏 | 不等待图标 | 不等待图标 |
| 同屏主题请求 batch | <= 12 packages | 8-10 packages |
| theme batch 并发 | <= 2 | 1 |
| theme 单图原始字节 | <= 48 KiB | <= 24 KiB |
| theme batch 原始字节 | <= 384 KiB | <= 192 KiB |
| Rust theme cache | <= 32 entries / 1.5 MiB | <= 1 MiB |
| WebUI theme cache | <= 32 entries | 16-24 entries |
| Companion PNG cache | <= 2 MiB | <= 2 MiB |
| Companion 单原始图标 | <= 256 KiB | <= 64 KiB |
| 原始输出尺寸 | 128 x 128 px | 128 x 128 px |
| 设置切换到首批稳定图标 p95 | <= 700 ms | <= 350 ms |
| 缓存命中单图解析 p95 | <= 20 ms | <= 5 ms |
| 主线程同步图标 I/O | 0 | 0 |
| 额外常驻线程 | 0 | 0 |
| 额外网络请求 | 0 | 0 |

测量必须区分：

```text
preference resolve
capability query
CLI/UDS overhead
ZIP lookup/decode
Base64 encode/decode
browser image decode
first paint
cache hit
```

不能把所有耗时只记录为“图标加载”。

## 15. 失败与回退矩阵

| 条件 | 结果 | 诊断代码 |
|---|---|---|
| 主题路径不存在 | 使用原始图标 | `theme_icon_provider_missing` |
| 路径是 symlink/目录 | 使用原始图标 | `theme_icon_provider_unsafe_file` |
| ZIP 损坏 | 使用原始图标 | `theme_icon_archive_invalid` |
| ZIP 超过 64 MiB | 使用原始图标 | `theme_icon_archive_too_large` |
| entry 不存在 | 单应用使用原始图标 | `theme_icon_entry_missing` |
| entry 超过 48 KiB | 单应用使用原始图标 | `theme_icon_entry_too_large` |
| PNG signature/IHDR 非法 | 单应用使用原始图标 | `theme_icon_png_invalid` |
| revision 在请求中变化 | 丢弃旧 batch，重新查询 | `theme_icon_revision_changed` |
| typed request 超时 | 当前 batch 使用原始图标 | `theme_icon_request_timeout` |
| Base64 非法 | 单应用使用原始图标 | `theme_icon_payload_invalid` |
| Companion 包已卸载 | 首字符占位 | `original_icon_package_missing` |
| PackageManager 抛异常 | 首字符占位 | `original_icon_load_failed` |
| PNG 编码失败/超限 | 首字符占位 | `original_icon_encode_failed` |
| WebView Activity 关闭 | 取消前端消费者并释放 handler | 不记录错误 |

图标失败不得：

- 阻止应用列表展示；
- 把应用从代理候选中移除；
- 改变选中状态；
- 触发配置回滚；
- 弹出逐图标错误消息；
- 退回网络 URL。

## 16. TDD 与验证要求

### 16.1 Rust 单元测试

至少覆盖：

1. 合法 fixed-path provider capability；
2. missing、symlink、目录、超大 archive；
3. invalid central directory；
4. entry grammar 和 package validator；
5. Stored/Deflate 正常读取；
6. 加密、未知压缩算法拒绝；
7. entry size、batch size、entry count 限制；
8. PNG signature、IHDR、width/height；
9. 同包多 density 的确定性选择；
10. duplicate package 去重并保持顺序；
11. revision 变化清空 positive/negative cache；
12. 不调用或暴露 archive extract；
13. 恶意 `../`、绝对路径和混淆 entry 不可命中；
14. 全部错误映射为稳定 reason code，不泄露路径。

fixture 由测试生成，不提交真机主题 ZIP。

### 16.2 Protocol/CLI 测试

- 新方法 before fixture 明确不存在，after fixture 明确存在；
- capability DTO 严格字段和枚举；
- batch 数量 0、13、非法包名拒绝；
- 单字符串 64 KiB 和整帧 1 MiB 边界；
- CLI argv 只接受 typed 子命令；
- KernelSU/Android bridge allowlist 正反例；
- 图标方法只读，不要求 expected digest，不产生 mutation event；
- stdout/stderr 不包含 Root 路径或真机包名日志。

### 16.3 Companion JVM/instrumentation

- `AndroidPackageRepository` snapshot 被 list/info/icon 复用；
- `ApplicationInfo.loadIcon()` 成功；
- AdaptiveIconDrawable、VectorDrawable 和普通 BitmapDrawable；
- 128 px 输出和透明通道；
- 2 MiB LRU 淘汰；
- package update revision 不匹配返回 404；
- invalid path、query、fragment、包名返回 404；
- `PathHandler` 并发请求无竞态；
- close 后请求返回 404，Bitmap/stream/cache 释放；
- Manifest 仍无 `INTERNET`；
- `QUERY_ALL_PACKAGES` contract 保持。

### 16.4 WebUI unit/browser

- preference allowlist、默认值、损坏值回退；
- Settings `OptionDropdown` 两个选项和实时切换；
- theme supported/unavailable/unsupported；
- batch 16 ms 合并、12 上限、2 并发；
- in-flight dedupe 和 LRU；
- revision 变化失效；
- theme found、missing、invalid、timeout 回退；
- original error 回到首字符；
- 快速 style/package 切换不接受旧结果；
- 虚拟滚动只加载 mounted rows；
- 图标加载不改变应用行几何尺寸；
- 应用搜索、排序、选中和自动保存无回归；
- localStorage 不出现包名、Base64 或 data URL。

### 16.5 真机矩阵

至少包含：

| 场景 | 验收 |
|---|---|
| 当前 MIUI/HyperOS 主题包 | 命中包显示主题 PNG |
| 主题包缺少包名 | 同行显示应用自带图标 |
| 切换为应用自带图标 | 全部行停止使用主题 provider |
| 来回切换 20 次 | 无错位、闪烁、崩溃和缓存串包 |
| 快速滚动 500+ 应用 | 无明显主线程卡顿，内存有界 |
| 切换手机主题 | revision 更新，旧主题缓存失效 |
| 应用升级 | original URL revision 更新 |
| Companion 关闭重开 | 无 Root/UDS/stream 残留 |
| provider 文件临时不可用 | 页面正常，全部回退原始图标 |
| KernelSU WebUI | original 可用；theme typed provider 按 capability 生效 |

桌面 Launcher 数据库不属于真机功能测试依赖。

## 17. 实施顺序

项目尚未正式发布，不保留旧 `packageIconSource()` 兼容层。按以下顺序 TDD：

1. 冻结当前 original 图标、应用列表和 localStorage before 基线；
2. 建立 Rust ZIP parser fixture 和安全限制 RED；
3. 实现 `MiuiThemeArchiveProvider` 及 revision/cache；
4. 增加 protocol、daemon、CLI typed capability/batch；
5. 扩展 KernelSU/Android bridge allowlist；
6. 重构 Companion `AndroidPackageRepository` 和 original URL revision；
7. 新增 WebUI preference、batch loader 和 `ApplicationIcon.vue`；
8. 设置页接入 `OptionDropdown`；
9. 完成 Rust、WebUI、Companion 和模块发布回归；
10. 构建模块并执行主题切换真机验收。

每一步必须先有失败测试，再实现，再删除被替代的手写逻辑。禁止同时保留新旧两套图标 resolver。

## 18. 不采纳方案

### 18.1 只使用 PackageManager

优点是通用、简单，但无法优先使用已验证的厂商主题 PNG，不能满足用户选择“手机主题图标”的明确需求。保留为 original provider 和 theme fallback。

### 18.2 只使用主题 ZIP

当前主题包只有 109 个条目对应当前 568 个安装包。主题 ZIP 是预置目录，不完整。只使用它会产生大量空白图标。

### 18.3 使用 LauncherApps 作为应用列表

会丢失没有 Launcher Activity 的代理候选应用，并把 Activity 级图标与包级代理策略混合。不采纳。

### 18.4 读取 Launcher 数据库

数据库是 OEM 私有实现，包含用户桌面布局和敏感偏好，schema 可变，也与 NetHop 应用范围无关。不采纳。

### 18.5 Rust 解析 APK 图标

需要自行实现 `resources.arsc`、Split APK、Vector Drawable、Adaptive Icon、资源覆盖和密度选择，重复 Android Framework。不采纳。

### 18.6 每图标执行 shell unzip

会为虚拟列表滚动创建大量 Root 命令，无法共享 central directory 和缓存，难以建立并发与输出边界。不采纳。

### 18.7 整包解压到模块或 Companion cache

增加约 28 MiB 以上重复磁盘数据、清理和主题失效逻辑，并扩大 Zip Slip 风险。不采纳。

### 18.8 Base64 写入 localStorage

扩大同步存储、泄露安装应用痕迹、增加主线程序列化和失效复杂度。不采纳。

### 18.9 引入 Coil/Glide/Fresco

图标来源全部是本地 Android Drawable 或本地 PNG，不需要网络图片框架。现有 WebView、LRU 和浏览器解码已足够。不采纳。

### 18.10 自动选择且不给用户设置

用户可能偏好品牌原始图标，也可能偏好手机主题一致性。自动策略无法表达该偏好，且厂商覆盖不完整。不采纳。

## 19. 完成定义

以下条件全部满足才算完成：

1. 设置页提供“手机主题图标”和“应用自带图标”，实时生效。
2. 主题模式按单应用回退，不影响应用列表和代理策略。
3. 当前 MIUI/HyperOS 主题 ZIP 命中图标可被准确展示。
4. 主题 ZIP 不被整包解压，不读取任意路径，不依赖 Launcher 数据库。
5. 原始图标通过 `ApplicationInfo`/Manager provider 获取，覆盖无 Launcher Activity 应用。
6. 虚拟列表只加载可见范围，批量请求、并发和缓存均有硬上限。
7. 图标切换、滚动和异步回包不会发生串包或布局抖动。
8. localStorage 只保存枚举，不保存包名和图标字节。
9. Companion 仍不声明 `INTERNET`，WebView 无网络 fallback。
10. 新依赖进入 lockfile、SBOM、许可证和模块体积门禁。
11. Rust、protocol、CLI、WebUI、Companion、模块 contracts 全部通过。
12. MIUI/HyperOS 真机完成主题/原始切换、主题更新和 500+ 应用滚动验证。

## 20. 资料与证据

### 20.1 Android 官方

- Adaptive icons：<https://developer.android.com/develop/ui/compose/system/icon_design_adaptive>
- Android 13 themed app icons：<https://developer.android.com/about/versions/13/features#themed-app-icons>
- `AdaptiveIconDrawable`：<https://developer.android.com/reference/android/graphics/drawable/AdaptiveIconDrawable>
- `ApplicationInfo`：<https://developer.android.com/reference/android/content/pm/ApplicationInfo>
- `PackageManager`：<https://developer.android.com/reference/android/content/pm/PackageManager>
- `LauncherActivityInfo`：<https://developer.android.com/reference/android/content/pm/LauncherActivityInfo>
- `LauncherApps`：<https://developer.android.com/reference/android/content/pm/LauncherApps>
- Package visibility：<https://developer.android.com/training/package-visibility>
- Load in-app content：<https://developer.android.com/develop/ui/views/layout/webapps/load-local-content>
- `WebViewAssetLoader.PathHandler`：<https://developer.android.com/reference/androidx/webkit/WebViewAssetLoader.PathHandler>

### 20.2 AOSP 官方源码

- Launcher3 `LauncherAppState`：<https://android.googlesource.com/platform/packages/apps/Launcher3/+/master/src/com/android/launcher3/LauncherAppState.java>
- Launcher3 `LauncherIcons`：<https://android.googlesource.com/platform/packages/apps/Launcher3/+/master/src/com/android/launcher3/icons/LauncherIcons.java>
- Launcher3 `Utilities.getFullDrawable`：<https://android.googlesource.com/platform/packages/apps/Launcher3/+/master/src/com/android/launcher3/Utilities.java>

### 20.3 Rust 依赖资料

- `zip 7.2.0` metadata/MSRV/features：<https://docs.rs/crate/zip/7.2.0/source/Cargo.toml.orig>
- `ZipArchive`：<https://docs.rs/zip/7.2.0/zip/read/struct.ZipArchive.html>
- zip-rs extract advisory：<https://github.com/zip-rs/zip2/security/advisories/GHSA-94vh-gphv-8pm8>
- `lru 0.18.0` metadata/MSRV：<https://docs.rs/crate/lru/0.18.0/source/Cargo.toml.orig>

### 20.4 本地参考源码

- `refer/KernelSU/js/README.md`
- `refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/webui/WebViewHelper.kt`
- `refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/util/AppIconCache.kt`
- `refer/KernelSU/manager/app/src/main/java/me/weishu/kernelsu/ui/webui/SuFilePathHandler.java`
- `refer/libsu-master`
- `companion/app/src/main/kotlin/com/jinghumoon/nethop/companion/packages/PackageIconPathHandler.kt`
- `companion/app/src/main/kotlin/com/jinghumoon/nethop/companion/packages/AndroidPackageAdapter.kt`
- `webui/src/bridge/package-icon.ts`
- `webui/src/views/ApplicationsView.vue`
- `webui/src/runtime/storage.ts`

厂商主题 ZIP 的路径、结构和数量来自 2026-08-16 当前真机只读审计。它是设备证据，不是 Android 官方 API，也不应推广为所有 MIUI/HyperOS 版本的稳定兼容承诺。
