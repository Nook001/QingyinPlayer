import QtQuick
import QtQuick.Controls
import Qingyin 1.0

ApplicationWindow {
    id: root
    width: 1180
    height: 760
    visible: true
    color: theme.backgroundColor
    property string result: "pending"
    property int phase: 0
    property int trackId: 0
    function testResult() { return result }
    function finish(message) {
        result = message
        steps.stop()
        timeout.stop()
        backend.shutdown()
        Qt.quit()
    }
    Theme { id: theme; darkTheme: false }
    AppBridge { id: backend }
    Component.onCompleted: {
        // Instantiate the nested QObjects before starting callbacks, as Main's library view does.
        if (!backend.library.busy) backend.restore_session()
    }
    Loader {
        id: page
        anchors.fill: parent
        active: false
        sourceComponent: NowPlaying { theme: theme; playerBackend: backend.playback }
    }
    PlayerBar {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 12
        width: 720
        height: 80
        theme: theme
        playerBackend: backend.playback
        onNowPlayingRequested: page.active = true
    }
    Timer {
        id: timeout
        interval: 15000
        running: true
        onTriggered: root.finish("timeout in phase " + root.phase + ": " + backend.playback.playback_error + "; " + backend.library.scan_status + "; position=" + backend.playback.playback_position + "; state=" + backend.playback.playback_state + "; line=" + (page.item ? page.item.activeLine : -99) + "; loading=" + backend.playback.lyrics_loading + "; lyrics=" + backend.playback.lyrics.length)
    }
    Timer {
        id: steps
        interval: 50
        running: true
        repeat: true
        onTriggered: {
            const playback = backend.playback
            if (root.phase === 0) {
                if (backend.library.busy) return
                const id = backend.library.library_model.track_id_at(0)
                if (id <= 0) return
                root.trackId = id
                backend.library.play_track(id)
                page.active = true
                root.phase = 1
            } else if (root.phase === 1) {
                if (playback.lyrics_loading || playback.lyrics.length === 0 || playback.playback_position < 250) return
                if (playback.current_audio.sampleRate !== 8000 || playback.current_audio.bitDepth !== 16
                    || playback.current_audio.channels !== 1 || playback.current_audio.format !== "WAV") {
                    root.finish("source audio properties did not reach QML")
                    return
                }
                if (!playback.lyrics_synchronized || playback.lyrics[1].text !== "灯光映在你的眼里") {
                    root.finish("parsed lyrics did not reach QML")
                    return
                }
                playback.toggle_playback()
                root.phase = 2
            } else if (root.phase === 2) {
                if (playback.playback_state !== "paused") return
                playback.seek_to(1500)
                root.phase = 3
            } else if (root.phase === 3) {
                if (page.item.activeLine !== 1) return
                page.active = false
                root.phase = 4
            } else if (root.phase === 4) {
                page.active = true
                root.phase = 5
            } else if (root.phase === 5) {
                if (!page.item || page.item.activeLine !== 1) return
                if (playback.current_track_id !== root.trackId || playback.playback_state !== "paused"
                    || playback.playback_position !== 1500 || playback.lyrics_loading) {
                    root.finish("page switch altered playback or reloaded lyrics")
                    return
                }
                steps.stop()
                root.finish("ok")
            }
        }
    }
}
