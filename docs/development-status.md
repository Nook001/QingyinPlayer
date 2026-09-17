# Qingyin 功能开发状态

最后更新：2026-09-17

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

- [`TrackMetadata`](../crates/metadata/src/lib.rs) 已定义路径、标题、专辑、歌手和时长字段，
  并支持 serde 序列化与反序列化。
- [`PlaybackQueue`](../crates/core/src/lib.rs) 已实现 `push`、`pop`、`len` 和
  `is_empty`。
- [`Settings`](../crates/core/src/lib.rs) 已定义音乐目录列表，并支持 serde。
- [`AppCore`](../crates/core/src/lib.rs) 已聚合曲库、播放队列、播放状态和设置。

### 中文搜索基础

- [`search_key`](../crates/chinese/src/lib.rs) 已实现输入规范化、全拼键和拼音首字母键生成。
- 非汉字字母与数字可参与搜索键生成；空白和标点不会进入拼音索引。
- 已有单元测试覆盖“清音 Player”混合输入。

### 数据库基础

- [`Database::open`](../crates/storage/src/lib.rs) 和 `Database::open_in_memory` 可建立
  SQLite 连接并执行初始迁移。
- 已建立 `tracks`、`track_artists` 和 `search_terms` 表。
- 已启用外键约束，并将数据库 schema 版本设为 `1`。
- 已有单元测试验证内存数据库初始化和 schema 版本。

### 播放器基础

- [`Player::initialize`](../crates/player/src/lib.rs) 可初始化 GStreamer。
- [`PlaybackState`](../crates/player/src/lib.rs) 已定义 `Stopped`、`Paused` 和 `Playing`。
- `Player::state` 可读取当前状态；默认状态为 `Stopped`。

### Qt Bridge 与界面

- [`AppBridge`](../crates/ui_bridge/src/lib.rs) 已注册为 `Qingyin 1.0/AppBridge`。
- QML 可调用 `application_name()` 和 `version()`。
- [`Main.qml`](../qml/Main.qml) 已使用无边框窗口，KDE 原生标题栏不再显示。
- [`WindowControls.qml`](../qml/WindowControls.qml) 已实现窗口拖动、最小化、最大化/还原和关闭。
- [`ResizeHandle.qml`](../qml/ResizeHandle.qml) 已通过系统级缩放 API 提供四边和四角缩放。
- 普通窗口使用透明背景和 10px 圆角，最大化时自动恢复直角。
- 侧边栏已纵向贯穿应用，顶部显示应用标题和折叠按钮，底部显示设置入口。
- 侧边栏折叠后仅显示导航图标，导航和设置页切换可用。
- [`Settings.qml`](../qml/Settings.qml) 已提供浅色与深色主题选择，两套主题可即时切换。
- [`Library.qml`](../qml/Library.qml) 可打开系统文件夹选择对话框。
- [`Search.qml`](../qml/Search.qml) 已提供中文及拼音提示的搜索输入框。
- 歌手、专辑和播放栏视图已完成初始布局与空状态。

## 已有骨架但未贯通

| 模块 | 当前已有 | 尚未贯通 |
| --- | --- | --- |
| Core | `AppCore`、队列、设置类型 | 未建立应用服务生命周期，未被 `AppBridge` 持有 |
| Player | GStreamer 初始化、播放状态 | 未创建 pipeline，不能加载或控制音频 |
| Metadata | `TrackMetadata` 类型、Lofty 依赖 | 尚未从音频文件读取标签和封面 |
| Library | 内存轨道容器、标题搜索键生成 | 尚未扫描目录、监听文件变化或持久化 |
| Storage | SQLite schema 和迁移 | 尚无轨道 CRUD、事务和搜索查询 |
| Chinese | 全拼与首字母搜索键 | ICU4X 排序、`ARTISTSORT` 和多音字覆盖未实现 |
| UI Bridge | QObject 注册、名称和版本方法 | 无曲库模型、播放属性、信号和异步任务接口 |
| QML | 无边框圆角窗口、八向缩放、窗口控制、可折叠导航、双主题、空状态和输入控件 | 页面未绑定真实数据，播放控件仍禁用，主题未持久化 |

## 待实现接口

以下签名用于明确模块职责，尚未承诺为最终稳定 API。

### P0：导入并播放一首本地歌曲

#### Metadata

- `read_track(path) -> Result<TrackMetadata, MetadataError>`：使用 Lofty 读取标签和时长。
- `read_cover(path) -> Result<Option<CoverArt>, MetadataError>`：读取并限制封面尺寸。
- 缺失标签应提供文件名等回退值，而不是阻断曲库导入。

#### Storage

- `upsert_track(track, search_key) -> Result<TrackId, StorageError>`
- `remove_track(path) -> Result<bool, StorageError>`
- `list_tracks(query) -> Result<Vec<TrackRecord>, StorageError>`
- `search_tracks(query, limit) -> Result<Vec<TrackRecord>, StorageError>`
- 批量扫描写入需要事务接口，避免逐曲提交。

#### Library

- `scan_directory(path) -> Result<ScanSummary, LibraryError>`
- `watch_directories(paths) -> Result<LibraryWatcher, LibraryError>`
- `refresh_path(path) -> Result<LibraryChange, LibraryError>`
- 扫描应调用 Metadata、Chinese 和 Storage，而不是在 QML 中处理文件。

#### Player

- `load(uri) -> Result<(), PlayerError>`
- `play() -> Result<(), PlayerError>`
- `pause() -> Result<(), PlayerError>`
- `stop() -> Result<(), PlayerError>`
- `seek(position) -> Result<(), PlayerError>`
- 查询接口：当前曲目、播放状态、位置、时长和音量。
- 事件接口：状态变化、位置变化、曲目结束和播放错误。

#### Core

- 建立 `AppCore` 初始化入口，统一创建数据库、曲库服务和播放器。
- 建立导入目录、查询曲库、修改队列和控制播放的应用级方法。
- 明确后台扫描线程向 Qt 主线程发送事件的通道。

#### UI Bridge

- 曲库模型：实现 `QAbstractListModel`，提供标题、歌手、专辑、时长和封面角色。
- 属性：`playbackState`、`currentTrack`、`position`、`duration`、`volume`。
- 方法：`addLibraryFolder`、`search`、`playTrack`、`togglePlayback`、`seek`。
- 信号：曲库变化、扫描进度、播放状态变化和错误通知。
- 跨线程结果必须通过 Qt queued callback 回到主线程。

#### QML

- 将文件夹选择结果连接到 `AppBridge.addLibraryFolder`。
- 曲库、歌手、专辑和搜索页绑定后端模型。
- 播放、暂停、上一首、下一首、进度和音量控件连接播放器属性与方法。
- 增加扫描中、空结果、文件不可读和播放失败状态。

### P1：完整曲库体验

- 使用 ICU4X 实现中文排序，并优先采用音频标签中的 `ARTISTSORT`。
- 建立可配置的多音字覆盖表，并在索引重建时应用。
- 实现配置文件读取、保存和版本迁移。
- 将当前主题选择写入配置，并在应用启动时恢复。
- 使用 `notify` 增量更新新增、删除和修改的音频文件。
- 使用 zbus 实现 MPRIS 播放控制和媒体信息同步。
- 实现封面缓存、专辑聚合、歌手聚合和队列持久化。

### P2：Linux 交付

- 将 QML 和默认资源嵌入二进制，移除运行时源码路径依赖。
- 添加应用图标、`.desktop` 文件、AppStream 元数据和安装规则。
- 确认 PipeWire 环境下的 GStreamer sink 选择与错误提示。
- 建立 Linux CI，以及发行构建和打包流程。

## 当前未接通的界面操作

- 文件夹对话框会发出 `folderSelected`，但没有接收者，选择后不会扫描音乐。
- 搜索框可以输入，但没有触发后端搜索，也没有结果模型。
- 播放、上一首和下一首按钮被禁用，没有后端动作。
- 播放栏显示固定占位文本，不反映真实播放状态。
- 浅色/深色主题可即时切换，但重启应用后不会保留选择。

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