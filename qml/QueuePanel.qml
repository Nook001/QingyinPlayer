pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property var playerBackend

    signal closeRequested()

    RoundedRect {
        anchors.fill: parent
        radius: 16
        color: root.theme.surfaceColor
        borderColor: root.theme.dividerColor
        borderWidth: 1
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        RowLayout {
            Layout.fillWidth: true

            Text {
                text: "正在播放"
                color: root.theme.textColor
                font.pixelSize: root.theme.bodySize
                font.weight: Font.DemiBold
            }

            Text {
                text: root.playerBackend.play_mode === "shuffle"
                    ? "随机顺序 · 下一首在下一行"
                    : queueList.count + " 首"
                color: root.theme.mutedTextColor
                font.pixelSize: root.theme.metaSize
                visible: queueList.count > 0
            }

            Item { Layout.fillWidth: true }

            FlatButton {
                theme: root.theme
                iconName: "close"
                iconSize: 16
                preferredWidth: 32
                preferredHeight: 32
                Accessible.name: "关闭正在播放"
                onClicked: root.closeRequested()
            }
        }

        ListView {
            id: queueList

            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            reuseItems: true
            boundsBehavior: Flickable.StopAtBounds
            model: root.playerBackend.queue_model
            spacing: 2
            readonly property int currentRow: root.playerBackend.current_queue_index
            onCurrentRowChanged: if (currentRow >= 0)
                positionViewAtIndex(currentRow, ListView.Contain)

            displaced: Transition {
                NumberAnimation { properties: "y"; duration: 120; easing.type: Easing.OutCubic }
            }

            delegate: Item {
                id: queueRow

                required property int index
                required property string title
                required property string artist
                required property string duration
                required property int trackId

                readonly property bool isCurrent: index === root.playerBackend.current_queue_index
                width: queueList.width
                height: 52

                RoundedRect {
                    anchors.fill: parent
                    radius: 8
                    color: queueRow.isCurrent ? root.theme.subtleColor
                        : (rowHover.hovered ? root.theme.hoverColor : "transparent")
                }

                Item {
                    id: rowBody
                    width: parent.width
                    height: parent.height

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 4
                        anchors.rightMargin: 8
                        spacing: 8

                        Item {
                            Layout.preferredWidth: 28
                            Layout.fillHeight: true

                            Icon {
                                anchors.centerIn: parent
                                name: "list"
                                size: 14
                                color: root.theme.mutedTextColor
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.SizeVerCursor
                                drag.target: rowBody
                                drag.axis: Drag.YAxis
                                drag.threshold: 6
                                preventStealing: true
                                onPressed: queueRow.z = 2
                                onReleased: {
                                    const slot = queueRow.height + queueList.spacing
                                    let destination = queueRow.index + Math.round(rowBody.y / slot)
                                    destination = Math.max(0, Math.min(queueList.count - 1, destination))
                                    rowBody.y = 0
                                    queueRow.z = 1
                                    if (destination !== queueRow.index)
                                        root.playerBackend.move_queue_row(queueRow.index, destination)
                                }
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2

                            TapHandler {
                                onTapped: root.playerBackend.play_queue_row(queueRow.index)
                            }

                            Text {
                                Layout.fillWidth: true
                                text: queueRow.title
                                color: queueRow.isCurrent ? root.theme.accentColor : root.theme.textColor
                                elide: Text.ElideRight
                                font.pixelSize: root.theme.bodySize
                                font.weight: Font.DemiBold
                            }

                            Text {
                                Layout.fillWidth: true
                                text: queueRow.artist || "未知歌手"
                                color: root.theme.mutedTextColor
                                elide: Text.ElideRight
                                font.pixelSize: root.theme.metaSize
                            }
                        }

                        Text {
                            text: queueRow.duration
                            color: root.theme.mutedTextColor
                            font.pixelSize: root.theme.digitSize
                            font.family: root.theme.digitFamily
                        }

                        FlatButton {
                            theme: root.theme
                            iconName: "close"
                            iconSize: 14
                            preferredWidth: 28
                            preferredHeight: 28
                            Accessible.name: "从正在播放移除"
                            onClicked: root.playerBackend.remove_queue_row(queueRow.index)
                        }
                    }

                }

                HoverHandler {
                    id: rowHover
                }
            }

            Text {
                anchors.centerIn: parent
                visible: queueList.count === 0
                text: "从曲库点一首歌后，播放顺序会显示在这里"
                color: root.theme.mutedTextColor
                font.pixelSize: root.theme.metaSize
            }
        }
    }
}
