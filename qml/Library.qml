pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property var session
    property string pendingQuery: ""
    property real savedContentY: 0

    readonly property bool filtering: searchField.text.trim() !== ""
    readonly property bool waitingForSearch: filtering
        && (root.session.searching
            || searchField.text.trim() !== String(root.session.search_query).trim())

    function commitPendingQuery() {
        const query = searchField.text.trim()
        root.pendingQuery = query
        if (query === "")
            root.session.clear_search()
        else
            root.session.search_tracks(query)
    }

    Component.onCompleted: {
        if (root.pendingQuery !== "")
            searchField.text = root.pendingQuery
        if (trackTable.count > 0)
            trackTable.restoreContentY(root.savedContentY)
    }

    Component.onDestruction: {
        root.pendingQuery = searchField.text
        root.savedContentY = trackTable.contentY
        if (searchField.text.trim() !== String(root.session.search_query).trim())
            root.commitPendingQuery()
    }

    Timer {
        id: searchDelay
        interval: 180
        repeat: false
        onTriggered: root.commitPendingQuery()
    }

    FolderDialog {
        id: folderDialog
        title: "选择音乐目录"
        onAccepted: root.session.add_library_folder(selectedFolder)
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 34
        spacing: 20

        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Text {
                text: "曲库"
                color: root.theme.textColor
                font.pixelSize: 28
                font.weight: Font.DemiBold
            }

            Item { Layout.fillWidth: true }

            TextField {
                id: searchField

                Layout.preferredWidth: 220
                Layout.maximumWidth: 260
                Layout.preferredHeight: 32
                placeholderText: "搜索歌曲"
                leftPadding: 32
                rightPadding: 12
                font.pixelSize: 13
                color: root.theme.textColor
                onTextChanged: {
                    if (text.trim() === "") {
                        searchDelay.stop()
                        root.session.clear_search()
                    } else {
                        searchDelay.restart()
                    }
                }

                background: RoundedRect {
                    color: root.theme.fieldColor
                    borderColor: searchField.activeFocus
                        ? root.theme.accentColor : root.theme.dividerColor
                    borderWidth: searchField.activeFocus ? 2 : 1
                    radius: height / 2

                    Icon {
                        anchors.left: parent.left
                        anchors.leftMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        name: "search"
                        size: 14
                        color: root.theme.mutedTextColor
                    }
                }
            }

            AccentButton {
                theme: root.theme
                text: "添加目录"
                iconName: "folderPlus"
                preferredWidth: 108
                preferredHeight: 32
                cornerRadius: 16
                enabled: !root.session.busy
                onClicked: folderDialog.open()
            }
        }

        Text {
            Layout.fillWidth: true
            visible: root.session.scan_status !== "" || root.session.watch_status !== ""
                || root.filtering
            text: {
                if (root.filtering)
                    return root.session.search_status
                if (root.session.watch_status !== "")
                    return root.session.scan_status + (root.session.scan_status !== "" ? " · " : "")
                        + root.session.watch_status
                return root.session.scan_status
            }
            color: root.theme.mutedTextColor
            font.pixelSize: 12
            wrapMode: Text.Wrap
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
            trackModel: root.session.library_model
            sortColumn: String(root.session.sort_column_name)
            sortAscending: root.session.sort_ascending
            visible: count > 0 && !root.waitingForSearch
            onTrackActivated: function(trackId) { root.session.play_track(trackId) }
            onSortRequested: function(column) { root.session.set_sort(column) }
            onContentYChanged: root.savedContentY = contentY
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !trackTable.visible

            Column {
                anchors.centerIn: parent
                spacing: 13

                RoundedRect {
                    width: 76
                    height: 76
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: !root.filtering
                    radius: 38
                    color: root.theme.subtleColor

                    Icon {
                        anchors.centerIn: parent
                        name: "library"
                        size: 30
                        color: root.theme.accentColor
                    }
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: {
                        if (root.waitingForSearch)
                            return "正在搜索…"
                        if (root.filtering)
                            return "没有找到匹配的歌曲"
                        return root.session.scanning ? "正在扫描音乐" : "曲库还是空的"
                    }
                    color: root.theme.textColor
                    font.pixelSize: 18
                    font.weight: Font.DemiBold
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: !root.filtering
                    text: root.session.scan_status || "添加一个本地目录开始整理音乐"
                    color: root.theme.mutedTextColor
                    font.pixelSize: 13
                }
            }
        }
    }
}
