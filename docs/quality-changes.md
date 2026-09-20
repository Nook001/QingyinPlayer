# 质量改动清单

依据 [quality-review-2026-09.md](quality-review-2026-09.md) 拆出的可落地项。  
不含路线图功能（可视播放队列、MPRIS、嵌入 QML、安装打包）。

## 用法

- `- [ ]` 未做；`- [x]` 已合入。
- 一次只做一个编号；合入后勾选，并在提交说明里写上编号（如 `Q-03`）。
- 规模：小 ≈ 单文件/几处替换；中 ≈ 一两个 crate 行为变化；大 ≈ 跨线程或拆类型。
- 完成标准写的是「怎样算做完」，不是实现步骤。

建议顺序：先 A，再 B → C → D；E、F 可穿插；G 放到曲库明显变大之后。

---

## A. 小修（无行为争议）

- [x] **Q-01** 小 · 去掉 `qml/TrackTable.qml` 专辑/时长表头的重复 `onClicked`  
  完成：每个表头只保留一次点击处理，`qmllint` 通过。

- [x] **Q-02** 小 · 从 `qingyin-player` 移除未使用的 `zbus` 依赖  
  完成：`crates/player` 不再链接 zbus；workspace 里 zbus 仅在真正做 MPRIS 时再引入。

- [x] **Q-03** 小 · ICU collator 初始化失败时打 `tracing::warn`  
  完成：失败仍回退小写比较，但日志可见。

- [x] **Q-04** 小 · `AppBridge` 里 `expect("player was initialized")` 改为 `let Some(...) else { return }`  
  完成：播放热路径不再 panic。

- [x] **Q-05** 小 · 给 `AppBridge` 的 `library_tracks` / `tracks` / `playback_tracks` 补一行字段注释  
  完成：三者职责能从代码读出。

- [x] **Q-06** 小 · 给 `TrackMetadata` 的 `*_sort` 与 `*_key` 补字段注释  
  完成：标签与派生校对键不再混读。

- [x] **Q-07** 小 · 搜索防抖 Timer 仅在输入非空时 `running`  
  完成：空查询不空转 180ms Timer。

- [x] **Q-08** 小 · `format_duration` 只保留一份（`ui_bridge` 内共享）  
  完成：`lib.rs` 与 `collections.rs` 不再各写一份。

- [x] **Q-09** 小 · 括号剥离不用 `Vec<char>::remove(0)`  
  完成：行为与现测试一致（含 `「极地暗流」 - Narwhal`），实现改为 filter / drain。

---

## B. 播放热路径

- [x] **Q-10** 中 · 去掉 `playbin.state(3s)` 对调用线程的阻塞等待  
  完成：`load` / `play` / `pause` 不再在 Qt 主线程空等最多 3 秒；状态靠 Bus 的 `StateChanged` / `AsyncDone` 或错误回传。切歌、暂停、失败提示仍正确。

- [x] **Q-11** 中 · GStreamer Bus 改为 watch/回调，不再 250ms `timed_pop`  
  完成：EOS 与 Error 仍能回到 Qt 主线程；不再常驻轮询线程（或仅在 Playing 时工作）。

- [x] **Q-12** 中 · 播放进度由播放器推送，QML 不再 250ms 拉取  
  完成：`PlayerBar` 去掉（或显著降低）轮询 Timer；拖动 seek 不被后台进度覆盖的现有行为保持。

- [x] **Q-13** 小 · 为 `ensure_format_plugins` 和 Bus 错误格式化补单测  
  完成：不依赖真实播放即可测缺插件列表与错误字符串。

---

## C. 解析与封面

- [x] **Q-14** 中 · 一次 Lofty 打开同时产出标签和封面  
  完成：扫描/导入路径不再对同一文件 `read_from_path` 两次；无封面文件行为不变。

- [x] **Q-15** 中 · 封面缓存键改为路径 + mtime（或把哈希写入 SQLite）  
  完成：已有缓存时启动不必为了算哈希再解码内嵌图。

- [x] **Q-16** 中 · 冷启动只拼接已有 `file://` 封面 URL，缺文件再回源  
  完成：`restore_session` 不再对全库调用 `read_cover`；监听增量仍可复用 URL。

---

## D. SQLite 与扫描

- [x] **Q-17** 小 · 打开数据库时 `journal_mode=WAL` 且 `busy_timeout=5000`  
  完成：新连接默认 WAL；监听与搜索并行不再轻易 `SQLITE_BUSY`。有迁移或打开测试。

- [x] **Q-18** 中 · `list_tracks` / `search_tracks` 一次 JOIN 取回歌手，去掉 N+1  
  完成：列出 N 首曲目不再额外 N 次 `artists_for_track`；多歌手顺序仍按 `position`。

- [x] **Q-19** 小 · 监听 worker 使用不 `sync_tracks` 的刷新入口  
  完成：`refresh_path` 成功不再为 worker 整表 `list_tracks`；扫描器若仍需要内存列表则走显式同步。

- [x] **Q-20** 中 · 一批 debounce 路径处理完后，UI 只全表加载一次  
  完成：10 个文件变更不会触发 10 次 `list_tracks` + 封面扫描。

- [x] **Q-21** 小 · 为 `search_terms(field, normalized)` 等加索引  
  完成：`EXPLAIN` 或测试能证明搜索不再纯全表扫（在上 FTS 之前的过渡）。

- [x] **Q-22** 中 · 全量扫描按批提交事务（如 50 首，失败再拆单条）  
  完成：首次导入万级时不再每文件一次 commit；单曲失败仍不中止整批。

---

## E. 聚合与列表模型

- [x] **Q-23** 小 · 歌手/专辑分桶改 `HashMap`，去掉 `Vec::find`  
  完成：聚合结果与现测试一致，组查找不再随组数线性。

- [x] **Q-24** 中 · 聚合改为索引或 `Arc`，避免每组 `clone` 整份 `TrackMetadata`  
  完成：多歌手曲目不再按组数深拷贝；QML 仍能打开详情并播放。

- [x] **Q-25** 中 · 歌手/专辑聚合移出 Qt 主线程  
  完成：`apply_scan_result` 不再同步 `aggregate_*` 卡 UI；模型更新仍在主线程。

- [x] **Q-26** 中 · 内存排序改 `layoutChanged`（或等价），不用 `begin_reset_model`  
  完成：点表头排序尽量保持滚动位置，可见封面不全部重载。拼音序仍在 Rust 里比。

---

## F. 结构收敛（拆开才能继续做队列）

- [x] **Q-27** 小 · 删除未使用的 `AppCore`  
  完成：无调用方的聚合结构消失。

- [x] **Q-28** 小 · `qingyin-core` 在不做队列前只依赖设置所需 crate  
  完成：`Settings` 不再因 `AppCore` 间接依赖 `library` / `player`。`PlaybackQueue` 要么删到做 2B.6 再加，要么留在 core 且不拉播放器依赖。

- [x] **Q-29** 中 · 抽出独立的曲目列表模型，与 `AppBridge` 的会话对象分开  
  完成：QML 的 `libraryModel` 不再必须等于整个应用对象；角色映射只写一份。

- [x] **Q-30** 中 · 抽出 `PlaybackController`（或同名 QObject）  
  完成：load/play/EOS/进度/音量不写在列表模型里；`PlayerBar` 绑控制器。

- [x] **Q-31** 中 · 扫描 + 监听回调收到独立的 library session 类型  
  完成：`queued_callback` 不再直接把 IO 细节塞进巨型 `AppBridge` 方法列表。

- [ ] **Q-32** 中 · 工作线程回调携带 `thiserror` 枚举，而不是 `String`  
  完成：扫描/搜索失败在 Rust 侧可匹配，QML 仍显示中文文案。

- [ ] **Q-33** 大 · 校对键从 `TrackMetadata` 拆到装入 UI 快照时计算  
  完成：`metadata` crate 不再依赖 `chinese`；storage 仍持久化 `*_sort` 标签；列表排序行为不变。

---

## G. 稍后（规模上来再做）

- [ ] **Q-34** 大 · 搜索改 FTS5（或同等全文索引），替换 `instr` 全表匹配  
  完成：万级曲库汉字/拼音/首字母查询可接受；语义与现在一致（汉字不误伤拉丁）。

- [ ] **Q-35** 中 · `Theme.qml`（或 Rust 单例）承接 `Main.qml` 颜色属性  
  完成：子页不再靠 window 上二十多个 color 间接传主题。

- [ ] **Q-36** 中 · 共用按钮样式组件，去掉重复的 `contentItem`/`background`  
  完成：侧栏、表头、添加文件夹等主要按钮走同一套样式。

- [ ] **Q-37** 中 · 扫描/搜索共用一个工作队列，避免并行多个全量扫描抢 SQLite  
  完成：快速连点「添加文件夹」不会叠多个 `thread::spawn` 写库。

- [ ] **Q-38** 小 · 多值 `artist` 标签拆成 `track_artists` 多行  
  完成：`feat. ` / 多 ARTIST 帧不再整段塞进一个字符串（需样例标签测试）。

---

## 不做（刻意排除）

- 用 QML `QSortFilterProxyModel` 按显示字符串排中文。
- 为拆 `AppBridge` 先写大段设计注释。
- 把封面 PNG 字节带回 Qt 主线程。
- 在本清单里做 MPRIS、嵌入 QML、可视播放队列（走 [roadmap.md](roadmap.md)）。
