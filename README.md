# 清音 Qingyin

轻量、原生、中文优先的 Linux 本地音乐播放器。当前已可导入本地曲库、按拼音搜索与排序、浏览歌手/专辑/目录、查看源文件音频信息和同步本地歌词，并用 GStreamer 播放；可视播放队列与 MPRIS 尚未完成。

## 技术栈

| 层次 | 选型 |
| --- | --- |
| 领域逻辑 | Rust 2024（workspace，`rust-version` 1.92） |
| 界面 | Qt 6 Quick / QML |
| Rust ↔ QML | `qmetaobject` |
| 播放 | GStreamer `playbin`（状态切换在独立 GLib 线程）→ PipeWire |
| 元数据 | Lofty |
| 曲库存储 | SQLite（`rusqlite`，bundled） |
| 中文 | `pinyin` 搜索键 + ICU4X collator 排序 |
| 文件监听 | `notify` |
| 设置 | XDG 下的 TOML |
| 日志 | `tracing` |

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

## 正在播放与歌词

- 点击底部播放器封面进入全页歌词；`Esc` 返回曲库，`F11` 切换系统全屏。歌曲标题仍可用于定位当前歌曲。
- 优先读取歌曲旁的同名 `.lrc`（例如 `歌曲.flac` / `歌曲.lrc`），同名文件缺失或内容为空时读取内嵌文本歌词。支持 UTF-8、带 BOM 的 UTF-16、普通文本、多时间戳、`[offset:毫秒]` 和同时间译文。
- 点击带时间戳的歌词跳转；手动滚动暂停自动跟随，点击“回到当前歌词”恢复。歌词在打开歌词页面或胶囊模式时异步读取，切歌丢弃过期结果，页面切换共享当前播放会话。
- 点击封面下方的音频摘要查看源文件格式、采样率、位深、声道、码率、文件大小和路径。未知参数不显示；MP4、MPEG 等格式名称不代表具体编码器或声卡输出规格。
- 数据库升级至 v6 后，下一次启动扫描会重新读取已有歌曲，补齐源文件属性并保留歌曲 ID。文件未变化时，后续扫描继续使用缓存。

交互检查：`bash scripts/qml-interaction-check.sh`。尚未在真实指针设备上复现的项目见 [docs/verification.md](docs/verification.md)。`cargo test --workspace` 包含使用隔离 XDG 目录和 GStreamer `fakesink` 的真实播放链路测试。

## 窗口操作

曲库、歌词和设置页共用无边框圆角窗口，顶部提供最小化、最大化／还原和关闭按钮。拖动顶部空白区域移动窗口，双击最大化／还原；四边及四角支持系统缩放。最大化时取消圆角，全屏时隐藏窗口栏，可用 `F11` 进入或退出全屏。

## 胶囊模式

- 点击底部播放器音量左侧的胶囊图标，一键切换为 420 × 72 的无边框小窗口；默认显示封面、当前歌词和播放／暂停，无同步歌词时显示歌名。
- 鼠标悬停后向下展开到 420 × 132，显示歌名、歌手、上一首／下一首、进度条和返回按钮。移开后延迟 450 ms 收起；键盘操作或拖动进度条时保持展开。
- 点击封面、展开区返回按钮或按 `Esc` 回到原窗口。原窗口的尺寸、全屏状态和曲库浏览位置保留；关闭胶囊窗口也返回主窗口。
- 在歌词或空白区域按住拖动窗口，移动交给窗口管理器处理。胶囊与主窗口共享播放状态，隐藏主窗口时暂停其背景捕获并卸载大歌词页。

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

`AppBridge` 组装 `LibrarySession`、列表模型和 `PlaybackController`。扫描、搜索、封面、监听和歌词在工作线程完成，回到主线程后才改界面。主线程上的曲库按曲目 id 存放一份快照，点播队列与它共用 `Arc<TrackMetadata>`。

播放命令在独立的 GLib 线程上执行，有命令才唤醒。进度、错误和曲末事件带加载世代，过期结果丢弃。窗口不可见时降低进度发布频率。上一首和下一首作用在点播队列上，可视队列尚未实现。顺序播放到列表末尾后停止。

数据库 schema 为 6，路径是 `$XDG_DATA_HOME/qingyin/library.sqlite3`。未变化文件用修改时间和大小跳过。`tracks` 含格式、采样率、位深、声道和码率，界面据此显示 Hi-Res。封面按内容摘要缓存，不把原图放进数据库。设置在 `~/.config/qingyin/settings.toml`，损坏或缺失时不会用空目录清库。
