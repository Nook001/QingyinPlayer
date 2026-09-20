pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls

Button {
    id: root

    required property var theme
    property int preferredWidth: 112
    property int preferredHeight: 38

    implicitWidth: root.preferredWidth
    implicitHeight: root.preferredHeight
    flat: true
    hoverEnabled: true

    contentItem: Text {
        text: root.text
        color: "#FFFFFF"
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        font.pixelSize: 13
        font.weight: Font.DemiBold
        opacity: root.enabled ? 1 : 0.5
    }

    background: Rectangle {
        radius: 6
        color: !root.enabled
            ? root.theme.subtleColor
            : (root.down ? root.theme.accentPressedColor : root.theme.accentColor)
    }
}
