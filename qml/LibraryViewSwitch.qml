pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls

TabBar {
    id: root

    required property var theme

    implicitWidth: 252
    implicitHeight: 32
    padding: 2
    spacing: 2

    background: RoundedRect {
        color: root.theme.fieldColor
        radius: height / 2
    }

    Repeater {
        model: ["全部", "歌手", "专辑", "目录"]

        TabButton {
            id: tab

            required property string modelData
            required property int index

            objectName: "libraryViewTab" + index
            width: (root.availableWidth - root.spacing * 3) / 4
            height: root.availableHeight
            text: modelData
            Accessible.name: text

            background: RoundedRect {
                color: tab.checked ? root.theme.subtleColor
                    : (tab.hovered ? root.theme.hoverColor : "transparent")
                radius: height / 2
                borderWidth: tab.visualFocus ? 1 : 0
                borderColor: root.theme.accentColor
            }

            contentItem: Text {
                text: tab.text
                color: tab.checked ? root.theme.textColor : root.theme.mutedTextColor
                font.pixelSize: 13
                font.weight: tab.checked ? Font.Medium : Font.Normal
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
            }
        }
    }
}
