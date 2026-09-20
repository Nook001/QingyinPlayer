# Qingyin 开发路线图

本文以可验收的纵向功能为单位推进开发。架构边界以
[`architecture.md`](architecture.md) 为准。

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
- 已完成：`play_track(track_id)` 按稳定曲目 ID 加载并播放。
- 已完成：`toggle_playback()` 在播放和暂停之间切换。
- 已完成：`play_previous()`、`play_next()` 与 EOS 自动播放下一首。
- 已完成：`scanning`、`scanStatus`、`playbackState`、当前标题和当前歌手属性。
- 已完成：播放错误、当前封面、曲目封面模型角色和封面缓存。
- 已完成：播放位置、seek 和音量。
- 待完成：细粒度扫描进度。

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

当前状态：阶段一主链路已完成。真实 FLAC 与 MP3 已验证可解码至 EOS；剩余工作归入
播放体验和曲库体验两个独立迭代。

## 阶段二 A：播放体验闭环

目标：让底部播放栏具备日常播放所需的完整反馈与控制，再扩展更大的曲库功能面。

1. Player 提供 position、duration、seek 和 volume 接口。
2. UI Bridge 定时同步播放位置，并避免用户拖动进度条时被后台值覆盖。
3. Player Bar 显示当前时间/总时长，支持拖动进度与调节音量。
4. 明确上一首、下一首和列表末尾策略，并为 EOS、错误与边界行为补测试。
5. 限制并缩放超大封面，避免大曲库因原图缓存产生过高磁盘和内存开销。

验收：连续播放、暂停恢复、seek、音量和错误状态始终与界面一致；MP3 与 FLAC 各完成
一次真实交互播放验证。

当前状态：已完成。position、duration、seek、volume、居中播放控件、双击切歌、封面缩放和
导航边界测试均已落地，真实交互播放验收通过。

## 阶段二 B：完整曲库体验

目标：补齐搜索、排序、聚合、增量监听和持久化设置。

- 使用 SQLite 搜索索引支持汉字、全拼和拼音首字母查询。
- 使用 ICU4X 排序，并优先采用 `ARTISTSORT` 标签和多音字覆盖。
- 聚合歌手与专辑视图，复用已完成的封面缓存。
- 使用 `notify` 处理新增、修改和删除文件。
- 持久化音乐目录、主题、音量和播放队列。
- 建立明确的播放队列模型，并持久化队列。

实施顺序：

1. 已完成：曲库顶部搜索框按歌名、歌手、专辑分字段检索；汉字只匹配原文，拼音/首字母只
	用于无汉字查询，歌名命中优先，并支持按歌曲名、专辑和时长切换升序/降序。
2. 已完成：持久化音乐目录、主题、音量和排序，启动时从 SQLite 恢复曲库并后台核对目录。
3. 已完成：专辑和歌手聚合模型及详情视图。
4. 已完成：接入文件监听，在运行期间增量处理新增、修改和删除。
5. 已完成：中文排序、`ARTISTSORT` 和多音字覆盖。
6. 建立可视播放队列并持久化。

当前状态：搜索、设置恢复、歌手/专辑聚合、运行期文件监听和中文拼音排序已完成。剩余可视播放队列。

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