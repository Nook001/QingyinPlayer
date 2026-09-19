pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property bool darkMode
    required property string musicFolders
    signal themeRequested(bool dark)

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 34
        spacing: 18

        Text {
            text: "设置"
            color: root.theme.textColor
            font.pixelSize: 28
            font.weight: Font.DemiBold
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.dividerColor
        }

        Text {
            text: "主题"
            color: root.theme.textColor
            font.pixelSize: 16
            font.weight: Font.DemiBold
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            TapControl {
                id: lightThemeButton
                objectName: "lightThemeButton"

                Layout.preferredWidth: 88
                Layout.preferredHeight: 36
                radius: 6
                restFill: !root.darkMode ? root.theme.subtleColor : "transparent"
                hoverFill: root.theme.hoverColor
                onTapped: root.themeRequested(false)

                Text {
                    anchors.centerIn: parent
                    text: "浅色"
                    color: root.theme.textColor
                    font.pixelSize: 14
                }
            }

            TapControl {
                id: darkThemeButton
                objectName: "darkThemeButton"

                Layout.preferredWidth: 88
                Layout.preferredHeight: 36
                radius: 6
                restFill: root.darkMode ? root.theme.subtleColor : "transparent"
                hoverFill: root.theme.hoverColor
                onTapped: root.themeRequested(true)

                Text {
                    anchors.centerIn: parent
                    text: "深色"
                    color: root.theme.textColor
                    font.pixelSize: 14
                }
            }
        }

        Text {
            text: "音乐文件夹"
            color: root.theme.textColor
            font.pixelSize: 16
            font.weight: Font.DemiBold
        }

        Text {
            Layout.fillWidth: true
            visible: root.musicFolders.trim() !== ""
            text: root.musicFolders
            color: root.theme.mutedTextColor
            font.pixelSize: 13
            wrapMode: Text.Wrap
        }

        Text {
            Layout.fillWidth: true
            visible: root.musicFolders.trim() === ""
            text: "尚未添加音乐文件夹"
            color: root.theme.mutedTextColor
            font.pixelSize: 13
        }

        Item { Layout.fillHeight: true }
    }
}
