# 清音 Qingyin

轻量、原生、中文优先的 Linux 本地音乐播放器。目前仓库处于基础架构阶段。

## 技术栈

- Rust 负责领域逻辑、音频、存储与桌面集成
- Qt 6 Quick / QML 负责界面
- `qmetaobject`（Qt Bridge for Rust）负责 Rust 与 QML 桥接
- GStreamer、SQLite、Lofty、ICU4X、zbus 提供底层能力

## 开发环境

需要 Rust 1.92 或更高版本，以及 Qt 6、Qt Quick Controls、GStreamer、
PipeWire 和 `pkg-config` 的开发包。构建脚本通过 `qmake6` 定位 Qt 6。

```bash
cargo check --workspace
cargo test --workspace
cargo run -p qingyin-ui-bridge
```

可设置 `RUST_LOG=qingyin=debug` 查看调试日志。

## 目录

- `crates/core`：应用状态、队列与设置
- `crates/player`：GStreamer 播放与 MPRIS
- `crates/library`：曲库扫描和文件监听
- `crates/metadata`：音频元数据读取
- `crates/storage`：SQLite 持久化
- `crates/chinese`：拼音搜索和中文排序
- `crates/ui_bridge`：Qt/QML 桥接与应用入口
- `qml`：Qt Quick 界面

架构决策与后续边界见 [docs/architecture.md](docs/architecture.md)。
