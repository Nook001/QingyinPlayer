pragma ComponentBehavior: Bound

import QtQuick
import "pointer.js" as Pointer

Item {
    id: root

    property bool actionEnabled: true
    property string tooltip
    property color hoverFill: "transparent"
    property color restFill: "transparent"
    property color pressFill: hoverFill
    property real radius: 0
    property string debugName: root.objectName || root.tooltip || ""

    readonly property bool hovered: hover.hovered
    readonly property bool pressed: tap.active

    signal tapped()

    HoverHandler {
        id: hover
        blocking: false
        grabPermissions: PointerHandler.ApprovesTakeOverByAnything
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        onActiveChanged: Pointer.debug(active ? "hover-grab" : "hover-ungrab",
                                       root.debugName, "")
    }

    PointHandler {
        id: tap
        acceptedButtons: Qt.LeftButton
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        target: null
        grabPermissions: PointerHandler.ApprovesTakeOverByAnything
        onGrabChanged: function(transition, point) {
            Pointer.debug("grab", root.debugName, String(transition))
        }
        onActiveChanged: {
            Pointer.debug(active ? "pressed" : "released", root.debugName, "")
            if (active) {
                return
            }
            Pointer.afterGesture()
            if (!root.actionEnabled)
                return
            Pointer.debug("tapped", root.debugName, "")
            Pointer.later(function() { root.tapped() })
        }
    }

    Rectangle {
        anchors.fill: parent
        color: tap.active ? root.pressFill
                          : (hover.hovered ? root.hoverFill : root.restFill)
        radius: root.radius
        z: -1
    }

    Rectangle {
        visible: hover.hovered && root.tooltip !== ""
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.bottom
        anchors.topMargin: 6
        z: 20
        width: tipText.implicitWidth + 14
        height: tipText.implicitHeight + 8
        radius: 4
        color: "#202A27"

        Text {
            id: tipText
            anchors.centerIn: parent
            text: root.tooltip
            color: "#F1F4F2"
            font.pixelSize: 11
        }
    }
}
