.pragma library

// Pointer policy for this QML tree:
// 1. ListView/GridView are display-only: interactive false, no handlers
//    as children. Flickable takes an exclusive grab on press/wheel even
//    then; input lives on a sibling ViewInput overlay.
// 2. Hidden StackLayout pages keep their size. ViewInput.visible follows
//    inputActive so PointerHandlers are locally not visible.
// 3. After startSystemMove / startSystemResize, drop the Qt grab immediately.
//    Wayland compositors consume the pointer and Qt will not see a release.
// 4. Do not mix MouseArea with PointerHandlers on the same surface.
//    Buttons are TapControl / PlaybackButton. Sliders are PointHandler.
// 5. List playback uses two completed taps, not Qt's double-tap grab.
// 6. Pointer debug must not rebuild QML items during a handler callback.
// 7. Never assign handler.enabled (Pointer.release) except after handing
//    the pointer to the compositor. That assignment destroys QML bindings.
// 8. Do not use Controls.ToolTip (xdg-popup) and do not set
//    HoverHandler.cursorShape on buttons.
// 9. Do not set PointerHandler.enabled false while the handler is pressed
//    or active. Gate the action instead.
// 10. Never call into playback / model reset inside a pointer callback.
//     Qt.callLater so the grab can be released first.
// 11. After a gesture ends, and after play_*, ask Rust to drop leftover
//     exclusive grabs. Do not fake a release after n milliseconds.

var debugEnabled = false
var debugSink = null
var dropGrab = null
var lastWheelLogAt = 0

function setDebug(enabled, sink) {
    debugEnabled = !!enabled
    debugSink = sink
}

function setDropGrab(fn) {
    dropGrab = fn
}

function afterGesture() {
    Qt.callLater(function() {
        if (dropGrab)
            dropGrab()
    })
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
    const tip = item.ToolTip && item.ToolTip.text ? String(item.ToolTip.text) : ""
    return name || tip || text || String(item)
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
    if ((isButton || isSlider) && !item.__qingyinPointerHooked) {
        item.__qingyinPointerHooked = true
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

function release(handler) {
    handler.enabled = false
    Qt.callLater(function () {
        handler.enabled = true
    })
}

function handOff(handler, action) {
    action()
    release(handler)
    afterGesture()
}

function viewIndexAt(view, point) {
    if (!view || !point || !view.contentItem)
        return -1
    const scene = point.scenePosition
    const mapped = scene
        ? view.contentItem.mapFromItem(null, scene.x, scene.y)
        : view.contentItem.mapFromItem(view, point.x, point.y)
    return view.indexAt(mapped.x, mapped.y)
}

function resyncFlickable(view) {
    if (!view)
        return
    Qt.callLater(function() {
        if (!view)
            return
        const maxY = Math.max(0, view.contentHeight - view.height)
        const y = Math.max(0, Math.min(maxY, view.contentY))
        view.contentY = y
        if (view.forceLayout)
            view.forceLayout()
    })
}

function scrollY(flickable, event) {
    if (!flickable)
        return
    const delta = event.pixelDelta.y !== 0 ? event.pixelDelta.y : event.angleDelta.y * 0.75
    const maximumY = Math.max(0, flickable.contentHeight - flickable.height)
    flickable.contentY = Math.max(0, Math.min(maximumY, flickable.contentY - delta))
    // Accept so Flickable's built-in wheel handler does not take a grab.
    event.accepted = true
}

function later(action) {
    Qt.callLater(function() {
        action()
        afterGesture()
    })
}
