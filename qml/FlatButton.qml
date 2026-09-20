pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls

Button {
    id: root

    required property var theme
    property int preferredWidth: 40
    property int preferredHeight: 40
    property string iconName
    property int iconSize: 18
    property int fontPixelSize: 16
    property int contentAlignment: Text.AlignLeft
    property bool emphasized: false

    implicitWidth: root.preferredWidth
    implicitHeight: root.preferredHeight
    padding: 0
    flat: true
    hoverEnabled: true

    contentItem: Item {
        Icon {
            anchors.centerIn: parent
            visible: root.iconName !== ""
            name: root.iconName
            size: root.iconSize
            color: root.emphasized ? "#FFFFFF" : root.theme.textColor
            opacity: root.enabled ? 1 : 0.35
        }

        Text {
            anchors.fill: parent
            visible: root.iconName === ""
            text: root.text
            color: root.theme.textColor
            opacity: root.enabled ? 1 : 0.35
            horizontalAlignment: root.contentAlignment
            verticalAlignment: Text.AlignVCenter
            font.pixelSize: root.fontPixelSize
        }
    }

    background: RoundedRect {
        radius: root.emphasized ? height / 2 : 6
        color: {
            if (root.emphasized && root.enabled)
                return root.down ? root.theme.accentPressedColor : root.theme.accentColor
            if (root.hovered && root.enabled)
                return root.theme.hoverColor
            return "transparent"
        }
    }
}
