pragma ComponentBehavior: Bound

import QtQuick
import "pointer.js" as Pointer

Item {
    id: root

    property string text
    property string tooltip
    property bool actionEnabled: true
    property color textColor: "#1D2523"
    property int pixelSize: 16

    signal tapped()

    implicitWidth: 44
    implicitHeight: 40
    opacity: root.actionEnabled ? 1 : 0.35

    HoverHandler {
        id: hover
        blocking: false
        grabPermissions: PointerHandler.ApprovesTakeOverByAnything
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        onActiveChanged: Pointer.debug(active ? "hover-grab" : "hover-ungrab",
                                       root.objectName || root.tooltip, "")
    }

    PointHandler {
        id: tap
        acceptedButtons: Qt.LeftButton
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        target: null
        grabPermissions: PointerHandler.ApprovesTakeOverByAnything
        onGrabChanged: function(transition, point) {
            Pointer.debug("grab", root.objectName || root.tooltip, String(transition))
        }
        onActiveChanged: {
            Pointer.debug(active ? "pressed" : "released",
                          root.objectName || root.tooltip, "")
            if (active) {
                return
            }
            Pointer.afterGesture()
            if (!root.actionEnabled)
                return
            Pointer.debug("tapped", root.objectName || root.tooltip, "")
            Pointer.later(function() { root.tapped() })
        }
    }

    Text {
        anchors.centerIn: parent
        text: root.text
        color: root.textColor
        font.pixelSize: root.pixelSize
    }

    Rectangle {
        visible: hover.hovered && root.tooltip !== ""
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.top
        anchors.bottomMargin: 6
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
