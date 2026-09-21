pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

FocusScope {
    id: root
    required property var theme
    required property var playerBackend
    property bool expanded: false
    property int collapseDelay: 450
    property bool active: visible
    readonly property bool keyboardInteraction: previous.visualFocus || next.visualFocus
        || restore.visualFocus || progress.visualFocus || cover.visualFocus || play.visualFocus
    readonly property bool interacting: hover.hovered || keyboardInteraction || progress.pressed
    readonly property int desiredHeight: expanded ? 132 : 72
    readonly property string currentLyric: {
        if (!root.active || !root.playerBackend.lyrics_synchronized) return ""
        const lines = root.playerBackend.lyrics
        const position = root.playerBackend.playback_position
        let low = 0, high = lines.length
        while (low < high) {
            const middle = Math.floor((low + high) / 2)
            if (lines[middle].time <= position) low = middle + 1
            else high = middle
        }
        return low > 0 ? lines[low - 1].text.split("\n")[0] : ""
    }
    readonly property string displayText: currentLyric || playerBackend.current_title || "未在播放"
    signal restoreRequested()
    signal dragRequested()

    implicitWidth: 420
    implicitHeight: desiredHeight
    clip: true

    function formatTime(milliseconds) {
        const seconds = Math.max(0, Math.floor(milliseconds / 1000))
        return Math.floor(seconds / 60) + ":" + String(seconds % 60).padStart(2, "0")
    }
    onInteractingChanged: {
        if (interacting) {
            collapse.stop()
            expanded = true
        } else {
            collapse.restart()
        }
    }
    onActiveChanged: if (!active) {
        collapse.stop()
        expanded = false
    }
    Timer {
        id: collapse
        interval: root.collapseDelay
        onTriggered: if (!root.interacting) root.expanded = false
    }
    HoverHandler { id: hover }

    RoundedRect {
        anchors.fill: parent
        radius: 36
        color: root.theme.surfaceColor
        borderWidth: 1
        borderColor: root.theme.dividerColor
    }
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton
        cursorShape: Qt.SizeAllCursor
        onPressed: root.dragRequested()
    }

    Button {
        id: cover
        objectName: "capsuleCover"
        x: 14; y: 14
        width: 44; height: 44
        padding: 0
        hoverEnabled: true
        Accessible.name: "返回完整窗口"
        ToolTip.visible: hovered
        ToolTip.text: "返回完整窗口"
        onClicked: root.restoreRequested()
        contentItem: CoverImage {
            displaySize: 44
            theme: root.theme
            source: root.playerBackend.current_cover
            overlayVisible: cover.hovered || cover.visualFocus
            overlayIcon: "expand"
        }
        background: null
    }
    Text {
        objectName: "capsuleLyric"
        x: 72; y: 14
        width: play.x - x - 14
        height: 44
        text: root.displayText
        color: root.currentLyric ? root.theme.accentColor : root.theme.textColor
        font.pixelSize: 14
        font.weight: Font.Medium
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    FlatButton {
        id: play
        objectName: "capsulePlay"
        x: root.width - width - 16; y: 16
        preferredWidth: 40; preferredHeight: 40
        theme: root.theme
        emphasized: true
        enabled: root.playerBackend.current_track_id > 0
        iconName: root.playerBackend.playback_state === "playing" ? "pause" : "play"
        Accessible.name: root.playerBackend.playback_state === "playing" ? "暂停" : "播放"
        onClicked: root.playerBackend.toggle_playback()
    }

    Item {
        id: details
        objectName: "capsuleDetails"
        x: 24; y: 70
        width: root.width - 48
        height: 54
        opacity: root.expanded ? 1 : 0
        visible: opacity > 0
        enabled: root.expanded
        Behavior on opacity { NumberAnimation { duration: 140 } }

        RowLayout {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: 28
            spacing: 6
            Text {
                Layout.fillWidth: true
                text: root.playerBackend.current_title
                    + (root.playerBackend.current_artist ? " · " + root.playerBackend.current_artist : "")
                elide: Text.ElideRight
                color: root.theme.mutedTextColor
                font.pixelSize: 12
            }
            FlatButton {
                id: previous
                objectName: "capsulePrevious"
                preferredWidth: 28; preferredHeight: 28
                iconSize: 16
                theme: root.theme
                enabled: root.playerBackend.current_track_id > 0
                iconName: "skipBack"
                Accessible.name: "上一首"
                ToolTip.visible: hovered; ToolTip.text: Accessible.name
                onClicked: root.playerBackend.play_previous()
            }
            FlatButton {
                id: next
                objectName: "capsuleNext"
                preferredWidth: 28; preferredHeight: 28
                iconSize: 16
                theme: root.theme
                enabled: root.playerBackend.current_track_id > 0
                iconName: "skipForward"
                Accessible.name: "下一首"
                ToolTip.visible: hovered; ToolTip.text: Accessible.name
                onClicked: root.playerBackend.play_next()
            }
            FlatButton {
                id: restore
                objectName: "capsuleRestore"
                preferredWidth: 28; preferredHeight: 28
                iconSize: 16
                theme: root.theme
                iconName: "expand"
                Accessible.name: "返回完整窗口"
                ToolTip.visible: hovered; ToolTip.text: Accessible.name
                onClicked: root.restoreRequested()
            }
        }
        RowLayout {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            spacing: 8
            Text {
                text: root.formatTime(progress.pressed ? progress.value : root.playerBackend.playback_position)
                color: root.theme.mutedTextColor
                font.pixelSize: 10
            }
            Slider {
                id: progress
                objectName: "capsuleProgress"
                Layout.fillWidth: true
                implicitHeight: 20
                padding: 0
                from: 0
                to: Math.max(1, root.playerBackend.playback_duration)
                enabled: root.playerBackend.current_track_id > 0 && root.playerBackend.playback_duration > 0
                Accessible.name: "播放进度"
                onPressedChanged: if (!pressed && enabled) root.playerBackend.seek_to(Math.round(value))
                Keys.onLeftPressed: root.playerBackend.seek_to(Math.max(0, value - 5000))
                Keys.onRightPressed: root.playerBackend.seek_to(Math.min(to, value + 5000))
                Binding {
                    target: progress; property: "value"
                    value: root.playerBackend.playback_position
                    when: !progress.pressed
                    restoreMode: Binding.RestoreNone
                }
                background: Rectangle {
                    y: (progress.height - height) / 2
                    width: progress.width; height: 3; radius: 1.5
                    color: root.theme.dividerColor
                    Rectangle {
                        width: progress.visualPosition * parent.width
                        height: parent.height; radius: 1.5
                        color: root.theme.accentColor
                    }
                }
                handle: Rectangle {
                    width: 8; height: 8; radius: 4
                    x: progress.visualPosition * (progress.width - width)
                    y: (progress.height - height) / 2
                    color: root.theme.accentColor
                    visible: progress.hovered || progress.pressed || progress.visualFocus
                }
            }
            Text {
                text: root.formatTime(root.playerBackend.playback_duration)
                color: root.theme.mutedTextColor
                font.pixelSize: 10
            }
        }
    }
}
