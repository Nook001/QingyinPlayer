# 代码质量评估（临时）

- 日期：2026-09-19
- 范围：当前 `main` 工作区实现（约 4.3k 行 Rust + 1.3k 行 QML，不含测试行数重复统计）
- 性质：**一次性快照**，不是路线图，也不替代 [development-status.md](development-status.md)
- 评分：软件工程质量，10 分制。8–10 结构清楚、可长期演进；5–7 能工作但有明显债；4 及以下会拖住下一阶段功能

评估维度：复杂度、复用、简洁、注释（冗余/不足），并单独看依赖方向、语言/框架用法和性能热点。

---

## 1. 架构是否合理

### 1.1 分层意图 vs 实际

设计文档里的图是 `QML → Bridge → Core → {Library, Player, Storage}`。  
实现里是 `QML → AppBridge → 各 crate 直接调用`。

`core::AppCore` 与 `PlaybackQueue` 没有任何调用方。真正的会话状态（曲库三份拷贝、播放列表、搜索世代、watcher、设置）全部堆在 `AppBridge`（`crates/ui_bridge/src/lib.rs`，约 1270 行）。`core` crate 目前几乎只是 `Settings` 的外壳，却仍依赖 `library` 和 `player`，把 GStreamer / notify / SQLite 间接绑进“核心”图。

这不是循环依赖，但是 **依赖方向名不副实**：名叫 core 的包不是核心，名叫 bridge 的包是核心。

### 1.2 不合理或偏斜的依赖

| 问题 | 说明 |
| --- | --- |
| `ui_bridge` 依赖本仓库全部 crate | 桥接层同时是应用服务、列表模型、封面流水线。后续 MPRIS/队列会继续膨胀 |
| `core` → `library` / `player` | 仅为未使用的 `AppCore` 字段；`Settings` 并不需要这些依赖 |
| `ui_bridge` → `chinese` | 只为 `compare_keys`；排序比较应留在 library/core，桥只调“排这一列” |
| `metadata` → `chinese` | `TrackMetadata` 带 `title_key` 等派生字段，领域 DTO 绑死校对算法 |
| `player` → `zbus` | Cargo 依赖存在，源码零引用。MPRIS 未实现却拉进编译图 |
| `storage` → `metadata` | 合理（存的就是 TrackMetadata）；但 `list_tracks` 再 `refresh_sort_keys`，排序键不落库，每次加载全表重算拼音 |

循环依赖：**没有**。crate 边界本身是单向的。问题是 **边界切错了位置**，而不是箭头画反。

### 1.3 其它设计裂缝

1. **三份曲目向量**（`library_tracks` / `tracks` / `playback_tracks`）加封面 URL 平行数组，靠下标对齐。排序、搜索、播放各自一份，语义清楚，但没有类型区分（例如 `LibrarySnapshot` vs `PlaybackSession`），全是 `Vec<TrackMetadata>`。
2. **播放队列在错误的层。** `PlaybackQueue` 是 `VecDeque` 骨架；实际队列是 `AppBridge.playback_tracks`。2B.6 若继续在 bridge 里长队列模型，core 会更空。
3. **SQLite 连接无共享策略。** 扫描线程、搜索线程、监听线程、启动恢复各自 `Database::open`。未开 WAL、未设 `busy_timeout`。监听与全量扫描并发时，可能 SQLITE_BUSY 或长时间锁。
4. **`MusicLibrary` 身兼扫描器、增量更新器和内存缓存。** 监听 worker 里 `MusicLibrary::default()` 只为调 `refresh_path`，每次成功还 `sync_tracks` 整表读回——worker 并不使用那份内存列表，纯浪费。
5. **聚合在 library、展示模型在 ui_bridge。** 方向合理。但 `aggregate_artists` 用 `Vec::find` 线性找组，并对每个（曲目×歌手）`clone` 整份 `TrackMetadata`。
6. **QML 主题是 `Main.qml` 上二十多个颜色属性。** 子组件靠 `property var theme` 传入 window。能工作，但没有 `QtObject` 单例或 `qmlRegisterSingletonInstance`，深色切换靠绑 `backend.dark_theme`。
7. **QML 仍从源码路径加载**（`CARGO_MANIFEST_DIR/../../qml/Main.qml`）。与“可安装应用”目标不一致，属已知债，不是模块方向错误。

架构总分：**5.5 / 10**。模块切分对中文/存储/播放是对的；应用层没有真正的核心，桥接层过重。

建议的收敛方向（本文件只记录，不在本次修改）：

- `core` 只保留设置与将来的队列/会话；去掉对 `library`/`player` 的依赖，或删掉空的 `AppCore`。
- `ui_bridge` 拆：`TrackListModel`、`PlaybackController`、`LibrarySession`（扫描+监听回调）、封面服务。
- `TrackMetadata` 与 `title_key` 分离：存储/扫描产出“标签”，校对键在装入 UI 快照时计算一次。
- player 的 `zbus` 等到做 MPRIS 再引入。

---

## 2. 分模块评分

分数为该维度 1–10，**综合** 不是四项平均，会加重“以后改它的成本”。

### 2.1 `chinese`（约 350 行）

| 维度 | 分 | 依据 |
| --- | --- | --- |
| 复杂度 | 8 | 搜索键直线循环；排序键有括号剥离 + 多音字 + 假名短路，仍然局部 |
| 复用 | 8 | `sort_key` / `compare_keys` / `search_key` 被 metadata、storage、library、ui_bridge 共用 |
| 简洁 | 7 | 括号处理从“删整段前缀”改成“只删括号字符”，逻辑对了，但 `Vec<char>` + `remove(0)` 偏手工 |
| 注释 | 8 | 公开 API 有 rustdoc，括号规则有例子；没有废话注释 |
| **综合** | **8.0** | |

不足：collator 失败时静默退回 `to_lowercase` 字节序，没有日志。`compare_labels` 每次现算键，调用方若在 `sort_by` 里用会重复转拼音（library 聚合已改为预计算，此处 API 仍容易误用）。

可改进：用 `VecDeque` 或直接 `chars().filter` 去掉 CJK 括号；`AUDIO` 式查表把多音字改 `phf` / `match` 已够用。

### 2.2 `metadata`（约 210 行）

| 维度 | 分 | 依据 |
| --- | --- | --- |
| 复杂度 | 8 | `read_track` / `read_cover` 结构对称、浅 |
| 复用 | 7 | DTO 全局复用；`read_cover` 与 `read_track` 各 parse 一遍同一文件 |
| 简洁 | 8 | `from_display` + `refresh_sort_keys` 路径清楚 |
| 注释 | 7 | 错误类型和公开函数有文档；字段本身无说明（三个 sort 与三个 key 易混） |
| **综合** | **7.5** | |

`read_track` 的测试只覆盖 fallback 文件名和排序键，**没有用真实音频测 Lofty 标签**（真实验证在开发状态里靠手工）。歌手只取 `Accessor::artist` 一条，多值标签可能被压成单个字符串，和 `track_artists` 的“多行歌手”模型不完全对齐。

### 2.3 `storage`（约 710 行，含测试）

| 维度 | 分 | 依据 |
| --- | --- | --- |
| 复杂度 | 7 | 迁移 v1–v3 可读；搜索 SQL 有字段优先级，可维护 |
| 复用 | 8 | `TrackRow` / `stored_track` 统一 list 与 search |
| 简洁 | 7 | upsert 事务完整；`list_tracks` 对每行再查 `track_artists`（N+1） |
| 注释 | 8 | 公开方法有 `# Errors`；schema 意图靠 SQL 自解释 |
| **综合** | **7.5** | |

缺口：无 `PRAGMA journal_mode=WAL`、无 `busy_timeout`、无 FTS5。`search_terms` 用 `instr` 全表扫。`ORDER BY title` 是 Unicode 码位，与界面拼音序无关——注释里写了，调用方必须再排一次。

### 2.4 `library`（扫描 + 监听 + 聚合，约 1020 行）

| 维度 | 分 | 依据 |
| --- | --- | --- |
| 复杂度 | 6 | `refresh_path` 分支（文件/目录/消失/临时/非音频）偏多但有测试 |
| 复用 | 7 | 扫描与监听共用 `refresh_path`；聚合与扫描挤在同一 crate 合理偏大 |
| 简洁 | 6 | `zip_tracks` 全量 clone；`push_grouped_track` 线性查找；worker 里无用的 `sync_tracks` |
| 注释 | 7 | 扫描/监听公开 API 文档好；聚合内部几乎无注释（可以接受） |
| **综合** | **6.5** | |

`watch.rs` 职责单一，debounce 实现清楚，是本仓库较好的一块。主要扣分在：监听批量每条路径都可能全表 reload；聚合算法 O(曲目×已有组) 且拷贝重。

### 2.5 `player`（约 330 行）

| 维度 | 分 | 依据 |
| --- | --- | --- |
| 复杂度 | 7 | 状态机小；Bus 线程 + Drop 生命周期清楚 |
| 复用 | 6 | API 适合被唯一会话使用；`zbus` 未用；播放列表不在本 crate |
| 简洁 | 7 | `playbin` 封装没有过度抽象 |
| 注释 | 7 | 公开方法文档齐全；Bus 轮询为何 250ms 未说明 |
| **综合** | **6.5** | |

**无单元测试**（GStreamer 集成确实难，但 `ensure_format_plugins`、状态枚举、错误格式化可测）。  
`set_gstreamer_state` 在调用线程同步等待最多 3 秒——而调用线程是 Qt 主线程。这是播放路径最大的工程问题，见第 4 节。

### 2.6 `core`（约 280 行）

| 维度 | 分 | 依据 |
| --- | --- | --- |
| 复杂度 | 8 | `Settings` 读写、clamp、未知列丢弃，测试覆盖好 |
| 复用 | 4 | 仅 Settings 被使用；`AppCore`/`PlaybackQueue` 死代码 |
| 简洁 | 6 | 设置部分干净；空类型造成“有核心层”的假象 |
| 注释 | 7 | Settings 文档充分；AppCore 无“尚未接线”说明 |
| **综合** | **5.5** | |

### 2.7 `ui_bridge`（约 1430 行）

| 维度 | 分 | 依据 |
| --- | --- | --- |
| 复杂度 | 4 | `AppBridge` 同时是 `QAbstractListModel`、扫描控制器、搜索、设置、播放、收藏、监听宿主 |
| 复用 | 5 | `DetailTrackModel` 与 `AppBridge::data` 角色映射重复；`format_duration` 两份；`open_artist`/`open_album` 几乎复制 |
| 简洁 | 4 | `qt_method!` 再转 `*_internal`，为的是 qmetaobject 限制，但方法列表过长；错误普遍 `String` |
| 注释 | 5 | 几乎没有模块级说明；公开 `register_qml_types` 一行；内部回调密度高、缺“为何 clone 整表” |
| **综合** | **4.5** | |

这是下一阶段（队列、MPRIS、进度细分）的主要风险点。继续往同一结构体加字段会很难测、很难拆线程边界。

`collections.rs` 相对干净（约 140 行），是从 lib.rs 拆出的正确方向，但 `CollectionModel`/`DetailTrackModel` 仍可与主列表共用一个 Track 角色模型。

### 2.8 QML（约 1520 行）

| 维度 | 分 | 依据 |
| --- | --- | --- |
| 复杂度 | 6 | `Main.qml` 主题 + 无边框窗口 + 导航偏满；子页大多是绑属性 |
| 复用 | 7 | `CollectionBrowser` 让 Artist/Album 各 21 行；`TrackTable` 曲库与详情共用 |
| 简洁 | 6 | 大量自定义 `Button` 背景复制（侧栏、表头、添加文件夹、播放条） |
| 注释 | 5 | 无文档注释；`pragma ComponentBehavior: Bound` 和 `required property` 用法正确，靠代码自解释 |
| **综合** | **6.5** | |

`TrackTable.qml` 里 `albumHeading` 曾出现重复 `onClicked`（一行为 `if (sortable)` 空体、一行真正排序）——属于应在 qmllint/人工审里抓住的噪音，建议修掉。

---

## 3. 语言与框架：可提升处

只列 **对当前代码有明确收益** 的，不写风格空话。

### 3.1 Rust

| 现状 | 更合适的写法 / 标准库能力 |
| --- | --- |
| `player` 声明 `zbus` 未用 | 删掉依赖，或 `#[cfg]` 到 MPRIS 特性 |
| `OnceLock<Option<Collator>>` | 保持即可；失败分支加 `tracing::warn` |
| `Vec::find` 做歌手分桶 | `HashMap<String, Vec<_>>`，再收集排序（万级曲库时差距明显） |
| `zip_tracks` `cloned()` 全表 | 聚合用索引 / `Arc<TrackMetadata>`，或只存 `usize` 指向 library 快照 |
| `refresh_path` 成功必 `sync_tracks` | 监听 worker 根本不读 `library.tracks`，应提供 `refresh_path` 的无同步版本 |
| `list_tracks` N+1 | 一次 `SELECT` + `LEFT JOIN track_artists` 在 Rust 里拼 Vec |
| `AppBridge` 里 `expect("player was initialized")` | `let Some(player) = ... else { return }`，与同文件其它分支一致 |
| 错误 `map_err(\|e\| e.to_string())` 进出线程 | 用已有 `thiserror` 类型，`queued_callback` 传枚举而不是 `String` |
| `DefaultHasher` 做封面内容哈希 | 对缓存文件名够用；若要稳定跨版本，换显式 `blake3`/`xxhash` 并写进注释 |
| `std::thread::spawn` 每次扫描/搜索 | 长期可用单工作队列 / `async` 运行时，避免并发多个全量扫描打 SQLite |
| `qt_method` 闭包 `path.clone()` 进 callback | 已必要；把 `ScanResult` 再缩小（不要把封面 PNG 字节带回主线程——当前只带回 URL，这点是对的） |

Rust 2024 / 1.92：已在用 `let` 链、`is_some_and`。不必为了新语法改写稳定代码。更值得做的是 **删死代码、收依赖、减 clone**，不是上宏。

### 3.2 QML

| 现状 | Qt Quick 更贴切的用法 |
| --- | --- |
| `Main.qml` 一堆 `readonly property color` | `QtObject` 单例 `Theme.qml`，或 `qmlRegisterSingletonInstance` 从 Rust 推主题 |
| 每个 Button 手写 `contentItem`/`background` | 一个 `QingyinButton.qml` / `Control` 附加样式；或 `qtquickcontrols2.conf` + 自定义 style |
| `property var theme` / `property var libraryModel` | 能的话改成具体类型（qmetaobject 对自定义 QObject 导出有限，可先保持 var，但主题用 QtObject） |
| 播放进度 `Timer` 250ms 拉属性 | GStreamer 侧用 bus/clock 回调推 `position`，QML 只绑属性；或 `NumberAnimation` 仅做平滑，seek 时校正 |
| `begin_reset_model` 用于排序 | `layoutAboutToBeChanged` / `layoutChanged`，或 `beginMoveRows`，避免列表滚回顶部、Image 全重载 |
| 搜索 180ms `Timer` | 合理；可再加 `running: text.length > 0` 避免空转 |
| 无边框窗口 + 自绘缩放 | 已有 `ResizeHandle`；可考虑 `Qt.Window` + 系统装饰作为设置项，减少 Main 里窗口几何代码 |

`pragma ComponentBehavior: Bound`、`required property`、信号 `trackActivated(int row)` 这一套是好的，应作为后续 QML 的默认模板。

### 3.3 Qt / qmetaobject

- **不要用 QSortFilterProxyModel 在 QML 里按字符串排中文**——拼音序必须留在 Rust，这一点当前是对的。
- `AppBridge` 既是 root context 对象又是 list model，QML 里 `libraryModel` 和 `playerBackend` 指向同一实例。拆成两个 `QObject` 更符合 Qt 习惯：`LibraryModel` + `PlayerController`，`Main.qml` 仍可一次创建、互相引用。
- `data()` 每次委托刷新都 `QString::from(track.title.clone())`。角色热路径可考虑缓存 `QString`，或接受现状（万级列表 + 委托回收通常可接受）。
- 封面：`Image { asynchronous: true }` 已做。模型重置仍会让可见委托重新设 `source`。

### 3.4 注释政策（现状判断）

- **够用：** 各 crate 公开函数的 rustdoc 和 `# Errors`（storage / player / library 扫描）。
- **不足：** `AppBridge` 字段含义（三份 tracks 的区别）、SQLite 并发假设、为何 Bus 要 poll。
- **冗余：** `PlaybackQueue::len` 一类一行包装无需再写散文；不要为“拆 bridge”提前写长设计注释。
- **错误注释：** `core` 和 README 旧句曾暗示 AppCore 已是运行时中枢——本次 README 已改掉。`player` 的 zbus 依赖会让读者以为 MPRIS 已接。

---

## 4. 性能热点（播放 / 解析 / 扫描）

按 **用户可感知程度** 排序。万级曲库是路线图验收口径。

### 4.1 播放（高）

1. **主线程同步等 GStreamer 状态（最多 3s）。** `Player::play`/`pause`/`load` 里 `playbin.state(Some(3s))`。卡在 Qt 线程会冻整窗（进度条、滚动、搜索）。应：`load/play` 放到工作线程，或 `set_state` 后靠 Bus 的 `AsyncDone`/`StateChanged` 再更新 QML。
2. **Bus 250ms 轮询 + UI 250ms 轮询 position。** 两条节奏叠加：额外线程空转，进度粒度 250ms。GStreamer 有 `bus.add_watch` / `connect`；position 可用 1s 级查询或在 Playing 时用时钟推算。
3. **`load` 每次 `Null` 再设 URI。** 切歌正确，但会拆 pipeline。可接受；若切歌间隙大，再考虑 `about-to-finish` 预滚下一首（那是队列阶段的事）。

### 4.2 解析与封面（中高，出在扫描/启动）

1. **同一文件 Lofty 打开两次**（`read_track` + `read_cover`）。扫描 300+ 首时磁盘和解析翻倍。应一次 `read_from_path` 同时出标签和封面，或扫描阶段不解码封面、首屏后再懒加载。
2. **启动/扫描对每首有封面的曲目解码并缩放 PNG。** 有 16MB 源上限和 512 边长，合理，但仍是 CPU 热点。`load_stored_library` 启动时对**全库**走 `cache_cover_urls`：已有缓存文件会跳过写入，但仍 `read_cover` 才能哈希——**若未记住哈希，启动仍会解析全部内嵌图**。`reuse_or_cache_covers` 只在监听增量时按路径复用 URL；冷启动不走这条路。
3. 建议：封面缓存键用 `(path, mtime)` 或把哈希存 SQLite，启动只拼 `file://` 路径、`Image` 异步读盘失败再回源。

### 4.3 扫描与 SQLite（中）

1. **逐文件 upsert + 每文件独立事务**（当前是每曲一条事务，正确偏安全）。万级首次导入会偏慢，可按批 commit（如 50 首），失败再拆单条。
2. **`list_tracks` N+1 查歌手。** 启动恢复、每次扫描结束、每次监听批量都会打。应 join 一次取回。
3. **监听 `sync_tracks` 全表。** 十个文件改动 = 十次全库读。worker 应只返回变更路径，UI 侧 patch 或单次 list。
4. **`instr(search_terms.normalized, ?)`。** 无 FTS、无 `normalized` 索引时，搜索随 `search_terms` 行数线性增长（每曲至少 1–N 行）。500 条 LIMIT 不能减少扫描行数。下一阶段应用 FTS5 或至少对 `(field, normalized)` 建索引。
5. **多连接无 WAL。** 监听写入与 UI 搜索并行时容易卡住。`PRAGMA journal_mode=WAL; busy_timeout=5000` 应视为扫描/监听的标配。

### 4.4 内存聚合与 UI 模型（中，出在扫描完成时）

1. `aggregate_artists` 对多歌手曲目多次 `TrackMetadata` clone；组查找线性。万级 × 平均 1.2 歌手仍可能在主线程 **扫描回调里同步聚合**（`apply_scan_result` → `refresh_collections`）。大库会卡一下 UI。聚合应放工作线程，或只传索引。
2. `show_library_tracks` clone 整库到可见 `tracks`。搜索清空时再 clone 回来。可接受，但排序 `begin_reset_model` 会丢掉滚动位置并重载可见封面。
3. ICU 与拼音键：键在 load 时算一次，比较热路径只 `compare_keys`，这部分 **已经是对的**。

### 4.5 已做得好的

- 扫描/搜索/监听不在 QML 里做 IO。
- mtime 跳过未改文件。
- 封面有边长和字节上限，输出 PNG 限 512。
- 排序键不在 `sort_by` 闭包里现转拼音（曲目列表）。
- 搜索汉字不误伤拉丁歌名。

---

## 5. 总表

| 部分 | 综合 | 一句话 |
| --- | --- | --- |
| 架构 / 依赖方向 | 5.5 | 底层 crate 单向且干净；应用层名实不符，bridge 过重 |
| chinese | 8.0 | 领域核心，边界清楚 |
| metadata | 7.5 | 小而正确；重复解析、测试偏薄 |
| storage | 7.5 | schema 与迁移认真；缺 WAL/FTS/join |
| library | 6.5 | 扫描语义对；clone 与全表 sync 偏贵 |
| player | 6.5 | API 小；主线程阻塞 + 无测试 + 空 zbus |
| core | 5.5 | Settings 好；AppCore 是死代码 |
| ui_bridge | 4.5 | 质量瓶颈，不拆很难安全做队列/MPRIS |
| QML | 6.5 | 页面复用不错；主题和按钮样式复制多 |
| **仓库整体** | **6.0** | 作为 2B 中段的桌面播放器，主路径能跑；工程结构还停在“单一 AppBridge 应用” |

优先顺序（若随后做改造，而不是继续堆功能）：

1. 播放状态变更离开 Qt 主线程（或改为异步 Bus）。
2. 封面与 Lofty 单次解析；启动不要全库解码内嵌图。
3. SQLite WAL + `list_tracks` join + 监听不要每事件全表读。
4. 拆 `AppBridge` / 删或缩小 `AppCore`；去掉未用 `zbus`。
5. 搜索 FTS 放到曲库规模真正变大时。

可落地的原子改动与勾选状态见 [quality-changes.md](quality-changes.md)。本评估仍是快照，勾选清单才跟踪进度。
