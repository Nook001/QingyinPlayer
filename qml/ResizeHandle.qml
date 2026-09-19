pragma ComponentBehavior: Bound

import QtQuick
import "pointer.js" as Pointer

Item {
    id: root

    required property var targetWindow
    required property int resizeEdges
    required property int resizeCursor

    z: 1000
    visible: root.targetWindow.visibility === Window.Windowed

    HoverHandler {
        blocking: false
        cursorShape: root.resizeCursor
        grabPermissions: PointerHandler.ApprovesTakeOverByAnything
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
    }

    DragHandler {
        id: resizeHandler

        target: null
        acceptedButtons: Qt.LeftButton
        grabPermissions: PointerHandler.CanTakeOverFromAnything
                         | PointerHandler.ApprovesTakeOverByAnything
        onActiveChanged: {
            Pointer.debug(active ? "resize-grab" : "resize-ungrab",
                          "edges=" + root.resizeEdges, "")
            if (active) {
                Pointer.handOff(resizeHandler, function() {
                    root.targetWindow.startSystemResize(root.resizeEdges)
                })
            }
        }
    }
}
