# Qingyin 开发路线图

本文以可验收的纵向功能为单位推进开发。架构边界以
[`architecture.md`](architecture.md) 为准，完成状态以
[`development-status.md`](development-status.md) 为准。

## 阶段一：导入、展示并播放

目标：用户选择本地音乐目录后，应用在后台导入曲目，在曲库列表展示结果，并可点击曲目播放。

### 已完成的后端接口

- Metadata：`read_track(path) -> Result<TrackMetadata, MetadataError>`。
- Storage：事务式 `upsert_tracks`、`list_tracks`、`remove_track`。
- Library：`scan_directory(path) -> Result<ScanSummary, LibraryError>`，支持递归扫描和按修改时间跳过。
- Player：`initialize`、`load`、`play`、`pause`、`stop`，使用 GStreamer `playbin`。

### Qt 接口进度

- 已完成：`AppBridge` 直接实现曲目列表模型，提供 `title`、`artist`、`album`、`duration`、`path` 角色。
- 已完成：`add_library_folder(url)` 在工作线程扫描，不阻塞 Qt 主线程。
- 已完成：`play_track(row)` 按模型行加载并播放曲目。
- 已完成：`toggle_playback()` 在播放和暂停之间切换。
- 已完成：`scanning`、`scanStatus`、`playbackState`、当前标题和当前歌手属性。
- 待完成：独立错误信号、细粒度扫描进度、位置、音量和播放结束事件。

### 线程边界

- 目录遍历、Lofty 读取和 SQLite 写入在工作线程执行。
- `QObject`、`QAbstractListModel` 和所有 QML 可见属性只在 Qt 主线程修改。
- 工作线程结果必须通过 `qmetaobject::queued_callback` 回到 Qt 主线程。
- QML 不直接访问文件系统、SQLite、Lofty 或 GStreamer。

### 验收标准

- 选择 `/mnt/workspace/Music/` 后界面保持可响应，并最终显示 315 首曲目。
- 重复导入同一目录不产生重复曲目，未修改文件被跳过。
- 点击 MP3 或 FLAC 曲目可播放，播放/暂停/停止状态与界面一致。
- 单个损坏或不可读文件不会中止整次扫描，界面显示失败摘要。

## 阶段二：完整曲库体验

目标：补齐搜索、排序、聚合、增量监听和持久化设置。

- 使用 SQLite 搜索索引支持汉字、全拼和拼音首字母查询。
- 使用 ICU4X 排序，并优先采用 `ARTISTSORT` 标签和多音字覆盖。
- 聚合歌手与专辑视图，增加封面读取、缩放和缓存。
- 使用 `notify` 处理新增、修改和删除文件。
- 持久化音乐目录、主题、音量和播放队列。
- 增加 seek、进度、音量、上一首、下一首和播放结束事件。

验收：万级曲库搜索和滚动保持流畅；文件变化无需全量重扫；重启后恢复设置和曲库。

## 阶段三：Linux 桌面集成

目标：成为可安装、可由桌面环境控制的 Linux 应用。

- 使用 zbus 实现 MPRIS 播放控制和媒体信息。
- 嵌入 QML 与默认资源，移除源码路径依赖。
- 提供图标、`.desktop`、AppStream 元数据和安装规则。
- 验证 PipeWire 环境下的 sink 选择、设备变化和错误提示。

验收：安装包可独立启动，桌面媒体键和锁屏控件可控制播放。

## 阶段四：发布质量

目标：建立持续验证、性能基线和发行流程。

- Linux CI 执行格式化、测试、Clippy、QML lint 和构建。
- 覆盖数据库迁移、异常媒体、非 UTF-8 路径和大曲库场景。
- 建立启动时间、扫描速度、内存占用和播放稳定性基线。
- 生成版本说明和可复现发行构建。

## 持续验证

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
/usr/lib/qt6/bin/qmllint -I qml qml/*.qml
```