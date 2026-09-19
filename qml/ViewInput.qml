pragma ComponentBehavior: Bound

import QtQuick
import "pointer.js" as Pointer

// Sits on top of a ListView/GridView. The view itself must not receive
// pointer events: Flickable takes an exclusive grab on press/wheel even
// when interactive is false, and that grab is what freezes the window.
// This overlay stays visible so hits never fall through to the Flickable.
Item {
    id: root

    property var view
    property bool inputActive: true
    property bool doubleActivate: true
    property string debugLabel: "view"

    z: 1

    readonly property real hoverX: hover.point.position.x
    readonly property real hoverY: hover.point.position.y
    readonly property int hoverRow: {
        if (!root.inputActive || !hover.hovered || !root.view)
            return -1
        const _ = root.hoverX + root.hoverY
        return Pointer.viewIndexAt(root.view, hover.point)
    }

    signal activated(int row)

    property real lastTapAt: 0
    property int lastTapRow: -1

    Rectangle {
        anchors.fill: parent
        color: "transparent"
    }

    HoverHandler {
        id: hover
        enabled: root.inputActive
        blocking: false
        grabPermissions: PointerHandler.ApprovesTakeOverByAnything
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
    }

    ListWheel {
        flickable: root.view
        inputActive: root.inputActive
    }

    PointHandler {
        id: tap
        enabled: root.inputActive || active
        acceptedButtons: Qt.LeftButton
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        target: null
        grabPermissions: PointerHandler.ApprovesTakeOverByAnything
        property int pressRow: -1

        onGrabChanged: function(transition, point) {
            Pointer.debug("grab", root.debugLabel, String(transition))
        }

        onActiveChanged: {
            if (active) {
                tap.pressRow = Pointer.viewIndexAt(root.view, point)
                Pointer.debug("list-press", root.debugLabel,
                              "contentY=" + (root.view ? root.view.contentY : "?")
                              + " row=" + tap.pressRow)
                return
            }
            Pointer.debug("list-release", root.debugLabel,
                          "contentY=" + (root.view ? root.view.contentY : "?"))
            Pointer.afterGesture()
            const row = tap.pressRow
            if (row < 0)
                return
            if (!root.doubleActivate) {
                Pointer.debug("list-tap", root.debugLabel, "row=" + row)
                Pointer.later(function() { root.activated(row) })
                return
            }
            const now = Date.now()
            if (root.lastTapRow === row
                    && root.lastTapAt !== 0
                    && (now - root.lastTapAt) <= 400) {
                root.lastTapAt = 0
                root.lastTapRow = -1
                Pointer.debug("list-double-tap", root.debugLabel, "row=" + row)
                Pointer.later(function() { root.activated(row) })
                return
            }
            root.lastTapAt = now
            root.lastTapRow = row
            Pointer.debug("list-tap", root.debugLabel, "row=" + row)
        }
    }
}
