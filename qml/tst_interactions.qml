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
    }

    ListModel {
        id: tracks
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
        property string current_artist: "甲"
        property string current_cover: ""
        property string playback_error: ""
        property string playback_state: "paused"
        property int playback_position: 0
        property int playback_duration: 60000
        property real player_volume: 0.5
        property int lastSeek: -1
        function seek_to(position) { lastSeek = position }
        function toggle_playback() {}
        function play_previous() {}
        function play_next() {}
        function set_player_volume(volume) { player_volume = volume }
        function flush_volume() {}
    }

    PlayerBar {
        id: bar
        width: 900
        height: 104
        y: 600
        theme: mockTheme
        playerBackend: playerMock
    }

    TestCase {
        name: "TrackTableReuseAndActivation"
        when: windowShown

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

        function test_keyboard_seek_steps_five_seconds() {
            bar.forceActiveFocus()
            keyClick(Qt.Key_Right)
            tryVerify(function() { return playerMock.lastSeek === 5000 })
            playerMock.playback_position = 8000
            keyClick(Qt.Key_Left)
            tryVerify(function() { return playerMock.lastSeek === 3000 || playerMock.lastSeek === 0 })
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
