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

    signal collectionOpened(int row)
    signal collectionClosed()
    signal trackActivated(int row)

    readonly property bool showingDetail: root.selectedName !== ""

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 34
        spacing: 18

        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Button {
                id: backButton

                visible: root.showingDetail
                Layout.preferredWidth: 40
                Layout.preferredHeight: 40
                flat: true
                onClicked: root.collectionClosed()

                contentItem: Text {
                    text: "‹"
                    color: root.theme.textColor
                    font.pixelSize: 28
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }

                background: Rectangle {
                    color: backButton.hovered ? root.theme.hoverColor : "transparent"
                    radius: 6
                }
            }

            Rectangle {
                visible: root.showingDetail
                Layout.preferredWidth: 52
                Layout.preferredHeight: 52
                color: root.theme.artworkColor
                radius: 6
                clip: true

                Image {
                    id: headerCover
                    anchors.fill: parent
                    source: root.selectedCover
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    visible: status === Image.Ready
                }

                Text {
                    anchors.centerIn: parent
                    text: "♫"
                    color: root.theme.accentColor
                    font.pixelSize: 18
                    visible: headerCover.status !== Image.Ready
                }
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

        GridView {
            id: collectionGrid

            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !root.showingDetail && count > 0
            clip: true
            cellWidth: 176
            cellHeight: 232
            model: root.collectionModel
            boundsBehavior: Flickable.StopAtBounds

            WheelHandler {
                target: null
                onWheel: function(event) {
                    const maximumY = Math.max(0, collectionGrid.contentHeight - collectionGrid.height)
                    collectionGrid.contentY = Math.max(0, Math.min(maximumY,
                        collectionGrid.contentY - event.angleDelta.y * 0.75))
                    event.accepted = true
                }
            }

            delegate: Item {
                id: collectionCard

                required property int index
                required property string name
                required property string subtitle
                required property string cover

                width: collectionGrid.cellWidth
                height: collectionGrid.cellHeight

                Column {
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: 8

                    Rectangle {
                        width: parent.width
                        height: parent.width
                        color: root.theme.artworkColor
                        radius: 8
                        clip: true

                        Image {
                            id: coverImage
                            anchors.fill: parent
                            source: collectionCard.cover
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            visible: status === Image.Ready
                        }

                        Text {
                            anchors.centerIn: parent
                            text: "♫"
                            color: root.theme.accentColor
                            font.pixelSize: 28
                            visible: coverImage.status !== Image.Ready
                        }
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

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.collectionOpened(collectionCard.index)

                    Rectangle {
                        anchors.fill: parent
                        color: parent.containsMouse ? root.theme.hoverColor : "transparent"
                        radius: 10
                        z: -1
                    }
                }
            }
        }

        TrackTable {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.showingDetail
            theme: root.theme
            trackModel: root.detailModel
            sortable: false
            onTrackActivated: function(row) { root.trackActivated(row) }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !root.showingDetail && collectionGrid.count === 0

            Column {
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
}
