# Qingyin 代码质量与性能评估

评估日期：2026-09-20。评估对象：当前工作区，Git HEAD `82de4708e85dfd3028173627d53583fec35ab854`。

**整体参考分：5.9 / 10；软件工程：6.1 / 10；性能设计：5.7 / 10。**

项目已经具备清晰的底层 crate 分工、可复用的页面组件、真实的 SQLite 持久化、异步播放线程和基础测试。主要短板是：曲库删除与恢复存在正确性漏洞，QML 长列表没有虚拟化，监听后的刷新仍在主线程执行重 I/O，异步播放缺少完整的命令确认和状态回传。这些问题比函数是否再缩短几行更影响“轻量、简洁、现代”的目标。

本报告只新增评估文档，不修改实现。保留原有 `docs/quality-review-2026-09.md` 历史快照。本次重新读取当前代码，不沿用旧报告的缺陷清单或分数。

## 1. 范围、证据与评分方法

### 1.1 审查范围与限制

- 覆盖全部 7 个 Rust crate、8 个 QML 文件、入口、Cargo 配置、QML 类型声明、现有测试和架构文档。
- 审查开始时，`crates/ui_bridge/cpp/` 是已存在的未跟踪目录。其 Widgets 实验代码也作了静态检查，但当前 Cargo/入口没有接入，不计入运行时性能得分，不删除或修改。
- 实际执行 Rust 格式检查、workspace 测试、Clippy 和 qmllint；另用隔离的临时数据库、临时目录和离屏 QML 做小型复现。
- 没有修改用户曲库、配置或音乐文件；没有启动真实曲库进行播放压力测试。
- 没有测量真实 Wayland/NVIDIA 场景的帧时间、GPU 占用、播放 CPU、RSS、耗电或万首导入耗时。性能分是**源码与复杂度层面的设计评估**，不是跑分。
- 本文整理本次审查发现的全部主要问题和可行动优化点，不代表形式化证明不存在其他问题。特别是异步事件交错、输入假死和音频设备故障，需要专项运行验证。

证据标签：**[复现]** 为本次隔离探针确认；**[代码]** 为当前源码直接确定；**[风险]** 为有具体执行路径、尚未复现的故障；**[测量]** 为必须先测成本再决定的优化。

优先级：**P1** 尽快修复，涉及记录误删、主要功能错误或关键规模瓶颈；**P2** 下一轮质量改进；**P3** 可随相关改动顺手处理。未发现需要宣称为 P0 的不可恢复音频文件破坏；文中的“删除”均指数据库曲库记录，除非另有说明。

### 1.2 两条评分轴

每个子项 0–10 分，先计算未舍入的加权分，最后保留一位小数。

| 软件工程子项 | 权重 | 评价内容 |
| --- | ---: | --- |
| E1 结构与依赖 | 25% | 职责单一、合理复用、依赖方向、模块边界、状态所有权 |
| E2 正确性与数据流 | 30% | 状态一致性、错误传播、异步时序、持久化和更新语义 |
| E3 表达与一致性 | 25% | Rust/QML 特性使用、命名、类型约束、配置收敛、不过度防御 |
| E4 验证与可维护性 | 20% | 测试有效性、接口可测性、文档准确性、诊断能力 |

`E = 0.25×E1 + 0.30×E2 + 0.25×E3 + 0.20×E4`。

| 性能子项 | 权重 | 评价内容 |
| --- | ---: | --- |
| P1 响应与线程 | 35% | 主线程阻塞、按需实例化、首次交互延迟 |
| P2 算法与增量 | 25% | 搜索、排序、聚合、队列、局部更新的规模成本 |
| P3 内存与 I/O | 25% | 数据复制、文件读取、封面解码、缓存、数据库操作 |
| P4 静态与后台 | 15% | 无交互、暂停、后台播放、唤醒与无效通知 |

`P = 0.35×P1 + 0.25×P2 + 0.25×P3 + 0.15×P4`。性能子项 P1 与缺陷优先级 P1 仅同名，含义不同。

模块综合分 `S = (E + P) / 2`。仓库分使用第 2 节的模块权重；文件分用于定位，**不再次计入总分**，避免同一问题因跨多个文件而重复加权。模块分按该模块整体职责独立判断，不按文件行数机械平均。

| 分数区间 | 含义 |
| --- | --- |
| 9–10 | 有充分验证，边界与规模策略成熟；10 分不表示绝对完美 |
| 8–<9 | 扎实，主要剩局部改进 |
| 7–<8 | 良好，有明确但可控的缺口 |
| 6–<7 | 基础成立，需要一轮针对性改进 |
| 4–<6 | 存在影响主路径或扩展性的结构问题 |
| <4 | 与该维度目标明显冲突，优先重做关键路径 |

这是工程判断量表，不是客观测量精确到小数点。未实现 MPRIS、可视队列等产品功能不直接扣分；已实现路径的错误、复杂度与缺少相应验证会扣分。不因使用 `var`、文件较长、没有异步运行时或没有 `#[inline]` 自动扣分。

## 2. 第一层：大模块评分

子项顺序分别对应 E1/E2/E3/E4 与 P1/P2/P3/P4。

| 模块 | 仓库权重 | E 子项 | 工程 E | P 子项 | 性能 P | 综合 S |
| --- | ---: | --- | ---: | --- | ---: | ---: |
| core / 设置 | 5% | 8 / 5 / 8 / 6 | 6.7 | 6 / 8 / 6 / 9 | 7.0 | 6.8 |
| chinese / 中文检索与排序 | 5% | 8 / 6 / 8 / 7 | 7.2 | 8 / 8 / 8 / 9 | 8.2 | 7.7 |
| metadata / 音频标签 | 10% | 8 / 6 / 8 / 5 | 6.8 | 7 / 8 / 5 / 9 | 7.1 | 6.9 |
| storage / SQLite | 15% | 8 / 4 / 8 / 6 | 6.4 | 7 / 4 / 6 / 8 | 6.2 | 6.3 |
| library / 扫描、监听、聚合 | 15% | 7 / 4 / 7 / 6 | 5.9 | 5 / 6 / 4 / 8 | 5.5 | 5.7 |
| player / GStreamer 后端 | 15% | 7 / 4 / 6 / 5 | 5.5 | 7 / 8 / 7 / 8 | 7.4 | 6.4 |
| ui_bridge / 会话、模型、封面 | 20% | 6 / 5 / 7 / 4 | 5.6 | 3 / 4 / 4 / 6 | 4.0 | 4.8 |
| QML / 页面与控件 | 15% | 8 / 6 / 7 / 4 | 6.4 | 3 / 3 / 3 / 8 | 3.8 | 5.1 |
| **仓库加权** | **100%** | — | **6.1** | — | **5.7** | **5.9** |

各模块主要依据：

- **core：** Settings 和 serde 默认值清楚，非有限音量有校验；直接覆盖文件、加载失败一律默认、版本被强制改写，削弱了持久化可靠性。不能把“core 只有 Settings”本身算成架构错误。
- **chinese：** 拼音键预计算、ICU 单例、标签优先和语言分支合理；姓氏多音字覆盖全局应用到歌曲标题，并与搜索读音不一致。无需为了几百行代码另搭框架。
- **metadata：** 标签与封面单次解析是优点；多值歌手、封面选择与输入内存界限不足，两个读取包装仍做额外工作。
- **storage：** WAL、busy timeout、JOIN 和批事务已经落实；Linux 路径删除大小写语义错误、迁移非原子、真实搜索未获现有索引有效加速，是主要扣分。
- **library：** 扫描/监听/聚合 API 可辨，HashMap 与 Arc 已有使用；权限错误被转成删除、离线删除未对账、时间戳精度与监听风暴控制不足。
- **player：** GLib Bus watch 和播放时进度源比双重轮询好；状态仍由命令提交端乐观维护，异步失败有日志无确认，关闭等待不真正有界。
- **ui_bridge：** 已拆出 LibrarySession、PlaybackController 和模型，不再是旧报告中的单体 AppBridge；但会话内仍混入封面缓存、路径策略和数据库编排，主线程刷新、复制和异步世代不统一。
- **QML：** TrackTable、CollectionBrowser、Artist/Album 复用合理，页面 Loader 和无常驻动画有利于静态成本；两个大 Repeater、全尺寸封面加载、过时类型元数据和交互状态回归是主要问题。

## 3. 第二层：核心文件评分

以下文件采用同一量表；分数用来比较当前修改优先级，不用来衡量作者水平。

| 核心文件 | 工程 | 性能 | 核心理由 / 对应问题 |
| --- | ---: | ---: | --- |
| [core/src/lib.rs](/mnt/workspace/Projects/Qingyin/crates/core/src/lib.rs:27) | 6.7 | 7.0 | 设置表达简洁；异常恢复与写入策略需改，C03、C10、E03 |
| [chinese/src/lib.rs](/mnt/workspace/Projects/Qingyin/crates/chinese/src/lib.rs:24) | 7.2 | 8.2 | 单例与预计算正确；读音语义有误，C11、E06 |
| [metadata/src/lib.rs](/mnt/workspace/Projects/Qingyin/crates/metadata/src/lib.rs:67) | 6.8 | 7.1 | 解析入口收敛；多歌手/封面边界和测试不足，C12、P06 |
| [storage/src/lib.rs](/mnt/workspace/Projects/Qingyin/crates/storage/src/lib.rs:40) | 6.4 | 6.2 | CRUD 复用好；删除、迁移、搜索需修，C02、C04、P05 |
| [library/src/lib.rs](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:329) | 5.9 | 5.5 | 负责扫描与聚合的主体；C01、C05、C06、C12、P07、P08 |
| [library/src/watch.rs](/mnt/workspace/Projects/Qingyin/crates/library/src/watch.rs:74) | 6.2 | 6.0 | 空闲阻塞等待好；持续事件饥饿、无界队列与 join，C09、P04 |
| [ui_bridge/src/lib.rs](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/lib.rs:58) | 6.7 | 5.8 | 编排已拆分；回调反向借用、设置重复写与信号过宽，C10、E01、P10 |
| [ui_bridge/src/library_session.rs](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:94) | 4.8 | 3.5 | 最需集中治理：I/O、封面、模型、异步生命周期，C03、C08、P02、P03、P06 |
| [ui_bridge/src/playback.rs](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/playback.rs:21) | 5.2 | 6.0 | 队列与界面分离好；状态确认、旧事件、整表复制，C07、P08、P09 |
| [ui_bridge/src/collections.rs](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/collections.rs:75) | 6.2 | 5.0 | 角色映射共用；排序通知语义、双数组与 snapshot 复制，C13、P08、E02 |
| [player/src/lib.rs](/mnt/workspace/Projects/Qingyin/crates/player/src/lib.rs:87) | 5.5 | 7.4 | 播放线程与 Bus 结构合理；命令失败/状态/关闭验证欠缺，C07、C09、P09 |
| [ui_bridge/src/main.rs](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/main.rs:5) | 6.5 | 7.0 | 入口很小；源码绝对路径运行依赖、启动失败检查不足，E07 |
| [qml/TrackTable.qml](/mnt/workspace/Projects/Qingyin/qml/TrackTable.qml:124) | 6.0 | 2.5 | 公共组件成立；全量实例化和手写双击逻辑，P01、P03、C14 |
| [qml/CollectionBrowser.qml](/mnt/workspace/Projects/Qingyin/qml/CollectionBrowser.qml:116) | 6.5 | 3.0 | 复用有效；网格全量创建，隐藏时仍保留对象，P01、P03 |
| [qml/Library.qml](/mnt/workspace/Projects/Qingyin/qml/Library.qml:89) | 6.0 | 5.0 | 搜索 debounce 正确；Loader 返回后的状态同步、失败显示，C15、E05 |
| [qml/PlayerBar.qml](/mnt/workspace/Projects/Qingyin/qml/PlayerBar.qml:194) | 6.5 | 7.0 | 无轮询 Timer、拖动时暂停绑定正确；键盘 seek、音量频繁保存，C16、C10 |
| [qml/Main.qml](/mnt/workspace/Projects/Qingyin/qml/Main.qml:257) | 7.0 | 7.5 | 按页 Loader、原生窗口与主题集中；页面状态需外置，C15、E04 |

小文件 `Artist.qml`、`Album.qml` 各 21 行，作为 CollectionBrowser 的薄适配是合理复用，不单独给高分稀释核心文件缺陷。`Settings.qml` 主要是声明式展示，纳入 QML 模块，不因缺少复杂逻辑另设指标。

`qml/Qingyin/plugins.qmltypes` 属于接口工具元数据，工程参考 **3.0/10**，运行时性能不评分：它仍描述旧 AppBridge，当前静态检查已经报出缺失属性。未接入的 `cpp/widgets.cpp` / `.hpp` 不给运行时分；若将来启用，需要重新评估其旧接口与 100ms 轮询。

## 4. 当前架构与数据流判断

实际 Cargo 依赖是无环的：

```text
QML → ui_bridge → library → storage → metadata
                  │          └──────→ chinese
                  ├───────────────→ metadata / chinese
          ├────→ player → GStreamer
          ├────→ core → Settings
          └────→ storage / metadata（直接编排）
```

`metadata` 当前不依赖 `chinese`；`ui_bridge` 当前也没有直接依赖 `chinese`。README 中对应依赖图和文字已落后于 Cargo.toml。

**不需要为了“层数整齐”强制所有调用经过 core。** 对这个规模的应用，由 AppBridge 组装曲库、播放、设置是可接受的 composition root。真正应调整的是把 Qt 无关的封面缓存、扫描协调、数据库生命周期从 LibrarySession 移出，使 Qt 适配层只发布模型与属性。

当前最重要的线程边界：

| 路径 | 实际工作位置 | 结论 |
| --- | --- | --- |
| 启动恢复数据库、封面和集合 | restore worker | 已离开 UI；但缺缓存时会先回源全库再出首屏 |
| 主动扫描、首次封面生成 | scan worker | 正确方向；整次完成后才更新界面 |
| 文件事件更新 SQLite | watcher worker | 正确方向 |
| 文件事件后的全库加载、排序键、缺失封面生成 | **Qt 主线程** | P02，不能因为随后把集合聚合放线程就称整个刷新已异步 |
| 搜索 SQL | 每次新建搜索线程 | UI 不等 SQL，但旧任务不取消；结果应用还有 O(KN) 查找 |
| 列排序、可见模型复制与发布 | Qt 主线程 | 大库和 Repeater 组合放大开销 |
| 播放 set_state、seek、音量 | 已建立的 GLib context | 正常路径已串行；首次初始化、路径检查、关闭仍可能阻塞 |
| 设置持久化 | Qt 主线程 queued callback | queued 只是延后，不是离开主线程 |

## 5. 正确性、状态与持久化问题

### C01 · P1 · 目录读取失败被错误当作目录删除 [复现]

位置：[library/src/lib.rs:415](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:415)，递归读取在 475–509 行。

`refresh_directory` 将所有 `LibraryError::Io` 都交给 `remove_missing_prefix`。例如父目录仍在，只是其中一个子目录无读取权限，递归扫描返回 PermissionDenied，最后却删除父目录下全部已存记录。

探针：父目录已有 1 条数据库记录，子目录权限设为 `000`，对父目录调用 `refresh_watched_path`。结果为 `Ok(Removed(1))`，剩余 0 条。

**建议：** 只对确认不存在的目标执行删除；PermissionDenied、临时 I/O 故障和子目录读取失败返回可诊断失败。扫描结果应区分“完整枚举成功”和“局部失败”。**验收：** 权限失败、磁盘临时离线都保留记录；真正删除目录仍准确移除记录。

### C02 · P1 · SQLite LIKE 与 Linux 路径大小写语义不一致 [复现]

位置：[storage/src/lib.rs:194](/mnt/workspace/Projects/Qingyin/crates/storage/src/lib.rs:194)。

`DELETE ... path LIKE '/music/Album/%'` 在当前 SQLite 默认语义下，对 ASCII 大小写不敏感。在 Linux 上 `/music/Album/a.wav` 和 `/music/album/b.wav` 可以是不同目录；删除前者目录的记录会把后者一起删掉。探针实际删除 **2 条**，预期为 1 条。现有 `%`、`_` 转义没有解决大小写问题。[SQLite LIKE 语义](https://www.sqlite.org/lang_expr.html#like)

**建议：** 使用明确的 BINARY 路径前缀比较或经过严格边界证明的范围查询；不要简单改成未转义 GLOB，也不要依赖已不推荐的全局 case_sensitive_like pragma。**验收：** 同时覆盖大小写、目录分隔符、`%`、`_`、`[`、中文路径和 `/music/a` 与 `/music/ab`。

### C03 · P1 · 配置读取失败后，默认空目录会触发清空曲库记录 [复现：组合路径]

位置：[core/src/lib.rs:59](/mnt/workspace/Projects/Qingyin/crates/core/src/lib.rs:59)、[ui_bridge/src/lib.rs:81](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/lib.rs:81)、[library_session.rs:733](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:733)、[library/src/lib.rs:543](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:543)。

损坏/不可读配置 → `load_or_default` 返回空 `music_directories` → restore 调用 `prune_unwatched_tracks` → 每首都不在 roots 内 → 删除全部记录。首次缺少配置但遗留有数据库也有同类语义。独立调用同样的默认回退与 prune 组合，1 条记录被删除；没有让真实 GUI 访问用户数据复现。

**建议：** 区分首次默认、成功读取、读取失败；恢复失败时保留数据库，不能把“不知道 roots”解释为“用户明确移除了全部 roots”。只有确定的目录配置变更才能 prune。**验收：** 损坏、缺失、权限受限配置配合已有数据库，均不会自动清库；显式移除目录另测。

### C04 · P1 · schema 迁移不具备整体原子性 [代码 / 风险]

位置：[storage/src/lib.rs:235](/mnt/workspace/Projects/Qingyin/crates/storage/src/lib.rs:235)。

v2 分支先 DROP/CREATE 并设置 `user_version=2`，随后才 `rebuild_search_terms`；中途失败或进程退出，下一次打开会跳过未完成的重建。v3 多次 ALTER 也没有整体事务；中途完成一列后，下一次只检查 `title_sort`，可能遗漏其余列。多个连接同时首次 migrate 也缺少串行迁移协调；高于当前支持版本的数据库没有明确拒绝。

**建议：** 在应用初始化阶段用一个连接完成事务化迁移，成功后才提升版本，再启动扫描/搜索/监听；对更高版本报明确错误。**验收：** 各迁移阶段注入失败后重开，schema 与索引数据完整；并发冷启动不重复迁移。

### C05 · P2 · 全量扫描不处理应用关闭期间删除的文件 [复现]

位置：[library/src/lib.rs:329](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:329)、[library/src/lib.rs:543](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:543)。

扫描只 upsert 已发现文件；prune 只检查是否位于配置 roots 内，不检查这次完整扫描是否发现它。探针在一个空目录对应的库中预存已删除文件：`discovered=0, pruned=0, remaining=1`。文件在程序关闭时删除，就没有 watcher 事件帮助清理。

**建议：** 成功完整扫描某 root 后做按 root 的集合差异对账；发生 C01 所述读取失败时不执行删除阶段。避免全局 `exists()` 扫描将临时断开的挂载点当作永久删除。**验收：** 离线删除/重命名能修正；不可读目录不会被清空。

### C06 · P2 · 文件指纹只有秒级 mtime，扫描错误隔离不完整 [代码]

位置：[library/src/lib.rs:340](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:340)、[library/src/lib.rs:593](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:593)。

同一秒内修改标签或替换文件可能仍判定为 unchanged；封面缓存也使用该秒值，因此同样可能过期。`modified_at(path)?` 和 `track_modified_at(path)?` 会提前返回整批错误；例如枚举后单个文件消失，尚未 flush 的 pending 也不会提交。递归 `read_dir` 遇到一个不可读子树会中止整次收集，与“单文件失败不影响其他文件”的文档承诺不完全一致。

**建议：** 文件指纹收敛为至少纳秒 mtime + 文件大小；普通单文件错误计入 summary 并继续；数据库不可用等整库错误单独中止。扫描大目录时可流式枚举，保留明确的失败子树信息。**验收：** 同秒不同纳秒修改、扫描期间删除文件、部分目录不可读都覆盖。

### C07 · P1 · 播放命令提交、真实状态、界面状态没有形成一致协议 [代码 / 时序风险]

位置：[player/src/lib.rs:165](/mnt/workspace/Projects/Qingyin/crates/player/src/lib.rs:165)、[player/src/lib.rs:247](/mnt/workspace/Projects/Qingyin/crates/player/src/lib.rs:247)、[playback.rs:173](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/playback.rs:173)、[playback.rs:332](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/playback.rs:332)。

有四个相互关联的缺口：

1. `play/pause/stop/seek` 返回 `Ok`，线程内 `set_state` / `seek_simple` 失败只 `warn!`。函数文档声称失败会到 `PlayerEvent::Error`，但这些失败分支没有主动 emit，不能假设一定另有 Bus Error。
2. `Player.state` 在命令发出时更新；Bus `StateChanged` 只控制进度源，没有把真实状态同步给 controller。`set_playback_error` 又只改界面字段，未停 pipeline 或重置 Player 的状态。切换到不存在的文件时，旧曲可能继续播放，界面却显示 stopped/error。
3. `play_in_flight` 在 Qt 下一次 `publish_playback_ui` 时清零，**不代表 GStreamer load/play 完成**。名称与注释容易让后续代码误以为有后端串行确认。
4. `PlayerEvent` 没有曲目/命令世代。controller 的 generation 只保护 PendingPlaybackUi，不能拒绝旧曲已经排入 Qt 队列的 Progress/Error/EOS。快速切歌或更换列表时可能将旧事件应用于新曲。

**建议：** 定义轻量的命令 ID/会话世代，区分 Requested/Loading 与确认后的状态；由后端返回 StateChanged、CommandFailed 和带身份的事件。无需引入大型状态机框架。加载失败明确约定是保留旧播放还是停止，并使 UI 和 Player 一致。**验收：** 快速 A→B→C、加载失败、暂停与新加载交错、旧 EOS、seek 失败、连续缺失文件，均有可控 fake 后端测试；真实 GStreamer 用 fakesink 测事件顺序。

补充：`skip_missing_current_track` 只尝试紧邻下一首，并在当前路径仍缺失时停止；不会搜索后续第一首可播放曲目。队列连续缺失项的跳过策略应明确，而非依赖偶然的下一次调用。

### C08 · P2 · 扫描、恢复、搜索、监听的异步世代没有统一 [代码 / 风险]

位置：[library_session.rs:128](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:128)、[library_session.rs:280](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:280)、[library_session.rs:358](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:358)、[library_session.rs:459](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:459)。

- restore 未将 `scanning` 置 true，可与用户立即导入并行；restore/scan 回调没有请求世代检查，旧快照可能覆盖新结果。
- 监听回调有 library generation，但 scan 的发布会递增它并使已排队的监听结果失效；scan 自身又没有对应的 stale 检查，最终显示可能停留在较早快照。
- 搜索只检查 query generation；库变更没有统一使旧查询失效。主动扫描完成时，非空搜索不会重跑（监听分支会），搜索结果可能遗漏新导入或保留旧数据。
- 查询旧结果丢弃只省下 UI 更新，不能取消已经创建的线程与数据库工作。

**建议：** 先建立一个有界的曲库任务协调器，查询携带 `(query_generation, library_revision)`；扫描提交后统一刷新当前查询。记录任务句柄/取消标志与 shutdown 状态。**验收：** 控制各任务完成顺序，最终模型总是对应最新 roots、query 和库版本。

### C09 · P2 · 关闭和 watcher 重建可能同步卡住 UI [代码 / 风险]

位置：[watch.rs:57](/mnt/workspace/Projects/Qingyin/crates/library/src/watch.rs:57)、[library_session.rs:433](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:433)、[player/src/lib.rs:534](/mnt/workspace/Projects/Qingyin/crates/player/src/lib.rs:534)。

`self.watcher=None` 会在 Qt 主线程执行 Drop，发送 Stop 后 `join`；如果 watcher 正在大目录扫描或等待 SQLite，Stop 要等当前任务完结。Player Drop 虽然先等待最多 2 秒，之后 EventMonitor Drop 仍无期限 `join`，因此“关闭最多 2 秒”不成立。首次 `set_event_handler` 的 `ready_rx.recv()` 也没有超时。

独立启动的 scan/search/restore 线程没有统一 shutdown 协议；QPointer 能防止发布到已销毁 QObject，但不能取消磁盘工作或保证最后一次状态写入完成。

**建议：** 可取消批任务、后台异步关闭协调、worker 明确确认退出；退出阶段设期限并报告失败，避免把强杀作为常规退出方式。**验收：** 扫描中关闭、锁库时重建 watcher、音频后端不响应时关闭，窗口仍能响应且进程按明确策略退出。

### C10 · P2 · 设置直接覆盖且音量滑动会频繁写盘 [代码]

位置：[PlayerBar.qml:266](/mnt/workspace/Projects/Qingyin/qml/PlayerBar.qml:266)、[playback.rs:314](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/playback.rs:314)、[ui_bridge/src/lib.rs:99](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/lib.rs:99)、[core/src/lib.rs:113](/mnt/workspace/Projects/Qingyin/crates/core/src/lib.rs:113)。

音量每次 `onMoved` 都排队 persist，主线程序列化全部 Settings 并 `fs::write`。快速连续滑动可能积累多个保存回调，部分回调又读取最新音量，产生重复写入。直接覆盖在异常终止时可能留下截断配置，进一步触发 C03。

**建议：** 实时只应用音量；保存使用末值合并或短 debounce，停止拖动/退出时 flush。以同目录临时文件 + rename 原子替换配置，按需要决定 fsync，不为普通设置加入庞大备份系统。**验收：** 连续滑动期间保存次数有界、最终值正确；保存失败不破坏旧文件；退出不丢最后设置。

### C11 · P2 · 姓氏多音字规则污染歌曲名排序，且与搜索不同 [复现]

位置：[chinese/src/lib.rs:66](/mnt/workspace/Projects/Qingyin/crates/chinese/src/lib.rs:66)、[chinese/src/lib.rs:191](/mnt/workspace/Projects/Qingyin/crates/chinese/src/lib.rs:191)。

`polyphone_reading` 对所有位置和字段生效。本次实际结果：`快乐 → sort=kuaiyue / search=kuaile`，`单纯 → shanchun / danchun`，`曾经 → zengjing / cengjing`。姓氏优先不能无条件用于整个标题/专辑；按同一读音查找也可能与排序理解不一致。

**建议：** 显式区分歌手姓名和标题语境，保留标签覆盖优先；将读音策略收敛为共享接口，需要时加入少量词组规则或可编辑覆盖。先修已知语义，不要求立即引入分词模型。**验收：** 单姓/曾姓、快乐/音乐、单纯/单人、曾经/曾姓、日文与已有 sort 标签的交叉用例。

### C12 · P2 · 元数据与专辑模型尚不足以支持正确的专辑播放顺序 [代码]

位置：[metadata/src/lib.rs:113](/mnt/workspace/Projects/Qingyin/crates/metadata/src/lib.rs:113)、[library/src/lib.rs:194](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:194)、[library/src/lib.rs:756](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:756)。

- `Accessor::artist` 只取一个值，随后包装成单元素 Vec；下游虽然支持多歌手，但真实多值标签没有充分读取。
- 专辑仅以标题分组，不同歌手同名专辑会合并；“未知专辑”字符串也充当真实分组身份。
- 未保留 album artist、disc number、track number；专辑详情按标题排序并作为播放队列，不能恢复专辑原有曲序。
- 同排序键的 collection 名称缺少最终稳定 tie-breaker；输入来自 HashMap，相同校对键的集合顺序可能跨刷新变化。

**建议：** 读取格式实际提供的多值 artist，不随意按 `/` 等符号拆人名；AlbumKey 使用专辑名与 album artist 等明确身份规则，未知值用枚举/Option；详情按 disc/track 排序、标题作为兜底。**验收：** 同名不同专辑、合辑、双碟、多歌手和同校对键集合。

### C13 · P2 · 排序只发 dataChanged，丢失“行身份移动”语义 [代码]

位置：[collections.rs:90](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/collections.rs:90)。

当前重排底层 Vec 后通知整表数据变化。它可以让当前 Repeater 文本刷新，但不能表达持久索引、选择和当前曲目身份随排序移动；也导致所有角色重新读取。应使用 layoutAboutToBeChanged / 持久索引重映射 / layoutChanged，或适当的行移动通知。[Qt 模型通知约定](https://doc.qt.io/qt-6/qabstractitemmodel.html#layoutChanged)

已检查本机 `qmetaobject 0.2.10`，它提供 `layout_about_to_be_changed`、`update_model_indexes`、`layout_changed`，无需先更换桥接库。**验收：** 选中某 track 后排序仍选中相同 track，滚动锚点按产品约定保持；模型变更使用 Qt ModelTester 或等价测试验证。

### C14 · P2 · 手写双击与延迟 row 存在身份变化窗口 [代码 / 风险]

位置：[TrackTable.qml:151](/mnt/workspace/Projects/Qingyin/qml/TrackTable.qml:151)、[TrackTable.qml:237](/mnt/workspace/Projects/Qingyin/qml/TrackTable.qml:237)。

用 `Date.now` 和固定 400ms 识别双击，不跟随桌面双击间隔，也没有位移约束。`lastClickAt` 只在 index 改变时清空；当前 sort 仅替换同一 index 的数据，两次点击可能跨不同曲目。`Qt.callLater` 只捕获 row，若发布前排序/刷新模型，row 可能指向另一首。CollectionBrowser 同样按延迟 row 打开集合。

**建议：** 输入使用标准双击信号或合适的 Pointer Handler，并携带稳定 track/collection ID；在完成必要输入回归前保留 `callLater` 的重入隔离意义。不能因为源码看起来冗余就一次性移除所有延迟。**验收：** 不同系统双击间隔、滚动后双击、排序/刷新夹在两次点击之间、切页后点击，均无错播。

### C15 · P2 · Loader 重建页面时，搜索框与后端筛选可能不一致 [代码 / 风险]

位置：[Main.qml:257](/mnt/workspace/Projects/Qingyin/qml/Main.qml:257)、[Library.qml:89](/mnt/workspace/Projects/Qingyin/qml/Library.qml:89)。

切页销毁 Library，后端 search_query 和筛选模型仍保留；返回后 TextField 默认空值，没有从后端恢复。可能出现搜索框空白但列表仍筛选，或者构造阶段 onTextChanged 清除查询；不应依赖构造信号顺序定义产品行为。滚动位置也没有独立保存。

**建议：** 明确“保留筛选/清空筛选”的单一策略，页面状态放到独立轻量状态对象或 LibrarySession，并显式恢复。保留 Loader 的内存收益。**验收：** 输入查询→切页→返回、未完成 debounce 时切页、搜索执行中切页，都保持确定行为。

### C16 · P2 · 进度条默认键盘步长与毫秒单位不匹配 [复现]

位置：[PlayerBar.qml:194](/mnt/workspace/Projects/Qingyin/qml/PlayerBar.qml:194)。

Slider 的范围使用毫秒，但没有设置 `stepSize`。Qt 在未指定步长时按 0.1 个数值单位增加/减少，即这里的 0.1ms；随后 `Math.round(value)` 将单次按键的变化舍回原整数。离屏 QtTest 加载实际 PlayerBar，把当前位置设为 30,000ms，按一次右方向键，实际收到 `seek_to(30000)`，没有前进。

**建议：** 明确键盘步长，例如 5,000ms，结合合适的 snapMode 保持鼠标拖动连续；必要时单独实现键盘步进。Qt 的方向键本身会改变 pressed，不能把问题误诊为“onPressedChanged 不支持键盘”。[Qt Slider 实现与步长规则](https://raw.githubusercontent.com/qt/qtdeclarative/dev/src/quicktemplates/qquickslider.cpp)

**验收：** 一次方向键按约定秒数 seek、接近起止位置正确限制，鼠标拖动不抖动，后端进度刷新不反向触发 seek。

## 6. 性能问题与优化方向

### P01 · P1 · 歌曲与集合列表一次性实例化全部条目 [复现 / 代码]

位置：[TrackTable.qml:124](/mnt/workspace/Projects/Qingyin/qml/TrackTable.qml:124)、[CollectionBrowser.qml:116](/mnt/workspace/Projects/Qingyin/qml/CollectionBrowser.qml:116)。

两处是 ScrollView + Column/Grid + Repeater。`clip` 只影响显示裁剪，不能使 Repeater 虚拟化；`Image.asynchronous` 也不会限制 delegate 数量。[Qt Repeater 官方说明](https://doc.qt.io/qt-6/qml-qtquick-repeater.html)

本次离屏加载**实际 TrackTable 组件**，给 1,000 条记录、1,000×700 窗口，递归统计带 `lastClickAt` 的行对象，确认实例化 **1,000 行**。这不是 FPS 测试，但直接证明全量创建。

**优化：** 歌曲列表使用固定行高 ListView，开启 `reuseItems`；集合使用 GridView 按视口创建；适度设置 cacheBuffer。Qt ListView 的复用默认并未开启；池内对象仍可能响应绑定/信号，应在 pooled/reused 时清理局部交互状态。[Qt ListView 复用说明](https://doc.qt.io/qt-6/qml-qtquick-listview.html#reusing-items)

当前“表格”只有整行内容，不必为了名称迁移到 QAbstractTableModel/TableView；只有确实需要单元格级编辑/选择再考虑 TableView。`fetchMore` 是后端模型分页，与 delegate 虚拟化不同，先解决全量 UI 创建，再按真实内存决定后端分页。

**验收：** 1k/10k/50k 曲目时，delegate 数随视口和缓冲区变化，不随总条数线性增长；复用不会串封面/双击状态；完成鼠标、触摸板、快速切页与播放交叉回归。历史输入假死报告不能作为永久放弃虚拟化的依据，也不能凭此次静态审查宣布根因已解决。

### P02 · P1 · 监听结果在 Qt 主线程重读全库并解码封面 [代码]

位置：[library_session.rs:459](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:459)、[library_session.rs:798](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:798)。

调用链：queued callback → `apply_watch_summary` → `load_stored_tracks` → Database::open/list_tracks → 重算所有 TrackSnapshot keys → `reuse_or_cache_covers` → 必要时 Lofty 读文件、图片 decode/thumbnail/PNG write。最后才 spawn 集合聚合线程。

这意味着单个标签变化可能让 UI 承担 O(N) 数据库读取、排序键计算、缓存检查，以及一个或多个完整封面解码。SQLite 配置 5 秒 busy timeout；它不是所有查询必然等待 5 秒，但若这里的数据库操作遇到锁等待，等待发生在 UI 线程。

**优化：** 整条刷新管线放入同一个后台任务，Qt 端只应用不可变快照；随后再实现 ChangedTrack/RemovedTrack 集合与局部 model 通知。第一步不必直接建设复杂增量数据库镜像。**验收：** tracing 中 UI 线程无 SQLite/Lofty/封面编码调用；批量修改期间仍能滚动和暂停。

### P03 · P1/P2 · 封面显示尺寸与解码尺寸脱节，缺缓存拖延首屏 [代码 / 测量]

位置：[TrackTable.qml:180](/mnt/workspace/Projects/Qingyin/qml/TrackTable.qml:180)、[CollectionBrowser.qml:164](/mnt/workspace/Projects/Qingyin/qml/CollectionBrowser.qml:164)、[PlayerBar.qml:44](/mnt/workspace/Projects/Qingyin/qml/PlayerBar.qml:44)、[library_session.rs:733](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:733)。

- Rust 缓存最长边 512，但 44/52/54px 等 QML Image 没有 `sourceSize`，会加载不必要的大图。512×512×4 约 1MiB，只是单张 RGBA 理论展开量，不是本次显存测量；同 URL 共享、驱动布局和实际尺寸都会改变总量。
- 缓存命中时启动不需要重新解析音频，这是已做好的；**缓存缺失时** `existing_cover_urls(..., true)` 会先给全库回源，再发布首屏。此前文档“启动只拼 URL”不完全准确。
- 缓存按曲目路径存；同一专辑每首相同图也会产生不同 URL/文件，限制 Qt 图像缓存共享。
- 进入详情只隐藏集合 ScrollView，集合对象与图像仍保留；没有持续动画不等于对象不占内存。

**优化：** 按显示尺寸与 DPR 设置固定档位 `sourceSize`，避免窗口每变 1px 就改变 sourceSize 导致重载；先发布文字数据，再优先生成首屏/当前播放封面。专辑或内容哈希去重作为下一步，首次解析时算哈希，不在每次启动重新读取原图。详情列表和集合可用 Loader 按模式卸载。[Qt Image sourceSize](https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceSize-prop)

**验收：** 冷缓存首屏不用等待全部封面；1×/2×DPR 清晰度正确；对比图像内存、首屏时间、回源次数，不能只看 PNG 文件大小。

### P04 · P2 · watcher 持续事件可推迟刷新，无界积压与重复子树扫描 [代码]

位置：[watch.rs:79](/mnt/workspace/Projects/Qingyin/crates/library/src/watch.rs:79)、[watch.rs:109](/mnt/workspace/Projects/Qingyin/crates/library/src/watch.rs:109)、[watch.rs:163](/mnt/workspace/Projects/Qingyin/crates/library/src/watch.rs:163)。

每次 Paths 都把 deadline 重设为现在 +400ms；持续拷贝文件时可能一直不 flush。通道是无界 mpsc；后端处理慢时积压。BTreeSet 只去掉完全相同路径，父目录事件和全部子文件事件仍可能重复扫描。只去掉相同 roots，不去掉嵌套 roots；recursive watcher 仍可能监视扫描策略排除的整片 vendor/runtime 子树。

创建监听失败仅写日志，甚至所有 root 都未成功监听仍返回 watcher；缺失 root 恢复挂载、事件丢失/rescan 提示没有明确协调策略。`event_paths` 对每条事件做 canonicalize/is_dir，也应计入事件高峰 I/O 成本。

**优化：** 安静窗口 + 最大等待时限；有界、按路径合并的邮箱；子树覆盖去重；有效 roots 规范化一次；报告实际监听成功/失败范围。溢出触发一次合并后的完整对账，避免每个失败重扫。**验收：** 持续导入也周期可见，峰值队列有界，嵌套根不重复处理，同一批父子事件不会多次读相同文件。

### P05 · P2 · 真实搜索扫描全文，当前索引测试并未覆盖它 [复现]

位置：[storage/src/lib.rs:126](/mnt/workspace/Projects/Qingyin/crates/storage/src/lib.rs:126)、[storage/src/lib.rs:805](/mnt/workspace/Projects/Qingyin/crates/storage/src/lib.rs:805)。

实际 SQL 是 `instr(...)` 三列 OR、GROUP BY、rank 排序再 LIMIT。现有 `(field, value)` B-tree 索引并不自动加速任意子串检索。本次对代码中的**原始搜索 SQL**执行 EXPLAIN QUERY PLAN：

```text
CO-ROUTINE ranked
SCAN search_terms
SEARCH tracks USING INTEGER PRIMARY KEY (rowid=?)
USE TEMP B-TREE FOR GROUP BY
USE TEMP B-TREE FOR ORDER BY
SCAN ranked
SEARCH track_artists USING INDEX sqlite_autoindex_track_artists_1 (track_id=?) LEFT-JOIN
USE TEMP B-TREE FOR LAST TERM OF ORDER BY
```

现有 `field_normalized_lookup_uses_search_index` 测的是 `field='title' AND normalized='qingyin'`，与产品 SQL 不同。索引存在不等于当前查询受益；索引还增加写入和磁盘成本。该探针使用空的隔离 schema，证明当前计划，不构成 10k/50k 耗时测试。

此外，搜索每次重新 open/configure/migrate/prepare；Qt 回调为每个结果线性搜索全库封面，O(KN)，K≤500；全库 N 大时 SQL 异步仍不足以保证 UI 顺畅。SQL LIMIT 后才过滤 roots，可能使有效结果少于限额；500 条还没有“结果已截断”的明确语义。

**优化顺序：** 复用专属后台读连接和 prepared statements；建立 path→cover/track 映射；把 roots 约束放入查询前；再根据真实数据与搜索语义选择 prefix 索引、FTS 或 trigram。不要用默认 FTS 分词直接替代现有任意子串功能；FTS5 trigram 对不足 3 字符查询有限制，拼音首字母和单汉字需单独策略。[SQLite FTS5 trigram](https://www.sqlite.org/fts5.html#the_trigram_tokenizer)

**验收：** 真实搜索 SQL 的 query plan 与 p50/p95 用同一组输入记录；汉字、全拼、首字母、英文中间子串与字段排序语义不回退。

### P06 · P2 · 封面内存上限放得偏晚，缓存失效与失败策略不完整 [代码]

位置：[metadata/src/lib.rs:132](/mnt/workspace/Projects/Qingyin/crates/metadata/src/lib.rs:132)、[library/src/lib.rs:349](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:349)、[library_session.rs:875](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:875)。

1. 16MiB 检查发生在 `prepare_cached_cover`，之前 Lofty 已读图、`to_vec()` 已复制，批扫描还会积累最多 50 份 CoverArt。128MiB 是单次解码限制，不是导入过程总内存上限。即使每张仅 16MiB，50 张也约 800MiB 压缩字节，不计解析中暂存和解码。
2. 增量 `refresh_audio_file` 读取封面后丢弃；UI 更新缺缓存时再次 read_cover，因此“音频文件只解析一次”只对主动扫描导入路径成立。
3. `read_track` 调用 read_tagged_track 再丢封面，`read_cover` 同样计算全部标签/属性；Lofty 本地 ParseOptions 已支持 `read_cover_art(false)`、`read_properties(false)`，可按场景验证使用。
4. 没封面会写 `.missing`，有图但超限/解码失败却不记录失败标记；每次重扫可重复读取同一不可处理图。read_cover I/O 失败也没有重试退避或分类。
5. PNG 直接 `fs::write`，并发生成同一 key 或写到一半被读/进程中断，会留下不完整文件；`cached_cover_url` 只看 exists，后续可能长期当作有效缓存。
6. 缓存没有容量、旧 mtime 文件清理或格式版本；DefaultHasher 未承诺跨版本稳定，不宜作为长期格式契约。缓存可以失效重建，但应明确版本边界。
7. 选取 tags 顺序中的第一张图片，没有优先 FrontCover；遇到背面/艺术家图可能展示错误图片。

**优化：** 分离“数据库批量”与“图片字节驻留”，限制累计字节而不只限制首数；扫描 worker 及时生成缩略图或将封面以有界任务传走；缓存服务统一读写、原子替换、失败分类、key 版本和清理。超限判定尽量在额外复制前执行，明确解析库自身仍可能先分配。**验收：** 多首大封面导入的峰值 RSS、同 key 并发、损坏缓存、超限图重复扫描、磁盘写入失败都有可观测结果。

### P07 · P2 · 扫描/数据库还有重复工作，但不宜先盲目并行 [代码 / 测量]

位置：[library/src/lib.rs:340](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:340)、[library/src/lib.rs:543](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:543)、[storage/src/lib.rs:359](/mnt/workspace/Projects/Qingyin/crates/storage/src/lib.rs:359)。

- 每文件 metadata + `SELECT modified_at`，语句重复 prepare；可复用 statement 或一次取得 root 范围内指纹映射，权衡内存。
- prune 先 list 全部 metadata，后面 listed_tracks 又读一次；批量移除还逐条自动提交。只查需要的 path/id 并用事务删除即可减少开销。
- upsert 每曲先写再查 id，艺人/搜索项多次准备 SQL；考虑 `RETURNING id`、prepare_cached 和批次复用。
- 批事务失败后逐首重试对坏记录隔离合理，但对锁库/磁盘故障可能产生 50 次相同慢失败；按错误类别决定是否回退，并记录最初错误。
- 首次导入读取/封面编码串行，可能是耗时点，但并行会放大 P06 内存和随机 I/O。只在有基准后尝试小固定并发，SQLite 写入仍集中。

**验收：** 1k/10k 相同库首次与二次扫描分别统计 metadata 次数、SQL 次数、封面读写次数与峰值内存。不要以更高 CPU 占用误认更快。

### P08 · P2 · 全库与播放快照仍有多份深复制 [代码]

位置：[library_session.rs:541](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs:541)、[collections.rs:101](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/collections.rs:101)、[library/src/lib.rs:164](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs:164)、[ui_bridge/src/lib.rs:201](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/lib.rs:201)。

全库到可见列表会 clone 所有 metadata/keys/cover URL；每次双击 snapshot 又复制整张可见列表；歌手和专辑聚合各自创建一套 Arc<TrackMetadata>，只在同一歌手聚合的多歌手分桶之间共享，尚未全局共享。排序为了保持 tracks/covers 对齐，会 take→zip→sort→unzip 重新分配向量。播放上一首/下一首的索引计算本身已是 O(1)，不是瓶颈。

**优化：** 共享不可变 `TrackRecord`/`Arc<LibrarySnapshot>`，可见列表、集合和播放队列存稳定 TrackId 或共享引用及顺序数组；每行把 track 和 cover 组成一个记录，减少并行数组约束。保留“播放队列不会随界面筛选而变化”的现有良好语义。只需要顺序时不要复制所有展示字符串。

Model 的小变化用 insert/remove/dataChanged 精准角色；真正更换全量快照才 reset。排序先改通知语义（C13），再评估把排序索引放 worker，避免后台排序结果覆盖新查询。

**验收：** 10k 曲目双击播放不产生与全部标签字节数线性相关的分配尖峰；重复切歌不会累积队列副本；过滤/排序不改变播放会话顺序。

### P09 · P2/P3 · 后台播放与静态 UI 还有可减小的唤醒 [代码 / 测量]

位置：[player/src/lib.rs:397](/mnt/workspace/Projects/Qingyin/crates/player/src/lib.rs:397)、[playback.rs:39](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/playback.rs:39)、[PlayerBar.qml:184](/mnt/workspace/Projects/Qingyin/qml/PlayerBar.qml:184)。

优点：没有 QML 播放轮询 Timer；进度源仅在 pipeline Playing 时启用，暂停后会移除；watcher 空闲使用阻塞 recv；页面没有常驻粒子/模糊/循环动画。不能据此声称整个应用在播放时零唤醒，也不能把对象多等同于持续重绘。

剩余方向：

- 播放时固定 250ms 查询 position 和 duration，最小化/不可见时也保持同频率。可见时保留交互需要的粒度，后台降低 UI 进度发布频率；音频处理和必要桌面媒体事件不能被暂停。
- duration 通常稳定，可在加载、DurationChanged、seek 确认等时刷新，而非每 tick 查询；需要兼顾初始未知时长。
- position、duration、volume 共用 `playback_progress_changed`，每 tick 使音量/总时长相关绑定也失效。拆细 NOTIFY，仅值变化时通知；时间字符串可只在整秒变化时更新。
- 首次 `ensure_player` 在 UI 线程执行 gst::init、构建元素和等待 Bus ready；`Player::load` 的 is_file/canonicalize/plugin 探测仍在 UI，慢盘/挂载路径上需要测首播延迟。
- `position()`/`duration()` 公共接口最多同步等 100ms，目前 controller 热路径没有使用，不应误报为“每 250ms 阻塞 UI”。后续调用方宜使用推送快照。

**验收：** 无播放静止、暂停、可见播放、最小化播放分别测 CPU/唤醒/Qt 事件与帧次数，确认优化收益且恢复窗口后进度即时准确。

### P10 · P3 · 渲染与绑定的小优化，放在列表虚拟化之后 [代码 / 测量]

- `track_role_data` 每次取 role 会 clone/join/格式化；去掉不必要 String clone 或缓存展示值有空间，但先缩小 delegate 数量，再看 QML profiler/分配结果。
- 每行多层 RowLayout/ColumnLayout 和 clip；固定尺寸行可简化几何计算，裁剪优先放视图边界。不要为每张封面加独立 layer/shader 缓存，可能反而增加纹理与离屏渲染成本。[Qt Quick 性能建议](https://doc.qt.io/qt-6/qtquick-performance.html)
- `persist_settings` 与 `set_dark_theme_internal`、集合更新与 open/close 都存在重复粗粒度 signal；可合并一次发布。它们不会自动造成每帧绘制，但会造成不必要的绑定求值。
- 主窗口仍持有当前播放封面是必要成本；暂停/静止页面没有证据需要额外“每秒检查是否空闲”的 Timer。
- Release 配置可对 LTO/codegen-units/strip 做 A/B，分别衡量启动、RSS、包体与构建时间；不要未经测量直接宣称 `opt-level=z` 对播放器更快，也不要为了微优化默认禁用诊断信息。

## 7. 软件工程细节与收敛方向

| 编号 / 优先级 | 当前问题与证据 | 建议 |
| --- | --- | --- |
| E01 / P2 | LibrarySession 通过多个可选 closure 反向调用 AppBridge，再访问 PlaybackController；`lib.rs:129–182` 与 `library_session.rs:80–89`。编译期依赖虽无环，运行期耦合仍隐蔽 | 用小的 typed host/event 接口表达 play/persist/track-removed，尽量不要在持有 RefCell 可变借用时同步反向借用宿主。无需另建巨型 AppCore；在异步/重入测试后再改 |
| E02 / P2 | tracks 与 cover_urls 多处平行 Vec，resize/unwrap_or_default 静默修补错位；排序、快照、详情都有此约束 | `TrackRow { track, cover }` 或 TrackId→cover 表，构造时保证一致；只在 Qt 越界/无封面这种真实边界使用空值 |
| E03 / P2 | sort column 字符串在 Settings、session、cmp_column、QML 多处重复；设置 version 被 normalize 无条件改为当前版；目录去重散落 Settings/session/watch | Rust 内部 `SortColumn`/`SortOrder` 枚举，serde/QML 边界集中转换；集中 roots 规范化；显式版本迁移或拒绝未来版本。XDG config/data/cache 统一路径提供器并验证环境路径语义 |
| E04 / P3 | theme 通过整个 window 和 var 传入；按钮 contentItem/background 大段重复 | 可提取小 Theme QtObject、统一少量 button/cover 组件；优先修类型元数据再考虑强类型属性。当前主题颜色已集中，不值得为它建立完整设计系统 |
| E05 / P2 | qmllint 依赖的 qmltypes 描述旧接口；Library 无结果才显示 scan_status，有记录时扫描/监听失败可能只留日志；添加按钮未按 scanning 禁用，后台静默忽略重复提交 | 同步或生成类型描述，CI 将预期类别 warning 视为失败；让现有状态区在需要时可见并绑定交互可用性，不添加多余页面和说明文案 |
| E06 / P3 | 少数纯包装函数没有提供额外抽象：`MusicLibrary::scan_directory/refresh_path` 内 `let _=self`，`format_count` 只格式化；`TrackCollationKeys.artist` 当前无比较路径使用 | 移除无用实例状态/接口或改关联函数；只内联一次性且无语义价值的包装。已有关键域函数、错误转换和 Qt 宏转发可保留；`#[inline]` 交给测量决定，`const fn` 不等于运行时提速 |
| E07 / P2 | 入口使用编译机 `CARGO_MANIFEST_DIR` 拼源码 QML 绝对路径；未验证根对象加载失败；缺安装/打包/CI 清单 | Qt resource/qrc 嵌入或明确安装资源定位；加载失败返回非零并给错误。确认 1.92 MSRV 的真实构建。Cargo.lock 已跟踪，workspace deps/lints 已集中，不需要重复治理 |
| E08 / P2 | 声称通过的测试中有机器特定音频路径，不存在时直接 return，测试仍显示 passed；storage 索引测试查了不同 SQL；大部分 async controller 无行为测试 | 固定小型生成式/许可明确音频 fixture；明确 optional integration skip；测试真实查询与状态交错，不用测试实现的几行复制逻辑冒充覆盖率 |
| E09 / P3 | README/状态/历史报告不一致：metadata 依赖、启动线程、启动缺缓存行为；指针报告将前后多种假设写成最终因果；未接入 Widgets 文件仍在工作区 | 以当前代码与可复现证据更新架构/验证矩阵；历史记录标明版本和“假设/复现/已验证”。询问是否保留实验代码只在未来清理时需要，本次不删除 |
| E10 / P3 | Qt bridge snake_case 对应部分 QML camelCase，`libraryModel` 实际传会话，`play_in_flight` 不是后端完成；大量 public 辅助方法只在 crate 内用 | 内部收窄为 pub(crate)/私有；命名表达 Session/Model/RequestedState；跨语言边界可以各遵惯例，不必全仓机械改名 |

### 应保留的防御与不应新增的抽象

- **应保留：** QPointer 回调生命周期检查、Qt index 边界检查、SQL 参数绑定、图像尺寸/内存限制、音量非有限值校验、事务和旧搜索结果拒绝。它们是实际边界，不属于过度防御。
- **应改掉：** 任意 I/O 错误→删除记录、配置损坏→清库、播放器失败→仅日志、并行数组错位→静默补空、当前总是 Ok 的 Result 及调用端不可能命中的错误分支。重点是让状态契约准确，不是简单减少 if。
- **不必做：** 为全部代码加 trait、引入 Tokio 统一所有线程、给每个短函数加 inline、以宏合并所有 QObject、把现有拼音排序改成 QML 字符串比较。固定 worker + 清楚事件类型已足够解决当前主要问题。
- `Arc<Mutex<...>>` 在 GLib Send 回调约束下可能是桥接所需；先检查线程 API 要求，不要仅因“只有一条工作线程”就盲目替换为 Rc/RefCell。
- 本次核对了 glib 0.22.9 的 `with_thread_default`：闭包期间持有 context acquire guard。当前 `run_bus_loop` 在此范围内发送 ready 并运行 loop；不能仅根据 invoke 有可能内联这一条 API 说明就断言正常播放又回到了 Qt 线程。

## 8. 验证结果与证据等级

### 8.1 本次实际执行

环境：Linux，Rust/Cargo 1.98.0，pkg-config 报 Qt Quick 6.12.0、GStreamer 1.28.7。workspace 宣告 MSRV 1.92，**本次未切换到 1.92 验证**。查询到的 Qt 在线文档页版本为 6.11.2；本报告使用的 Repeater、ListView、Image 与模型通知基础能力也在本机代码/运行中核对，未假设某个仅新版提供的接口可用。

| 检查 | 结果 | 能说明什么 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 通过 | 格式符合当前规则 |
| `cargo test --workspace` | 58 passed、0 failed、0 ignored；doc tests 0 | 已有测试在此环境通过，不等于 async/GUI/性能有覆盖 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 当前 Rust 静态规则通过 |
| `/usr/lib/qt6/bin/qmllint -I qml qml/*.qml` | exit 0，**4 条 warning** | Main.qml:268 playback；277/285/293 library，qmltypes 缺失当前属性 |
| 隔离 Rust 路径/扫描/配置探针 | 见下表 | 调用当前 crate 公共 API，未改实现 |
| 原始搜索 SQL 的 EXPLAIN | `SCAN search_terms`，GROUP/ORDER 临时 B-tree | 当前查询计划不受等值索引测试保证 |
| 实际 TrackTable 离屏 1,000 行探针 | 1,000 行全部实例化 | 委托不是按视口加载；不代表真实 GPU/FPS 数据 |
| 实际 PlayerBar 离屏键盘探针 | 右方向键从 30,000ms 提交的仍为 30,000ms；QtTest 通过缺陷断言 | 默认 0.1ms 步长被整数舍入吞掉；不是不支持 pressed 事件 |

测试分布：chinese 7、core 3、library 17、metadata 4、player 5、storage 9、ui_bridge 13。`workspace_flac_has_embedded_cover_on_some_tag` 有文件不存在直接返回的分支，因此不能将 58 passed 描述成“58 项真实媒体场景均已执行”。

### 8.2 隔离探针输出与复现步骤

临时探针位于本次会话 `/tmp/qingyin-quality-audit-09f7pvcl/`；这是临时证据，不是交付依赖，也不是新增生产测试。

```text
case_sensitive_delete: removed=2 remaining=0
offline_deletion: discovered=0 pruned=0 remaining=1
permission_refresh: result=Ok(Removed(1)) remaining=0
invalid_settings_then_prune: roots=0 removed=1
pinyin 快乐: sort=kuaiyue search=kuaile
pinyin 单纯: sort=shanchun search=danchun
pinyin 曾经: sort=zengjing search=cengjing
```

| 探针 | 重做方式 | 期望修复后结果 |
| --- | --- | --- |
| 大小写删除 | 内存 Database 插入 `/music/Album/a.wav` 与 `/music/album/b.wav`，调用 remove_tracks_under(`/music/Album`) | removed=1，保留小写目录 |
| 离线删除 | 临时空目录，库中预存其下不存在文件；scan_directory 后 prune_unwatched_tracks | 完整扫描确认后删除该陈旧记录 |
| 权限错误 | 非 root 身份，父目录有预存记录，子目录 chmod 000；refresh_watched_path 父目录；随后恢复权限 | 返回/累计读取失败，保留记录 |
| 配置回退 | 写损坏 TOML，load_from 失败后取 default（对应 load_or_default 的回退）；将 roots 传 prune | 应在编排层拒绝因读取失败执行 prune |
| 中文语义 | 对相同输入分别调用 sort_key(text,None)、search_key(text) | 按字段语境正确发音，差异可解释 |
| QML 实例数 | 离屏 Basic/software 加载实际 TrackTable，ListModel 填 1,000 项，遍历 children 统计含 lastClickAt 的行 | 改为 ListView 后只创建视口/缓冲所需数量 |
| 键盘 seek | QtTest 加载实际 PlayerBar 与 fake backend，position=30000、duration=180000；聚焦进度 Slider，keyClick(Qt.Key_Right)，检查 seek_to 参数 | 前进约定步长，不再次提交原位置 |

离屏探针使用 `QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_QUICK_CONTROLS_STYLE=Basic`，统计值等于 1,000 时显式 `Qt.exit(42)`，本次退出码为 42。退出码是探针断言成功约定，不是应用启动异常；不以被环境过滤的 console 日志作为成功证据。

## 9. 建议的实施顺序与验收基线

### 第一批：修正记录与状态契约

1. C01/C02/C03：阻止误删曲库记录；用隔离 fixture 固化失败案例。
2. C04/C05/C06：事务迁移、完整扫描对账、文件指纹和局部错误隔离一起设计，避免修复离线删除又引入权限误删。
3. C07/C08/C09：播放命令确认、统一曲库 revision、任务关闭协议。先提取可测状态逻辑，再接真实 Qt/GStreamer。

### 第二批：直接改善轻量和交互

1. P01：ListView/GridView 虚拟化，并保持输入回归测试。
2. P02/P03/P06：监听整条 I/O 管线后台化，先文字首屏、后可见封面；限制图片驻留字节。
3. P08/C13：共享数据与稳定身份，模型增量通知及正确排序语义。
4. C10/P09：设置写入合并、NOTIFY 拆分、不可见窗口的进度发布节流。

### 第三批：由数据决定的优化与工程整理

真实 SQL 基准→选择检索方案；扫描 statement/事务复用；专辑封面去重；主题/公共控件与命名收敛；类型元数据、CI、资源打包。P3 风格工作不应阻塞前两批。

### 性能验收矩阵（建议目标，非本次测得）

固定 release 构建、同一机器/Qt/GStreamer/音频输出/DPR、相同曲库和存储介质。分别测试 1k/10k/50k 曲目；音频扫描用真实或生成的有效音频，UI 对象测试可用合成 metadata。冷缓存与热缓存分开，不在用户机器上全局 drop_caches。

| 场景 | 记录指标 | 验收重点 |
| --- | --- | --- |
| 首次导入、二次核对 | 时间、峰值 RSS、音频读取次数、SQL/封面写入数 | 二次扫描无需全量重解析；图片内存受限 |
| 冷/热启动 | 窗口出现、首屏可交互、第一批封面时间 | 首屏不等待全库封面 |
| 10k 列表滚动/主题切换 | delegate 数、QML profiler、主/渲染线程帧时间 | 活对象数与视口相关；60Hz 场景以 16.7ms 帧预算定位长帧 |
| 搜索/清空/排序 | p50/p95、UI 最大连续阻塞、分配量 | 建议普通 UI 主线程任务尽量 <8ms，避免 >50ms 明显停顿；不是已验证 SLA |
| 修改 1/50/1,000 文件 | 更新延迟、watcher 队列峰值、重读次数 | 局部变更不触发 UI 全库 I/O；持续事件不饿死刷新 |
| 无播放静止/暂停 | CPU、线程唤醒、帧提交、文件 I/O | 无业务驱动持续计时器/写盘；不承诺整进程绝对零 CPU |
| 可见/最小化播放 | CPU、唤醒、位置事件数、音频稳定性 | 后台 UI 通知下降，音频与恢复窗口的进度正确 |
| 快速切歌/失效曲目/退出 | 命令 ID、真实状态、UI 状态、退出时长 | 无旧事件串曲，无无期限 UI join |

测量工具可使用 QML Profiler、应用 tracing span、`perf stat`/`pidstat`、`/proc/<pid>/smaps_rollup` 和 Qt scene graph 诊断。先记录基线再优化；不能用 debug 构建的时间与 release 比较，也不要同时开大量诊断输出污染输入/渲染测量。

## 10. 不应再次当作未修复问题的事项

与历史报告相比，以下当前已经成立，继续保留即可：

- SQLite 已启用 WAL、5 秒 busy timeout、外键；list/search 已用 JOIN 获取艺人，日常读取不再是旧版 N+1。
- 主动扫描按 50 首批提交，失败时可退回单条；不是“每首都独立事务”。
- 元数据已与中文键解耦，TrackSnapshot 预先计算排序键；日常比较不再反复汉字转拼音。
- 歌手/专辑使用 HashMap 分组，多歌手分组共享 Arc；不是原先线性查找分桶，但仍有 P08 所述跨视图复制。
- AppBridge 已拆分 LibrarySession/PlaybackController/TrackListModel，公共角色映射已收敛。
- 播放正常路径的状态切换已移到 GLib context；Bus watch 替代 timed_pop 轮询，暂停移除进度源。
- QML 已使用页面 Loader、required property、ComponentBehavior: Bound、图片异步加载，没有默认加入大范围复杂效果。
- `Cargo.lock` 已跟踪、依赖版本和 lints 已 workspace 化；未看到理由为此强加新构建体系。

下一轮最值得投入的不是全面重写，而是把已经建立的模块边界兑现为**准确的状态协议、受控的后台任务和按需创建的界面对象**。
