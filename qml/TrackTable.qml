pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import "pointer.js" as Pointer

Item {
    id: root

    required property var theme
    required property var trackModel
    property bool sortable: true
    property string debugLabel: "tracks"
    property bool handlersEnabled: true
    readonly property alias count: trackList.count

    signal trackActivated(int row)

    function clampScroll() {
        Pointer.resyncFlickable(trackList)
    }

    function heading(label, column) {
        if (!root.sortable || root.trackModel.sort_column !== column)
            return label
        return label + (root.trackModel.sort_ascending ? " ↑" : " ↓")
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 12
            Layout.rightMargin: 12
            spacing: 14

            Text {
                Layout.preferredWidth: 44
                text: "封面"
                color: root.theme.mutedTextColor
                font.pixelSize: 12
            }

            TapControl {
                id: titleHeading
                objectName: "sortTitleButton"

                Layout.fillWidth: true
                Layout.preferredHeight: 28
                radius: 4
                actionEnabled: root.sortable
                hoverFill: root.theme.hoverColor
                onTapped: if (root.sortable)
                    root.trackModel.set_sort("title")

                Text {
                    anchors.fill: parent
                    text: root.heading("歌曲名", "title")
                    color: root.theme.mutedTextColor
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                }
            }

            TapControl {
                id: albumHeading
                objectName: "sortAlbumButton"

                Layout.preferredWidth: 190
                Layout.preferredHeight: 28
                radius: 4
                actionEnabled: root.sortable
                hoverFill: root.theme.hoverColor
                onTapped: if (root.sortable)
                    root.trackModel.set_sort("album")

                Text {
                    anchors.fill: parent
                    text: root.heading("专辑", "album")
                    color: root.theme.mutedTextColor
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                }
            }

            TapControl {
                id: durationHeading
                objectName: "sortDurationButton"

                Layout.preferredWidth: 48
                Layout.preferredHeight: 28
                radius: 4
                actionEnabled: root.sortable
                hoverFill: root.theme.hoverColor
                onTapped: if (root.sortable)
                    root.trackModel.set_sort("duration")

                Text {
                    anchors.fill: parent
                    text: root.heading("时长", "duration")
                    color: root.theme.mutedTextColor
                    horizontalAlignment: Text.AlignRight
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 12
                }
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true

            ListView {
                id: trackList

                anchors.fill: parent
                model: root.trackModel
                clip: true
                spacing: 2
                boundsBehavior: Flickable.StopAtBounds
                interactive: false
                pixelAligned: true
                maximumFlickVelocity: 0
                onHeightChanged: if (root.visible)
                    Pointer.resyncFlickable(trackList)
                onContentHeightChanged: if (root.visible)
                    Pointer.resyncFlickable(trackList)

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
                    color: listInput.hoverRow === trackRow.index
                        ? root.theme.hoverColor : "transparent"
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
                }
            }

            ViewInput {
                id: listInput
                anchors.fill: parent
                view: trackList
                inputActive: root.handlersEnabled
                debugLabel: root.debugLabel
                doubleActivate: true
                onActivated: function(row) { root.trackActivated(row) }
            }
        }
    }
}