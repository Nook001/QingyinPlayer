pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Window

Window {
    id: root
    required property var theme
    required property var playerBackend
    signal restoreRequested()
    signal activityChanged()
    transientParent: null
    flags: Qt.Window | Qt.FramelessWindowHint
    color: "transparent"
    title: "清音 · 胶囊"
    width: 420
    minimumWidth: 420
    maximumWidth: 420
    height: capsule.desiredHeight
    minimumHeight: 72
    maximumHeight: 132
    visible: false
    Behavior on height {
        enabled: root.visible
        NumberAnimation { duration: 160; easing.type: Easing.OutCubic }
    }
    onVisibilityChanged: root.activityChanged()
    onClosing: function(close) {
        close.accepted = false
        root.restoreRequested()
    }
    Shortcut {
        sequence: "Escape"
        enabled: root.visible
        onActivated: root.restoreRequested()
    }
    Shortcut {
        sequence: "Space"
        enabled: root.visible && !(root.activeFocusItem instanceof AbstractButton)
        onActivated: root.playerBackend.toggle_playback()
    }
    CapsulePlayer {
        id: capsule
        anchors.fill: parent
        theme: root.theme
        playerBackend: root.playerBackend
        active: root.visible && root.visibility !== Window.Minimized
        onRestoreRequested: root.restoreRequested()
        onDragRequested: root.startSystemMove()
    }
}
