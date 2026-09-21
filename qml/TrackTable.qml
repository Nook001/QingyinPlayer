pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property var trackModel
    property bool sortable: true
    property int currentTrackId: 0
    property string playbackState: "stopped"
    property int selectedTrackId: 0
    signal togglePlaybackRequested()
    property string sortColumn
    property bool sortAscending: true
    property real savedContentY: 0
    property int endSpacerCount: 3
    readonly property int rowHeight: 58
    readonly property int indexWidth: 24
    readonly property int albumWidth: 168
    readonly property int durationWidth: 48
    readonly property alias count: trackList.count
    readonly property alias contentY: trackList.contentY

    signal trackActivated(int trackId)
    signal sortRequested(string column)

    function heading(label, column) {
        if (!root.sortable || root.sortColumn !== column)
            return label
        return label + (root.sortAscending ? " ↑" : " ↓")
    }

    function restoreContentY(position) {
        trackList.contentY = Math.max(0, position)
    }

    function selectTrack(row, trackId) {
        root.selectedTrackId = trackId
        trackList.currentIndex = row
        trackList.forceActiveFocus()
    }

    function revealTrack(trackId) {
        const row = root.trackModel.index_of_track(trackId)
        if (row < 0)
            return
        trackList.forceLayout()
        root.selectTrack(row, trackId)
        trackList.positionViewAtIndex(row, ListView.Center)
    }

    function restoreSelection() {
        trackList.currentIndex = root.selectedTrackId === 0
            ? -1 : root.trackModel.index_of_track(root.selectedTrackId)
    }

    function moveSelection(delta) {
        if (!trackList.count)
            return
        const current = root.trackModel.index_of_track(root.selectedTrackId)
        const row = current < 0 ? 0 : Math.max(0, Math.min(trackList.count - 1, current + delta))
        root.selectTrack(row, root.trackModel.track_id_at(row))
        trackList.positionViewAtIndex(row, ListView.Contain)
    }

    function activateSelected() {
        if (root.trackModel.index_of_track(root.selectedTrackId) >= 0) {
            const id = root.selectedTrackId
            Qt.callLater(() => root.trackActivated(id))
        }
    }

    Connections {
        target: root.trackModel
        function onModelReset() { root.restoreSelection() }
        function onDataChanged() { root.restoreSelection() }
        function onRowsRemoved() { root.restoreSelection() }
        function onRowsInserted() { root.restoreSelection() }
        function onRowsMoved() { root.restoreSelection() }
        function onLayoutChanged() { root.restoreSelection() }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 4
            Layout.rightMargin: 16
            spacing: 10

            Text {
                Layout.preferredWidth: root.indexWidth
                Layout.minimumWidth: root.indexWidth
                Layout.maximumWidth: root.indexWidth
                text: "#"
                color: root.theme.mutedTextColor
                horizontalAlignment: Text.AlignHCenter
                font.pixelSize: 12
            }

            FlatButton {
                Layout.fillWidth: true
                preferredHeight: 28
                theme: root.theme
                enabled: root.sortable
                contentAlignment: Text.AlignLeft
                fontPixelSize: 12
                text: root.heading("歌曲", "title")
                Accessible.name: "按歌曲名排序"
                onClicked: if (root.sortable)
                    root.sortRequested("title")
            }

            FlatButton {
                Layout.preferredWidth: root.albumWidth
                preferredHeight: 28
                theme: root.theme
                enabled: root.sortable
                contentAlignment: Text.AlignLeft
                fontPixelSize: 12
                text: root.heading("专辑", "album")
                Accessible.name: "按专辑排序"
                onClicked: if (root.sortable)
                    root.sortRequested("album")
            }

            FlatButton {
                Layout.preferredWidth: root.durationWidth
                preferredHeight: 28
                theme: root.theme
                enabled: root.sortable
                contentAlignment: Text.AlignRight
                fontPixelSize: 12
                text: root.heading("时长", "duration")
                Accessible.name: "按时长排序"
                onClicked: if (root.sortable)
                    root.sortRequested("duration")
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ListView {
                id: trackList

                anchors.fill: parent
                clip: true
                reuseItems: true
                cacheBuffer: 580
                boundsBehavior: Flickable.StopAtBounds
                highlightMoveDuration: 0
                highlightResizeDuration: 0
                model: root.trackModel
                currentIndex: -1
                keyNavigationEnabled: false
                Keys.onUpPressed: root.moveSelection(-1)
                Keys.onDownPressed: root.moveSelection(1)
                Keys.onReturnPressed: root.activateSelected()
                Keys.onEnterPressed: root.activateSelected()
                spacing: 2
                footerPositioning: ListView.InlineFooter
                footer: Item {
                    width: trackList.width
                    height: root.endSpacerCount * (root.rowHeight + trackList.spacing)
                }

                delegate: ItemDelegate {
                    id: trackRow

                    required property int index
                    required property string title
                    required property string artist
                    required property string album
                    required property string duration
                    required property string cover
                    required property int trackId
                    objectName: "trackRow" + trackId
                    readonly property bool isCurrent: trackId === root.currentTrackId
                        && root.playbackState !== "stopped"
                    readonly property bool isSelected: trackId === root.selectedTrackId
                    focusPolicy: Qt.NoFocus

                    width: ListView.view ? ListView.view.width : 0
                    height: 58
                    padding: 0
                    hoverEnabled: true
                    text: trackRow.title
                    Accessible.name: trackRow.title


                    background: RoundedRect {
                        color: trackRow.isSelected ? root.theme.subtleColor
                            : (trackRow.hovered ? root.theme.hoverColor : "transparent")
                        borderWidth: trackRow.isSelected && trackList.activeFocus ? 1 : 0
                        borderColor: root.theme.accentColor
                        radius: 5
                    }

                    contentItem: RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 4
                        anchors.rightMargin: 16
                        spacing: 10

                        Item {
                            Layout.preferredWidth: root.indexWidth
                            Layout.preferredHeight: 32
                            Text {
                                anchors.centerIn: parent
                                text: String(trackRow.index + 1)
                                color: trackRow.isCurrent ? root.theme.accentColor : root.theme.mutedTextColor
                                font.pixelSize: 12
                            }
                        }

                        Item {
                            Layout.preferredWidth: 44
                            Layout.minimumWidth: 44
                            Layout.maximumWidth: 44
                            Layout.preferredHeight: 44
                            Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter

                            CoverImage {
                                id: rowCover
                                anchors.centerIn: parent
                                displaySize: 44
                                theme: root.theme
                                source: trackRow.cover
                                overlayVisible: trackRow.hovered
                                overlayIcon: trackRow.isCurrent && root.playbackState === "playing"
                                    ? "pause" : "play"
                            }

                            MouseArea {
                                id: rowPlayButton
                                objectName: "rowPlayButton" + trackRow.trackId
                                property string iconName: rowCover.overlayIcon
                                anchors.fill: rowCover
                                enabled: trackRow.hovered
                                cursorShape: Qt.PointingHandCursor
                                Accessible.role: Accessible.Button
                                Accessible.name: iconName === "pause" ? "暂停" : "播放"
                                onClicked: {
                                    root.selectTrack(trackRow.index, trackRow.trackId)
                                    if (trackRow.isCurrent) {
                                        Qt.callLater(() => root.togglePlaybackRequested())
                                    } else {
                                        const id = trackRow.trackId
                                        Qt.callLater(() => root.trackActivated(id))
                                    }
                                }
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2

                            Text {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignLeft
                                text: trackRow.title
                                color: trackRow.isCurrent ? root.theme.accentColor : root.theme.textColor
                                elide: Text.ElideRight
                                horizontalAlignment: Text.AlignLeft
                                font.pixelSize: 14
                                font.weight: Font.DemiBold
                            }

                            Text {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignLeft
                                text: trackRow.artist || "未知歌手"
                                color: root.theme.mutedTextColor
                                elide: Text.ElideRight
                                horizontalAlignment: Text.AlignLeft
                                font.pixelSize: 12
                            }
                        }

                        Text {
                            Layout.preferredWidth: root.albumWidth
                            text: trackRow.album
                            color: root.theme.mutedTextColor
                            elide: Text.ElideRight
                            font.pixelSize: 12
                        }

                        Text {
                            Layout.preferredWidth: root.durationWidth
                            text: trackRow.duration
                            color: root.theme.mutedTextColor
                            horizontalAlignment: Text.AlignRight
                            font.pixelSize: 12
                        }
                    }

                    onClicked: root.selectTrack(trackRow.index, trackRow.trackId)
                    onDoubleClicked: {
                        const id = trackRow.trackId
                        Qt.callLater(() => root.trackActivated(id))
                    }
                }
            }

            PageScrollBar {
                anchors.top: trackList.top
                anchors.right: trackList.right
                anchors.bottom: trackList.bottom
                theme: root.theme
                scroller: trackList
            }
        }
    }
}
