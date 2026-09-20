pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property var libraryModel

    readonly property bool filtering: searchField.text.trim() !== ""
    readonly property bool waitingForSearch: filtering
        && (root.libraryModel.searching
            || searchField.text.trim() !== String(root.libraryModel.search_query).trim())

    onVisibleChanged: {
        if (visible)
            Qt.callLater(trackTable.clampScroll)
    }

    Timer {
        id: searchDelay
        interval: 180
        repeat: false
        onTriggered: {
            if (searchField.text.trim() !== "")
                root.libraryModel.search_tracks(searchField.text)
        }
    }

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
                objectName: "addFolderButton"

                Layout.preferredWidth: 112
                Layout.preferredHeight: 38
                flat: true
                hoverEnabled: true
                text: "添加文件夹"
                onClicked: folderDialog.open()

                contentItem: Text {
                    text: addFolderButton.text
                    color: "#FFFFFF"
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                }

                background: Rectangle {
                    radius: 6
                    color: addFolderButton.down
                        ? root.theme.accentPressedColor : root.theme.accentColor
                }
            }
        }

        TextField {
            id: searchField

            Layout.fillWidth: true
            Layout.preferredHeight: 44
            placeholderText: "搜索歌名、歌手或专辑，也可输入拼音或首字母"
            leftPadding: 14
            rightPadding: 14
            font.pixelSize: 14
            color: root.theme.textColor
            onTextChanged: {
                if (text.trim() === "") {
                    searchDelay.stop()
                    root.libraryModel.clear_search()
                } else {
                    searchDelay.restart()
                }
            }

            background: Rectangle {
                color: root.theme.fieldColor
                border.color: searchField.activeFocus
                    ? root.theme.accentColor : root.theme.dividerColor
                border.width: searchField.activeFocus ? 2 : 1
                radius: 6
            }
        }

        Text {
            Layout.fillWidth: true
            visible: root.filtering
            text: root.libraryModel.search_status
            color: root.theme.mutedTextColor
            font.pixelSize: 12
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.dividerColor
        }

        TrackTable {
            id: trackTable
            Layout.fillWidth: true
            Layout.fillHeight: true
            theme: root.theme
            trackModel: root.libraryModel
            visible: count > 0 && !root.waitingForSearch
            onTrackActivated: function(row) { root.libraryModel.play_track(row) }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !trackTable.visible

            Column {
                anchors.centerIn: parent
                spacing: 13

                Rectangle {
                    width: 76
                    height: 76
                    anchors.horizontalCenter: parent.horizontalCenter
                    radius: 38
                    color: root.theme.subtleColor
                    visible: !root.filtering

                    Text {
                        anchors.centerIn: parent
                        text: "♫"
                        color: root.theme.accentColor
                        font.pixelSize: 30
                    }
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: {
                        if (root.waitingForSearch)
                            return "正在搜索…"
                        if (root.filtering)
                            return "没有找到匹配的歌曲"
                        return root.libraryModel.scanning ? "正在扫描音乐" : "曲库还是空的"
                    }
                    color: root.theme.textColor
                    font.pixelSize: 18
                    font.weight: Font.DemiBold
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: !root.filtering
                    text: root.libraryModel.scan_status || "添加一个本地文件夹开始整理音乐"
                    color: root.theme.mutedTextColor
                    font.pixelSize: 13
                }
            }
        }
    }
}
