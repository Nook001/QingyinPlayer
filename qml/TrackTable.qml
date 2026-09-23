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
    readonly property int indexWidth: 28
    readonly property int coverWidth: 44
    readonly property int durationWidth: 56
    readonly property int titleStretch: 6
    readonly property int albumStretch: 4
    readonly property alias count: trackList.count
    readonly property alias contentY: trackList.contentY

    signal trackActivated(int trackId)
    signal sortRequested(string column)
    property string membershipAction: ""
    property var playlistList: null
    signal membershipRequested(int trackId)
    signal playlistChosen(int playlistId, int trackId)
    signal playlistCreateRequested(int trackId)

    function openPlaylistPicker(trackId, anchor) {
        playlistPicker.trackId = trackId
        const overlay = Overlay.overlay
        if (!overlay)
            return
        const point = anchor.mapToItem(overlay, 0, anchor.height + 4)
        const popupWidth = playlistPicker.width
        const popupHeight = Math.min(320, Math.max(96, playlistPicker.implicitHeight))
        let x = point.x + anchor.width - popupWidth
        let y = point.y
        if (y + popupHeight > overlay.height - 8)
            y = Math.max(8, point.y - popupHeight - anchor.height - 8)
        playlistPicker.x = Math.max(8, Math.min(x, overlay.width - popupWidth - 8))
        playlistPicker.y = Math.max(8, y)
        playlistPicker.open()
    }

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

        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: 34

            RoundedRect {
                anchors.fill: parent
                radius: 8
                color: root.theme.subtleColor
            }

            Rectangle {
                width: 2
                height: 12
                radius: 1
                color: root.theme.accentColor
                opacity: 0.8
                anchors.left: parent.left
                anchors.leftMargin: 10
                anchors.verticalCenter: parent.verticalCenter
            }

            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.leftMargin: 12
                anchors.rightMargin: 12
                height: 1
                color: root.theme.dividerColor
                opacity: 0.65
            }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 4
                anchors.rightMargin: 16
                spacing: 10

                Text {
                    Layout.preferredWidth: root.indexWidth
                    Layout.minimumWidth: root.indexWidth
                    Layout.maximumWidth: root.indexWidth
                    text: "#"
                    color: root.theme.mutedTextColor
                    horizontalAlignment: Text.AlignHCenter
                font.pixelSize: root.theme.digitSize
                font.weight: Font.Medium
                font.family: root.theme.digitFamily
                }

                Item {
                    Layout.preferredWidth: root.coverWidth
                    Layout.minimumWidth: root.coverWidth
                    Layout.maximumWidth: root.coverWidth
                }

                FlatButton {
                    Layout.fillWidth: true
                    Layout.horizontalStretchFactor: root.titleStretch
                    Layout.preferredWidth: 0
                    Layout.minimumWidth: 120
                    preferredHeight: 28
                    theme: root.theme
                    enabled: root.sortable
                    contentAlignment: Text.AlignLeft
                    fontPixelSize: root.theme.metaSize
                    fontWeight: Font.Medium
                    labelColor: root.theme.mutedTextColor
                    text: root.heading("标题", "title")
                    Accessible.name: "按标题排序"
                    onClicked: if (root.sortable)
                        root.sortRequested("title")
                }

                FlatButton {
                    Layout.fillWidth: true
                    Layout.horizontalStretchFactor: root.albumStretch
                    Layout.preferredWidth: 0
                    Layout.minimumWidth: 80
                    preferredHeight: 28
                    theme: root.theme
                    enabled: root.sortable
                    contentAlignment: Text.AlignLeft
                    fontPixelSize: root.theme.metaSize
                    fontWeight: Font.Medium
                    labelColor: root.theme.mutedTextColor
                    text: root.heading("专辑", "album")
                    Accessible.name: "按专辑排序"
                    onClicked: if (root.sortable)
                        root.sortRequested("album")
                }

                FlatButton {
                    Layout.preferredWidth: root.durationWidth
                    Layout.minimumWidth: root.durationWidth
                    Layout.maximumWidth: root.durationWidth
                    preferredHeight: 28
                    theme: root.theme
                    enabled: root.sortable
                    contentAlignment: Text.AlignRight
                    fontPixelSize: root.theme.metaSize
                    fontWeight: Font.Medium
                    labelColor: root.theme.mutedTextColor
                    text: root.heading("时长", "duration")
                    Accessible.name: "按时长排序"
                    onClicked: if (root.sortable)
                        root.sortRequested("duration")
                }
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
                    required property bool hiRes
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
                                objectName: "rowIndex" + trackRow.trackId
                                anchors.centerIn: parent
                                text: String(trackRow.index + 1).padStart(2, "0")
                                color: trackRow.isCurrent ? root.theme.accentColor : root.theme.mutedTextColor
                                font.pixelSize: root.theme.digitSize
                                font.family: root.theme.digitFamily
                            }
                        }

                        Item {
                            Layout.preferredWidth: root.coverWidth
                            Layout.minimumWidth: root.coverWidth
                            Layout.maximumWidth: root.coverWidth
                            Layout.preferredHeight: 44
                            Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter

                            CoverImage {
                                id: rowCover
                                anchors.centerIn: parent
                                displaySize: root.coverWidth
                                clipOnly: true
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
                            Layout.horizontalStretchFactor: root.titleStretch
                            Layout.preferredWidth: 0
                            Layout.minimumWidth: 120
                            spacing: 2

                            Text {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignLeft
                                text: trackRow.title
                                color: trackRow.isCurrent ? root.theme.accentColor : root.theme.textColor
                                elide: Text.ElideRight
                                horizontalAlignment: Text.AlignLeft
                                font.pixelSize: root.theme.bodySize
                                font.weight: Font.DemiBold
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 6

                                Item {
                                    objectName: "rowHiRes" + trackRow.trackId
                                    visible: trackRow.hiRes
                                    Layout.alignment: Qt.AlignVCenter
                                    implicitWidth: hiResLabel.implicitWidth + 12
                                    implicitHeight: 16

                                    RoundedRect {
                                        anchors.fill: parent
                                        radius: height / 2
                                        color: root.theme.subtleColor
                                    }

                                    Text {
                                        id: hiResLabel
                                        anchors.centerIn: parent
                                        text: "Hi-Res"
                                        color: root.theme.accentColor
                                        font.pixelSize: root.theme.metaSize
                                        font.weight: Font.Medium
                                    }
                                }

                                Text {
                                    Layout.fillWidth: true
                                    Layout.alignment: Qt.AlignLeft | Qt.AlignVCenter
                                    text: trackRow.artist || "未知歌手"
                                    color: root.theme.mutedTextColor
                                    elide: Text.ElideRight
                                    horizontalAlignment: Text.AlignLeft
                                    font.pixelSize: root.theme.metaSize
                                }
                            }
                        }

                        Text {
                            Layout.fillWidth: true
                            Layout.horizontalStretchFactor: root.albumStretch
                            Layout.preferredWidth: 0
                            Layout.minimumWidth: 80
                            text: trackRow.album
                            color: root.theme.mutedTextColor
                            elide: Text.ElideRight
                            font.pixelSize: root.theme.metaSize
                        }

                        Text {
                            Layout.preferredWidth: root.durationWidth
                            Layout.minimumWidth: root.durationWidth
                            Layout.maximumWidth: root.durationWidth
                            text: trackRow.duration
                            color: root.theme.mutedTextColor
                            horizontalAlignment: Text.AlignRight
                            font.pixelSize: root.theme.digitSize
                            font.family: root.theme.digitFamily
                        }

                        FlatButton {
                            id: membershipButton
                            visible: root.membershipAction !== "" && trackRow.hovered
                            theme: root.theme
                            iconName: root.membershipAction === "remove" ? "close" : "plus"
                            iconSize: 14
                            preferredWidth: 28
                            preferredHeight: 28
                            Accessible.name: root.membershipAction === "remove" ? "从歌单移除" : "加入歌单"
                            onClicked: {
                                const id = trackRow.trackId
                                if (root.membershipAction === "add") {
                                    root.openPlaylistPicker(id, membershipButton)
                                    return
                                }
                                Qt.callLater(() => root.membershipRequested(id))
                            }
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

    Popup {
        id: playlistPicker
        objectName: "playlistPicker"
        property int trackId: 0
        parent: Overlay.overlay
        width: 232
        padding: 10
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        background: RoundedRect {
            color: root.theme.surfaceColor
            radius: 12
            borderWidth: 1
            borderColor: root.theme.dividerColor
        }

        contentItem: ColumnLayout {
            spacing: 2

            Text {
                Layout.fillWidth: true
                Layout.leftMargin: 8
                Layout.bottomMargin: 4
                text: "加入歌单"
                color: root.theme.mutedTextColor
                font.pixelSize: root.theme.metaSize
            }

            ListView {
                id: playlistChoices
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(220, contentHeight)
                visible: count > 0
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: root.playlistList
                spacing: 2

                delegate: Item {
                    id: playlistChoice
                    required property int playlistId
                    required property string name
                    required property int trackCount
                    width: ListView.view ? ListView.view.width : 0
                    height: 34

                    RoundedRect {
                        anchors.fill: parent
                        radius: 8
                        color: choiceHover.hovered ? root.theme.hoverColor : "transparent"
                    }

                    Text {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.leftMargin: 8
                        anchors.rightMargin: 8
                        text: playlistChoice.name
                        color: root.theme.textColor
                        elide: Text.ElideRight
                        font.pixelSize: root.theme.bodySize
                    }

                    HoverHandler { id: choiceHover }

                    TapHandler {
                        onTapped: {
                            const playlistId = playlistChoice.playlistId
                            const trackId = playlistPicker.trackId
                            playlistPicker.close()
                            root.playlistChosen(playlistId, trackId)
                        }
                    }
                }
            }

            Text {
                Layout.fillWidth: true
                Layout.leftMargin: 8
                Layout.topMargin: 4
                Layout.bottomMargin: 4
                visible: playlistChoices.count === 0
                text: "还没有歌单"
                color: root.theme.mutedTextColor
                font.pixelSize: root.theme.metaSize
            }

            FlatButton {
                Layout.fillWidth: true
                theme: root.theme
                text: "新建歌单"
                fontPixelSize: root.theme.metaSize
                preferredHeight: 32
                Accessible.name: "新建歌单并加入"
                onClicked: {
                    const trackId = playlistPicker.trackId
                    playlistPicker.close()
                    root.playlistCreateRequested(trackId)
                }
            }
        }
    }
}
