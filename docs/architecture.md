项目名

Qingyin / 清音

定位：轻量、原生、现代、中文优先的 Linux 本地音乐播放器。

技术栈
| 模块	| 技术 |
| --- | --- |
| 主语言 |	Rust |
| UI	 |Qt 6 Quick / QML |
| Rust ↔ Qt |	`qmetaobject` |
| 音频播放	 |GStreamer + gstreamer-rs |
| Linux 音频 |	PipeWire |
| Metadata |	Lofty |
| 数据库 |	SQLite + rusqlite |
| 中文排序 |	ICU4X / Pinyin Collation |
| 中文搜索 |	pinyin crate + 自定义索引 |
| 文件监听 |	notify |
| Linux 桌面集成 |	MPRIS / D-Bus + zbus |
| 配置 |	serde |
| Logging |	tracing |

产品架构
┌─────────────────┐
│         Qt Quick / QML           │
│                                  │
│ Library / Artist / Album / Track │
│ Queue / Player Bar / Settings    │
└───────┬─────────┘
                │
         Qt Rust Bridge
                │
┌───────▼─────────┐
│            Rust Core             │
│                                  │
│ Library   Search    Queue        │
│ Player    Metadata  Settings     │
└──┬────┬────┬────┘
       │        │        │
       ▼        ▼        ▼
 GStreamer    SQLite    Lofty
       │
       ▼
   PipeWire

Chinese Layer
├─ ICU4X 拼音排序
├─ 拼音全文搜索
├─ 拼音首字母搜索
├─ ARTISTSORT 优先
└─ 多音字 Override
Workspace

qingyin/
├── crates/
│   ├── core/
│   ├── player/
│   ├── library/
│   ├── metadata/
│   ├── storage/
│   ├── chinese/
│   └── ui_bridge/
│
├── qml/
│
└── assets/

核心原则：

Rust 负责逻辑，QML 只负责 UI；成熟组件负责播放和解码；自己重点做好中文曲库、搜索、排序和极简体验。

运行时模块依赖、数据流、播放后端与 SQLite schema 见仓库根目录 [README.md](../README.md)。本文保留产品边界与桥接决策。

桥接决策：使用 `qmetaobject` 由 Rust 导出 `QObject`、属性、信号和方法给 QML。曾用 Qt Widgets 壳验证卡死是否只属于 Quick；Hyprland 上同样在 `play_*` 后失去指针，已改回 QML。`playbin` 的状态切换改到 GLib 播放线程，不再和 Qt 主线程并发操作。