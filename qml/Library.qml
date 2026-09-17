pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    signal folderSelected(url folder)

    FolderDialog {
        id: folderDialog
        title: "选择音乐文件夹"
        onAccepted: root.folderSelected(selectedFolder)
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 34
        spacing: 20

        RowLayout {
            Layout.fillWidth: true

            ColumnLayout {
                spacing: 3

                Text {
                    text: "曲库"
                    color: root.theme.textColor
                    font.pixelSize: 28
                    font.weight: Font.DemiBold
                }

                Text {
                    text: "按歌曲、专辑与歌手管理本地收藏"
                    color: root.theme.mutedTextColor
                    font.pixelSize: 13
                }
            }

            Item { Layout.fillWidth: true }

            Button {
                id: addFolderButton

                text: "添加文件夹"
                onClicked: folderDialog.open()

                contentItem: Text {
                    text: addFolderButton.text
                    color: "#FFFFFF"
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }

                background: Rectangle {
                    implicitWidth: 112
                    implicitHeight: 38
                    color: addFolderButton.down
                        ? root.theme.accentPressedColor : root.theme.accentColor
                    radius: 6
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.dividerColor
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Column {
                anchors.centerIn: parent
                spacing: 13

                Rectangle {
                    width: 76
                    height: 76
                    anchors.horizontalCenter: parent.horizontalCenter
                    radius: 38
                    color: root.theme.subtleColor

                    Text {
                        anchors.centerIn: parent
                        text: "♫"
                        color: root.theme.accentColor
                        font.pixelSize: 30
                    }
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "曲库还是空的"
                    color: root.theme.textColor
                    font.pixelSize: 18
                    font.weight: Font.DemiBold
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "添加一个本地文件夹开始整理音乐"
                    color: root.theme.mutedTextColor
                    font.pixelSize: 13
                }
            }
        }
    }
}
