pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls

Button {
    id: root

    required property var theme
    property int preferredWidth: 40
    property int preferredHeight: 40

    implicitWidth: root.preferredWidth
    implicitHeight: root.preferredHeight
    flat: true
    hoverEnabled: true

    contentItem: Text {
        text: root.text
        color: root.theme.textColor
        opacity: root.enabled ? 1 : 0.35
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        font.pixelSize: 16
    }

    background: Rectangle {
        radius: 6
        color: root.hovered && root.enabled ? root.theme.hoverColor : "transparent"
    }
}
