pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property var libraryModel

    FolderDialog {
        id: folderDialog
        title: "选择音乐文件夹"
        onAccepted: root.libraryModel.add_library_folder(selectedFolder)
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

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 12
            Layout.rightMargin: 12
            spacing: 14
            visible: trackList.count > 0

            Text {
                Layout.preferredWidth: 44
                text: "封面"
                color: root.theme.mutedTextColor
                font.pixelSize: 12
            }

            Text {
                Layout.fillWidth: true
                text: "歌曲名"
                color: root.theme.mutedTextColor
                font.pixelSize: 12
            }

            Text {
                Layout.preferredWidth: 190
                text: "专辑"
                color: root.theme.mutedTextColor
                font.pixelSize: 12
            }

            Text {
                Layout.preferredWidth: 48
                text: "时长"
                color: root.theme.mutedTextColor
                horizontalAlignment: Text.AlignRight
                font.pixelSize: 12
            }
        }

        ListView {
            id: trackList

            Layout.fillWidth: true
            Layout.fillHeight: true
            model: root.libraryModel
            clip: true
            spacing: 2
            visible: count > 0
            boundsBehavior: Flickable.StopAtBounds

            WheelHandler {
                target: null
                onWheel: function(event) {
                    const maximumY = Math.max(0, trackList.contentHeight - trackList.height)
                    trackList.contentY = Math.max(0, Math.min(maximumY,
                        trackList.contentY - event.angleDelta.y * 0.75))
                    event.accepted = true
                }
            }

            delegate: Rectangle {
                id: trackRow

                required property int index
                required property string title
                required property string artist
                required property string album
                required property string duration
                required property string cover

                width: trackList.width
                height: 58
                color: rowMouseArea.containsMouse ? root.theme.hoverColor : "transparent"
                radius: 5

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 12
                    spacing: 14

                    Rectangle {
                        Layout.preferredWidth: 44
                        Layout.preferredHeight: 44
                        color: root.theme.artworkColor
                        radius: 4
                        clip: true

                        Image {
                            id: coverImage
                            anchors.fill: parent
                            source: trackRow.cover
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            visible: status === Image.Ready
                        }

                        Text {
                            anchors.centerIn: parent
                            text: "♫"
                            color: root.theme.accentColor
                            font.pixelSize: 18
                            visible: coverImage.status !== Image.Ready
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2

                        Text {
                            Layout.fillWidth: true
                            text: trackRow.title
                            color: root.theme.textColor
                            elide: Text.ElideRight
                            font.pixelSize: 14
                            font.weight: Font.DemiBold
                        }

                        Text {
                            Layout.fillWidth: true
                            text: trackRow.artist || "未知歌手"
                            color: root.theme.mutedTextColor
                            elide: Text.ElideRight
                            font.pixelSize: 12
                        }
                    }

                    Text {
                        Layout.preferredWidth: 190
                        text: trackRow.album
                        color: root.theme.mutedTextColor
                        elide: Text.ElideRight
                        font.pixelSize: 12
                    }

                    Text {
                        Layout.preferredWidth: 48
                        text: trackRow.duration
                        color: root.theme.mutedTextColor
                        horizontalAlignment: Text.AlignRight
                        font.pixelSize: 12
                    }
                }

                MouseArea {
                    id: rowMouseArea
                    anchors.fill: parent
                    hoverEnabled: true
                    onDoubleClicked: root.libraryModel.play_track(trackRow.index)
                }
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: trackList.count === 0

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
                    text: root.libraryModel.scanning ? "正在扫描音乐" : "曲库还是空的"
                    color: root.theme.textColor
                    font.pixelSize: 18
                    font.weight: Font.DemiBold
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: root.libraryModel.scan_status || "添加一个本地文件夹开始整理音乐"
                    color: root.theme.mutedTextColor
                    font.pixelSize: 13
                }
            }
        }
    }
}
