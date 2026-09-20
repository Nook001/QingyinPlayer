# Qingyin 功能开发状态

最后更新：2026-09-20

本文记录仓库当前已经实现的功能、仅完成的基础骨架，以及后续需要补齐的接口。
“依赖已引入”不代表对应功能已经实现。

## 状态说明

| 状态 | 含义 |
| --- | --- |
| 已完成 | 已有可调用实现，并通过当前自动化检查 |
| 骨架 | 类型、界面或基础设施已存在，但尚未形成完整用户功能 |
| 待实现 | 尚无业务实现，表中的接口为后续开发建议 |

## 已完成

### 工程基础

- Rust 2024 workspace 已建立，包含 `core`、`player`、`library`、`metadata`、
  `storage`、`chinese` 和 `ui_bridge` 七个 crate。
- Qt Bridge 使用 `qmetaobject`，Rust 可注册 `AppBridge` 并启动 Qt Quick/QML 应用。
- 已配置统一依赖、Rust 格式化规则、严格 Clippy 规则和 Git 忽略项。
- 应用可从源码目录加载 `qml/Main.qml`，日志由 `tracing` 初始化。

### 核心与数据模型

- [`TrackMetadata`](../crates/metadata/src/lib.rs) 已定义路径、标题、专辑、歌手、时长和可选排序标签，
  并支持 serde 序列化与反序列化。排序键在读取或从 SQLite 加载时计算一次。
- [`Settings`](../crates/core/src/lib.rs) 已定义音乐目录、主题、音量和排序，并支持 TOML 读写。

### 中文搜索与排序

- [`search_key`](../crates/chinese/src/lib.rs) 已实现输入规范化、全拼键和拼音首字母键生成。
- [`sort_key`](../crates/chinese/src/lib.rs) 将汉字转为无声调拼音后与拉丁文一起比较；`ARTISTSORT` 等标签优先。
- 含假名或韩文的名称保留原文，避免日文标题被汉语拼音插入英文字母段。
- 内置乐圈常见多音字读音（如 曾=zeng、单=shan、乐=yue）。
- 非汉字字母与数字可参与搜索键生成；空白和标点不会进入拼音索引。
- 已有单元测试覆盖“清音 Player”混合输入、汉英互插排序、标签优先和假名保留。
- SQLite 搜索按歌名、歌手、专辑分字段建索引；含汉字的查询只匹配原文，纯拼音/首字母查询
  才使用对应字段的全拼和首字母，歌名命中优先于歌手和专辑。

### 数据库基础

- [`Database::open`](../crates/storage/src/lib.rs) 和 `Database::open_in_memory` 可建立
  SQLite 连接并执行初始迁移。
- 已建立 `tracks`、`track_artists` 和分字段 `search_terms` 表。
- 已启用外键、WAL 与 `busy_timeout=5000`，并将数据库 schema 版本设为 `4`；打开旧库时会重建搜索索引、补排序标签列，并为 `search_terms` 建字段索引。
- 已实现事务式轨道 upsert（可按批提交）、曲库列表、按路径删除、目录前缀删除和修改时间查询。
- `list_tracks` / `search_tracks` 一次 JOIN 取回歌手，多歌手仍按 `position` 排序。
- SQLite 边界会校验时长等整数转换，重复路径不会产生重复轨道。
- 已有单元测试验证迁移、写入、更新、列表和删除。

### 元数据与曲库导入

- [`read_tagged_track`](../crates/metadata/src/lib.rs) 一次 Lofty 打开同时读取标签、时长和首张内嵌封面。
- 扫描导入用这次解析结果写封面缓存；冷启动只拼接已有 `file://` URL，缓存键为路径哈希 + mtime。
- 缺失标题时回退到文件名；读取错误保留文件路径和稳定错误文本。
- [`MusicLibrary`](../crates/library/src/lib.rs) 可递归扫描目录，组合 Metadata、Chinese 和 Storage。
- 扫描支持按修改时间跳过未变化文件，并汇总导入、跳过和失败数量。
- [`refresh_path`](../crates/library/src/lib.rs) 可对单路径增量导入、更新或删除，并忽略临时文件与监听目录之外的路径；成功后不自动整表 `list_tracks`。
- [`watch_directories`](../crates/library/src/watch.rs) 使用 `notify` 递归监听已保存目录，短延迟合并事件后在工作线程批量刷新；一批路径只让 UI 全表加载一次。
- 全量扫描按最多 50 首一批提交事务，单曲失败记入摘要而不中止整批。
- Storage 已提供按路径前缀删除目录下全部曲目，目录删除不必重扫父目录。
- `/mnt/workspace/Music/` 的 218 个 FLAC 和 97 个 MP3 已完成真实验证：首次导入 315 首，
  再次扫描 315 首均判定未变化，0 失败、0 重复。

### 播放器基础

- [`Player::initialize`](../crates/player/src/lib.rs) 可初始化 GStreamer 并创建 `playbin`。
- [`PlaybackState`](../crates/player/src/lib.rs) 已定义 `Stopped`、`Paused` 和 `Playing`。
- `Player` 可校验并加载本地文件，支持播放、暂停和停止，并可读取当前路径与状态。
- `Player` 在独立 `GLib` 主循环线程上用 Bus watch 接收 EOS、错误和状态变化，不再阻塞等待 `playbin` 状态，也不轮询 `timed_pop`。
- 播放中由播放器按 250ms 推送进度；暂停后停止推送。播放结束后可驱动下一首，错误可回传界面。
- 缺少格式插件时会给出具体元素名称，缺少 `autoaudiosink` 时尝试使用 `pipewiresink`。
- 播放器析构时会将 GStreamer 元素切换到 `Null` 状态。
- 已安装并验证 `gst-plugins-good` 与 `gst-plugins-base`：真实 FLAC、MP3 均可通过
  `playbin` 完整解码至 EOS。

### Qt Bridge 与界面

- [`AppBridge`](../crates/ui_bridge/src/lib.rs) 已注册为 `Qingyin 1.0/AppBridge`。
- QML 可调用 `application_name()` 和 `version()`。
- 曲库表绑定独立的 `TrackListModel`（标题、歌手、专辑、时长、路径、封面），与会话对象分开；详情列表共用同一套角色映射。
- [`PlaybackController`](../crates/ui_bridge/src/playback.rs) 承接 load/play/EOS/进度/音量；[`PlayerBar.qml`](../qml/PlayerBar.qml) 绑定 `backend.playback`。
- 文件夹扫描在工作线程执行，并通过 Qt queued callback 在主线程重置曲库模型。
- QML 可调用 `add_library_folder`、`play_track`、`toggle_playback`、`play_previous` 和
  `play_next`。
- 播放状态、错误、当前标题、歌手和封面已作为 Qt 属性绑定到播放栏。
- GStreamer EOS 会自动播放列表中的下一首；到达列表末尾后停止。
- Qt Bridge 已导出播放位置、总时长、seek 和音量控制；进度由播放器推送到属性，拖动 seek 时不被后台进度覆盖。
- `playbin` 的 `load` / `play` / `pause` / `seek` 排队到 GLib 播放线程，不再在 Qt 主线程上 `set_state`。
- [`Main.qml`](../qml/Main.qml) 使用系统标题栏与边框，侧栏、曲库、歌手、专辑、设置按当前页 `Loader` 装载。
- 曲库双击播放、单击不切歌；歌手/专辑单击打开详情、详情双击播放。
- [`Settings.qml`](../qml/Settings.qml) 已提供浅色与深色主题选择，主题、音量和音乐目录会写入配置并在启动时恢复。
- [`Library.qml`](../qml/Library.qml) 可选择文件夹、显示扫描状态和真实曲目列表，顶部提供拼音搜索与排序。
- [`PlayerBar.qml`](../qml/PlayerBar.qml) 已接通播放/暂停、上一首、下一首、进度拖动与音量。
- 启动恢复和添加文件夹后会启动目录监听；运行期文件变化经工作线程增量写入 SQLite，再回到主线程刷新曲库、歌手和专辑。
- Qt Widgets 壳实验已撤回：同样在 `play_*` 后指针假死，且 UX 不可接受。进程仍从 `qml/Main.qml` 启动。

## 已有骨架但未贯通

| 模块 | 当前已有 | 尚未贯通 |
| --- | --- | --- |
| Core | TOML 设置读写 | 未建立完整应用服务生命周期；播放队列尚未实现 |
| Player | `playbin`、播放/暂停/停止、位置、seek、音量、EOS 与错误事件 | 尚无主动状态变化事件 |
| Metadata | 标签、时长与封面读取、标题回退 | 超过缓存输入上限的封面会降级为占位图 |
| Library | 递归增量扫描、失败摘要、数据库持久化、歌手/专辑聚合、运行期文件监听 | 细粒度扫描进度仍未按文件回调 |
| Storage | SQLite schema、事务 upsert、列表、删除、目录前缀删除和分字段拼音搜索 | 尚无独立聚合查询，当前从曲目列表聚合 |
| Chinese | 全拼与首字母搜索键、拼音排序、多音字覆盖、`ARTISTSORT` | 外部 TOML 多音字配置尚未提供 |
| UI Bridge | 曲库模型、后台扫描、搜索、拼音排序、播放控制器、EOS 与错误处理、歌手/专辑模型、运行期监听 | 尚无细粒度扫描进度 |
| QML | 曲库内搜索、排序、封面、连续播放、完整播放栏、导航、双主题、歌手和专辑详情 | 播放队列尚未可视化 |

## 待实现接口

以下签名用于明确模块职责，尚未承诺为最终稳定 API。

### P0：导入并播放一首本地歌曲

#### Metadata

- `read_track(path) -> Result<TrackMetadata, MetadataError>`：已完成。
- `read_cover(path) -> Result<Option<CoverArt>, MetadataError>`：已完成首张内嵌封面读取；
  UI Bridge 会限制源图尺寸并缓存 512px 缩略图。
- 缺失标签应提供文件名等回退值，而不是阻断曲库导入。

#### Storage

- 事务式轨道 upsert：已完成。
- `remove_track(path) -> Result<bool, StorageError>`：已完成。
- `remove_tracks_under(directory) -> Result<usize, StorageError>`：已完成。
- `list_tracks() -> Result<Vec<TrackRecord>, StorageError>`：已完成。
- `search_tracks(query, limit) -> Result<Vec<TrackRecord>, StorageError>`：已完成。
- 批量扫描写入需要事务接口，避免逐曲提交。

#### Library

- `scan_directory(path) -> Result<ScanSummary, LibraryError>`：已完成同步后端实现。
- `watch_directories(paths) -> Result<LibraryWatcher, LibraryError>`：已完成；事件在工作线程合并后刷新。
- `refresh_path(path) -> Result<LibraryChange, LibraryError>`：已完成单路径新增、修改和删除。
- 扫描应调用 Metadata、Chinese 和 Storage，而不是在界面里处理文件。

#### Player

- `load(path) -> Result<(), PlayerError>`：已完成本地文件加载。
- `play() -> Result<(), PlayerError>`：已完成。
- `pause() -> Result<(), PlayerError>`：已完成。
- `stop() -> Result<(), PlayerError>`：已完成。
- `seek(position) -> Result<(), PlayerError>`：已完成。
- 已有查询接口：当前曲目和播放状态。
- 查询接口：当前曲目、播放状态、位置、时长和音量均已完成。
- 已有事件接口：曲目结束和播放错误；状态变化与位置变化仍需完善。

#### Core

- 启动时从配置和 SQLite 恢复主题、音量、排序和曲库：已完成。
- 建立导入目录、查询曲库、修改队列和控制播放的应用级方法。
- 明确后台扫描线程向 Qt 主线程发送事件的通道。

#### UI Bridge

- 曲库模型：已实现 `QAbstractListModel`，提供标题、歌手、专辑、时长和封面角色。
- 属性：`playbackState`、`currentTrack`、`position`、`duration`、`volume`。
- 方法：`addLibraryFolder`、`search`、`playTrack`、`togglePlayback`、`seek`。
- 信号：曲库变化、扫描进度、播放状态变化和错误通知。
- 跨线程结果必须通过 Qt queued callback 回到主线程。

#### 界面

- 将文件夹选择结果连接到 `AppBridge.add_library_folder`。
- 曲库、歌手和专辑页绑定后端模型。
- 播放、暂停、上一首、下一首、进度和音量均已接通。
- 增加扫描中、空结果、文件不可读和播放失败状态。

### P1：完整曲库体验

- 使用 ICU4X 实现中文拼音排序，并优先采用音频标签中的 `ARTISTSORT`：已完成。
- 内置常见多音字覆盖表已完成；外部 TOML 配置尚未提供。
- 配置文件读取、保存和启动恢复：已完成音乐目录、主题、音量和排序。
- 将当前主题选择写入配置，并在应用启动时恢复：已完成。
- 使用 `notify` 增量更新新增、删除和修改的音频文件：已完成。
- 使用 zbus 实现 MPRIS 播放控制和媒体信息同步。
- 封面缓存、专辑聚合和歌手聚合已完成；队列持久化待实现。

### P2：Linux 交付

- 将 QML 和默认资源嵌入二进制，移除运行时源码路径依赖。
- 添加应用图标、`.desktop` 文件、AppStream 元数据和安装规则。
- 确认 PipeWire 环境下的 GStreamer sink 选择与错误提示。
- 建立 Linux CI，以及发行构建和打包流程。

## 当前未接通的界面操作

- 播放队列尚未可视化，也尚未持久化。

## 验证基线

当前应保持以下命令通过：

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
/usr/lib/qt6/bin/qmllint -I qml qml/*.qml
```

应用启动检查：

```bash
cargo run -p qingyin-ui-bridge
```