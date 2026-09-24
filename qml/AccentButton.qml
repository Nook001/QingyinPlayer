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

    scale: root.enabled && root.down ? 0.95 : 1.0
    Behavior on scale {
        NumberAnimation { duration: 110; easing.type: Easing.OutCubic }
    }

    contentItem: Item {
        Row {
            anchors.centerIn: parent
            spacing: 6

            Icon {
                visible: root.iconName !== ""
                anchors.verticalCenter: parent.verticalCenter
                name: root.iconName
                size: root.iconSize
                color: root.theme.accentTextColor
                opacity: root.enabled ? 1 : 0.5
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.text
                color: root.theme.accentTextColor
                font.pixelSize: root.theme.metaSize
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

        Behavior on color {
            ColorAnimation { duration: 130; easing.type: Easing.OutCubic }
        }
    }
}
