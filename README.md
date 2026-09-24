# 清音 Qingyin

轻量、原生、中文优先的 Linux 本地音乐播放器。当前已可导入本地曲库、按拼音搜索与排序、浏览歌手/专辑/目录、维护不含播放顺序的歌单、查看源文件音频信息和同步本地歌词，并用 GStreamer 播放。正在播放列表按实际播放顺序显示。关闭窗口只隐藏到系统托盘；退出后按这份顺序恢复并暂停。桌面媒体控制（MPRIS）尚未接通。

![主页](/assets/library.png)

![胶囊](/assets/capsule-collapsed.png)


## 技术栈

| 层次 | 选型 |
| --- | --- |
| 领域逻辑 | Rust |
| 界面 | Qt 6 Quick / QML |
| 播放 | GStreamer `playbin`（状态切换在独立 GLib 线程）→ PipeWire |
| 元数据 | Lofty |
| 曲库存储 | SQLite（`rusqlite`，bundled） |
| 中文 | `pinyin` 搜索键 + ICU4X collator 排序 |
| 文件监听 | `notify` |

桌面媒体控制（MPRIS / `zbus`）已列入技术栈，**运行时代码尚未接通**。

## 开发环境

需要 Rust 1.92+，以及 Qt 6、Qt Quick Controls、GStreamer、PipeWire 和 `pkg-config`。构建脚本通过 `qmake6` 定位 Qt 6。

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/check-qml.sh
cargo run -p qingyin-ui-bridge --bin qingyin
```

QML 页面由 `qrc:/qml` 嵌入可执行文件。可设置 `RUST_LOG=qingyin=debug` 查看调试日志。

## 窗口操作

曲库、歌词和设置页共用无边框圆角窗口。窗口背景按所选颜色主题做对角漫射，主题为青瓷、霁蓝、松烟、暮山。

点击窗口右上角的胶囊按钮，切换为小型的歌词胶囊。

## 安装

```bash
PREFIX=/usr/local bash packaging/install.sh
QT_QPA_PLATFORM=offscreen qingyin --smoke
PREFIX=/usr/local bash packaging/install.sh uninstall
```

干净测试前缀：`bash scripts/check-install.sh`。QML 不单独安装；运行时需要 Qt 6 Quick Controls / Layouts / Dialogs / Window，以及 GStreamer `base`/`good` 与 PipeWire 插件。

## 仓库结构

```
qingyin/
├── crates/
│   ├── chinese/      # 拼音搜索键、排序键、ICU 比较
│   ├── metadata/     # Lofty 读标签、封面和歌词
│   ├── storage/      # SQLite schema、upsert、搜索
│   ├── library/      # 扫描、监听、封面缓存、聚合与排序键
│   ├── player/       # GStreamer 播放
│   ├── core/         # 设置（TOML）
│   └── ui_bridge/    # 应用入口、会话、模型和播放控制器
├── qml/              # Qt Quick 界面
├── docs/
└── assets/
```

模块、线程和数据所有权见 [docs/architecture.md](docs/architecture.md)。待办见 [docs/roadmap.md](docs/roadmap.md)。

---

## 实现要点

模块依赖、线程和数据所有权见 [docs/architecture.md](docs/architecture.md)。这里只保留和日常开发有关的行为。

`AppBridge` 组装 `LibrarySession`、列表模型和 `PlaybackController`。
扫描、搜索、封面、监听和歌词在工作线程完成，回到主线程后才改界面。主线程上的曲库按曲目 id 存放一份快照，点播队列与它共用 `Arc<TrackMetadata>`。

播放命令在独立的 GLib 线程上执行，有命令才唤醒。进度、错误和曲末事件带加载世代，过期结果丢弃。窗口不可见时降低进度发布频率。关闭主窗口只隐藏到系统托盘。退出时记住当前播放列表、曲目和进度，下次启动按这个顺序恢复并暂停。

数据库 schema 为 7，路径是 `$XDG_DATA_HOME/qingyin/library.sqlite3`。
未变化文件用修改时间和大小跳过。`tracks` 含格式、采样率、位深、声道和码率，界面据此显示 Hi-Res。歌单只保存曲目成员，不保存播放顺序；打开时按标题升序显示。封面按内容摘要缓存，不把原图放进数据库。设置在 `~/.config/qingyin/settings.toml`，损坏或缺失时不会用空目录清库。曲库默认也按标题升序。
