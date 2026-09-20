pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls

Button {
    id: root

    required property var theme
    property int preferredWidth: 112
    property int preferredHeight: 38
    property int cornerRadius: 8
    property string iconName
    property int iconSize: 14

    implicitWidth: root.preferredWidth
    implicitHeight: root.preferredHeight
    padding: 0
    flat: true
    hoverEnabled: true

    contentItem: Item {
        Row {
            anchors.centerIn: parent
            spacing: 6

            Icon {
                visible: root.iconName !== ""
                anchors.verticalCenter: parent.verticalCenter
                name: root.iconName
                size: root.iconSize
                color: "#FFFFFF"
                opacity: root.enabled ? 1 : 0.5
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.text
                color: "#FFFFFF"
                font.pixelSize: 12
                font.weight: Font.DemiBold
                opacity: root.enabled ? 1 : 0.5
            }
        }
    }

    background: RoundedRect {
        radius: root.cornerRadius
        color: !root.enabled
            ? root.theme.subtleColor
            : (root.down ? root.theme.accentPressedColor : root.theme.accentColor)
    }
}
