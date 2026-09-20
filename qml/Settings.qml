pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property bool darkMode
    required property string musicFolders
    property string settingsError
    signal themeRequested(bool dark)
    signal backRequested()

    Shortcut { sequence: "Escape"; onActivated: root.backRequested() }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 18
        spacing: 18

        RowLayout {
            Layout.fillWidth: true
            FlatButton {
                theme: root.theme
                iconName: "chevronLeft"
                Accessible.name: "返回曲库"
                onClicked: root.backRequested()
            }
            Text {
                text: "设置"
                color: root.theme.textColor
                font.pixelSize: 28
                font.weight: Font.DemiBold
            }
            Item { Layout.fillWidth: true }
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

            Button {
                id: lightThemeButton

                Layout.preferredWidth: 88
                Layout.preferredHeight: 36
                flat: true
                hoverEnabled: true
                text: "浅色"
                onClicked: root.themeRequested(false)

                contentItem: Text {
                    text: lightThemeButton.text
                    color: root.theme.textColor
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 14
                }

                background: RoundedRect {
                    radius: 6
                    color: lightThemeButton.down || lightThemeButton.hovered
                        ? root.theme.hoverColor
                        : (!root.darkMode ? root.theme.subtleColor : "transparent")
                }
            }

            Button {
                id: darkThemeButton

                Layout.preferredWidth: 88
                Layout.preferredHeight: 36
                flat: true
                hoverEnabled: true
                text: "深色"
                onClicked: root.themeRequested(true)

                contentItem: Text {
                    text: darkThemeButton.text
                    color: root.theme.textColor
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 14
                }

                background: RoundedRect {
                    radius: 6
                    color: darkThemeButton.down || darkThemeButton.hovered
                        ? root.theme.hoverColor
                        : (root.darkMode ? root.theme.subtleColor : "transparent")
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

        Text {
            Layout.fillWidth: true
            visible: root.settingsError.trim() !== ""
            text: root.settingsError
            color: root.theme.accentPressedColor
            font.pixelSize: 13
            wrapMode: Text.Wrap
        }

        Item { Layout.fillHeight: true }
    }
}
