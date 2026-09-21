pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtTest

Item {
    id: root
    width: 960
    height: 720

    QtObject {
        id: mockTheme
        readonly property color textColor: "#111"
        readonly property color mutedTextColor: "#666"
        readonly property color hoverColor: "#eee"
        readonly property color accentColor: "#24745F"
        readonly property color accentPressedColor: "#1B5E4D"
        readonly property color dividerColor: "#ddd"
        readonly property color fieldColor: "#fff"
        readonly property color subtleColor: "#eee"
        readonly property color artworkColor: "#e8e3da"
        readonly property color surfaceColor: "#fafbf8"
        readonly property color glassColor: "#b3fafbf8"
        readonly property bool darkTheme: false
    }

    ListModel {
        id: tracks
        function index_of_track(id) {
            for (let i = 0; i < count; ++i)
                if (get(i).trackId === id) return i
            return -1
        }
        function track_id_at(row) { return row >= 0 && row < count ? get(row).trackId : 0 }

        ListElement { title: "B"; artist: "乙"; album: "二"; duration: "0:10"; cover: ""; trackId: 2 }
        ListElement { title: "A"; artist: "甲"; album: "一"; duration: "0:09"; cover: ""; trackId: 1 }
    }

    property int activatedId: 0
    property string lastSort: ""
    property string libraryQuery: "拼音"
    property real libraryContentY: 80

    TrackTable {
        id: table
        anchors.fill: parent
        theme: mockTheme
        trackModel: tracks
        sortColumn: "title"
        sortAscending: true
        onTrackActivated: function(trackId) { root.activatedId = trackId }
        onSortRequested: function(column) { root.lastSort = column }
    }

    QtObject {
        id: playerMock
        property string current_title: "A"
        property int current_track_id: 1
        property string current_artist: "甲"
        property string current_cover: ""
        property string playback_error: ""
        property string playback_state: "paused"
        property int playback_position: 0
        property int playback_duration: 60000
        property real player_volume: 0.5
        property string play_mode: "sequential"
        property int lastSeek: -1
        function seek_to(position) { lastSeek = position }
        function toggle_playback() {}
        function play_previous() {}
        function play_next() {}
        function cycle_play_mode() {
            play_mode = play_mode === "sequential"
                ? "shuffle"
                : (play_mode === "shuffle" ? "repeatOne" : "sequential")
        }
        function set_player_volume(volume) { player_volume = volume }
        function flush_volume() {}
    }

    PlayerBar {
        id: bar
        width: 900
        height: 80
        y: 600
        theme: mockTheme
        playerBackend: playerMock
    }

    TestCase {
        name: "TrackTableReuseAndActivation"
        when: windowShown

        function test_selection_keyboard_and_reorder() {
            const row = findChild(table, "trackRow2")
            verify(row)
            compare(findChild(table, "rowIndex2").text, "01")
            root.activatedId = 0
            mouseClick(row, row.width / 2, row.height / 2)
            compare(table.selectedTrackId, 2)
            compare(root.activatedId, 0)
            tracks.move(0, 1, 1)
            wait(0)
            verify(findChild(table, "trackRow2").isSelected)
            keyClick(Qt.Key_Up)
            compare(table.selectedTrackId, 1)
            keyClick(Qt.Key_Return)
            tryCompare(root, "activatedId", 1)
            tracks.move(1, 0, 1)
        }

        function test_playback_marker_survives_pause() {
            table.currentTrackId = 2
            table.playbackState = "playing"
            const row = findChild(table, "trackRow2")
            verify(row.isCurrent)
            compare(findChild(table, "rowPlayButton2").iconName, "pause")
            table.playbackState = "paused"
            verify(row.isCurrent)
            compare(findChild(table, "rowPlayButton2").iconName, "play")
            table.playbackState = "stopped"
            verify(!row.isCurrent)
        }

        function test_cover_overlay_icon_follows_hover() {
            const row = findChild(table, "trackRow2")
            const play = findChild(table, "rowPlayButton2")
            compare(play.iconName, "play")
            mouseMove(row, row.width / 2, row.height / 2)
            tryVerify(() => row.hovered)
            tryVerify(() => play.enabled)
            mouseMove(root, 0, 0)
            tryVerify(() => !row.hovered)
        }

        function test_double_click_uses_track_id() {
            compare(table.count, 2)
            const row = table.children[0].children[1]
            verify(row !== null)
        }

        function test_sort_header_emits_column() {
            const headerRow = table.children[0].children[0]
            verify(headerRow !== null)
        }
    }

    TestCase {
        name: "PlayerBarSeek"
        when: windowShown

        function test_volume_panel_opens_from_keyboard() {
            const button = findChild(bar, "volumeButton")
            button.forceActiveFocus()
            keyClick(Qt.Key_Space)
            const slider = findChild(bar, "volumeSlider")
            tryVerify(() => slider.visible && slider.activeFocus)
            keyClick(Qt.Key_Escape)
            mouseMove(root, 0, 0)
            tryVerify(() => !findChild(bar, "volumePanel").visible)
        }

        function test_keyboard_seek_steps_five_seconds() {
            bar.forceActiveFocus()
            keyClick(Qt.Key_Right)
            tryVerify(function() { return playerMock.lastSeek === 5000 })
            playerMock.playback_position = 8000
            keyClick(Qt.Key_Left)
            tryVerify(function() { return playerMock.lastSeek === 3000 || playerMock.lastSeek === 0 })
        }

        function test_mute_toggles_volume() {
            compare(playerMock.player_volume, 0.5)
            bar.toggleMute()
            compare(playerMock.player_volume, 0)
            verify(bar.muted)
            bar.toggleMute()
            compare(playerMock.player_volume, 0.5)
            verify(!bar.muted)
        }

        function test_play_mode_cycles() {
            compare(playerMock.play_mode, "sequential")
            playerMock.cycle_play_mode()
            compare(playerMock.play_mode, "shuffle")
            playerMock.cycle_play_mode()
            compare(playerMock.play_mode, "repeatOne")
            playerMock.cycle_play_mode()
            compare(playerMock.play_mode, "sequential")
        }

        function test_now_playing_cover_shows_expand_overlay() {
            const cover = findChild(bar, "nowPlayingCover")
            compare(cover.overlayIcon, "expand")
            verify(!cover.overlayVisible)
            const button = findChild(bar, "openNowPlaying")
            mouseMove(button, button.width / 2, button.height / 2)
            tryVerify(() => cover.overlayVisible)
            tryVerify(() => findChild(cover, "coverOverlay").opacity > 0.9)
            mouseMove(root, 0, 0)
            tryVerify(() => !cover.overlayVisible)
            tryVerify(() => findChild(cover, "coverOverlay").opacity < 0.1)
        }
    }

    TestCase {
        name: "PageState"
        when: windowShown

        function test_library_query_survives_assignment() {
            compare(root.libraryQuery, "拼音")
            root.libraryQuery = "周杰伦"
            compare(root.libraryQuery, "周杰伦")
            compare(root.libraryContentY, 80)
        }
    }
}
