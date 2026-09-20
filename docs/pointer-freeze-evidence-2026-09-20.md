# 历史指针假死记录（证据状态）

日期：2026-09-20。对象：工作区实现对照
[`code-quality-assessment-2026-09-20.md`](code-quality-assessment-2026-09-20.md)
审查时的历史输入假死讨论。本文**不**改写该评估的分数或结论。

| 陈述 | 状态 | 说明 |
| --- | --- | --- |
| Repeater 长列表可能在指针/触摸板快速滑动时造成 UI 假死 | **假设** | 评估文档为源码复杂度判断，未在本环境对 1k/10k/50k 列表做输入复现 |
| 虚拟化 ListView/GridView 可降低活动 delegate 数 | **已实现，未作为根因验证** | `TrackTable` 使用固定行高 `ListView` + `reuseItems`；`CollectionBrowser` 使用 `GridView` + `Loader`。本轮没有独立的桌面鼠标/触摸板假死复现实验 |
| 复用后局部状态串扰是假死根因 | **假设，未复现** | 已在 delegate 上重置 `highlighted`；不能据此宣布历史假死已修复 |
| 未接入的 Widgets 实验文件参与运行时 | **否定（代码）** | `crates/ui_bridge/cpp/` 未进入 Cargo 入口或 QML 加载路径。本轮不实现、不删除这些文件 |

离屏 QML 入口：`bash scripts/qml-interaction-check.sh`。该脚本检查复用列表、双击 ID、键盘 5 秒 seek 与页面状态字段，**不是**真实指针设备复现。
