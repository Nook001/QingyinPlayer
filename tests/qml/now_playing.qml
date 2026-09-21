import QtQuick
import QtQuick.Window

Main {
    id: root
    property string result: "pending"
    property int phase: 0
    property int trackId: 0
    property var libraryItem
    property var oldSize
    function testResult() { return result }
    function resource(name) {
        for (let i = 0; i < root.contentItem.data.length; ++i)
            if (root.contentItem.data[i].objectName === name) return root.contentItem.data[i]
        return null
    }
    function child(item, name) {
        if (item.objectName === name) return item
        for (let i = 0; i < item.children.length; ++i) {
            const match = child(item.children[i], name)
            if (match) return match
        }
        return null
    }
    function finish(message) {
        result = message
        steps.stop()
        timeout.stop()
        const backend = resource("appBackend")
        if (backend) backend.shutdown()
        Qt.quit()
    }
    Timer {
        id: timeout
        interval: 15000
        running: true
        onTriggered: {
            const backend = root.resource("appBackend")
            if (!backend) { root.finish("app bridge missing"); return }
            root.finish("timeout in phase " + root.phase + ": " + backend.playback.playback_error
                + "; " + backend.library.scan_status + "; position=" + backend.playback.playback_position)
        }
    }
    Timer {
        id: steps
        interval: 50
        running: true
        repeat: true
        onTriggered: {
            const backend = root.resource("appBackend")
            if (!backend) return
            const playback = backend.playback
            const page = root.child(root.contentItem, "nowPlayingLoader")
            const capsule = root.resource("capsuleWindow")
            if (root.phase === 0) {
                if (backend.library.busy) return
                const id = backend.library.library_model.track_id_at(0)
                if (id <= 0) return
                if ((root.flags & Qt.FramelessWindowHint) === 0
                    || root.child(root.contentItem, "mainWindowFrame").cornerRadius !== 16) {
                    root.finish("main window is not rounded and frameless")
                    return
                }
                root.trackId = id
                root.libraryItem = root.child(root.contentItem, "libraryPageLoader").item
                backend.library.play_track(id)
                root.currentView = 2
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
                if (!page.item || page.item.activeLine !== 1) return
                root.oldSize = Qt.size(root.width, root.height)
                root.enterCapsule()
                root.phase = 4
            } else if (root.phase === 4) {
                if (root.visible || !capsule.visible || !root.capsuleMode || page.active) {
                    root.finish("capsule did not replace main window and unload lyrics page")
                    return
                }
                if (playback.current_track_id !== root.trackId || playback.playback_state !== "paused"
                    || playback.playback_position !== 1500 || playback.lyrics_loading) {
                    root.finish("switch to capsule altered playback or reloaded lyrics")
                    return
                }
                playback.play_next()
                root.phase = 5
            } else if (root.phase === 5) {
                if (playback.current_track_id === root.trackId || playback.lyrics_loading
                    || playback.lyrics.length === 0 || playback.playback_state !== "playing"
                    || playback.playback_position < 250) return
                if (playback.lyrics[0].text !== "胶囊中的下一首") {
                    root.finish("capsule lost lyrics subscription after full page was destroyed")
                    return
                }
                root.trackId = playback.current_track_id
                playback.toggle_playback()
                root.phase = 6
            } else if (root.phase === 6) {
                if (playback.playback_state !== "paused") return
                playback.seek_to(1200)
                capsule.close()
                root.phase = 7
            } else if (root.phase === 7) {
                if (!page.item) return
                if (!root.visible || capsule.visible || root.capsuleMode || root.currentView !== 2
                    || root.width !== root.oldSize.width || root.height !== root.oldSize.height
                    || root.child(root.contentItem, "libraryPageLoader").item !== root.libraryItem) {
                    root.finish("restoring main window lost geometry, view or library instance")
                    return
                }
                if (playback.current_track_id !== root.trackId || playback.playback_state !== "paused"
                    || playback.playback_position !== 1200 || playback.lyrics_loading) {
                    root.finish("restore altered playback")
                    return
                }
                root.toggleFullscreen()
                root.phase = 8
            } else if (root.phase === 8) {
                if (root.visibility !== Window.FullScreen) return
                root.enterCapsule()
                root.phase = 9
            } else if (root.phase === 9) {
                root.restoreWindow()
                root.phase = 10
            } else if (root.phase === 10) {
                if (!page.item) return
                if (root.visibility !== Window.FullScreen || !root.visible || capsule.visible
                    || root.child(root.contentItem, "mainWindowFrame").titleBarHeight !== 0) {
                    root.finish("fullscreen state was not restored")
                    return
                }
                root.finish("ok")
            }
        }
    }
}
