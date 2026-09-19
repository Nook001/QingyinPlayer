pragma ComponentBehavior: Bound

import QtQuick
import "pointer.js" as Pointer

// Child of the ViewInput overlay, not of the Flickable. Parent is an Item
// so this handler never shares a grab with ListView/GridView.
WheelHandler {
    id: handler

    property var flickable: parent
    property bool inputActive: true

    target: null
    blocking: false
    enabled: handler.inputActive || handler.active
    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
    activeTimeout: 0
    grabPermissions: PointerHandler.ApprovesTakeOverByAnything

    onGrabChanged: function(transition, point) {
        Pointer.debug("grab", "wheel", String(transition))
    }

    onActiveChanged: Pointer.debug(handler.active ? "wheel-active" : "wheel-idle",
                                   "list",
                                   handler.flickable
                                   ? ("contentY=" + handler.flickable.contentY)
                                   : "")

    onWheel: function(event) {
        Pointer.scrollY(handler.flickable, event)
        const delta = event.pixelDelta.y !== 0 ? event.pixelDelta.y : event.angleDelta.y
        Pointer.debug("wheel", "list",
                      "delta=" + delta
                      + " phase=" + event.phase
                      + " contentY=" + (handler.flickable ? handler.flickable.contentY : "?"))
    }
}
