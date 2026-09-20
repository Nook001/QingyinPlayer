pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "pointer.js" as Pointer

Item {
    id: root

    required property var theme
    required property var trackModel
    property bool sortable: true
    property string sortColumn
    property bool sortAscending: true
    property string debugLabel: "tracks"
    readonly property alias count: trackRepeater.count
    readonly property int doubleClickMs: 400

    signal trackActivated(int row)
    signal sortRequested(string column)

    function clampScroll() {
    }

    function heading(label, column) {
        if (!root.sortable || root.sortColumn !== column)
            return label
        return label + (root.sortAscending ? " ↑" : " ↓")
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
                objectName: "sortTitleButton"

                Layout.fillWidth: true
                Layout.preferredHeight: 28
                flat: true
                hoverEnabled: true
                enabled: root.sortable
                Accessible.name: "按歌曲名排序"
                onClicked: if (root.sortable)
                    root.sortRequested("title")

                contentItem: Text {
                    text: root.heading("歌曲名", "title")
                    color: root.theme.mutedTextColor
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 12
                }

                background: Rectangle {
                    radius: 4
                    color: titleHeading.hovered && titleHeading.enabled
                        ? root.theme.hoverColor : "transparent"
                }
            }

            Button {
                id: albumHeading
                objectName: "sortAlbumButton"

                Layout.preferredWidth: 190
                Layout.preferredHeight: 28
                flat: true
                hoverEnabled: true
                enabled: root.sortable
                Accessible.name: "按专辑排序"
                onClicked: if (root.sortable)
                    root.sortRequested("album")

                contentItem: Text {
                    text: root.heading("专辑", "album")
                    color: root.theme.mutedTextColor
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 12
                }

                background: Rectangle {
                    radius: 4
                    color: albumHeading.hovered && albumHeading.enabled
                        ? root.theme.hoverColor : "transparent"
                }
            }

            Button {
                id: durationHeading
                objectName: "sortDurationButton"

                Layout.preferredWidth: 48
                Layout.preferredHeight: 28
                flat: true
                hoverEnabled: true
                enabled: root.sortable
                Accessible.name: "按时长排序"
                onClicked: if (root.sortable)
                    root.sortRequested("duration")

                contentItem: Text {
                    text: root.heading("时长", "duration")
                    color: root.theme.mutedTextColor
                    horizontalAlignment: Text.AlignRight
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 12
                }

                background: Rectangle {
                    radius: 4
                    color: durationHeading.hovered && durationHeading.enabled
                        ? root.theme.hoverColor : "transparent"
                }
            }
        }

        ScrollView {
            id: trackScroll

            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            contentWidth: availableWidth
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

            Column {
                width: trackScroll.availableWidth
                spacing: 2

                Repeater {
                    id: trackRepeater
                    model: root.trackModel

                    delegate: ItemDelegate {
                        id: trackRow

                        required property int index
                        required property string title
                        required property string artist
                        required property string album
                        required property string duration
                        required property string cover

                        property real lastClickAt: 0

                        width: parent ? parent.width : 0
                        implicitHeight: 58
                        padding: 0
                        hoverEnabled: true
                        text: trackRow.title
                        objectName: root.debugLabel + "-" + trackRow.index
                        Accessible.name: trackRow.title

                        onIndexChanged: trackRow.lastClickAt = 0

                        background: Rectangle {
                            color: trackRow.hovered ? root.theme.hoverColor : "transparent"
                            radius: 5
                        }

                        contentItem: RowLayout {
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

                        onClicked: {
                            Pointer.debug("clicked", trackRow.objectName, "row=" + trackRow.index)
                            const now = Date.now()
                            if (trackRow.lastClickAt !== 0 && (now - trackRow.lastClickAt) <= root.doubleClickMs) {
                                trackRow.lastClickAt = 0
                                const row = trackRow.index
                                Qt.callLater(function() { root.trackActivated(row) })
                            } else {
                                trackRow.lastClickAt = now
                            }
                        }
                    }
                }
            }
        }
    }
}
