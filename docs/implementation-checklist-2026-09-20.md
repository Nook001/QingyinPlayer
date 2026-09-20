# Qingyin 原子改动清单

状态仅使用 `- [ ]` 未完成、`- [x]` 完成。每个编号对应一个可独立验收的改动；实现、必要验证和完成条件全部满足后才勾选，未全部满足时保持未完成。提交说明引用编号；若需继续拆分，先用子编号替换原项，不保留“部分完成”状态。前置编号必须先完成；同一项允许包含实现该行为所需的跨文件修改和回归测试。

按 P1 → P2 → P3 执行，依赖关系优先。使用隔离数据库、配置和媒体 fixture 验证；不操作用户真实曲库。Rust 改动按影响范围执行测试、格式检查与 Clippy，QML 改动执行 qmllint 和对应交互验证。本轮清单项均已落地并可验收。

## 1. 曲库记录与持久化

入口：[library](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs)、[storage](/mnt/workspace/Projects/Qingyin/crates/storage/src/lib.rs)、[core](/mnt/workspace/Projects/Qingyin/crates/core/src/lib.rs)、[library_session](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs)。

- [x] **DATA-01 · P1** 限制目录删除判定：权限错误、子目录读取失败和临时 I/O 错误不得触发前缀删除；根目录不可用时保留记录。完成：权限失败保留记录，已确认在线根下的真实目录删除正常清理。
- [x] **DATA-02 · P1** 将目录前缀删除改为大小写敏感、带路径分隔边界的匹配。完成：大小写、`%`、`_`、`[`、中文及 `a/ab` 目录回归通过。
- [x] **DATA-03 · P1** 区分配置成功读取、缺失和读取失败；启动恢复不因默认空 roots 清库，清理仅由明确目录配置变更触发。完成：损坏、缺失、不可读配置保留已有记录，显式移除目录仍生效。
- [x] **DATA-04 · P1** 将每个 schema 版本升级及其数据重建放入事务，成功后更新 `user_version`。完成：中途失败可回滚并重试，已有不完整旧 schema 能修复或明确报错。
- [x] **DATA-05 · P1** 在启动工作任务前完成迁移；数据库层用事务锁协调并发迁移，并拒绝未来 schema 版本。完成：双连接冷启动不会重复升级，未来版本不会被写入。前置：DATA-04。
- [x] **DATA-06 · P2** 为扫描结果记录完整枚举的根和失败子树；单文件消失或读取失败累计到摘要并继续。完成：正常文件仍提交，数据库整体失败明确中止，失败范围可被调用方识别。前置：DATA-01。
- [x] **DATA-07 · P2** 为完整扫描成功的根增加存量集合对账。完成：离线删除、重命名可清理；不可用根或枚举失败的根不执行删除。前置：DATA-02、DATA-06。
- [x] **DATA-08 · P2** 将文件指纹统一为高精度 mtime 与文件大小，并迁移存储字段及全部比较调用。完成：同秒不同纳秒修改和文件大小变化触发重解析，旧库可升级。前置：DATA-04。
- [x] **DATA-09 · P2** 统一配置、扫描和监听使用的 roots 规范化规则，保留离线根身份并消除有效根的重复、嵌套覆盖。完成：相同目录不重复扫描，大小写不同目录不误合并，离线根不因规范化失败被移除。
- [x] **DATA-10 · P2** 将配置保存改为同目录临时文件写入后原子替换，清理失败临时文件。完成：模拟写入失败保留旧配置，保存成功可完整读回。
- [x] **DATA-11 · P2** 明确设置版本迁移，移除 normalize 无条件改写版本的逻辑；拒绝覆盖未知未来版本。完成：旧版迁移、缺省版本、未来版本的读取和保存行为均有验证。

## 2. 播放命令与状态

入口：[player](/mnt/workspace/Projects/Qingyin/crates/player/src/lib.rs)、[playback](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/playback.rs)、[PlayerBar](/mnt/workspace/Projects/Qingyin/qml/PlayerBar.qml)。

- [x] **PLAY-01 · P1** 为异步播放命令提供明确的接收结果与完成/失败事件，携带命令 ID；接入 controller 错误处理。完成：`set_state`、seek 失败均可被上层观察，API 与文档不再将入队成功当执行成功。
- [x] **PLAY-02 · P1** 分离请求状态与后端确认状态，以 pipeline 状态事件更新 Player 和 controller；用实际确认结束 pending。完成：加载、播放、暂停、停止交错时 UI 与后端状态一致。前置：PLAY-01。
- [x] **PLAY-03 · P1** 为加载会话及其 Progress/Error/EOS 事件绑定世代，在后端和 Qt 发布端拒绝过期事件。完成：可控 A→B→C 乱序测试中，旧进度、旧错误和旧 EOS 不影响 C；不得将旧 Bus 消息简单标成最新世代。前置：PLAY-01。
- [x] **PLAY-04 · P1** 统一加载失败行为：停止旧播放、清理旧曲进度并发布一致的错误状态；停止失败也须回传实际状态。完成：切换到缺失或不可解码文件时，不出现旧音频继续播放而 UI 已停止。前置：PLAY-02、PLAY-03。
- [x] **PLAY-05 · P2** 将缺失曲目跳过改为队列内有界查找。完成：连续缺失项跳到下一可用曲目，全队列失效时停止且不死循环。前置：PLAY-04。
- [x] **PLAY-06 · P2** 将首次 GStreamer 初始化、元素创建、路径检查及插件检查移出 Qt 主线程，以事件返回准备结果。完成：慢初始化/慢路径探针期间 UI 可响应，正常播放仍在既有 GLib context 执行。前置：PLAY-01。
- [x] **PLAY-07 · P2** 为播放器 ready 和关闭建立显式确认与期限，移除 Qt 主线程无期限 recv/join。完成：后端不响应时按明确超时策略报告并退出关闭流程，超时后不再次无期限 join。前置：PLAY-06。
- [x] **PLAY-08 · P2** 将进度条键盘左右步长设为 5 秒，保持鼠标连续拖动和起止位置限制。完成：单次按键确实 seek，后台进度刷新不触发反向 seek。
- [x] **PLAY-09 · P2** 将时长查询改为加载及 DurationChanged 等事件驱动，初始未知时长使用有界补查。完成：已知时长不再每 250ms 重查，未知时长最终能更新。前置：PLAY-02。
- [x] **PLAY-10 · P2** 为位置、时长和音量拆分 NOTIFY，并仅在对应值变化时发出；展示时间按整秒更新。完成：位置 tick 不使音量和总时长绑定重复求值。
- [x] **PLAY-11 · P2** 在窗口不可见或最小化时降低 UI 进度发布频率，恢复可见时立即同步。完成：后台通知减少，音频、EOS 和错误事件正常，暂停不新增轮询。前置：PLAY-09、PLAY-10。

## 3. 后台任务与目录监听

入口：[library_session](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs)、[watch](/mnt/workspace/Projects/Qingyin/crates/library/src/watch.rs)、[ui_bridge](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/lib.rs)。

- [x] **ASYNC-01 · P1** 为 restore、scan、watch 的结果统一附加 roots 世代和曲库 revision，拒绝过期发布；被覆盖的变更必须触发最新快照重取。完成：控制完成顺序后，最终模型始终对应最新配置与已提交数据。
- [x] **ASYNC-02 · P1** 将监听后的数据库读取、排序键计算、封面处理和集合聚合整体放入 worker，Qt 仅应用结果。完成：监听刷新期间 Qt 线程无 SQLite、Lofty、封面解码或编码调用。前置：ASYNC-01。
- [x] **ASYNC-03 · P2** 为搜索结果同时校验 query 世代和曲库 revision，并在曲库提交后统一重跑当前非空查询。完成：扫描新增/移除歌曲后结果准确，旧查询不可覆盖新结果。前置：ASYNC-01。
- [x] **ASYNC-04 · P2** 将搜索改为单个长期 worker 与只保留最新待执行查询的有界邮箱，在可取消边界停止旧任务。完成：连续输入不随查询次数创建线程，取消后不再发布旧结果。前置：ASYNC-03。
- [x] **ASYNC-05 · P2** 统一 restore 与 scan 的忙碌状态和互斥入口，使 UI 可查询实际任务状态。完成：启动恢复期间导入有确定行为，不会启动互相覆盖的扫描。前置：ASYNC-01。
- [x] **ASYNC-06 · P2** 为扫描、恢复、搜索 worker 统一登记生命周期并响应 shutdown；批处理边界检查取消。完成：关闭后不提交新任务，不发布到失效 QObject，已有句柄有明确回收路径。前置：ASYNC-04、ASYNC-05。
- [x] **WATCH-01 · P2** 增加“安静窗口 + 最大等待时间”的双截止刷新。完成：持续文件事件仍按最大等待时间发布，不会一直推迟。
- [x] **WATCH-02 · P2** 将 watcher 事件邮箱改为有界合并结构，溢出只登记一次待对账请求。完成：压力测试队列不超界，溢出后的最终曲库与完整扫描一致。前置：DATA-07、WATCH-01。
- [x] **WATCH-03 · P2** 合并父子路径覆盖，复用 roots 与排除规则，避免事件回调逐条 canonicalize/is_dir。完成：同批父目录和子文件事件只扫描一次有效范围。前置：DATA-09。
- [x] **WATCH-04 · P2** 将监听成功和失败的根作为结构化状态回传，全失败不得报告监听正常。完成：单根失败、部分失败和全部失败都能被 UI 状态层识别。
- [x] **WATCH-05 · P2** 接入事件丢失/rescan 提示与根恢复监听策略，恢复后执行一次合并对账。完成：临时离线保留记录，重新挂载后重新监听并同步；重试有退避且可取消。前置：DATA-07、WATCH-02、WATCH-04。
- [x] **WATCH-06 · P2** 将 watcher 替换和关闭等待移到后台协调，停止请求可在扫描批次边界生效。完成：锁库或扫描中重建 watcher 时 Qt 不阻塞，旧 watcher 退出后不再发布。前置：ASYNC-06。

## 4. 列表模型与交互

入口：[collections](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/collections.rs)、[TrackTable](/mnt/workspace/Projects/Qingyin/qml/TrackTable.qml)、[CollectionBrowser](/mnt/workspace/Projects/Qingyin/qml/CollectionBrowser.qml)、[Library](/mnt/workspace/Projects/Qingyin/qml/Library.qml)。

- [x] **UI-01 · P1** 将歌曲 Repeater 替换为固定行高 ListView，启用 reuseItems 和有界缓冲，重置池化/复用后的局部状态。完成：1k/10k/50k 数据下活动 delegate 数与视口相关，滚动不串封面、不误触播放。
- [x] **UI-02 · P1** 将集合 Repeater 替换为 GridView，保持现有布局与导航行为。完成：活动 delegate 不随集合总数线性增长，调整窗口后点击和滚动正常。
- [x] **MODEL-01 · P2** 用 TrackRow 或稳定 ID 关联封面，替换 tracks/cover_urls 平行数组及静默补齐逻辑。完成：排序、筛选、详情、播放快照均无数组错位兜底。
- [x] **MODEL-02 · P2** 为模型排序发送正确的布局/移动通知并重映射持久索引。完成：排序后选择仍指向同一 TrackId，通过模型一致性验证。前置：MODEL-01。
- [x] **UI-03 · P2** 用标准双击机制替换固定 400ms 手写判定；延迟播放/打开操作携带稳定 TrackId/CollectionId。完成：排序、刷新、复用和切页夹在点击之间时不操作错误对象，必要的重入隔离保留。前置：UI-01、UI-02、MODEL-02、META-04。
- [x] **UI-04 · P2** 将曲库查询与滚动锚点存入页面外状态，并在 Loader 重建后恢复；离页前提交尚未生效的查询末值。完成：输入后立即切页、查询中切页和返回均保持筛选一致，缺失锚点有确定回退。
- [x] **UI-05 · P2** 用 Loader 按集合/详情模式卸载非活动视图。完成：进入详情后原网格对象释放，返回恢复原集合滚动锚点。前置：UI-02。
- [x] **UI-06 · P2** 让现有状态区域在曲库非空时也能显示扫描/监听失败，并按忙碌状态禁用重复导入。完成：有歌曲时错误可见，按钮可用性与实际任务一致。前置：ASYNC-05、WATCH-04。
- [x] **MODEL-03 · P2** 建立共享不可变曲目记录，使曲库、歌手和专辑视图使用相同记录，排序仅移动引用或 ID。完成：跨视图不再重复复制整份标签数据。前置：MODEL-01。
- [x] **MODEL-04 · P2** 将播放队列快照改为共享记录加固定顺序，去掉双击时的整表 metadata 深复制。完成：10k 曲目建队列无标签字节量级复制，后续筛选/排序不改变播放顺序。前置：MODEL-03。
- [x] **MODEL-05 · P2** 为监听结果提供变更/删除 ID 集合，模型按局部插入、删除和准确角色通知更新；仅整库替换使用 reset。完成：修改一首歌不触发整库重读和模型 reset，最终模型与数据库一致。前置：ASYNC-02、MODEL-02、MODEL-03。

## 5. 封面读取与缓存

入口：[metadata](/mnt/workspace/Projects/Qingyin/crates/metadata/src/lib.rs)、[library](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs)、[library_session](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs)、[PlayerBar](/mnt/workspace/Projects/Qingyin/qml/PlayerBar.qml)。

- [x] **ART-01 · P1** 为各处封面 Image 按显示尺寸和 DPR 设置固定档位 sourceSize。完成：1×/2×DPR 清晰，微小窗口尺寸变化不反复重载图片。
- [x] **ART-02 · P2** 将封面缓存读写和路径策略提取为 Qt 无关服务，保持现有调用行为。完成：缓存逻辑不依赖 QObject，worker 与会话共用同一实现。
- [x] **ART-03 · P2** 为封面请求建立去重、可取消、有并发及队列上限的任务调度，优先处理当前播放和可见项。完成：快速滚动不无限积压，同 key 不重复解码。前置：ART-02、ASYNC-06。
- [x] **ART-04 · P1** 启动先发布文字及已有缓存，再通过调度器补缺失封面。完成：清空隔离缓存后首屏可交互不等待全库封面，补图按 ID 精确更新。前置：ART-03、MODEL-01。
- [x] **ART-05 · P2** 在封面额外复制前检查压缩字节上限，并对在途图片累计字节实行背压，将其与数据库批次大小解耦。完成：多首大封面导入不累计整批原图，限制涵盖排队与处理中图片，保留解码尺寸上限。前置：ART-03。
- [x] **ART-06 · P2** 在增量刷新路径复用首次解析得到的封面，直接提交缓存服务。完成：单次标签变更不会为缺缓存再次解析同一音频。前置：ART-02、ART-05。
- [x] **ART-07 · P2** 为纯标签和纯封面读取设置最小必要的 Lofty ParseOptions。完成：纯标签读取不复制封面，纯封面读取跳过无关属性解析，已有格式结果不回退。
- [x] **ART-08 · P2** 优先读取 FrontCover，无前封面时采用固定回退顺序。完成：多图片标签稳定选中预期图片。
- [x] **ART-09 · P2** 缓存文件使用临时文件加原子替换；识别损坏缓存并使其重新生成。完成：并发请求和中断写入不留下永久命中的坏图。前置：ART-02、ART-03。
- [x] **ART-10 · P2** 为无图、超限、解码失败和临时 I/O 失败建立不同缓存/重试状态。完成：同指纹永久失败不反复回源，临时失败可退避重试，文件改变后可重新生成。前置：ART-02、ART-03。
- [x] **ART-11 · P2** 为缓存键引入明确版本和稳定摘要算法，统一使用高精度文件指纹。完成：旧键可安全失效，修改标签/封面不会命中过期文件。前置：DATA-08、ART-02。
- [x] **ART-12 · P2** 按图片内容摘要共享缓存文件和 URL，在首次解析时计算并保存映射。完成：同图曲目复用缓存，启动命中时不为摘要回读音频。前置：ART-11。
- [x] **ART-13 · P2** 增加可配置缓存容量和过期文件清理，后台执行并保护在用项。完成：达到容量后回收旧图，当前播放与可见封面正常，清理不扫描用户音频目录。前置：ART-09、ART-11、ART-12。

## 6. 元数据与排序语义

入口：[metadata](/mnt/workspace/Projects/Qingyin/crates/metadata/src/lib.rs)、[chinese](/mnt/workspace/Projects/Qingyin/crates/chinese/src/lib.rs)、[storage](/mnt/workspace/Projects/Qingyin/crates/storage/src/lib.rs)、[library](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs)。

- [x] **META-01 · P2** 统一排序与搜索的中文读音入口，显式区分标题和人名语境，保留 sort 标签优先。完成：快乐、音乐、单纯、曾经不被姓氏规则覆盖，人名、日文和已有标签测试通过。
- [x] **META-02 · P2** 从格式的实际多值 artist 标签读取有序歌手列表，不按斜杠机械拆分。完成：多歌手可持久化、搜索、聚合，含斜杠的单一名称不被拆开。
- [x] **META-03 · P2** 增加 album artist、disc number、track number 的解析、存储迁移及读回。完成：新旧数据库均可用，多碟和缺标签媒体正确往返。前置：DATA-04。
- [x] **META-04 · P2** 用稳定 AlbumKey 表达专辑身份，以专辑名及 album artist 分组，缺失值用显式类型并定义固定回退；向模型暴露稳定集合 ID。完成：同名不同歌手专辑分离，合辑按 album artist 聚合，未知值不与真实同名字符串混淆。前置：META-03。
- [x] **META-05 · P2** 将专辑详情默认顺序改为 disc/track，再以标题和稳定身份兜底。完成：双碟专辑按曲序播放，缺失编号顺序确定。前置：META-03、META-04。
- [x] **META-06 · P2** 为歌手和专辑集合排序增加名称及稳定身份的最终比较规则。完成：相同校对键的集合在重载、重建后顺序稳定。前置：META-04。

## 7. 搜索与数据库开销

入口：[storage](/mnt/workspace/Projects/Qingyin/crates/storage/src/lib.rs)、[library](/mnt/workspace/Projects/Qingyin/crates/library/src/lib.rs)、[library_session](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/library_session.rs)。

- [x] **DB-01 · P2** 在搜索 worker 内复用只读用途连接和 prepared statements，迁移仅由初始化执行。完成：连续搜索不重复 open/migrate，连接不跨线程误用。前置：DATA-05、ASYNC-04。
- [x] **DB-02 · P2** 为搜索结果建立 TrackId/path 到记录及封面的直接索引。完成：应用 K 条结果不执行 K 次全库线性查找。前置：MODEL-01。
- [x] **DB-03 · P2** 将 roots 范围约束放到搜索 SQL 的 LIMIT 之前，保持路径大小写及目录边界语义。完成：无效根记录不占有效结果额度。前置：DATA-02、DATA-09。
- [x] **DB-04 · P2** 搜索返回显式 has_more，并在现有结果状态中简洁标明截断。完成：满额度但无更多结果时不误报，超额度结果不被当作完整集合。
- [x] **DB-05 · P2** 将索引测试改为验证生产实际使用的搜索 SQL，复用同一查询入口。完成：同时断言搜索结果与排序；查询计划只能用于对应查询，不再用等值 SQL 代替子串 SQL。
- [x] **DB-06 · P2** 扫描使用复用 statement 或根内指纹映射查询变更，并复用 upsert/艺人/搜索项语句。完成：二次扫描结果一致，SQL prepare 次数不随歌曲数重复增长。
- [x] **DB-07 · P2** 将 upsert 后单独查 ID 改为同一写入语句返回 ID。完成：新增和冲突更新均返回正确稳定 ID，减少一次查询。
- [x] **DB-08 · P2** 清理仅查询所需 ID/path，删除使用批事务，去掉 prune 后重复整库 metadata 读取。完成：根清理准确，批删除不逐条自动提交。前置：DATA-07。
- [x] **DB-09 · P2** 按错误类别决定批写失败是否逐条回退，保留最初错误。完成：坏记录可隔离，锁库、磁盘故障不对整批逐首重复慢重试。

## 8. 设置、接口与局部整理

入口：[core](/mnt/workspace/Projects/Qingyin/crates/core/src/lib.rs)、[ui_bridge](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/lib.rs)、[playback](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/playback.rs)、[Main](/mnt/workspace/Projects/Qingyin/qml/Main.qml)。

- [x] **ENG-01 · P2** 设置持久化改为后台单写者、末值合并；拖动结束和退出时 flush。完成：音量实时生效，连续滑动写入有界，退出保留最终值，保存错误可见。前置：DATA-10、ASYNC-06。
- [x] **ENG-02 · P2** 将 SortColumn/SortOrder 收敛为 Rust 枚举，仅在 serde/QML 边界转换。完成：设置、比较函数和 UI 使用同一映射，未知配置有确定回退。
- [x] **ENG-03 · P2** 统一 XDG config/data/cache 路径提供器及环境变量处理。完成：默认路径、合法覆盖、空值和相对路径有一致规则，调用方不再各自拼接。
- [x] **ENG-04 · P2** 用小型类型化事件接口替换 LibrarySession 的多组可选宿主回调，调用后再发布宿主事件。完成：play/persist/track-removed 语义明确，可控重入测试不发生 RefCell 重借用。前置：ASYNC-01、PLAY-01。
- [x] **ENG-05 · P3** 合并主题切换、设置保存、集合打开/关闭路径的重复属性通知。完成：一次实际变化只通知一次，无变化不通知。
- [x] **ENG-06 · P3** 将主题值从整个 window 引用提取到轻量 Theme QtObject，并更新消费者。完成：明暗主题视觉一致，组件不依赖窗口无关属性。
- [x] **ENG-07 · P3** 提取现有重复按钮样式为一个小型公共组件。完成：重复 contentItem/background 收敛，尺寸、焦点、悬停和点击行为不变。
- [x] **ENG-08 · P3** 提取封面展示公共组件，统一占位和 sourceSize 档位。完成：列表、集合、播放器继续显示原有尺寸与样式。前置：ART-01。
- [x] **ENG-09 · P3** 将无实例语义的 MusicLibrary 包装改为关联函数或直接调用，删除仅做格式化的无价值转发。完成：调用点同步简化，领域边界和错误语义不变。
- [x] **ENG-10 · P3** 删除确认无消费者的 TrackCollationKeys 字段及对应计算。完成：搜索、排序、聚合结果保持，构造不再计算未使用键。前置：META-01。
- [x] **ENG-11 · P3** 修正 Session/Model、请求/确认状态相关命名，收窄 crate 内部辅助接口可见性。完成：QML 和 Rust 调用同步更新，公开接口仅保留实际跨 crate 使用项。前置：PLAY-02、ENG-04。

## 9. 类型检查、测试与交付

入口：[qmltypes](/mnt/workspace/Projects/Qingyin/qml/Qingyin/plugins.qmltypes)、[main](/mnt/workspace/Projects/Qingyin/crates/ui_bridge/src/main.rs)、[Cargo.toml](/mnt/workspace/Projects/Qingyin/Cargo.toml)、[README](/mnt/workspace/Projects/Qingyin/README.md)。

- [x] **QA-01 · P2** 同步 QML 类型元数据中的 AppBridge、LibrarySession、PlaybackController 与模型接口。完成：当前四条缺失属性警告消失，新增/调整暴露属性同步维护。
- [x] **QA-02 · P2** 用仓库内生成式或许可明确的小媒体 fixture 替换机器特定音频路径测试。完成：默认测试真实执行断言，可选环境测试明确 ignored/skip，不再文件缺失直接假通过。
- [x] **QA-03 · P1** 建立可控播放 fake backend，固化命令失败、乱序状态、过期 EOS 和连续缺失曲目的 controller 回归。完成：测试无需真实音频设备且能稳定复现对应失败。前置：PLAY-01、PLAY-02、PLAY-03、PLAY-05。
- [x] **QA-04 · P2** 增加实际 GStreamer 与 fakesink 的播放/暂停/切歌/错误/关闭集成验证。完成：生成媒体可重复运行，校验后端实际事件而非仅 fake 事件。前置：PLAY-07、QA-02。
- [x] **QA-05 · P2** 建立 QML 交互回归入口，覆盖列表复用、排序后双击、切页、键盘 seek 与拖动。完成：离屏自动检查可重复运行，实际桌面鼠标/触摸板输入验证有记录。前置：UI-03、UI-04、PLAY-08。
- [x] **BUILD-01 · P2** 将发布运行所需 QML 资源嵌入 Qt resource，移除编译机源码绝对路径依赖。完成：移走源码目录后构建产物仍能加载全部页面。
- [x] **BUILD-02 · P2** 检查 QML 根对象加载结果，失败时输出诊断并返回非零退出码。完成：损坏资源路径能可靠报错退出。
- [x] **BUILD-03 · P2** 增加明确的 Linux 安装/打包入口，收敛可执行文件、资源位置及 Qt/GStreamer 运行依赖说明。完成：在干净测试前缀安装、启动、卸载可重复执行。前置：BUILD-01、BUILD-02。
- [x] **BUILD-04 · P2** 添加 CI 的 fmt、workspace 测试、Clippy 和 qmllint 检查，使 QML 警告类别按明确规则导致失败。完成：故意引入缺失属性能使检查失败，正常代码通过。前置：QA-01、QA-02。
- [x] **BUILD-05 · P2** 验证声明的 Rust 1.92 MSRV，并将该版本构建纳入 CI。完成：锁定依赖在声明版本可构建；若确需提高版本，同步更新声明与用户要求。前置：BUILD-04。
- [x] **DOC-01 · P3** 更新当前架构、开发状态和运行说明中的 crate 依赖、线程边界、缓存首屏与关闭行为。完成：文字和图与届时代码一致，已有历史评估不改写成当前结果。
- [x] **DOC-02 · P3** 为历史指针假死记录补明确版本和证据状态，区分假设、已复现与已验证修复。完成：未验证推测不作为结论；未接入 Widgets 实验文件不纳入本轮实现或删除。

## 10. 测量任务

测量任务交付可重复脚本、固定数据规模和结果文件；没有证据的优化不直接修改实现。测量确认需要改动时，新增独立编号、具体改法和完成条件后实施。

- [x] **PERF-01 · P2** 建立 1k/10k/50k 曲库基准与关键 tracing span，记录扫描、搜索、首屏、封面、模型发布的时间和分配/内存指标。完成：冷/热缓存区分，运行环境可追溯，追踪不默认大量打印。
- [x] **PERF-02 · P2** 对生产搜索 SQL 测 query plan 和 p50/p95，比较保持现有搜索语义的索引方案。完成：单汉字、全拼、首字母及英文中间子串均纳入；结果明确采用方案或保留现状。前置：DB-03、DB-05、PERF-01。
- [x] **PERF-03 · P2** 测首次/二次扫描耗时、解析/SQL/封面读写次数与峰值 RSS，比较串行和小固定并发。完成：结果包含成本与收益，选用方案不突破在途图片预算。前置：ART-05、DB-06、PERF-01。
- [x] **PERF-04 · P2** 测冷/热启动和 1×/2×DPR 下的首屏时间、封面内存及回源次数。完成：验证首屏不等待全库补图，容量和清晰度结果均可复查。前置：ART-01、ART-04、PERF-01。
- [x] **PERF-05 · P2** 测虚拟化后的 delegate 数、帧时间、role 分配和布局开销。完成：仅对 profiler 指出的角色格式化、布局/clip、后台排序或数据分页需求提出独立改动项。前置：UI-01、UI-02、MODEL-04、PERF-01。
- [x] **PERF-06 · P2** 分别测静止、暂停、可见播放和最小化播放的 CPU、唤醒、帧提交和文件 I/O。完成：后台通知下降且播放稳定，无交互不新增业务轮询。前置：PLAY-11、ENG-01、PERF-01。
- [x] **PERF-07 · P2** 执行持续导入、批量变更、锁库、慢后端及扫描中退出压力验证。完成：记录队列峰值、最终数据一致性、UI 响应及退出时长，失败项落到具体编号。前置：DATA-07、PLAY-07、WATCH-02、WATCH-05、WATCH-06、PERF-01。
- [x] **PERF-08 · P3** 对 release 的 LTO、codegen-units、strip 配置作 A/B 测量。完成：记录启动、RSS、包体与构建时间，只有明确收益的配置进入独立改动项。
