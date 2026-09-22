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
    property int browseMode: 0
    property int currentTrackId: 0
    property string playbackState: "stopped"
    property var viewQueries: ["", "", "", ""]
    property int activeBrowseMode: 0
    property bool restoringSearch: false
    signal settingsRequested()
    signal trackRevealRequested(int trackId)
    signal togglePlaybackRequested()
    property real artistGridY: 0
    property real albumGridY: 0
    property real directoryGridY: 0
    property bool holdCompletedStatus: false
    property string completedStatusText: ""
    property bool statusReady: false

    readonly property bool filtering: searchField.text.trim() !== ""
    readonly property bool waitingForSearch: filtering
        && (root.session.searching
            || searchField.text.trim() !== String(root.session.search_query).trim())
    readonly property bool sessionBusy: root.session.busy
    readonly property string headerStatusText: {
        if (root.browseMode === 0 && root.filtering)
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

    function revealTrack(trackId) {
        root.browseMode = 0
        searchField.clear()
        root.commitPendingQuery()
        Qt.callLater(() => root.trackRevealRequested(trackId))
    }

    function commitQuery(mode, text) {
        const queries = root.viewQueries.slice()
        queries[mode] = text
        root.viewQueries = queries
        const query = text.trim()
        if (mode === 0) {
            root.pendingQuery = text
            if (query === "")
                root.session.clear_search()
            else
                root.session.search_tracks(query)
        } else {
            root.session.filter_collections(mode, query)
        }
    }

    function commitPendingQuery() {
        root.commitQuery(root.activeBrowseMode, searchField.text)
    }

    function restoreSearch() {
        root.restoringSearch = true
        searchField.text = root.browseMode === 0 ? root.pendingQuery : root.viewQueries[root.browseMode]
        root.restoringSearch = false
        root.activeBrowseMode = root.browseMode
    }

    Component.onCompleted: {
        root.restoreSearch()
        root.statusReady = true
        root.commitPendingQuery()
    }

    Component.onDestruction: {
        searchDelay.stop()
        root.commitPendingQuery()
    }

    onBrowseModeChanged: {
        if (!root.statusReady)
            return
        searchDelay.stop()
        root.commitPendingQuery()
        root.restoreSearch()
        root.commitPendingQuery()
        viewLoader.opacity = 0
        viewFadeIn.restart()
    }

    NumberAnimation {
        id: viewFadeIn
        target: viewLoader
        property: "opacity"
        to: 1
        duration: 160
        easing.type: Easing.OutCubic
    }

    Shortcut {
        sequences: [StandardKey.Find]
        enabled: root.visible
        onActivated: {
            searchField.forceActiveFocus()
            searchField.selectAll()
        }
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
        anchors.leftMargin: 14
        anchors.rightMargin: 14
        anchors.topMargin: 10
        anchors.bottomMargin: 0
        spacing: 10

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            RowLayout {
                spacing: 8
                Layout.alignment: Qt.AlignVCenter

                Icon {
                    Layout.alignment: Qt.AlignVCenter
                    name: "library"
                    size: 26
                    color: root.theme.textColor
                }

                Text {
                    text: "曲库"
                    color: root.theme.textColor
                    font.pixelSize: 28
                    font.weight: Font.DemiBold
                }

                Text {
                    objectName: "libraryTrackCount"
                    visible: root.session.track_count > 0
                    text: root.session.track_count + " 首"
                    color: root.theme.mutedTextColor
                    font.pixelSize: 14
                }
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
                objectName: "librarySearchField"

                Layout.alignment: Qt.AlignVCenter
                Layout.preferredWidth: 168
                Layout.maximumWidth: 188
                Layout.preferredHeight: 32
                placeholderText: ["搜索歌曲", "搜索歌手", "搜索专辑或歌手", "搜索文件夹或路径"][root.browseMode]
                placeholderTextColor: root.theme.mutedTextColor
                leftPadding: 32
                rightPadding: searchField.text !== "" ? 34 : 12
                font.pixelSize: 13
                color: root.theme.textColor
                onTextChanged: {
                    if (root.restoringSearch)
                        return
                    if (text.trim() === "") {
                        searchDelay.stop()
                        root.commitPendingQuery()
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

            FlatButton {
                theme: root.theme
                text: "添加目录"
                filled: true
                fontPixelSize: 13
                contentAlignment: Text.AlignHCenter
                preferredWidth: 108
                preferredHeight: 32
                enabled: !root.session.busy
                onClicked: folderDialog.open()
            }

            FlatButton {
                objectName: "librarySettingsButton"
                theme: root.theme
                filled: true
                preferredWidth: 32
                preferredHeight: 32
                iconName: "settings"
                Accessible.name: "设置"
                onClicked: root.settingsRequested()
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Item { Layout.fillWidth: true }

            LibraryViewSwitch {
                objectName: "libraryViewSwitch"
                Layout.alignment: Qt.AlignVCenter
                Layout.preferredHeight: 32
                theme: root.theme
                currentIndex: root.browseMode
                onCurrentIndexChanged: root.browseMode = currentIndex
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.dividerColor
        }

        Loader {
            id: viewLoader
            objectName: "libraryBodyLoader"
            Layout.fillWidth: true
            Layout.fillHeight: true
            sourceComponent: [allMusicView, artistView, albumView, directoryView][root.browseMode]
        }
    }

    Component {
        id: allMusicView
        Item {
            Connections {
                target: root
                function onTrackRevealRequested(trackId) { trackTable.revealTrack(trackId) }
            }
            TrackTable {
                id: trackTable
                objectName: "allMusicTable"
                property bool scrollReady: false
                Component.onCompleted: {
                    restoreContentY(root.savedContentY)
                    scrollReady = true
                }
                anchors.fill: parent
                theme: root.theme
                trackModel: root.session.library_model
                currentTrackId: root.currentTrackId
                playbackState: root.playbackState
                onTogglePlaybackRequested: root.togglePlaybackRequested()
                sortColumn: String(root.session.sort_column_name)
                sortAscending: root.session.sort_ascending
                visible: count > 0 && !root.waitingForSearch
                onTrackActivated: (trackId) => root.session.play_track(trackId)
                onSortRequested: (column) => root.session.set_sort(column)
                onContentYChanged: if (scrollReady) root.savedContentY = contentY
            }

            Item {
                anchors.fill: parent
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

    Component {
        id: artistView
        Artist {
            objectName: "artistBrowser"
            currentTrackId: root.currentTrackId
            playbackState: root.playbackState
            filterQuery: root.viewQueries[root.browseMode]
            onTogglePlaybackRequested: root.togglePlaybackRequested()
            theme: root.theme
            libraryModel: root.session
            embedded: true
            savedGridY: root.artistGridY
            onSavedGridYChanged: root.artistGridY = savedGridY
        }
    }

    Component {
        id: albumView
        Album {
            objectName: "albumBrowser"
            currentTrackId: root.currentTrackId
            playbackState: root.playbackState
            filterQuery: root.viewQueries[root.browseMode]
            onTogglePlaybackRequested: root.togglePlaybackRequested()
            theme: root.theme
            libraryModel: root.session
            embedded: true
            savedGridY: root.albumGridY
            onSavedGridYChanged: root.albumGridY = savedGridY
        }
    }

    Component {
        id: directoryView
        CollectionBrowser {
            objectName: "directoryBrowser"
            currentTrackId: root.currentTrackId
            playbackState: root.playbackState
            filterQuery: root.viewQueries[root.browseMode]
            onTogglePlaybackRequested: root.togglePlaybackRequested()
            theme: root.theme
            embedded: true
            title: "目录"
            compact: true
            collectionIcon: "folder"
            emptyTitle: "还没有音乐目录"
            emptySubtitle: "添加音乐后，目录将显示在这里"
            collectionModel: root.session.directory_model
            detailModel: root.session.directory_detail
            selectedName: root.session.selected_directory
            selectedSubtitle: root.session.selected_directory_subtitle
            selectedCover: root.session.selected_directory_cover
            savedGridY: root.directoryGridY
            onSavedGridYChanged: root.directoryGridY = savedGridY
            onCollectionOpened: (id) => root.session.open_directory(id)
            onCollectionClosed: root.session.close_directory()
            onTrackActivated: (id) => root.session.play_directory_track(id)
        }
    }
}
