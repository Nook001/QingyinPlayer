# 指针卡死根因报告

日期：2026-09-19  
现象：窗口不再响应鼠标，播放进度条和音频仍正常。  
环境：Qt 6.12 Quick + qmetaobject，Wayland（Hyprland / CachyOS）。

---

## 结论

抢的不是时间，也不是 CPU。抢的是 **窗口里同一份独占指针抓取**（exclusive pointer grab）。

Wayland 的 `wl_seat` 上同时只有一只指针。Qt Quick 再把它交给 **最多一个独占抓取者**。谁拿到这份抓取、却没在松开时还回去，后面所有按钮、列表、滚轮都收不到事件。日志里的毫秒时间戳只是事件何时被记下；时间戳停了，看起来像「时间被抢走」，其实是 **指针事件流断了**。

进度条还能走、歌还能播，是因为 Qt 主循环和 GStreamer 都没停。卡的是输入，不是播放。

---

## 「抢占同一个时间」到底是什么

一次点击在系统里走的是这条链，整条链上只有一个名额：

```
鼠标硬件
  → 合成器 seat（全局只有一只指针）
    → 清音这个窗口的表面
      → Qt Quick 投递代理（QQuickDeliveryAgent）
        → 独占抓取槽：同一时刻只能有一个 Item / Handler
          → 命中的那个按钮或列表
```

下一首、曲库双击、专辑卡片、侧栏，看起来是不同控件，但它们 **都要先进入这个独占槽，再在松开时退出**。正常情况是排队，不是并行：

| 日志 | 含义 |
| --- | --- |
| `grab … 1` | `GrabPassive`，开始盯这只指针 |
| `pressed` / `list-press` | 按下 |
| `tapped` / `list-double-tap` | 我们认成一次点击 |
| `slot play_*` | Rust 开始切歌 |
| `grab … 2` | `UngrabPassive`，我们的 handler 已放手 |

最后一次卡死（叠加层改完之后）仍然是：

```
list-double-tap  detail  row=0
slot  play_album_track row=0
grab  detail  2
（之后没有任何指针事件）
```

我们自己的 handler 已经 `UngrabPassive`。窗口却死了。说明 **还有一个没打日志的东西拿着独占抓取不放**，或者合成器认为指针已经离开这个表面。

不同按钮不会去抢「同一段时间」。它们抢的是 **同一把指针锁**。锁没还，所有人一起饿死。

---

## 交互模型有没有问题

有。问题不在「用了按钮」或「用了双击」，而在 **同一块窗口表面混了好几套互不相让的输入模型**：

1. **Qt Quick Controls 的 `MouseArea`**（旧的 `ToolButton` / `Slider` / `TextField` / `ToolTip`）默认独占抓取。
2. **PointerHandler**（`TapHandler` / `PointHandler` / `HoverHandler` / `WheelHandler`）另有一套被动 / 独占规则。
3. **`Flickable` / `ListView` / `GridView`** 即使 `interactive: false`，按下和滚轮仍可能自己独占抓取（内部滚轮 handler，日志里看不见）。
4. **Wayland 合成器手势**（`startSystemMove` / `startSystemResize`）会把指针从 Qt 手里拿走；Qt 往往收不到 `release`。
5. **切歌会在指针手势尚未完全结束时改场景**（换封面、改播放条、重置/刷新模型），投递代理容易把「已松开」记在一个已经换掉的对象上，锁就悬空。

在 X11 上很多组合能凑合。在 Wayland 上 **松开事件可以丢**，独占抓取没有超时回收，于是表现为整窗无法点击。

正确的模型应当是：

- 整个内容区只有一套输入：被动跟踪，不独占。
- `Flickable` 只负责画列表，不接收指针。
- 禁止 Controls 弹层（`ToolTip` 是独立 `xdg-popup`）。
- 禁止在 handler 回调里同步切歌 / 重置模型。
- 窗口拖动 / 缩放在交给合成器之前必须先丢掉 Qt 抓取。

目前是朝这个模型改，还没改完。搜索框仍是 `TextField`（内部 `MouseArea`），切歌仍会大幅改 QML 树。

---

## 各次卡死分别是哪一层

同一把锁，不同的人来拿。所以每次光标不一样、最后点的控件也不一样。

| 表现 | 拿走锁的东西 | 依据 |
| --- | --- | --- |
| 滚一下必须再点窗口才能点 | `Flickable` 内部滚轮：`ScrollBegin` 之后没有 `ScrollEnd`（触摸板 `phase=0`） | 早期滚动卡死；`interactive: false` 也不能关滚轮 |
| 点下一首后整页死，进度条还在动 | QQC2 按钮 `MouseArea` 独占抓取；切歌改了树，松开没还回去 | 日志停在 `slot play_next` |
| 死的时候光标还是手指 | `HoverHandler.cursorShape` 为了换光标也会抓指针；`ToolTip.visible: hovered` 立刻弹出独立窗口 | 光标停在 `PointingHand`；绑定 `visible` 会跳过延迟 |
| 切到别的页，曲库列表还在抢双击 | `StackLayout` 只把页面 `visible` 设为本地 false，子级 `TapHandler` 仍启用；`Item.enabled` 也传不进 `PointerHandler.enabled` | 歌手页上打出 `list-press library` |
| 专辑详情双击播放后死，默认箭头 | 列表 `TapHandler`/`PointHandler` 已 `UngrabPassive`，但 `ListView` 自己还有一份没记日志的抓取；或切歌改场景后锁悬空 | `grab 1` → `play_*` → `grab 2` → 静默 |
| Debug HUD 后立刻卡死 | 滚轮回调里 `Repeater` 重建树 | 中途改 QML 树 |

`grab` 数字：`1 = GrabPassive`，`2 = UngrabPassive`。我们记下的 handler **从来没有出现过 `16`（GrabExclusive）**。独占发生在我们没钩到的对象上。

---

## 为什么切歌特别容易触发

几乎每次最后一条业务日志都是 `play_track` / `play_next` / `play_album_track` / `play_artist_track`，然后指针事件断掉。

切歌在 Qt 主线程上会：

1. `GStreamer` `set_state(Null)` 再 `Playing`
2. 发 `playback_changed`：标题、封面 `Image`、播放按钮字形
3. 有时刷新列表模型

这些发生在「按下 → 松开」这条链的尾部。投递代理还认为这次点击没结束，场景已经换了。松开可能送给一个已经拆掉或不再命中的对象，**独占槽占着，新点击进不去**。

`Qt.callLater` 把切歌推到松开之后，能避开一部分重入，但 **切歌引起的场景更新仍然可能让某个未记录的对象把锁拿走**。最后一次复现已经是 `callLater` + `PointHandler` + 列表叠加层之后，仍然死在 `play_album_track` + `grab 2`。

---

## 已经排除的

- 主线程死锁：`grab 2` 是在 `play_*` 返回之后记下的，事件循环还在跑。
- 播放器线程崩了：进度和音频继续。
- 我们的 `PointHandler` 没放手：每次卡死前都有成对的 `1` 和 `2`。
- 「双击手势」本身：双击只是两次完整的按下/松开。

---

## 还没关掉的缺口

1. **切歌仍会改 QML 树**，锁可能落在未打日志的对象上（`Flickable` 内部、封面 `Image`、残留 Controls）。
2. **搜索框 `TextField`** 仍是 `MouseArea`。设置页主题切换已改为 `TapControl`。
3. **窗口边缘 `ResizeHandle` / 标题栏 `DragHandler`** 会把指针交给合成器；点到 6px 边上会走这条路（最近几次日志里没出现 `resize-grab`）。
4. **打开详情后第一次点击常 `row=-1`**：列表还没排完就点，命中失败；这不一定卡死，但说明布局和指针不同步。
5. 叠加层能否保证 `ListView` 完全吃不到事件，还不能从日志证明。

---

## 本轮修复（强制清锁，不是假松开）

没有按「按下后 n 毫秒就算松开」去做。Wayland 的松开是离散事件，应用层补超时会误伤拖动进度条。

改成：手势已经结束、或切歌已经改完场景之后，如果独占槽里还有人，就把它清掉。

- 切歌后读 `QQuickWindow::mouseGrabberItem()`，有则 `ungrabMouse()`；同时清 `QPointingDevice` 上残留的 exclusive grabber。
- `QQuickFlickable` 在 C++ 里 `setAcceptedMouseButtons(NoButton)`，不再只靠 QML 叠加层。
- 指针离开窗口、窗口失活、下一次按下若仍有残留独占，当作 cancel 清掉。
- 调试模式下记录 `qt-grab`：`16` 是 `GrabExclusive`，用来指认之前日志里看不见的那个抢锁者。
- 设置页 `RadioButton` 换成 `TapControl`，去掉一处 Controls `MouseArea`。

实验后：快速切页再点按钮仍会假死。卡死前日志是：

```
slot  play_album_track row=0
grabber  idle mouse=none
grab  detail  2
grabber-later  idle mouse=none
```

清锁在跑，但 `mouseGrabberItem` 和 exclusive grabber 都是空的，也从未出现 `qt-grab 16`。应用层 `ungrabMouse()` 是空操作。事件之后不再进入窗口，更像是 Qt 仍认为按键没松开，或 Wayland 合成器不再把指针事件交给这个表面。这两处都没有稳妥的应用层修法，继续换 QML handler 也打不到。

还没关：曲库搜索框仍是 `TextField`。此项暂时搁置。
