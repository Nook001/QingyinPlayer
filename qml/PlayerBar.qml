pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root

    required property var theme
    required property var playerBackend

    color: root.theme.surfaceColor

    function formatTime(milliseconds) {
        const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000))
        const minutes = Math.floor(totalSeconds / 60)
        const seconds = totalSeconds % 60
        return minutes + ":" + (seconds < 10 ? "0" : "") + seconds
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

            Button {
                id: playPreviousButton
                objectName: "playPreviousButton"

                implicitWidth: 44
                implicitHeight: 40
                flat: true
                hoverEnabled: true
                enabled: root.playerBackend.current_title !== ""
                Accessible.name: "上一首"
                onClicked: Qt.callLater(function() { root.playerBackend.play_previous() })

                contentItem: Text {
                    text: "◀|"
                    color: root.theme.textColor
                    opacity: playPreviousButton.enabled ? 1 : 0.35
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 16
                }

                background: Rectangle {
                    color: playPreviousButton.hovered && playPreviousButton.enabled
                        ? root.theme.hoverColor : "transparent"
                    radius: 4
                }
            }

            Button {
                id: togglePlaybackButton
                objectName: "togglePlaybackButton"

                implicitWidth: 46
                implicitHeight: 46
                flat: true
                hoverEnabled: true
                enabled: root.playerBackend.current_title !== ""
                Accessible.name: root.playerBackend.playback_state === "playing" ? "暂停" : "播放"
                onClicked: Qt.callLater(function() { root.playerBackend.toggle_playback() })

                contentItem: Text {
                    text: root.playerBackend.playback_state === "playing" ? "Ⅱ" : "▶"
                    color: root.theme.textColor
                    opacity: togglePlaybackButton.enabled ? 1 : 0.35
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 18
                }

                background: Rectangle {
                    color: togglePlaybackButton.hovered && togglePlaybackButton.enabled
                        ? root.theme.hoverColor : "transparent"
                    radius: 4
                }
            }

            Button {
                id: playNextButton
                objectName: "playNextButton"

                implicitWidth: 44
                implicitHeight: 40
                flat: true
                hoverEnabled: true
                enabled: root.playerBackend.current_title !== ""
                Accessible.name: "下一首"
                onClicked: Qt.callLater(function() { root.playerBackend.play_next() })

                contentItem: Text {
                    text: "|▶"
                    color: root.theme.textColor
                    opacity: playNextButton.enabled ? 1 : 0.35
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 16
                }

                background: Rectangle {
                    color: playNextButton.hovered && playNextButton.enabled
                        ? root.theme.hoverColor : "transparent"
                    radius: 4
                }
            }
        }

        RowLayout {
            width: parent.width
            spacing: 8

            Text {
                Layout.preferredWidth: 38
                text: root.formatTime(progressSlider.pressed
                    ? progressSlider.value
                    : root.playerBackend.playback_position)
                color: root.theme.mutedTextColor
                horizontalAlignment: Text.AlignRight
                font.pixelSize: 11
            }

            Slider {
                id: progressSlider
                objectName: "progressSlider"

                Layout.fillWidth: true
                from: 0
                to: Math.max(1, root.playerBackend.playback_duration)
                enabled: root.playerBackend.current_title !== ""
                    && root.playerBackend.playback_duration > 0
                onPressedChanged: if (!pressed && enabled) {
                    const position = Math.round(value)
                    Qt.callLater(function() { root.playerBackend.seek_to(position) })
                }

                Binding {
                    target: progressSlider
                    property: "value"
                    value: root.playerBackend.playback_position
                    when: !progressSlider.pressed
                    restoreMode: Binding.RestoreNone
                }

                background: Rectangle {
                    x: progressSlider.leftPadding
                    y: progressSlider.topPadding + (progressSlider.availableHeight - height) / 2
                    implicitHeight: 4
                    width: progressSlider.availableWidth
                    height: 4
                    radius: 2
                    color: root.theme.dividerColor

                    Rectangle {
                        width: progressSlider.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: root.theme.accentColor
                    }
                }

                handle: Rectangle {
                    x: progressSlider.leftPadding + progressSlider.visualPosition
                        * (progressSlider.availableWidth - width)
                    y: progressSlider.topPadding + (progressSlider.availableHeight - height) / 2
                    implicitWidth: 10
                    implicitHeight: 10
                    radius: 5
                    visible: progressSlider.enabled
                    color: root.theme.accentColor
                }
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
            id: volumeSlider
            objectName: "volumeSlider"

            Layout.fillWidth: true
            from: 0
            to: 1
            enabled: root.playerBackend.current_title !== ""
            value: root.playerBackend.player_volume
            onMoved: root.playerBackend.set_player_volume(value)

            background: Rectangle {
                x: volumeSlider.leftPadding
                y: volumeSlider.topPadding + (volumeSlider.availableHeight - height) / 2
                implicitHeight: 4
                width: volumeSlider.availableWidth
                height: 4
                radius: 2
                color: root.theme.dividerColor

                Rectangle {
                    width: volumeSlider.visualPosition * parent.width
                    height: parent.height
                    radius: 2
                    color: root.theme.accentColor
                }
            }

            handle: Rectangle {
                x: volumeSlider.leftPadding + volumeSlider.visualPosition
                    * (volumeSlider.availableWidth - width)
                y: volumeSlider.topPadding + (volumeSlider.availableHeight - height) / 2
                implicitWidth: 10
                implicitHeight: 10
                radius: 5
                visible: volumeSlider.enabled
                color: root.theme.accentColor
            }
        }
    }
}
