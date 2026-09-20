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
    property bool holdCompletedStatus: false
    property string completedStatusText: ""
    property bool statusReady: false

    readonly property bool filtering: searchField.text.trim() !== ""
    readonly property bool waitingForSearch: filtering
        && (root.session.searching
            || searchField.text.trim() !== String(root.session.search_query).trim())
    readonly property bool sessionBusy: root.session.busy
    readonly property string headerStatusText: {
        if (root.filtering)
            return String(root.session.search_status)
        if (root.sessionBusy)
            return root.bannerFromSession()
        if (root.holdCompletedStatus)
            return root.completedStatusText
        return ""
    }

    function bannerFromSession() {
        const scan = String(root.session.scan_status).trim()
        const watch = String(root.session.watch_status).trim()
        if (scan !== "" && watch !== "")
            return scan + " · " + watch
        return scan || watch
    }

    function commitPendingQuery() {
        const query = searchField.text.trim()
        root.pendingQuery = query
        if (query === "")
            root.session.clear_search()
        else
            root.session.search_tracks(query)
    }

    Component.onCompleted: {
        root.statusReady = true
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

    Timer {
        id: hideStatusTimer
        interval: 3000
        repeat: false
        onTriggered: root.holdCompletedStatus = false
    }

    onSessionBusyChanged: {
        if (!root.statusReady)
            return
        if (root.sessionBusy) {
            root.holdCompletedStatus = false
            hideStatusTimer.stop()
            return
        }
        const text = root.bannerFromSession()
        if (text === "") {
            root.holdCompletedStatus = false
            return
        }
        root.completedStatusText = text
        root.holdCompletedStatus = true
        hideStatusTimer.restart()
    }

    FolderDialog {
        id: folderDialog
        title: "选择音乐目录"
        onAccepted: root.session.add_library_folder(selectedFolder)
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: 34
        anchors.rightMargin: 34
        anchors.topMargin: 34
        anchors.bottomMargin: 0
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

            Text {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                visible: root.headerStatusText !== ""
                text: root.headerStatusText
                color: root.theme.mutedTextColor
                elide: Text.ElideRight
                font.pixelSize: 12
            }

            Item {
                Layout.fillWidth: true
                visible: root.headerStatusText === ""
            }

            TextField {
                id: searchField

                Layout.preferredWidth: 220
                Layout.maximumWidth: 260
                Layout.preferredHeight: 32
                placeholderText: "搜索歌曲"
                leftPadding: 32
                rightPadding: searchField.text !== "" ? 34 : 12
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

                MouseArea {
                    id: clearSearch
                    visible: searchField.text !== ""
                    width: 22
                    height: 22
                    anchors.right: parent.right
                    anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    z: 2
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    Accessible.role: Accessible.Button
                    Accessible.name: "清空搜索"
                    onClicked: {
                        searchField.clear()
                        searchField.forceActiveFocus()
                    }

                    RoundedRect {
                        anchors.fill: parent
                        radius: 11
                        color: clearSearch.containsMouse ? root.theme.hoverColor : "transparent"
                    }

                    Icon {
                        anchors.centerIn: parent
                        name: "close"
                        size: 12
                        color: clearSearch.containsMouse
                            ? root.theme.textColor : root.theme.mutedTextColor
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
