pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property var playerBackend

    focus: true

    RoundedRect {
        anchors.fill: parent
        color: root.theme.surfaceColor
        radius: height / 2
        borderColor: root.theme.dividerColor
        borderWidth: 1
    }

    function formatTime(milliseconds) {
        const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000))
        const minutes = Math.floor(totalSeconds / 60)
        const seconds = totalSeconds % 60
        return minutes + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    Keys.onLeftPressed: {
        if (root.playerBackend.current_title !== "")
            root.playerBackend.seek_to(root.playerBackend.playback_position - 5000)
    }
    Keys.onRightPressed: {
        if (root.playerBackend.current_title !== "")
            root.playerBackend.seek_to(root.playerBackend.playback_position + 5000)
    }

    RowLayout {
        anchors.left: parent.left
        anchors.leftMargin: 22
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(250, Math.max(150,
            (parent.width - Math.min(420, parent.width * 0.42)) / 2 - 44))
        spacing: 12

        CoverImage {
            displaySize: 48
            theme: root.theme
            source: root.playerBackend.current_cover
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
        spacing: 6

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

                Layout.fillWidth: true
                from: 0
                to: Math.max(1, root.playerBackend.playback_duration)
                enabled: root.playerBackend.current_title !== ""
                    && root.playerBackend.playback_duration > 0
                focus: true
                Keys.onLeftPressed: {
                    const position = Math.max(0, Math.round(value) - 5000)
                    Qt.callLater(function() { root.playerBackend.seek_to(position) })
                }
                Keys.onRightPressed: {
                    const position = Math.min(to, Math.round(value) + 5000)
                    Qt.callLater(function() { root.playerBackend.seek_to(position) })
                }
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
                    antialiasing: true
                    color: root.theme.dividerColor

                    Rectangle {
                        width: progressSlider.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: root.theme.accentColor
                    }
                }

                handle: RoundedRect {
                    x: progressSlider.leftPadding + progressSlider.visualPosition
                        * (progressSlider.availableWidth - width)
                    y: progressSlider.topPadding + (progressSlider.availableHeight - height) / 2
                    implicitWidth: 10
                    implicitHeight: 10
                    width: 10
                    height: 10
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

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 8

            FlatButton {
                preferredWidth: 36
                preferredHeight: 36
                iconName: "skipBack"
                iconSize: 18
                theme: root.theme
                enabled: root.playerBackend.current_title !== ""
                Accessible.name: "上一首"
                onClicked: Qt.callLater(function() { root.playerBackend.play_previous() })
            }

            FlatButton {
                preferredWidth: 36
                preferredHeight: 36
                iconName: root.playerBackend.playback_state === "playing" ? "pause" : "play"
                iconSize: 16
                emphasized: true
                theme: root.theme
                enabled: root.playerBackend.current_title !== ""
                Accessible.name: root.playerBackend.playback_state === "playing" ? "暂停" : "播放"
                onClicked: Qt.callLater(function() { root.playerBackend.toggle_playback() })
            }

            FlatButton {
                preferredWidth: 36
                preferredHeight: 36
                iconName: "skipForward"
                iconSize: 18
                theme: root.theme
                enabled: root.playerBackend.current_title !== ""
                Accessible.name: "下一首"
                onClicked: Qt.callLater(function() { root.playerBackend.play_next() })
            }
        }
    }

    RowLayout {
        anchors.right: parent.right
        anchors.rightMargin: 22
        anchors.verticalCenter: parent.verticalCenter
        width: 130
        spacing: 8

        Icon {
            name: "volume"
            size: 16
            color: root.theme.mutedTextColor
        }

        Slider {
            id: volumeSlider

            Layout.fillWidth: true
            from: 0
            to: 1
            enabled: root.playerBackend.current_title !== ""
            value: root.playerBackend.player_volume
            onMoved: root.playerBackend.set_player_volume(value)
            onPressedChanged: if (!pressed)
                root.playerBackend.flush_volume()

            background: Rectangle {
                x: volumeSlider.leftPadding
                y: volumeSlider.topPadding + (volumeSlider.availableHeight - height) / 2
                implicitHeight: 4
                width: volumeSlider.availableWidth
                height: 4
                radius: 2
                antialiasing: true
                color: root.theme.dividerColor

                Rectangle {
                    width: volumeSlider.visualPosition * parent.width
                    height: parent.height
                    radius: 2
                    antialiasing: true
                    color: root.theme.accentColor
                }
            }

            handle: RoundedRect {
                x: volumeSlider.leftPadding + volumeSlider.visualPosition
                    * (volumeSlider.availableWidth - width)
                y: volumeSlider.topPadding + (volumeSlider.availableHeight - height) / 2
                implicitWidth: 10
                implicitHeight: 10
                width: 10
                height: 10
                radius: 5
                visible: volumeSlider.enabled
                color: root.theme.accentColor
            }
        }
    }
}
