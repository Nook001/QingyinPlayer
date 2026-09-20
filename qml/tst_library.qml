pragma ComponentBehavior: Bound

import QtQuick
import QtTest

Rectangle {
    id: root
    width: 960
    height: 720
    color: theme.backgroundColor

    Theme { id: theme; darkTheme: false }
    ListModel { id: tracks }
    ListModel {
        id: collections
        ListElement { name: "现场"; subtitle: "2 首歌曲 · /music/现场"; cover: ""; collectionId: "directory:/music/现场" }
    }
    Component.onCompleted: {
        for (let i = 0; i < 100; ++i)
            tracks.append({ title: "歌曲 " + i, artist: "歌手", album: "专辑",
                duration: "3:20", cover: "", trackId: i + 1 })
    }

    QtObject {
        id: session
        property var library_model: tracks
        property var artist_model: collections
        property var album_model: collections
        property var directory_model: collections
        property var artist_detail: tracks
        property var album_detail: tracks
        property var directory_detail: tracks
        property string selected_artist: ""
        property string selected_artist_subtitle: ""
        property string selected_artist_cover: ""
        property string selected_album: ""
        property string selected_album_subtitle: ""
        property string selected_album_cover: ""
        property string selected_directory: ""
        property string selected_directory_subtitle: ""
        property string selected_directory_cover: ""
        property string sort_column_name: "title"
        property bool sort_ascending: true
        property string search_query: ""
        property string search_status: ""
        property bool searching: false
        property bool busy: false
        property bool scanning: false
        property string scan_status: ""
        property string watch_status: ""
        property string openedDirectory: ""
        property int playedTrack: 0
        function clear_search() { search_query = "" }
        function search_tracks(query) { search_query = query }
        function open_directory(id) {
            openedDirectory = id
            selected_directory = "现场"
            selected_directory_subtitle = "/music/现场"
        }
        function close_directory() { selected_directory = "" }
        function play_directory_track(id) { playedTrack = id }
    }

    Component {
        id: pageComponent
        Library { theme: theme; session: session }
    }

    TestCase {
        id: test
        name: "UnifiedLibrary"
        when: windowShown
        property var page

        function init() {
            session.search_query = ""
            session.selected_directory = ""
            session.openedDirectory = ""
            session.playedTrack = 0
            page = createTemporaryObject(pageComponent, root, { width: 960, height: 720 })
            verify(page)
            waitForRendering(page)
        }

        function selectView(index) {
            const button = findChild(page, "libraryViewTab" + index)
            verify(button)
            mouseClick(button)
            tryCompare(page, "browseMode", index)
        }

        function test_switches_all_four_views() {
            const loader = findChild(page, "libraryBodyLoader")
            for (let i = 1; i <= 3; ++i) {
                selectView(i)
                compare(loader.item.objectName, ["", "artistBrowser", "albumBrowser", "directoryBrowser"][i])
                tryVerify(() => findChild(page, "allMusicTable") === null)
            }
            selectView(0)
            verify(findChild(page, "allMusicTable") !== null)
        }

        function test_search_and_scroll_survive_switch() {
            const field = findChild(page, "librarySearchField")
            field.text = "歌曲"
            tryCompare(session, "search_query", "歌曲")
            const table = findChild(page, "allMusicTable")
            table.restoreContentY(180)
            tryCompare(page, "savedContentY", 180)
            selectView(1)
            verify(!field.visible)
            selectView(0)
            compare(field.text, "歌曲")
            compare(findChild(page, "allMusicTable").contentY, 180)
        }

        function test_directory_detail_navigation_and_playback() {
            selectView(3)
            const browser = findChild(page, "directoryBrowser")
            browser.collectionOpened("directory:/music/现场")
            compare(session.openedDirectory, "directory:/music/现场")
            verify(browser.showingDetail)
            browser.trackActivated(1)
            compare(session.playedTrack, 1)
            selectView(0)
            selectView(3)
            const restored = findChild(page, "directoryBrowser")
            verify(restored.showingDetail)
            restored.collectionClosed()
            verify(!restored.showingDetail)
        }

        function test_restores_mode_when_page_is_recreated() {
            selectView(2)
            const restored = createTemporaryObject(pageComponent, root,
                { width: 960, height: 720, browseMode: page.browseMode })
            verify(restored)
            compare(findChild(restored, "libraryViewSwitch").currentIndex, 2)
            verify(findChild(restored, "albumBrowser") !== null)
        }
    }
}
