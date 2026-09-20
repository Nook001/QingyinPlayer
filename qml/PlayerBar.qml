pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property var playerBackend

    focus: true

    readonly property real progressInset: root.height / 2
    readonly property int progressLineHeight: 2
    property real lastAudibleVolume: 0.7
    readonly property bool muted: root.playerBackend.player_volume <= 0.001

    function toggleMute() {
        if (root.muted) {
            const restore = root.lastAudibleVolume > 0.001 ? root.lastAudibleVolume : 0.7
            root.playerBackend.set_player_volume(restore)
        } else {
            if (root.playerBackend.player_volume > 0.001)
                root.lastAudibleVolume = root.playerBackend.player_volume
            root.playerBackend.set_player_volume(0)
        }
        root.playerBackend.flush_volume()
    }

    Component.onCompleted: {
        if (root.playerBackend.player_volume > 0.001)
            root.lastAudibleVolume = root.playerBackend.player_volume
    }

    RoundedRect {
        anchors.fill: parent
        color: root.theme.surfaceColor
        radius: height / 2
        borderColor: root.theme.dividerColor
        borderWidth: 1
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.AllButtons
        onWheel: function(wheel) {
            wheel.accepted = true
        }
    }

    Item {
        id: progressEdge

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: root.progressInset
        anchors.rightMargin: root.progressInset
        anchors.top: parent.top
        height: root.progressLineHeight
        z: 4

        RoundedRect {
            anchors.fill: parent
            radius: height / 2
            color: root.theme.dividerColor

            RoundedRect {
                width: progressSlider.visualPosition * parent.width
                height: parent.height
                radius: height / 2
                color: root.theme.accentColor
            }
        }
    }

    Slider {
        id: progressSlider

        anchors.left: progressEdge.left
        anchors.right: progressEdge.right
        anchors.top: parent.top
        height: 16
        z: 5
        padding: 0
        from: 0
        to: Math.max(1, root.playerBackend.playback_duration)
        enabled: root.playerBackend.current_title !== ""
            && root.playerBackend.playback_duration > 0
        live: true
        Keys.onLeftPressed: {
            const position = Math.max(0, Math.round(value) - 5000)
            Qt.callLater(() => root.playerBackend.seek_to(position))
        }
        Keys.onRightPressed: {
            const position = Math.min(to, Math.round(value) + 5000)
            Qt.callLater(() => root.playerBackend.seek_to(position))
        }
        onPressedChanged: if (!pressed && enabled) {
            const position = Math.round(value)
            Qt.callLater(() => root.playerBackend.seek_to(position))
        }

        Binding {
            target: progressSlider
            property: "value"
            value: root.playerBackend.playback_position
            when: !progressSlider.pressed
            restoreMode: Binding.RestoreNone
        }

        background: Item {
            implicitHeight: 16
        }

        handle: Item {
            implicitWidth: 1
            implicitHeight: 1
            visible: false
        }
    }

    function formatTime(milliseconds) {
        const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000))
        const minutes = Math.floor(totalSeconds / 60)
        const seconds = totalSeconds % 60
        return `${minutes}:${String(seconds).padStart(2, "0")}`
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
        anchors.leftMargin: 16
        anchors.right: transportColumn.left
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 10

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

    Row {
        id: transportColumn
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        spacing: 10

        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: 38
            text: root.formatTime(progressSlider.pressed
                ? progressSlider.value
                : root.playerBackend.playback_position)
            color: root.theme.mutedTextColor
            horizontalAlignment: Text.AlignRight
            font.pixelSize: 11
        }

        Row {
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

            FlatButton {
                preferredWidth: 36
                preferredHeight: 36
                iconName: {
                    if (root.playerBackend.play_mode === "shuffle")
                        return "shuffle"
                    if (root.playerBackend.play_mode === "repeatOne")
                        return "repeatOne"
                    return "repeat"
                }
                iconSize: 16
                theme: root.theme
                Accessible.name: {
                    if (root.playerBackend.play_mode === "shuffle")
                        return "随机播放"
                    if (root.playerBackend.play_mode === "repeatOne")
                        return "单曲循环"
                    return "顺序播放"
                }
                onClicked: Qt.callLater(function() { root.playerBackend.cycle_play_mode() })
            }
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: 38
            text: root.formatTime(root.playerBackend.playback_duration)
            color: root.theme.mutedTextColor
            font.pixelSize: 11
        }
    }

    Item {
        id: volumeCluster

        anchors.right: parent.right
        anchors.rightMargin: 10
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Math.round((root.height - volumeButton.preferredHeight) / 2)
        width: 36
        height: volumeButton.preferredHeight
            + (volumeCluster.expanded ? volumePanel.height + 6 : 0)
        z: 20
        clip: false

        readonly property bool expanded: volumeHover.hovered || volumeSlider.pressed

        HoverHandler {
            id: volumeHover
        }

        RoundedRect {
            id: volumePanel
            visible: volumeCluster.expanded
            anchors.top: parent.top
            anchors.horizontalCenter: parent.horizontalCenter
            width: 32
            height: 112
            radius: 16
            color: root.theme.surfaceColor
            borderColor: root.theme.dividerColor
            borderWidth: 1

            Slider {
                id: volumeSlider

                anchors.horizontalCenter: parent.horizontalCenter
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.topMargin: 10
                anchors.bottomMargin: 10
                width: 24
                orientation: Qt.Vertical
                from: 0
                to: 1
                onMoved: {
                    root.playerBackend.set_player_volume(value)
                    if (value > 0.001)
                        root.lastAudibleVolume = value
                }
                onPressedChanged: if (!pressed)
                    root.playerBackend.flush_volume()

                Binding {
                    target: volumeSlider
                    property: "value"
                    value: root.playerBackend.player_volume
                    when: !volumeSlider.pressed
                    restoreMode: Binding.RestoreNone
                }

                background: Rectangle {
                    x: volumeSlider.leftPadding + (volumeSlider.availableWidth - width) / 2
                    y: volumeSlider.topPadding
                    implicitWidth: 4
                    implicitHeight: 92
                    width: 4
                    height: volumeSlider.availableHeight
                    radius: 2
                    antialiasing: true
                    color: root.theme.dividerColor

                    Rectangle {
                        width: parent.width
                        height: (1 - volumeSlider.visualPosition) * parent.height
                        anchors.bottom: parent.bottom
                        radius: 2
                        antialiasing: true
                        color: root.theme.accentColor
                    }
                }

                handle: RoundedRect {
                    x: volumeSlider.leftPadding + (volumeSlider.availableWidth - width) / 2
                    y: volumeSlider.topPadding + volumeSlider.visualPosition
                        * (volumeSlider.availableHeight - height)
                    implicitWidth: 10
                    implicitHeight: 10
                    width: 10
                    height: 10
                    radius: 5
                    color: root.theme.accentColor
                }
            }
        }

        FlatButton {
            id: volumeButton
            anchors.bottom: parent.bottom
            anchors.horizontalCenter: parent.horizontalCenter
            preferredWidth: 36
            preferredHeight: 36
            iconName: root.muted ? "volumeMuted" : "volume"
            iconSize: 16
            theme: root.theme
            Accessible.name: root.muted ? "取消静音" : "静音"
            onClicked: root.toggleMute()
        }
    }
}
