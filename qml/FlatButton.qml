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
    property int fontPixelSize: 0
    property int fontWeight: Font.Normal
    property int contentAlignment: Text.AlignLeft
    property bool emphasized: false
    property bool filled: false
    property color labelColor: root.theme.textColor

    implicitWidth: root.preferredWidth
    implicitHeight: root.preferredHeight
    padding: 0
    leftInset: 0
    rightInset: 0
    topInset: 0
    bottomInset: 0
    flat: true
    hoverEnabled: true

    scale: root.enabled && root.down ? 0.94 : 1.0
    Behavior on scale {
        NumberAnimation { duration: 110; easing.type: Easing.OutCubic }
    }

    contentItem: Item {
        implicitWidth: root.preferredWidth
        implicitHeight: root.preferredHeight

        Icon {
            anchors.centerIn: parent
            visible: root.iconName !== ""
            name: root.iconName
            size: root.iconSize
            color: root.emphasized ? root.theme.accentTextColor : root.labelColor
            opacity: root.enabled ? 1 : 0.35
            Behavior on color {
                ColorAnimation { duration: 130; easing.type: Easing.OutCubic }
            }
        }

        Text {
            anchors.fill: parent
            visible: root.iconName === ""
            text: root.text
            color: root.labelColor
            opacity: root.enabled ? 1 : 0.35
            horizontalAlignment: root.contentAlignment
            verticalAlignment: Text.AlignVCenter
            font.pixelSize: root.fontPixelSize > 0 ? root.fontPixelSize : root.theme.bodySize
            font.weight: root.fontWeight
            font.letterSpacing: 0
            Behavior on color {
                ColorAnimation { duration: 130; easing.type: Easing.OutCubic }
            }
        }
    }

    background: Item {
        implicitWidth: root.preferredWidth
        implicitHeight: root.preferredHeight

        RoundedRect {
            anchors.centerIn: parent
            width: root.emphasized ? Math.min(parent.width, parent.height) : parent.width
            height: root.emphasized ? width : parent.height
            radius: root.emphasized || root.filled ? Math.min(width, height) / 2 : 6
            borderWidth: root.filled ? 1 : 0
            borderColor: root.theme.dividerColor
            color: {
                const pressedCol = root.theme.pressedColor || root.theme.subtleColor || root.theme.hoverColor
                if (root.emphasized && root.enabled)
                    return root.down ? root.theme.accentPressedColor : root.theme.accentColor
                if (root.filled && root.enabled && root.down)
                    return pressedCol
                if (root.hovered && root.enabled)
                    return root.down ? pressedCol : root.theme.hoverColor
                if (root.enabled && root.down)
                    return pressedCol
                if (root.filled)
                    return root.theme.fieldColor
                return "transparent"
            }

            Behavior on color {
                ColorAnimation { duration: 130; easing.type: Easing.OutCubic }
            }
        }
    }
}
