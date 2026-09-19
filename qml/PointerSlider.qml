pragma ComponentBehavior: Bound

import QtQuick
import "pointer.js" as Pointer

// Seek/volume without MouseArea. PointHandler keeps a passive grab only, so
// a lost Wayland release cannot pin the whole window.
Item {
    id: root

    property real from: 0
    property real to: 1
    property real value: 0
    property bool inputEnabled: true
    property color trackColor: "#DEE2DC"
    property color fillColor: "#24745F"

    property bool dragging: false
    property real dragValue: 0

    readonly property real visualValue: root.dragging ? root.dragValue : root.value

    signal dragged(real value)
    signal committed(real value)

    implicitWidth: 120
    implicitHeight: 22

    function ratioOf(v) {
        const span = root.to - root.from
        if (span <= 0)
            return 0
        return Math.max(0, Math.min(1, (v - root.from) / span))
    }

    function valueAt(x) {
        const span = root.to - root.from
        const r = Math.max(0, Math.min(1, x / Math.max(1, root.width)))
        return root.from + r * span
    }

    function applyX(x) {
        root.dragValue = root.valueAt(x)
        root.dragged(root.dragValue)
    }

    Rectangle {
        id: track
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: 4
        radius: 2
        color: root.trackColor

        Rectangle {
            width: parent.width * root.ratioOf(root.visualValue)
            height: parent.height
            radius: 2
            color: root.fillColor
        }
    }

    Rectangle {
        width: 10
        height: 10
        radius: 5
        visible: root.inputEnabled
        color: root.fillColor
        anchors.verticalCenter: parent.verticalCenter
        x: Math.round(track.width * root.ratioOf(root.visualValue) - width / 2)
    }

    HoverHandler {
        blocking: false
        grabPermissions: PointerHandler.ApprovesTakeOverByAnything
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
    }

    PointHandler {
        id: handle
        acceptedButtons: Qt.LeftButton
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        grabPermissions: PointerHandler.ApprovesTakeOverByAnything
        onActiveChanged: {
            Pointer.debug(active ? "slider-press" : "slider-release",
                          root.objectName || "slider", "")
            if (!root.inputEnabled)
                return
            if (active) {
                root.dragging = true
                root.applyX(point.position.x)
            } else if (root.dragging) {
                root.committed(root.dragValue)
                root.dragging = false
                Pointer.afterGesture()
            }
        }
    }

    readonly property real pointerX: handle.point.position.x
    onPointerXChanged: if (handle.active)
        root.applyX(pointerX)
}
