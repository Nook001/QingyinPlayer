pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property var trackModel
    property bool sortable: true
    property string sortColumn
    property bool sortAscending: true
    property real savedContentY: 0
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

            FlatButton {
                Layout.fillWidth: true
                preferredHeight: 28
                theme: root.theme
                enabled: root.sortable
                text: root.heading("歌曲名", "title")
                Accessible.name: "按歌曲名排序"
                onClicked: if (root.sortable)
                    root.sortRequested("title")
            }

            FlatButton {
                Layout.preferredWidth: 190
                preferredHeight: 28
                theme: root.theme
                enabled: root.sortable
                text: root.heading("专辑", "album")
                Accessible.name: "按专辑排序"
                onClicked: if (root.sortable)
                    root.sortRequested("album")
            }

            FlatButton {
                Layout.preferredWidth: 48
                preferredHeight: 28
                theme: root.theme
                enabled: root.sortable
                text: root.heading("时长", "duration")
                Accessible.name: "按时长排序"
                onClicked: if (root.sortable)
                    root.sortRequested("duration")
            }
        }

        ListView {
            id: trackList

            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            reuseItems: true
            cacheBuffer: 580
            boundsBehavior: Flickable.StopAtBounds
            model: root.trackModel
            spacing: 2

            delegate: ItemDelegate {
                id: trackRow

                required property int index
                required property string title
                required property string artist
                required property string album
                required property string duration
                required property string cover
                required property int trackId

                width: ListView.view ? ListView.view.width : 0
                height: 58
                padding: 0
                hoverEnabled: true
                text: trackRow.title
                Accessible.name: trackRow.title

                ListView.onPooled: trackRow.highlighted = false
                ListView.onReused: trackRow.highlighted = false

                background: Rectangle {
                    color: trackRow.hovered ? root.theme.hoverColor : "transparent"
                    radius: 5
                }

                contentItem: RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 12
                    spacing: 14

                    CoverImage {
                        displaySize: 44
                        theme: root.theme
                        source: trackRow.cover
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

                onDoubleClicked: {
                    const id = trackRow.trackId
                    Qt.callLater(function() { root.trackActivated(id) })
                }
            }
        }
    }
}
