pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property var trackModel
    property bool sortable: true
    readonly property alias count: trackList.count

    signal trackActivated(int row)

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

            Button {
                id: titleHeading

                Layout.fillWidth: true
                Layout.preferredHeight: 28
                flat: true
                enabled: root.sortable
                onClicked: if (root.sortable) root.trackModel.set_sort("title")

                contentItem: Text {
                    text: root.heading("歌曲名", "title")
                    color: root.theme.mutedTextColor
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                }

                background: Rectangle {
                    color: titleHeading.hovered ? root.theme.hoverColor : "transparent"
                    radius: 4
                }
            }

            Button {
                id: albumHeading

                Layout.preferredWidth: 190
                Layout.preferredHeight: 28
                enabled: root.sortable
                onClicked: if (root.sortable) root.trackModel.set_sort("album")

                contentItem: Text {
                    text: root.heading("专辑", "album")
                    color: root.theme.mutedTextColor
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                }

                background: Rectangle {
                    color: albumHeading.hovered ? root.theme.hoverColor : "transparent"
                    radius: 4
                }
            }

            Button {
                id: durationHeading

                Layout.preferredWidth: 48
                Layout.preferredHeight: 28
                enabled: root.sortable
                onClicked: if (root.sortable) root.trackModel.set_sort("duration")

                contentItem: Text {
                    text: root.heading("时长", "duration")
                    color: root.theme.mutedTextColor
                    horizontalAlignment: Text.AlignRight
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 12
                }

                background: Rectangle {
                    color: durationHeading.hovered ? root.theme.hoverColor : "transparent"
                    radius: 4
                }
            }
        }

        ListView {
            id: trackList

            Layout.fillWidth: true
            Layout.fillHeight: true
            model: root.trackModel
            clip: true
            spacing: 2
            boundsBehavior: Flickable.StopAtBounds

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
                color: rowHover.hovered ? root.theme.hoverColor : "transparent"
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

                HoverHandler {
                    id: rowHover
                }

                TapHandler {
                    acceptedButtons: Qt.LeftButton
                    onDoubleTapped: root.trackActivated(trackRow.index)
                }
            }
        }
    }
}