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

    signal collectionOpened(string collectionId)
    signal collectionClosed()
    signal trackActivated(int trackId)

    readonly property bool showingDetail: root.selectedName !== ""

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 34
        spacing: 18

        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            FlatButton {
                visible: root.showingDetail
                preferredWidth: 40
                preferredHeight: 40
                theme: root.theme
                text: "‹"
                Accessible.name: "返回"
                onClicked: Qt.callLater(function() { root.collectionClosed() })
            }

            CoverImage {
                visible: root.showingDetail
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
            Layout.preferredHeight: 1
            color: root.theme.dividerColor
        }

        Loader {
            id: bodyLoader
            Layout.fillWidth: true
            Layout.fillHeight: true
            sourceComponent: root.showingDetail ? detailComponent : gridComponent
        }
    }

    Component {
        id: gridComponent

        Item {
            id: gridRoot

            GridView {
                id: collectionGrid
                anchors.fill: parent
                clip: true
                reuseItems: true
                cacheBuffer: 464
                cellWidth: 176
                cellHeight: 232
                model: root.collectionModel
                visible: collectionGrid.count > 0
                Component.onCompleted: contentY = Math.max(0, root.savedGridY)
                onContentYChanged: root.savedGridY = contentY

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

                    background: Rectangle {
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
                        Qt.callLater(function() { root.collectionOpened(id) })
                    }
                }
            }

            Column {
                visible: collectionGrid.count === 0
                anchors.centerIn: parent
                spacing: 13

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: root.emptyTitle
                    color: root.theme.textColor
                    font.pixelSize: 18
                    font.weight: Font.DemiBold
                }

                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: root.emptySubtitle
                    color: root.theme.mutedTextColor
                    font.pixelSize: 13
                }
            }
        }
    }

    Component {
        id: detailComponent

        TrackTable {
            theme: root.theme
            trackModel: root.detailModel
            sortable: false
            onTrackActivated: function(trackId) { root.trackActivated(trackId) }
        }
    }
}
