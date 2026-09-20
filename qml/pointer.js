.pragma library

// Pointer policy for this QML tree:
// 1. System chrome owns move and resize. Do not call startSystemMove.
// 2. Lists are ItemDelegate rows inside one ScrollView Flickable
//    (Repeater, not ListView/GridView). A second interactive view
//    on the same surface takes an exclusive grab on Wayland.
// 3. Buttons and sliders are Qt Quick Controls. Do not mix PointHandler
//    or MouseArea onto the same surface.
// 4. Hidden pages are unloaded (Loader). Change currentView with
//    Qt.callLater so a live Flickable is not destroyed mid-click.
// 5. Track playback is two ItemDelegate clicks within 400ms. Collection
//    cards activate on one click. Both emit after Qt.callLater.
// 6. Play, next, previous, seek, and page switches also use callLater
//    so they do not run inside the pointer event.
// 7. Do not use Controls.ToolTip.visible: hovered (xdg-popup on hover).
//    Prefer Accessible.name, or ToolTip with the default delay.
// 8. Pointer debug must not rebuild QML items during a handler callback.

var debugEnabled = false
var debugSink = null
var lastWheelLogAt = 0
var hookedItems = []

function setDebug(enabled, sink) {
    debugEnabled = !!enabled
    debugSink = sink
}

function debug(kind, target, extra) {
    if (!debugEnabled || !debugSink)
        return
    if (kind === "wheel") {
        const now = Date.now()
        if (now - lastWheelLogAt < 80)
            return
        lastWheelLogAt = now
    }
    const k = String(kind)
    const t = target == null ? "" : String(target)
    const e = extra == null ? "" : String(extra)
    Qt.callLater(function() {
        debugSink(k, t, e)
    })
}

function describeButton(item) {
    const name = item.objectName ? String(item.objectName) : ""
    const text = item.text !== undefined ? String(item.text) : ""
    return name || text || String(item)
}

function isFlickableView(item) {
    return item && item.indexAt !== undefined && item.contentY !== undefined
}

function hookButtons(item) {
    if (!item)
        return
    const isButton = item.clicked !== undefined
        && item.pressed !== undefined
        && item.checkable !== undefined
    const isSlider = item.from !== undefined
        && item.to !== undefined
        && item.pressed !== undefined
        && item.moved !== undefined
    if ((isButton || isSlider) && hookedItems.indexOf(item) === -1) {
        hookedItems.push(item)
        const label = describeButton(item)
        if (item.clicked !== undefined)
            item.clicked.connect(function() {
                debug("clicked", label, item.enabled ? "" : "disabled")
            })
        item.pressedChanged.connect(function() {
            debug(item.pressed ? "pressed" : "released", label,
                  "enabled=" + item.enabled
                  + (item.down !== undefined ? " down=" + item.down : ""))
        })
        if (item.canceled !== undefined)
            item.canceled.connect(function() {
                debug("canceled", label, "")
            })
    }
    if (isFlickableView(item))
        return
    const children = item.children
    for (let i = 0; i < children.length; ++i)
        hookButtons(children[i])
}
