pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property bool darkMode
    required property string musicFolders
    signal themeRequested(bool dark)

    ButtonGroup {
        id: themeGroup
    }

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

        RadioButton {
            text: "浅色"
            checked: !root.darkMode
            ButtonGroup.group: themeGroup
            palette.buttonText: root.theme.textColor
            onClicked: root.themeRequested(false)
        }

        RadioButton {
            text: "深色"
            checked: root.darkMode
            ButtonGroup.group: themeGroup
            palette.buttonText: root.theme.textColor
            onClicked: root.themeRequested(true)
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