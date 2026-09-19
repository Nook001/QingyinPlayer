pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root

    required property var theme
    required property real cornerRadius
    required property var playerBackend

    color: root.theme.surfaceColor
    radius: root.cornerRadius

    function formatTime(milliseconds) {
        const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000))
        const minutes = Math.floor(totalSeconds / 60)
        const seconds = totalSeconds % 60
        return minutes + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    Rectangle {
        anchors.left: parent.left
        width: root.cornerRadius
        height: parent.height
        color: parent.color
    }

    Rectangle {
        anchors.top: parent.top
        width: parent.width
        height: root.cornerRadius
        color: parent.color
    }

    Rectangle {
        anchors.top: parent.top
        width: parent.width
        height: 1
        color: root.theme.dividerColor
    }

    RowLayout {
        anchors.left: parent.left
        anchors.leftMargin: 22
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(250, Math.max(150,
            (parent.width - Math.min(420, parent.width * 0.42)) / 2 - 44))
        spacing: 14

        Rectangle {
            Layout.preferredWidth: 54
            Layout.preferredHeight: 54
            color: root.theme.artworkColor
            radius: 5
            clip: true

            Image {
                id: currentCover
                anchors.fill: parent
                source: root.playerBackend.current_cover
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                visible: status === Image.Ready
            }

            Text {
                anchors.centerIn: parent
                text: "♫"
                color: root.theme.accentColor
                font.pixelSize: 23
                visible: currentCover.status !== Image.Ready
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 3

            Text {
                Layout.fillWidth: true
                text: root.playerBackend.current_title || "未在播放"
                elide: Text.ElideRight
                color: root.theme.textColor
                font.pixelSize: 14
                font.weight: Font.DemiBold
            }

            Text {
                Layout.fillWidth: true
                text: root.playerBackend.playback_error
                    || root.playerBackend.current_artist
                    || "从曲库中选择一首歌曲"
                elide: Text.ElideRight
                color: root.playerBackend.playback_error
                    ? root.theme.accentPressedColor : root.theme.mutedTextColor
                font.pixelSize: 12
                ToolTip.visible: truncated && mouseArea.containsMouse
                ToolTip.text: text

                MouseArea {
                    id: mouseArea
                    anchors.fill: parent
                    hoverEnabled: true
                }
            }
        }
    }

    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(420, parent.width * 0.42)
        spacing: 7

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 8

            ToolButton {
                text: "◀|"
                enabled: root.playerBackend.current_title !== ""
                onClicked: root.playerBackend.play_previous()
                ToolTip.visible: hovered
                ToolTip.text: "上一首"
            }

            RoundButton {
                width: 46
                height: 46
                text: root.playerBackend.playback_state === "playing" ? "Ⅱ" : "▶"
                enabled: root.playerBackend.current_title !== ""
                onClicked: root.playerBackend.toggle_playback()
                ToolTip.visible: hovered
                ToolTip.text: root.playerBackend.playback_state === "playing" ? "暂停" : "播放"
            }

            ToolButton {
                text: "|▶"
                enabled: root.playerBackend.current_title !== ""
                onClicked: root.playerBackend.play_next()
                ToolTip.visible: hovered
                ToolTip.text: "下一首"
            }
        }

        RowLayout {
            width: parent.width
            spacing: 8

            Text {
                Layout.preferredWidth: 38
                text: root.formatTime(progressSlider.pressed
                    ? progressSlider.dragPosition
                    : root.playerBackend.playback_position)
                color: root.theme.mutedTextColor
                horizontalAlignment: Text.AlignRight
                font.pixelSize: 11
            }

            Slider {
                id: progressSlider

                property real dragPosition: 0

                Layout.fillWidth: true
                from: 0
                to: Math.max(1, root.playerBackend.playback_duration)
                value: pressed ? dragPosition : root.playerBackend.playback_position
                enabled: root.playerBackend.current_title !== ""
                    && root.playerBackend.playback_duration > 0
                onPressedChanged: {
                    if (pressed) {
                        dragPosition = root.playerBackend.playback_position
                    } else {
                        root.playerBackend.seek_to(Math.round(dragPosition))
                    }
                }
                onMoved: dragPosition = value
            }

            Text {
                Layout.preferredWidth: 38
                text: root.formatTime(root.playerBackend.playback_duration)
                color: root.theme.mutedTextColor
                font.pixelSize: 11
            }
        }
    }

    RowLayout {
        anchors.right: parent.right
        anchors.rightMargin: 22
        anchors.verticalCenter: parent.verticalCenter
        width: 130
        spacing: 8

        Text {
            text: "🔊"
            color: root.theme.mutedTextColor
            font.pixelSize: 15
        }

        Slider {
            Layout.fillWidth: true
            from: 0
            to: 1
            value: root.playerBackend.player_volume
            enabled: root.playerBackend.current_title !== ""
            onMoved: root.playerBackend.set_player_volume(value)
        }
    }
}