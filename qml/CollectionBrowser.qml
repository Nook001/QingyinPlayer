pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property string title
    required property string emptyTitle
    required property string emptySubtitle
    required property var collectionModel
    required property var detailModel
    required property string selectedName
    required property string selectedSubtitle
    required property string selectedCover

    property real savedGridY: 0
    property bool embedded: false
    property bool compact: false
    property string collectionIcon: "artist"
    property string filterQuery: ""
    property int currentTrackId: 0
    property string playbackState: "stopped"
    signal togglePlaybackRequested()

    Shortcut {
        sequences: ["Escape", "Alt+Left"]
        enabled: root.visible && root.showingDetail
        onActivated: root.collectionClosed()
    }

    signal collectionOpened(string collectionId)
    signal collectionClosed()
    signal trackActivated(int trackId)

    readonly property bool showingDetail: root.selectedName !== ""

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: root.embedded ? 0 : 18
        anchors.rightMargin: root.embedded ? 0 : 18
        anchors.topMargin: root.embedded ? 0 : 28
        anchors.bottomMargin: 0
        spacing: 18

        RowLayout {
            Layout.fillWidth: true
            visible: !root.embedded || root.showingDetail
            spacing: 12

            FlatButton {
                visible: root.showingDetail
                preferredWidth: 40
                preferredHeight: 40
                theme: root.theme
                iconName: "chevronLeft"
                Accessible.name: "返回"
                onClicked: Qt.callLater(() => root.collectionClosed())
            }

            CoverImage {
                visible: root.showingDetail && !root.compact
                displaySize: 52
                theme: root.theme
                source: root.selectedCover
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 3

                Text {
                    text: root.showingDetail ? root.selectedName : root.title
                    color: root.theme.textColor
                    font.pixelSize: 28
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }

                Text {
                    visible: root.showingDetail
                    text: root.selectedSubtitle
                    color: root.theme.mutedTextColor
                    font.pixelSize: 13
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            visible: !root.embedded || root.showingDetail
            Layout.preferredHeight: 1
            color: root.theme.dividerColor
        }

        Loader {
            id: bodyLoader
            Layout.fillWidth: true
            Layout.fillHeight: true
            sourceComponent: root.showingDetail ? detailComponent
                : (root.compact ? listComponent : gridComponent)
        }
    }

    Component {
        id: gridComponent

        Item {
            id: gridRoot

            GridView {
                id: collectionGrid
                property bool scrollReady: false
                anchors.fill: parent
                clip: true
                reuseItems: true
                cacheBuffer: 464
                cellWidth: 176
                cellHeight: 232
                model: root.collectionModel
                visible: collectionGrid.count > 0
                footer: Item {
                    width: collectionGrid.width
                    height: 180
                }
                Component.onCompleted: {
                    contentY = Math.max(0, root.savedGridY)
                    scrollReady = true
                }
                onContentYChanged: if (scrollReady) root.savedGridY = contentY
                highlightMoveDuration: 0

                delegate: ItemDelegate {
                    id: collectionCard

                    required property string name
                    required property string subtitle
                    required property string cover
                    required property string collectionId

                    width: 176
                    height: 232
                    padding: 8
                    hoverEnabled: true
                    text: collectionCard.name
                    Accessible.name: collectionCard.name

                    GridView.onPooled: collectionCard.highlighted = false
                    GridView.onReused: collectionCard.highlighted = false

                    background: RoundedRect {
                        color: collectionCard.hovered ? root.theme.hoverColor : "transparent"
                        radius: 10
                    }

                    contentItem: Column {
                        spacing: 8

                        CoverImage {
                            displaySize: 160
                            theme: root.theme
                            source: collectionCard.cover
                        }

                        Text {
                            width: parent.width
                            text: collectionCard.name
                            color: root.theme.textColor
                            elide: Text.ElideRight
                            font.pixelSize: 14
                            font.weight: Font.DemiBold
                        }

                        Text {
                            width: parent.width
                            text: collectionCard.subtitle
                            color: root.theme.mutedTextColor
                            elide: Text.ElideRight
                            font.pixelSize: 12
                        }
                    }

                    onClicked: {
                        const id = collectionCard.collectionId
                        Qt.callLater(() => root.collectionOpened(id))
                    }
                }
            }

            PageScrollBar {
                anchors.top: collectionGrid.top
                anchors.right: collectionGrid.right
                anchors.bottom: collectionGrid.bottom
                theme: root.theme
                scroller: collectionGrid
            }

            Column {
                visible: collectionGrid.count === 0
                anchors.centerIn: parent
                spacing: 13

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: root.filterQuery.trim() ? "没有找到匹配的" + root.title : root.emptyTitle
                    color: root.theme.textColor
                    font.pixelSize: 18
                    font.weight: Font.DemiBold
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: root.emptySubtitle
                    visible: !root.filterQuery.trim()
                    color: root.theme.mutedTextColor
                    font.pixelSize: 13
                }
            }
        }
    }

    Component {
        id: listComponent
        Item {
            ListView {
                id: collectionList
                objectName: "compactCollections"
                anchors.fill: parent
                clip: true
                reuseItems: true
                cacheBuffer: 392
                model: root.collectionModel
                boundsBehavior: Flickable.StopAtBounds
                property bool scrollReady: false
                Component.onCompleted: {
                    contentY = Math.max(0, root.savedGridY)
                    scrollReady = true
                }
                onContentYChanged: if (scrollReady) root.savedGridY = contentY
                footer: Item { width: collectionList.width; height: 180 }
                delegate: ItemDelegate {
                    id: collectionRow
                    required property string name
                    required property string subtitle
                    required property string collectionId
                    width: ListView.view ? ListView.view.width : 0
                    height: 68
                    padding: 12
                    hoverEnabled: true
                    Accessible.name: name
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: name + "\n" + subtitle
                    background: RoundedRect {
                        radius: 6
                        color: collectionRow.hovered ? root.theme.hoverColor : "transparent"
                        borderWidth: collectionRow.visualFocus ? 1 : 0
                        borderColor: root.theme.accentColor
                    }
                    contentItem: RowLayout {
                        spacing: 14
                        Icon {
                            name: root.collectionIcon
                            size: 22
                            color: root.theme.mutedTextColor
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 4
                            Text {
                                Layout.fillWidth: true
                                text: collectionRow.name
                                color: root.theme.textColor
                                font.pixelSize: 14
                                font.weight: Font.Medium
                                elide: Text.ElideRight
                            }
                            Text {
                                Layout.fillWidth: true
                                text: collectionRow.subtitle
                                color: root.theme.mutedTextColor
                                font.pixelSize: 12
                                elide: Text.ElideMiddle
                            }
                        }
                        Icon { name: "chevronRight"; size: 16; color: root.theme.mutedTextColor }
                    }
                    onClicked: {
                        const id = collectionRow.collectionId
                        Qt.callLater(() => root.collectionOpened(id))
                    }
                }
            }
            PageScrollBar {
                anchors.top: collectionList.top
                anchors.right: collectionList.right
                anchors.bottom: collectionList.bottom
                theme: root.theme
                scroller: collectionList
            }
            Text {
                anchors.centerIn: parent
                visible: collectionList.count === 0
                text: root.filterQuery.trim() ? "没有找到匹配的" + root.title : root.emptyTitle
                color: root.theme.mutedTextColor
                font.pixelSize: 16
            }
        }
    }

    Component {
        id: detailComponent

        TrackTable {
            theme: root.theme
            trackModel: root.detailModel
            currentTrackId: root.currentTrackId
            playbackState: root.playbackState
            onTogglePlaybackRequested: root.togglePlaybackRequested()
            sortable: false
            onTrackActivated: (trackId) => root.trackActivated(trackId)
        }
    }
}
