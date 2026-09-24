# 路线图

架构以 [architecture.md](architecture.md) 为准。已完成的能力合并在一起，不再按旧阶段拆开。

## 已完成

本地目录可以导入、搜索、排序，并按歌手、专辑和目录浏览。未修改文件按指纹跳过，运行中的新增、修改和删除由监听增量处理。设置、曲库和播放模式可以在重启后恢复。

播放使用 GStreamer。底部播放条负责进度、音量和上一首／下一首；正在播放列表显示实际播放顺序。歌词页读取同名 `.lrc` 或内嵌歌词；胶囊窗口与主窗口共用播放状态。曲目行标出 Hi-Res。歌单只保存成员，不保存播放顺序。关闭主窗口只隐藏到系统托盘。退出时记住当前播放顺序、曲目和进度，下次启动按这个顺序恢复并暂停。窗口无边框，颜色主题取代浅色／深色开关，QML 已嵌入可执行文件，并带有 `.desktop` 与安装脚本。检查在本地运行，仓库不再附带 GitHub Actions。

## 未完成

- MPRIS，使桌面媒体键和锁屏可以控制播放。
- 发行质量：启动、扫描、内存和播放的性能基线，以及可复现的版本构建。异常媒体和非 UTF-8 路径已有测试。

## 验证

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/check-qml.sh
bash scripts/qml-interaction-check.sh
```

离屏交互与尚未在真实指针设备上复现的事项见 [verification.md](verification.md)。
