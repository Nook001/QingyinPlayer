pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtTest

Item {
    id: root
    width: 640
    height: 360
    Theme { id: theme; darkTheme: false }
    QtObject {
        id: player
        property string current_title: "夜曲"
        property string current_artist: "示例歌手"
        property string current_cover: ""
        property int current_track_id: 1
        property string playback_state: "playing"
        property int playback_position: 1500
        property int playback_duration: 60000
        property bool lyrics_synchronized: true
        property var lyrics: [{time: 0, text: "第一句\nFirst line"}, {time: 1000, text: "第二句"}]
        property int previousCalls: 0
        property int nextCalls: 0
        property int seekPosition: -1
        function toggle_playback() { playback_state = playback_state === "playing" ? "paused" : "playing" }
        function play_previous() { ++previousCalls }
        function play_next() { ++nextCalls }
        function seek_to(value) { seekPosition = value; playback_position = value }
    }
    CapsulePlayer {
        id: capsule
        x: 80; y: 80
        width: 420
        height: desiredHeight
        theme: theme
        playerBackend: player
        collapseDelay: 50
    }
    Component {
        id: windowComponent
        CapsuleWindow { theme: theme; playerBackend: player }
    }
    SignalSpy { id: restoreSpy; target: capsule; signalName: "restoreRequested" }
    SignalSpy { id: dragSpy; target: capsule; signalName: "dragRequested" }
    TestCase {
        name: "CapsuleInteractions"
        when: windowShown
        function init() {
            mouseMove(root, 10, 10)
            capsule.active = true
            capsule.expanded = false
            player.current_track_id = 1
            player.playback_state = "playing"
            player.lyrics_synchronized = true
            player.playback_position = 1500
            player.previousCalls = 0
            player.nextCalls = 0
            player.seekPosition = -1
            restoreSpy.clear()
            dragSpy.clear()
        }
        function cleanup() {
            root.forceActiveFocus()
            mouseMove(root, 10, 10)
            capsule.active = false
        }
        function test_current_lyric_and_fallback() {
            compare(capsule.displayText, "第二句")
            player.playback_position = 0
            compare(capsule.displayText, "第一句")
            player.lyrics_synchronized = false
            compare(capsule.displayText, "夜曲")
        }
        function test_hover_expands_without_moving_play_button() {
            const play = findChild(capsule, "capsulePlay")
            const x = play.x, y = play.y
            compare(capsule.height, 72)
            mouseMove(capsule, 180, 30)
            tryCompare(capsule, "expanded", true)
            compare(capsule.height, 132)
            compare(play.x, x)
            compare(play.y, y)
            mouseMove(root, 10, 10)
            compare(capsule.expanded, true)
            tryCompare(capsule, "expanded", false)
            compare(capsule.height, 72)
        }
        function test_play_navigation_and_restore() {
            mouseClick(findChild(capsule, "capsulePlay"))
            compare(player.playback_state, "paused")
            compare(dragSpy.count, 0)
            capsule.expanded = true
            wait(180)
            mouseClick(findChild(capsule, "capsulePrevious"))
            mouseClick(findChild(capsule, "capsuleNext"))
            compare(player.previousCalls, 1)
            compare(player.nextCalls, 1)
            mouseClick(findChild(capsule, "capsuleRestore"))
            compare(restoreSpy.count, 1)
            mouseClick(findChild(capsule, "capsuleCover"))
            compare(restoreSpy.count, 2)
        }
        function test_drag_only_from_background() {
            mousePress(capsule, 180, 30)
            compare(dragSpy.count, 1)
            mouseRelease(capsule, 180, 30)
        }
        function test_keyboard_and_seek_keep_expanded() {
            const progress = findChild(capsule, "capsuleProgress")
            capsule.expanded = true
            wait(180)
            progress.forceActiveFocus(Qt.TabFocusReason)
            mouseMove(root, 10, 10)
            wait(100)
            verify(capsule.expanded)
            keyClick(Qt.Key_Right)
            compare(player.seekPosition, 6500)
            root.forceActiveFocus()
            tryCompare(capsule, "expanded", false)
        }
        function test_seek_drag_does_not_collapse() {
            capsule.expanded = true
            wait(180)
            const progress = findChild(capsule, "capsuleProgress")
            mousePress(progress, progress.width / 2, progress.height / 2)
            mouseMove(root, 10, 10)
            wait(100)
            verify(capsule.expanded)
            mouseRelease(root, 10, 10)
            verify(player.seekPosition >= 0)
        }
        function test_no_track_and_inactive() {
            player.current_track_id = 0
            verify(!findChild(capsule, "capsulePlay").enabled)
            verify(!findChild(capsule, "capsuleNext").enabled)
            capsule.active = false
            compare(capsule.currentLyric, "")
            compare(capsule.expanded, false)
        }
        function test_window_flags_and_close_restores() {
            const window = createTemporaryObject(windowComponent, root)
            verify(window !== null)
            verify((window.flags & Qt.FramelessWindowHint) !== 0)
            compare(window.transientParent, null)
            compare(window.width, 420)
            compare(window.height, 72)
            let restores = 0
            window.restoreRequested.connect(() => ++restores)
            window.show()
            window.close()
            compare(restores, 1)
            window.hide()
        }
    }
}
