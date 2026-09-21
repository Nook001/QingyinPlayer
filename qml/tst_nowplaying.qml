pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtTest

Item {
    id: root
    width: 1180
    height: 760
    Theme { id: theme; darkTheme: false }
    Rectangle { anchors.fill: parent; color: theme.backgroundColor }
    QtObject {
        id: player
        property string current_title: "夜曲"
        property string current_artist: "示例歌手"
        property string current_cover: ""
        property int current_track_id: 1
        property var current_audio: ({format: "FLAC", sampleRate: 96000, bitDepth: 24, channels: 2, bitrate: 1800, fileSize: 16000000, path: "/音乐/夜曲.flac"})
        property var lyrics: []
        property bool lyrics_synchronized: true
        property bool lyrics_loading: false
        property string lyrics_error: ""
        property int playback_position: 0
        property int requestedSeek: -1
        property bool lyricsVisible: false
        function seek_to(position) { requestedSeek = position; playback_position = position }
        function set_lyrics_visible(visible) { lyricsVisible = visible }
    }
    Loader {
        id: loader
        anchors.fill: parent
        active: false
        sourceComponent: NowPlaying { theme: theme; playerBackend: player }
    }
    TestCase {
        name: "NowPlayingInteractions"
        when: windowShown
        function init() {
            root.width = 1180
            theme.darkTheme = false
            player.playback_position = 0
            player.requestedSeek = -1
            player.lyrics_synchronized = true
            player.lyrics = [
                {time: 1000, text: "晚风吹过安静的街道"},
                {time: 5000, text: "灯光映在你的眼里"},
                {time: 10000, text: "让这一首歌慢慢播放"}
            ]
            loader.active = true
            tryCompare(loader, "status", Loader.Ready)
            waitForRendering(loader.item)
        }
        function cleanup() {
            loader.active = false
            compare(player.lyricsVisible, false)
        }
        function test_sync_seek_and_follow() {
            compare(player.lyricsVisible, true)
            compare(loader.item.activeLine, -1)
            player.playback_position = 5100
            compare(loader.item.activeLine, 1)
            const list = findChild(loader.item, "lyricsList")
            list.forceLayout()
            const line = list.itemAtIndex(1)
            verify(line !== null)
            mouseClick(line, 30, line.height / 2)
            compare(player.requestedSeek, 5000)
            compare(loader.item.following, true)
            loader.item.following = false
            player.playback_position = 11000
            compare(loader.item.activeLine, 2)
            const resume = findChild(loader.item, "resumeLyrics")
            verify(resume.visible)
            mouseClick(resume)
            compare(loader.item.following, true)
            player.playback_position = 0
            compare(loader.item.activeLine, -1)
        }
        function test_plain_and_missing_lyrics() {
            player.lyrics_synchronized = false
            player.lyrics = [{time: -1, text: "没有时间戳的歌词"}]
            compare(loader.item.activeLine, -1)
            const list = findChild(loader.item, "lyricsList")
            list.forceLayout()
            mouseClick(list.itemAtIndex(0))
            compare(player.requestedSeek, -1)
            player.lyrics = []
            compare(list.count, 0)
            verify(!findChild(loader.item, "resumeLyrics").visible)
        }
        function test_audio_details() {
            const button = findChild(loader.item, "audioDetailsButton")
            verify(button.text.indexOf("96 kHz") !== -1)
            mouseClick(button)
            const popup = findChild(loader.item, "audioDetailsPopup")
            tryCompare(popup, "opened", true)
            keyClick(Qt.Key_Escape)
            tryCompare(popup, "opened", false)
            player.current_audio = {format: "MP3", bitrate: 320, sampleRate: 44100, channels: 2}
            verify(button.text.indexOf("320 kbps") !== -1)
            verify(button.text.indexOf("bit ·") === -1)
            player.current_audio = {}
            compare(button.text, "源文件信息")
        }
        function test_wheel_and_large_lyrics() {
            const lines = []
            for (let i = 0; i < 2000; ++i) lines.push({time: i * 1000, text: "第 " + i + " 行歌词"})
            player.lyrics = lines
            player.playback_position = 1000000
            const list = findChild(loader.item, "lyricsList")
            tryCompare(loader.item, "activeLine", 1000)
            waitForRendering(list)
            verify(list.contentItem.children.length < 60)
            mouseWheel(list, 100, 100, 0, -120)
            tryCompare(loader.item, "following", false)
        }
        function test_narrow_dark() {
            root.width = 640
            theme.darkTheme = true
            player.playback_position = 5100
            waitForRendering(loader.item)
            const list = findChild(loader.item, "lyricsList")
            verify(list.width > 0 && list.height > 0)
            compare(loader.item.activeLine, 1)
        }
    }
}
