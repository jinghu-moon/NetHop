# NetHop 节点国家/地区元数据与 WebUI 节点页改进设计

> 状态：开发期设计候选
> 日期：2026-08-13
> 目标平台：Android arm64 Root 模块、Magisk/KernelSU WebUI
> 参考原型：`refer/proxy_node_list.html`
> 真实样本：`refer/glados-facility.com_khi1215215@163.yaml`、`refer/魔戒.yaml`、`refer/fsllist.yaml`
> 上位约束：`00-nethop-system-design.md`、`08-webui-design.md`、`10-subscription-selection-and-node-optimization-refactor-design.md`、`13-rust-node-benchmark-engine-design.md`
> 影响范围：`nethop-subscription`、`nethop-core`、`nethopd`、`nethop-protocol`、`nethopctl`、WebUI、模块构建与许可证清单

## 1. 文档目的

当前节点页已经具备多订阅合并、来源分组、自动/手动选择、活动节点解析、两列虚拟列表和 Rust 批量测速，但缺少易于扫描的地区信息和足够明确的当前活动节点摘要。

本文基于 `proxy_node_list.html`，只吸收已经由真实数据和现有后端能力支持的设计：

1. 节点卡片左下角展示可选的本地 SVG 国家/地区旗帜；
2. 节点卡片右下角突出延迟数字，并按固定阈值使用绿、黄、红三档颜色；
3. 节点页顶部展示后端确认的实际活动终端节点详情；
4. Rust 后端维护唯一的节点名称国家/地区识别规则，WebUI 不解析节点名称；
5. 地区无法可靠识别时保持未知，不通过服务器 IP、域名或 SNI 猜测；
6. 保留现有多订阅、测速、选优、手动选择、虚拟滚动和事件同步能力。

项目尚未发布。本次允许直接升级 generation registry、IPC 和前端 DTO，不保留旧版本兼容分支；但必须先冻结当前有效行为，再用新旧对比测试证明既有功能没有退化。

## 2. 已核实事实

### 2.1 sing-box 不提供节点地区元数据

sing-box 普通代理 outbound 没有通用的 `country`、`country_code`、`region` 或 `region_code` 字段：

- Clash API 节点信息主要包含 tag、类型、UDP 能力和测速历史；
- selector/urltest group 额外提供 `now`、`all`，仍不包含地区；
- libbox/gRPC outbound group item 提供 tag、类型和 URLTest 时间/延迟，不包含地区；
- GeoIP 国家代码用于目标流量路由匹配，不是代理节点出口位置；
- SFA 不会根据节点自动生成国家/地区旗帜。

因此，NetHop 不能从 sing-box 控制面获得 `region_code`。

### 2.2 Clash YAML 也没有标准地区字段

`glados-facility.com_khi1215215@163.yaml` 的代理项包含：

```text
name, type, server, port, cipher, password,
udp, udp-over-tcp, plugin, plugin-opts, host, mode
```

地区只存在于名称中：

```text
US-D1-1             -> US
TW-IPv6-P2-1        -> TW
JP-Dedicated-P1-1   -> JP
SG-X5-1             -> SG
```

`Fast-B2-1`、`Balancer-B1-1` 等名称不能可靠判断地区。

`魔戒.yaml` 同样没有结构化地区字段，但 42 个真实节点名称包含较明确的中文名称或 ISO 代码：

```text
日本JP-HY2
新加坡SG-A-Gemini
香港HK-A-Gemini
韩国KR-HY2
美国LA-优化-GPT
加拿大-优化
法国FR-A
```

另外有两个代理项实际承担订阅状态展示：

```text
剩余流量：388.64 GB
套餐到期：长期有效
```

“信息节点识别与过滤”是订阅质量治理问题，不等同于地区识别。本文不允许用“没有地区”作为过滤节点的依据。

`fsllist.yaml` 也没有结构化国家/地区字段，但 51 个节点全部采用规整的 `ISO alpha-2 + 城市名称 + 编号` 命名：

```text
GB-Canary Wharf-...
US-New York-...
IN-Mumbai-...
JP-Tokyo-...
RO-Bucharest-...
HK-Hong Kong-...
NL-Amsterdam-...
SG-Singapore-...
FR-Paris-...
ID-Surabaya-...
```

其 alpha-2 分布为 `US 19`、`RO 12`、`GB 4`、`JP 4`、`NL 4`、`HK 3`、`SG 2`、`FR 1`、`IN 1`、`ID 1`。51 个名称均可由开头的独立 alpha-2 token 确定识别，未发现代码与城市名称冲突，也没有信息型伪节点。该样本证明标准 alpha-2 token 应优先由官方 registry 全量支持，不需要为每个随附城市立即扩充 `LocationRecord`。

### 2.3 服务器地址不能证明出口地区

禁止使用 `server` IP、域名、SNI 或证书信息推断旗帜，原因包括：

- 入口可能位于 CDN 或 Anycast 网络；
- 中转、IEPL、多跳线路可能入口和出口分离；
- 同一个入口地址可以通过端口或认证映射到多个地区出口；
- 供应商内部域名字符没有标准语义。

例如 `魔戒.yaml` 中大量不同国家节点共享 `a.mjanyt.com`，只依靠 server 无法区分地区。

### 2.4 当前 NetHop 已有的数据

当前 generation 节点注册表已经包含：

```rust
struct GenerationNodeRecord {
    stable_node_id: String,
    internal_tag: String,
    display_name: String,
    protocol: String,
    source_ids: Vec<String>,
    auto_candidate: bool,
}
```

`node.list` 已返回：

```text
id, name, protocol, source_ids,
latency_ms, alive, is_requested, is_active
```

因此当前节点名称、协议、来源、延迟和活动状态都有真实数据来源。缺口只有地区展示元数据及相应前端呈现。

## 3. 目标与非目标

### 3.1 目标

- 对常见节点名称保守推导 ISO 3166-1 alpha-2 国家/地区代码；
- 同一种订阅正文经不同入口格式解析后得到一致结果；
- 多订阅去重后仍能利用全部 alias 推导地区；
- 地区元数据随 generation 固化并通过 typed IPC 输出；
- WebUI 只消费后端字段，不维护第二套名称识别规则；
- 当前节点卡片严格展示 `is_active`/`active_node_id` 指向的终端节点；
- 延迟展示与 D13/D14 Rust benchmark 结果一致；
- 本地旗帜按需加载，不依赖网络或系统 Emoji 字体；
- 保持两列节点布局和虚拟滚动，节点数增加时不产生明显卡顿。

### 3.2 非目标

- 不验证代理的真实公网出口国家；
- 不接入公网 GeoIP、IPInfo 或第三方定位 API；
- 不向 sing-box 配置写入自定义地区字段；
- 不从 `server`、SNI、端口或证书猜测地区；
- 不把地区参与节点去重、测速、fair pool 或自动选优；
- 不按地区修改路由策略；
- 不重新引入此前已移除的搜索栏、协议下拉筛选；
- 首版不加入地区筛选 chips；
- 不展示原型中的 IEPL、BGP 等线路标签，因为当前后端没有结构化线路类型；
- 不在节点页重复展示实时流量、今日流量或服务总开关；这些已有明确的概览/设置归属；
- 不为未发布的旧 IPC、旧 generation registry 或旧 WebUI 保留兼容层。

地区筛选只有在实际节点规模和用户反馈证明存在需求时再设计。当前节点页已有来源分组与虚拟列表，YAGNI 优先。

## 4. 核心决策

| 编号 | 决策 |
|---|---|
| R1 | 领域模型使用 `Territory`，面向用户称“国家/地区”；国家是主体，香港、台湾、澳门等拥有独立 alpha-2 代码且在代理订阅中通常单独标识的地区作为同级展示单元 |
| R2 | 字段命名为 `display_territory_code`，明确它是展示推断，不宣称真实出口位置 |
| R3 | Rust 是国家/地区识别唯一事实源；WebUI 禁止解析 `name` |
| R4 | 识别输入只使用订阅提供的节点显示名称及去重 aliases |
| R5 | 识别结果为 `Option<DisplayTerritoryCode>`；未知或冲突时返回 `None` |
| R6 | 标准身份/显示名称、节点识别别名、位置别名分层维护，不建立“万能国家词典” |
| R7 | 标准表由固定版本官方数据生成；识别 aliases 由 NetHop 基于真实订阅人工审核 |
| R8 | 识别优先级为国旗 Emoji、明确国家/地区名称、独立 ISO 代码、IATA 机场/都市区代码、城市名称 |
| R9 | 弱别名不得单独决定结果；冲突证据不得按“第一个匹配”静默获胜 |
| R10 | 国家/地区不进入节点 fingerprint，增加或修正规则不得改变稳定 node ID |
| R11 | generation registry 直接破坏性升级，sealed generation 内的识别结果不可漂移 |
| R12 | 首版不增加历史/保留代码状态，也不导入全球机场数据库 |
| R13 | 节点卡片高亮表示实际活动节点；manual requested 但尚未 active 时不得冒充已生效 |
| R14 | 延迟颜色由纯前端展示函数计算，阈值固定并有边界测试 |
| R15 | 旗帜使用本地 `country-flag-icons` 3:2 SVG，作为构建期资源，不使用 CDN 或 Emoji |
| R16 | 顶部详情不回退到列表第一个节点；active 无法解析时展示明确同步/降级状态 |

## 5. Rust 国家/地区模型

### 5.1 值类型

在 `nethop-subscription` 增加窄值类型：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DisplayTerritoryCode([u8; 2]);
```

构造约束：

- 固定两个 ASCII 大写字母；
- 必须属于受控支持集合；
- 序列化为 `"HK"`、`"JP"`、`"SG"`；
- 不提供任意字符串构造后绕过校验的路径。

不使用约 249 个成员的 enum；受控静态 registry 提供同样严格的校验，同时更适合生成和维护。首版不增加 `TerritorySource`、置信度分数或 `RegionStatus`。历史、删除、保留代码没有当前真实订阅需求，提前建模违反 YAGNI。

### 5.2 三层数据模型

标准身份、节点识别和位置识别必须拆开：

```rust
pub struct TerritoryRecord {
    /// ISO 3166-1 alpha-2；唯一身份及最终 IPC 输出。
    pub code: DisplayTerritoryCode,

    /// ISO 3166-1 alpha-3。
    pub alpha3: [u8; 3],

    /// CLDR canonical/common UI names，不是冗长法定全称。
    pub english_name: &'static str,
    pub chinese_name: &'static str,
}

pub struct TerritoryRecognition {
    pub territory_code: DisplayTerritoryCode,

    /// 不重复 canonical name，只保存额外识别名称。
    pub english_aliases: &'static [&'static str],
    pub chinese_aliases: &'static [&'static str],

    /// 只保存 UK 等常见非标准代码；alpha-2/alpha-3 自动参与识别。
    pub code_aliases: &'static [&'static str],
}

pub struct LocationRecord {
    pub territory_code: DisplayTerritoryCode,
    pub city_names: &'static [&'static str],
    pub airport_codes: &'static [&'static str],
    pub metropolitan_codes: &'static [&'static str],
}
```

职责边界：

```text
TerritoryRecord      -> 标准身份和规范显示名
TerritoryRecognition -> 供应商节点名称识别
LocationRecord       -> 城市、IATA 单机场和都市区代码的弱位置证据
```

`code` 和 `alpha3` 天然参与代码识别，不在 `code_aliases` 中重复。`UK -> GB` 之类的非标准但常见写法才进入 `code_aliases`。

典型记录：

```rust
TerritoryRecord {
    code: territory_code!("GB"),
    alpha3: *b"GBR",
    english_name: "United Kingdom",
    chinese_name: "英国",
}

TerritoryRecognition {
    territory_code: territory_code!("GB"),
    english_aliases: &["Great Britain", "Britain"],
    chinese_aliases: &["英國", "大不列颠"],
    code_aliases: &["UK"],
}

LocationRecord {
    territory_code: territory_code!("JP"),
    city_names: &["Tokyo", "东京", "東京"],
    airport_codes: &["NRT", "HND"],
    metropolitan_codes: &["TYO"],
}
```

官方 canonical name 可以是 `China` / `中国`；`中国大陆`若作为供应商节点名称出现，应属于 NetHop 的识别 alias，而不是擅自改写官方记录。面向节点 UI 时仍以 alpha-2 对应旗帜为主要输出，不必展示完整名称。

### 5.3 模块职责

新增纯模块：

```text
crates/nethop-subscription/src/territory.rs
```

公开窄接口：

```rust
pub fn infer_display_territory<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Option<DisplayTerritoryCode>;
```

职责只有：

1. 从一个节点的 canonical display name 和 aliases 收集地区证据；
2. 按固定优先级消解；
3. 冲突或无证据时返回 `None`。

该模块不得依赖下载、DNS、GeoIP、sing-box API 或 WebUI 资源。

### 5.4 为什么放在 `nethop-subscription`

- Clash YAML、sing-box JSON、Surfboard INI 和 URI 列表都会归一化为同一个 `ProxyNode`；
- 地区输入来自订阅名称，而不是运行时 core 状态；
- 去重阶段掌握同一节点的全部 aliases；
- 规则可以脱离 daemon、文件系统和网络做穷举单元测试；
- WebUI、CLI 和 Manager APK 将共享同一个后端结果。

不能把规则放在 `NodesView.vue`，否则未来 CLI、Manager 和 WebUI 会形成三套不一致实现。

## 6. 国家/地区识别算法

### 6.1 证据等级

从强到弱分五级：

| 等级 | 类型 | 示例 |
|---|---|---|
| E1 | Unicode 国旗 Emoji | `🇯🇵 东京` -> `JP` |
| E2 | canonical/common 国家/地区名称及人工审核别名 | `日本`、`Hong Kong`、`Singapore` |
| E3 | 独立大写 ISO alpha-2/alpha-3 或受控代码别名 | `JP-01`、`JPN`、`UK X5` |
| E4 | IATA 单机场或都市区代码 | `NRT`、`HND`、`TYO`、`LAX`、`HKG` |
| E5 | 受控城市名称 | `Tokyo`、`Frankfurt`、`洛杉矶` |

同一名称内优先使用更强证据；同级出现不同代码视为冲突。扫描全部 aliases 后：

- 全部有效证据指向同一代码：返回该代码；
- 强证据唯一、弱证据冲突：强证据获胜；
- 同级强证据冲突：返回 `None`；
- 只有歧义弱 token：返回 `None`；
- 没有证据：返回 `None`。

### 6.2 Emoji 解析

旗帜 Emoji 是两个 Regional Indicator Symbol 组成。只接受完整的两字符组合，并转换为 `A..Z`：

```text
🇭 + 🇰 -> HK
🇯 + 🇵 -> JP
```

转换后的代码仍必须进入受控支持集合。孤立 indicator、非国家旗帜和多旗帜冲突都不产生结果。

### 6.3 中文和英文名称

canonical name 来自固定版本 CLDR；额外识别 alias 由 NetHop 人工审核。中文名称可按确定性子串匹配，因为常见国家/地区名称本身边界明确：

```text
香港 -> HK
日本 -> JP
新加坡 -> SG
韩国 / 南韩 -> KR
台湾 -> TW
美国 -> US
加拿大 -> CA
法国 -> FR
德国 -> DE
英国 -> GB
越南 -> VN
俄罗斯 -> RU
乌克兰 -> UA
土耳其 -> TR
尼日利亚 -> NG
印度 -> IN
澳大利亚 / 澳洲 -> AU
```

英文 canonical/common name 和 alias 采用 ASCII 大小写不敏感的单词或短语边界匹配：

```text
Hong Kong, Japan, Singapore, South Korea,
United States, Canada, France, Germany, United Kingdom
```

禁止普通 `contains()` 扫描短英文片段。

### 6.4 标准和非标准代码 token

ISO alpha-2、alpha-3 和人工审核的非标准代码 alias 只在以下条件同时满足时识别：

- 原始文本中是大写；
- 左右边界是字符串边界或受控分隔符；
- 代码属于支持集合；
- 不位于更长的连续 ASCII 字母串中。

受控分隔符包括空格、`-`、`_`、`|`、`/`、`.`、括号和常见中日韩标点。

示例：

```text
JP-X5-1       -> JP
TW_01         -> TW
SG Premium    -> SG
RUSSIA        -> 不得因包含 US 而识别为 US
STATUS        -> 不得因包含 US 而识别为 US
SINGAPORE     -> 不得因包含 IN 而识别为 IN
```

`JP/JPN`、`US/USA`、`GB/GBR` 等从标准表自动参与识别。`UK` 不是 ISO alpha-2 正式代码，但供应商普遍使用，只能作为受控 alias 映射到 `GB`，不得让输出出现 `UK`。

### 6.5 IATA 机场、都市区和城市标识

机场和都市区代码是代理节点命名中大众公认、辨识度高的标识，应当支持；但它们属于 `LocationRecord`，不是国家/地区标准表的一部分。需要区分：

```text
NRT / HND / KIX  -> IATA 单机场代码
TYO / OSA        -> IATA 城市/都市区代码
Tokyo / 东京     -> 城市名称
```

首版只人工收录常见、明确、已有真实样本或显著 UI 价值的记录：

```text
东京 / Tokyo / NRT / HND / TYO -> JP
大阪 / Osaka / KIX / ITM / OSA -> JP
新加坡 / Singapore / SIN       -> SG
首尔 / Seoul / ICN / GMP / SEL -> KR
台北 / Taipei / TPE / TSA      -> TW
洛杉矶 / Los Angeles / LAX     -> US
圣何塞 / San Jose / SJC        -> US
纽约 / New York / JFK/EWR/LGA/NYC -> US
伦敦 / London / LHR/LGW/LCY/LON   -> GB
法兰克福 / Frankfurt / FRA     -> DE
悉尼 / Sydney / SYD             -> AU
香港 / Hong Kong / HKG         -> HK
```

以下短码不得单独映射：

- `LA`：可能表示 Los Angeles，也可能是普通线路分级或其他缩写；
- `EU`：区域集合，不是单一国家/地区；
- `HKT`：是普吉国际机场代码，不能因看起来像 `HK` 就映射香港；仅在正式加入普吉记录时映射 `TH`；
- `IN`、`NO`、`IS`、`IT` 等易与英文单词混淆的代码，在非大写独立 token 场景不得识别。

`美国LA-优化` 因“美国”这个 E2 强证据识别为 `US`，不是因为 `LA`。独立的 `HKT-A` 按 IATA 代码识别为 `TH`；`香港HKT-A` 因“香港”识别为 `HK`，其中 `HKT` 是指向 `TH` 的较弱位置证据，不能推翻明确国家/地区名称，但必须记录可测试的弱证据冲突。

IATA 完整数据库存在授权和体积问题。NetHop 不抓取或整体复制全球机场库，只维护经真实订阅验证的精简表。每条机场/都市区代码必须有来源说明、独立 token 测试和相近字符串误判测试。

### 6.6 多订阅去重与 aliases

同一 endpoint/fingerprint 可能在多个订阅中使用不同名称。地区识别应在 dedupe 完成、aliases 合并后执行：

```text
Primary alias: Fast-B2-1
Backup alias:  Japan-Tokyo-01
Result: JP
```

如果 aliases 给出冲突强证据：

```text
Primary alias: 日本-01
Backup alias:  美国-01
Result: None
```

不得按订阅顺序选择第一个国家。这能避免同一节点因供应商命名错误显示确定但错误的旗帜。

### 6.7 与稳定身份的隔离

`display_territory_code` 是派生展示元数据：

- 不进入 `canonical_node_bytes()`；
- 不进入 fingerprint；
- 不改变 `StableNodeId`；
- 不参与 dedupe；
- 不参与 fair candidate pool；
- 不参与 latency tolerance 或 selector 决策。

维护映射表后，节点身份和用户 manual intent 必须保持稳定。

## 7. 标准数据、识别词典与来源维护

### 7.1 官方数据只生成标准表

`TerritoryRecord` 不应手工逐项抄写。构建仓库内的固定快照生成链路：

```text
固定版本 UN M49/ISO 代码数据
  -> alpha-2 / alpha-3 identity

固定版本 Unicode CLDR en + zh-Hans
  -> English/Chinese canonical common display names

生成器
  -> generated/territory_registry.rs
  -> 唯一性、格式、数量、交叉引用测试
```

来源约束：

- ISO 官方确认 alpha-2 是通用推荐代码、alpha-3 更接近名称，并允许免费使用 ISO 3166 代码；
- ISO Online Browsing Platform 用于人工核对当前分配，不能把收费 Country Codes Collection 当成项目可自由再分发的数据包；
- UN Statistics Division M49 官方表同时列出 country/area、ISO alpha-2 和 alpha-3，适合固定快照和交叉校验；
- Unicode CLDR 提供适合 UI 的 territory common names，而不是要求使用冗长法定全称；
- CLDR JSON/XML 固定版本必须记录 `Unicode-3.0` 许可证；
- CLDR 的洲、区域集合和 territory containment，如 `EU`、`Asia`、`North America`，不得生成 `DisplayTerritoryCode` 结果。

官方入口：

| 数据 | 官方入口 | 使用边界 |
|---|---|---|
| ISO 3166 说明与当前代码查询 | `https://www.iso.org/iso-3166-country-codes.html`、`https://www.iso.org/obp/` | 核对 alpha-2/alpha-3 身份和分配状态；不镜像收费 Country Codes Collection |
| UN M49 country/area 表 | `https://unstats.un.org/unsd/methodology/m49/` | 获取/交叉校验 alpha-2、alpha-3 和 M49 country/area 行；排除 region/sub-region 聚合行 |
| Unicode CLDR | `https://cldr.unicode.org/`、`https://github.com/unicode-org/cldr` | 获取固定 release 的 `en`、`zh-Hans` territory canonical/common names |
| CLDR JSON 分发与许可证 | `https://github.com/unicode-org/cldr-json` | 若生成器使用 JSON 分发，固定 release/tag 并随产物保留 `Unicode-3.0` |

`source-versions.json` 必须记录抓取日期、release/tag、原始文件相对路径、SHA-256 和许可证标识。生成器不得在普通 `cargo build` 或 Android 模块构建期间联网；官方快照更新是显式、可审核的维护操作。

建议仓库布局：

```text
data/territories/PROVENANCE.md
data/territories/source-versions.json
data/territories/recognition.toml
data/territories/locations.toml
scripts/generate-territory-registry.*
crates/nethop-subscription/src/generated/territory_registry.rs
licenses/Unicode-3.0.txt
```

生成器只在开发/构建校验阶段运行；Android runtime 不解析 CLDR、XML、CSV 或 TOML。

### 7.2 人工维护节点识别词典

ISO 和 CLDR 不直接成为节点识别词典。`TerritoryRecognition` 只收录经真实订阅验证、人工审核的额外别名，例如：

```text
UK   -> GB
南韩 -> KR
美利坚 -> US
```

canonical name、alpha-2、alpha-3 自动参与识别，不能在人工表重复。CLDR 中的 alternative/variant 名称也必须经过同样审核，禁止整库 `contains()`。

`LocationRecord` 同样人工维护，明确区分 `city_names`、`airport_codes` 和 `metropolitan_codes`。不加入供应商私有单字母线路编码。

运行时实现使用静态 slice 和线性扫描。识别表规模小、每个名称上限 128 bytes，无需正则引擎、Trie、数据库、运行时配置或 lazy global。

### 7.3 首版覆盖

首版至少覆盖三份真实样本中的国家/地区及常见亚洲、北美、欧洲、大洋洲节点：

```text
CN, HK, TW, MO,
JP, SG, KR, US, CA, GB, FR, DE, TH,
IN, ID, VN, RU, RO, UA, TR, NG, AU, NL, CH, SE
```

这里以国家为主体；`HK/TW/MO` 因具有独立 alpha-2 代码且在代理订阅中通常独立命名，与国家记录使用同一种展示契约。`EU`、`Asia`、`North America`、`华东`、`California` 等洲、区域集合或行政区划不得作为首版结果。

新增地区必须同时提供：

1. 正向名称样本；
2. alpha-2/alpha-3/code alias token 边界样本；
3. 至少一个相近字符串误判样本；
4. 对应 WebUI 本地 SVG；
5. 许可证/来源清单更新。

### 7.4 维护原则

- 只加入能够确定指向单一地区的别名；
- 优先加入正式 IATA 三字码、都市区码和完整城市名，不加入供应商私有单字母编码；
- 对真实订阅新增 fixture，不能只添加人工构造用例；
- 删除或修正规则属于展示变化，不需要迁移节点 ID；
- 不允许用户在 TOML 配置该表。首版没有明确的用户覆写需求。
- 不增加 `RegionStatus` 或历史/保留代码兼容层；出现真实需求后再扩展。

## 8. 后端数据流与破坏性升级

### 8.1 数据流

```text
Clash / sing-box / Surfboard / URI subscription
  -> format parser preserves display_name
  -> semantic validation -> ProxyNode
  -> cross-source dedupe + aliases
  -> infer_display_territory(names)
  -> StableConversion node metadata
  -> GenerationNodeRecord.display_territory_code
  -> sealed generation registry
  -> nethopd NodeListItem.display_territory_code
  -> typed IPC node.list
  -> WebUI NodeDto.displayTerritoryCode
  -> local flag asset + display only
```

地区推断每次订阅 conversion 只执行一次，不进入流量、测速或 reconcile 热路径。

### 8.2 `nethop-subscription`

`DedupedNode` 增加派生字段或只读 accessor：

```rust
pub display_territory_code: Option<DisplayTerritoryCode>
```

计算时机必须在全部 sources 完成 dedupe 和 aliases 合并之后。若当前 pipeline 分阶段构造 `StableConversion`，就在冻结 stable conversion 前计算一次。

所有格式共享同一函数。禁止在 `clash_yaml.rs`、`singbox_json.rs`、`surfboard.rs` 和 `uri.rs` 分别复制映射逻辑。

### 8.3 generation registry

直接把 registry schema 从当前 `nethop-generation-nodes-v2` 升为新版本：

```rust
struct GenerationNodeRecord {
    // existing fields...
    #[serde(skip_serializing_if = "Option::is_none")]
    display_territory_code: Option<DisplayTerritoryCode>,
}
```

开发期不读取旧 registry。升级模块后必须由当前订阅/LKG 重新生成 generation；失败时走既有事务回滚，不发布半成品 registry。

国家/地区字段纳入 registry digest，以保证 sealed generation 的 UI 元数据和 node list 一致，但不得纳入节点 fingerprint。

### 8.4 nethopd 节点快照

`NodeListItem` 增加：

```rust
display_territory_code: Option<DisplayTerritoryCode>
```

`join_node_snapshot()` 只能从当前 generation registry 复制该字段，不得再次解析名称。这样：

- 同一 generation 内所有调用结果稳定；
- 规则升级只在新 generation 发布后生效；
- node list、active node 和事件快照不会出现不同国家/地区结果。

### 8.5 IPC 与协议版本

节点 JSON 示例：

```json
{
  "id": "nh1s-0123456789abcdef",
  "name": "日本JP-HY2",
  "protocol": "hysteria2",
  "display_territory_code": "JP",
  "source_ids": ["src_0123456789abcdef0123456789abcdef"],
  "latency_ms": 89,
  "alive": true,
  "is_requested": false,
  "is_active": true
}
```

未知国家/地区省略 `display_territory_code`，不发送空字符串、`ZZ`、`UNKNOWN` 或虚构旗帜。

由于当前 Rust DTO 和 WebUI parser 都是严格字段集合，本次直接升级控制协议版本并同步更新所有消费者、golden fixtures 和 hello 协商。不得用“双写旧字段 + 新字段”保留兼容。

`nethopctl node list --json` 原样输出新字段；人类可读输出首版无需增加旗帜或地区列，避免窄终端布局退化。

### 8.6 事件一致性

以下事件触发后，WebUI 应重新使用同一份 `node.list` 快照：

- generation 发布；
- node selection 变化；
- active terminal 变化；
- benchmark 完成；
- subscription active set 变化。

事件 payload 不复制完整节点元数据，避免多个 DTO 漂移；保留“事件通知失效、query 获取事实”的现有模型。

## 9. WebUI 设计

### 9.1 页面结构

节点页由四个稳定区域组成：

```text
页面标题 + 全部测速/更多按钮
实际活动节点摘要
自动优选控制
按来源分组的两列虚拟节点列表
```

不复制原型的手机外框、状态栏、主题按钮、订阅更新按钮、搜索栏、地区 chips、底部连接开关和流量统计。

### 9.2 实际活动节点摘要

顶部摘要只使用 `selection.activeNodeId` 查找 `nodesById`：

```text
[旗帜] 日本JP-HY2                   89 ms
       Hysteria2 · Primary
       自动优选
```

可展示字段：

- `displayTerritoryCode` 对应的旗帜，未知时使用中性 globe 图标；
- 节点名称；
- 协议的显示名称；
- 来源名称：由现有 subscription snapshot 的 `source_id -> name` 映射恢复；
- 当前延迟及颜色；
- `自动优选` 或 `手动选择` 模式。

多来源去重节点：

- 空间足够时展示第一个来源名和 `+N`；
- 点击 `+N` 或更多操作才展开全部来源；
- 不复制订阅 URL。

异常状态：

| 后端状态 | 摘要表现 |
|---|---|
| active node 已解析 | 展示真实节点详情 |
| active ID 存在但节点快照暂未同步 | `正在同步活动节点`，不显示任意节点 |
| degraded reason 存在 | 展示稳定诊断对应文案和重试入口 |
| 服务停止/核心未运行 | `代理未运行`，不显示陈旧节点为当前节点 |
| direct/block terminal | 展示对应非代理状态，不伪装为订阅节点 |

禁止退回 `allNodes[0]`。

### 9.3 节点卡片

保持一行两列、左右/上下组合：

```text
┌──────────────────┐
│ 节点名称       状态 │
│ protocol           │
│ 🇯🇵          89 ms │
└──────────────────┘
```

- 左上：单行节点名称，超长省略；
- 左中：协议；
- 左下：20x14 或相近 3:2 旗帜；未知地区不保留空白旗位，可显示中性 globe；
- 右下：tabular number 延迟；
- active 节点：使用明确但克制的背景与边框变化；
- requested 但未 active：只显示 pending 状态，不使用 active 背景；
- 点击非 requested 节点执行现有 manual select；
- 卡片尺寸必须固定，测速状态和不同位数延迟不得造成布局跳动。

卡片不显示服务器地址、端口、凭据、内部 tag 或完整 source IDs。

### 9.4 延迟分档

采用原型阈值并冻结边界：

```ts
type LatencyTier = "good" | "medium" | "poor" | "unknown";

function latencyTier(latencyMs?: number): LatencyTier {
  if (latencyMs === undefined) return "unknown";
  if (latencyMs < 120) return "good";
  if (latencyMs < 250) return "medium";
  return "poor";
}
```

显示语义：

| 状态 | 文本 | 颜色 |
|---|---|---|
| `1..119ms` | 数字 + `ms` | 绿色 |
| `120..249ms` | 数字 + `ms` | 黄色 |
| `>=250ms` | 数字 + `ms` | 红色 |
| 未测试 | `--` | 次要文本色 |
| 本轮测速中 | `···` 或轻量 spinner | 次要文本色 |
| timeout/unavailable/protocol error | `超时`/`不可用` | 次要文本色，不冒充高延迟 |

颜色只作为增强，文本始终保留数字或终态。虽然项目不以无障碍为范围，但不能只靠颜色表达测速结果。

### 9.5 测速交互

右上角闪电按钮继续调用 D13/D14 `NodeTestAll`：

1. 接收 ACK 后进入 running；
2. 当前 auto pool 节点显示测速中，非候选节点保持原状态；
3. 终态报告到达后按稳定 ID更新延迟；
4. auto 模式可能切换 active node，摘要和 active 卡片随 selection/event 更新；
5. manual 模式只更新延迟，不改变 active node；
6. 4.9 秒 deadline 后所有候选必须处于明确终态。

WebUI 不并发发起逐节点测试，不自行挑选最低延迟节点，也不根据颜色触发切换。

### 9.6 来源分组与地区

现有按来源分组保留。地区旗帜是节点属性，不替代来源：

- 单来源节点显示在该来源分组；
- 多来源去重节点沿用当前确定性主分组规则；
- 卡片可在详情中展示全部来源；
- 不按地区重排节点，避免破坏来源理解和稳定滚动位置。

### 9.7 旗帜资源

选用 `country-flag-icons` 的 3:2 SVG：

- 仅作为构建期/静态资源来源；
- 不使用 React 组件入口；
- 不引入 CDN；
- 不使用 Unicode Emoji 作为主渲染方案；
- 不把原型中的手绘旗帜 SVG 复制进 Vue 模板；
- 资源文件使用小写或大写 ISO code 的单一规范命名；
- 构建产物必须包含许可证和来源记录。

首版只打包 Rust 可能输出的代码对应资源。前端维护 `territoryCode -> asset URL` 的纯展示映射，并以契约测试保证 Rust 支持集合全部有资源。它不是第二套国家/地区识别规则。

未知代码在 DTO parser 阶段拒绝；缺失代码正常渲染中性占位。图片加载失败不得破坏卡片布局。

### 9.8 组件边界

建议拆分：

```text
views/NodesView.vue
components/nodes/ActiveNodeSummary.vue
components/nodes/NodeCard.vue
components/nodes/TerritoryFlag.vue
model/node-territory.ts
model/node-latency.ts
```

职责：

- `NodesView`：加载、操作、事件响应、虚拟列表；
- `ActiveNodeSummary`：当前活动节点投影；
- `NodeCard`：固定格式节点呈现与选择命令；
- `TerritoryFlag`：严格 code 到本地资源；
- `node-latency`：纯分档与文案；
- `node-territory`：只包含允许代码和显示名称/资产，不解析节点名称。

不为三个小组件引入新的状态管理库或通用 design-system wrapper。

## 10. 性能与资源预算

地区识别不进入热路径，但仍设置门禁：

| 指标 | 门槛 |
|---|---:|
| 2,000 个名称、每个 128 bytes 的地区推断 p95 | `<= 5ms`（host release benchmark） |
| 单节点推断额外堆分配 | `0` 或仅最终可选值，无临时正则/String 分配 |
| node.list 单节点新增 JSON | `<= 32 bytes`（存在字段时） |
| 首屏新增同步 JS | `<= 8 KiB gzip` |
| 旗帜总资源 ZIP 增量 | 实测记录，目标 `<= 100 KiB` |
| 节点列表滚动 | 保留既有虚拟列表，无整表 DOM |
| 地区规则对 benchmark 5 秒 SLA | 零影响 |

如果首版 flags 超过 ZIP 预算，优先减少未支持地区的资源，不改用远程 CDN。

前端只渲染可视虚拟行中的旗帜；不得用 base64 把所有 SVG 内联到主 JS chunk。

## 11. 安全与隐私

- 不发送 server IP、域名或节点名称到第三方定位服务；
- 不新增网络权限或远程资源；
- 不记录订阅 URL、密码、UUID、token 或完整 outbound；
- 节点名称继续以文本插值渲染，禁止 `v-html`；
- `display_territory_code` 在 Rust 和 TypeScript 两端均做严格 allowlist 校验；
- 旗帜路径不得由未经校验的字符串直接拼接为任意文件路径；
- 地区解析诊断只记录规则类别和 node stable ID，不记录敏感 endpoint；
- 地区字段不得进入安全策略、路由 admission 或 capability 判断。

## 12. 错误与降级语义

| 情况 | 行为 |
|---|---|
| 无国家/地区证据 | `display_territory_code` 缺失，UI 使用中性图标或无旗帜 |
| 名称包含冲突国家 | 返回 `None`，不得任取一个 |
| aliases 互相冲突 | 返回 `None` |
| 前端缺少对应 SVG | 中性占位并记录本地开发诊断，页面继续工作 |
| active node 无法解析 | 显示同步/降级状态，不回退第一个节点 |
| 延迟未知 | 显示 `--`，不按 0ms 处理 |
| 测速超时 | 显示 `超时`，不归入红色高延迟数字 |
| generation 切换 | 丢弃旧快照和旧 benchmark 结果，加载新 registry |
| 新 registry 构建失败 | 保留当前 generation，地区 UI 不影响代理可用性 |

地区识别失败永远不是订阅解析失败、generation admission 失败或代理启动失败的理由。

## 13. TDD 与新旧对比测试

### 13.1 重构前基线

在修改生产代码前冻结：

- Clash YAML、sing-box JSON、Surfboard、URI parser golden；
- 多订阅 dedupe、aliases、source attribution；
- stable node ID 与 fingerprint；
- generation registry round-trip/digest；
- `node.list`、selection、benchmark protocol golden；
- auto/manual 选择行为；
- NodesView 两列虚拟列表、测试全部、更多操作和来源分组；
- WebUI production bundle 大小与模块 ZIP 大小。

### 13.2 Rust RED 用例

至少覆盖：

```text
🇭🇰 香港 01                 -> HK
日本JP-HY2                  -> JP
新加坡SG-A-Gemini           -> SG
US-D1-1                     -> US
TW-IPv6-P2-1                -> TW
JP-Dedicated-P1-1           -> JP
SG-X5-1                     -> SG
韩国KR-HY2                  -> KR
印度-优化                   -> IN
美国LA-优化-GPT             -> US
加拿大-优化                 -> CA
法国FR-A                    -> FR
德国-优化                   -> DE
英国-优化                   -> GB
越南VN-A                    -> VN
俄罗斯RU-A                  -> RU
乌克兰UA-A                  -> UA
土耳其TR-A                  -> TR
尼日利亚NG-A                -> NG
Fast-B2-1                   -> None
Balancer-B1-1               -> None
STATUS                      -> None
RUSSIA                      -> RU only by full name rule, never US
SINGAPORE                   -> SG only by full name rule, never IN
LA-Premium                  -> None
HKT-A                       -> TH
香港HKT-A                  -> HK with weak-evidence conflict diagnostic
日本-US-冲突                -> JP according to stronger full-name evidence
日本-美国-冲突              -> None
```

还必须覆盖：

- Emoji 非法/孤立序列；
- ISO token 大小写和全部分隔符边界；
- 全角/中日韩标点边界；
- aliases 一致、补充、冲突；
- 超长名称上限；
- 所有支持 code 的序列化往返；
- 地区规则变化不改变 fingerprint/node ID。

### 13.3 真实 fixture

对三份本地 YAML 建立去敏后的最小 fixture 或仅保留节点名称/非敏感结构，禁止把凭据复制进测试快照。

验收：

- GLaDOS fixture：明确 `US/TW/JP/SG` 节点正确识别，`Fast/Balancer` 保持未知；
- 魔戒 fixture：42 个地区节点按名称识别；两个信息节点的地区必须为未知；
- fsllist fixture：51 个节点全部按开头独立 alpha-2 token 识别，覆盖 `GB/US/IN/JP/RO/HK/ID/NL/SG/FR`，代码和城市名称不得产生冲突；
- 不因地区未知而过滤任意节点；
- 原 accepted/rejected/duplicate 数量保持不变。

### 13.4 后端契约测试

- `StableConversion -> GenerationNodeRecord` 正确传播字段；
- registry 新 schema round-trip 和 digest 稳定；
- `join_node_snapshot` 不重新推断名称；
- `node.list` 对已知/未知地区使用字段/省略字段；
- active、requested、source IDs、latency、alive 既有字段不变；
- benchmark 完成只更新观察值，不修改地区；
- generation superseded 后旧结果不污染新节点；
- CLI JSON 与 protocol golden 同步；
- 旧协议版本在 hello 协商时被明确拒绝。

### 13.5 WebUI 测试

- DTO 接受 allowlist 内 code，拒绝小写、长度错误和未知 code；
- 每个后端支持 code 都存在本地 SVG；
- `TerritoryFlag` 缺失值和资源错误降级；
- 延迟边界 `119/120/249/250`；
- 未测试、测速中、超时、不可用状态；
- active 卡片背景与 requested pending 区分；
- current summary 不回退列表第一项；
- auto benchmark 切换后摘要更新；
- manual benchmark 后活动节点不变；
- 多来源 `Primary +N`；
- 超长名称、三位/四位延迟不改变两列布局；
- virtual row 稳定，滚动后节点状态不串行。

### 13.6 前后对比门禁

| 既有能力 | 重构后要求 |
|---|---|
| 多格式订阅解析 | accepted/rejected 行为不退化 |
| 多订阅合并去重 | node ID、aliases、source attribution 不退化 |
| fair auto pool | 候选集合与顺序不因地区变化 |
| 自动/手动选择 | intent 与 active terminal 语义不变 |
| Rust 全部测速 | 5 秒 SLA 与终态不退化 |
| 节点排除/导出 | 命令仍可用 |
| 两列虚拟列表 | DOM 和滚动性能不退化 |
| 模块离线 WebUI | 无 CDN、无新增网络依赖 |

## 14. 实施阶段

### 阶段 A：冻结基线

- 固化现有 parser、dedupe、generation、selection、benchmark 和 WebUI 测试；
- 记录 bundle/ZIP 基线；
- 从真实 YAML 生成去敏地区 fixture。

### 阶段 B：官方标准表与 Rust 纯识别引擎

- 先写 RED 测试；
- 固定 UN M49/ISO 与 CLDR 来源版本，先生成并校验 `TerritoryRecord`；
- 实现 `DisplayTerritoryCode`、`TerritoryRecognition`、`LocationRecord`、边界 tokenizer、Emoji 解析和冲突消解；
- 完成误判、属性和性能测试；
- 不接触 daemon/UI。

### 阶段 C：conversion 与 generation

- dedupe 后基于 aliases 计算一次；
- 升级 generation registry schema；
- 证明 fingerprint、node ID、candidate pool 不变；
- 更新 generation golden。

### 阶段 D：typed IPC 消费链

- 扩展 `NodeListItem`；
- 升级 control protocol；
- 更新 nethopctl、golden 和严格 parser；
- 验证事件/快照一致性。

### 阶段 E：旗帜资产与纯展示组件

- 固定 `country-flag-icons` 版本和许可证；
- 只提取受支持 SVG；
- 实现 `TerritoryFlag` 和 asset coverage test；
- 实现延迟分档纯函数及边界测试。

### 阶段 F：节点页重构

- 增加 `ActiveNodeSummary`；
- 重构 `NodeCard` 的旗帜、延迟和 active 状态；
- 保留来源分组、两列虚拟列表和现有操作；
- 接入 benchmark running/terminal 状态；
- 删除被新组件替代的重复 CSS。

### 阶段 G：回归与发布门禁

- Rust workspace、WebUI unit/component/e2e 全量测试；
- production bundle/metafile/license/SBOM；
- 模块 ZIP 构建与 checksum 安装验证；
- 记录国家/地区识别引擎、bundle、ZIP 和列表滚动证据；
- 真机验证当前节点、全部测速、auto 切换和 manual 保持行为。

每个阶段只在当前测试转绿后进入下一阶段，禁止先改全链路再补测试。

## 15. 不采纳方案

### 15.1 直接在 Vue 中正则解析名称

会让 WebUI、CLI、Manager APK 得到不同结果，也无法利用 dedupe aliases。不采纳。

### 15.2 用服务器 IP GeoIP

入口不等于出口，且增加数据库、更新、体积和误导。不采纳。

### 15.3 联网查询出口位置

泄露节点信息、增加延迟和外部依赖，也需要逐节点真实代理流量验证。不采纳。

### 15.4 用 Emoji 国旗

Android、桌面浏览器和 WebView 字体表现不一致，部分系统可能缺字。不采纳。

### 15.5 把地区加入节点 fingerprint

地区是可变的推断元数据，规则更新会导致 node ID 漂移并破坏 manual intent。不采纳。

### 15.6 为每个供应商配置私有映射

当前没有必须覆盖的供应商私有编码，配置模型和维护成本高。不采纳。

### 15.7 复制原型全部功能

搜索、地区 chips、底部开关、主题按钮和流量信息并非本轮需求，且部分与现有页面职责重复。不采纳。

## 16. 完成定义

满足以下全部条件才算完成：

1. 三份真实样本中的明确国家/地区节点得到预期 ISO 代码，其中 fsllist 51/51 节点可识别；
2. 无法确定和冲突名称保持未知，没有 server/IP 推断；
3. 国家/地区规则是 Rust 唯一事实源，WebUI 不解析名称；
4. 地区变化不改变 fingerprint、稳定 node ID、dedupe 或 candidate pool；
5. generation、IPC、CLI 和 WebUI 严格契约同步升级；
6. 顶部详情只显示真实 active terminal，不回退列表第一项；
7. 节点卡片显示本地旗帜和绿/黄/红延迟分档；
8. active 与 requested pending 状态视觉语义不同；
9. auto/manual 测速和切换行为保持 D13/D14 契约；
10. 两列虚拟列表、来源分组、排除、导出和更多操作不退化；
11. 无 CDN、无 GeoIP 服务、无敏感数据外发；
12. flags 许可证、SBOM、bundle 和 ZIP 体积证据完整；
13. Rust、WebUI、模块构建及真机关键流程全部通过。

## 17. 最终设计结论

NetHop 可以维护节点地区映射表，但该表的正确定位是“从供应商节点名称推导 UI 展示代码”，不是地理事实数据库。实现必须保持保守：明确则展示，不明确则留空。

前端只负责把后端提供的受控 ISO 代码渲染成本地 SVG，并用真实 benchmark 数据呈现延迟。后端在订阅归一化和跨来源去重后一次性计算地区，将结果随 generation 固化，再通过严格 IPC 交给所有消费者。这样既能吸收 `proxy_node_list.html` 中最有价值的视觉设计，也不会引入错误的出口定位、重复解析规则或新的运行时依赖。
