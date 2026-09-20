# 桌面交互验证记录

离屏自动检查：`bash scripts/qml-interaction-check.sh`（`QT_QPA_PLATFORM=offscreen`）。

| 场景 | 离屏 | 桌面鼠标 | 触摸板 |
| --- | --- | --- | --- |
| ListView 复用后双击仍使用 TrackId | 脚本覆盖 | 待在真实桌面补记 | 待在真实桌面补记 |
| 排序表头点击后仍按 TrackId 播放 | 脚本覆盖表头信号 | 待补记 | 待补记 |
| Loader 切页保留查询与滚动字段 | 脚本覆盖字段赋值 | 待补记 | 待补记 |
| 进度条左右键 5 秒 seek | 脚本覆盖 | 待补记 | 待补记 |
| 进度条拖动不在刷新时反向 seek | 绑定 `when: !pressed` | 待补记 | 待补记 |
