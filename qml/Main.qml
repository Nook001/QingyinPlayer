pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Qingyin 1.0

ApplicationWindow {
    id: window

    width: 1180
    height: 760
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    title: backend.application_name()
    color: appTheme.backgroundColor

    function qingyinRootLoaded() {
        return true
    }

    signal revealTrack(int trackId)

    property int currentView: 0
    property bool wasMaximized: false

    function leaveNowPlaying() {
        if (window.visibility === Window.FullScreen) {
            if (wasMaximized) window.showMaximized()
            else window.showNormal()
        }
        currentView = 0
    }

    function toggleFullscreen() {
        if (window.visibility === Window.FullScreen) {
            if (wasMaximized) window.showMaximized()
            else window.showNormal()
        } else {
            wasMaximized = window.visibility === Window.Maximized
            window.showFullScreen()
        }
    }

    Shortcut {
        sequence: "Escape"
        enabled: window.currentView === 2
        onActivated: window.leaveNowPlaying()
    }
    Shortcut {
        sequence: "F11"
        enabled: window.currentView === 2
        onActivated: window.toggleFullscreen()
    }
    property bool darkTheme: backend.dark_theme
    property string libraryQuery: ""
    property var libraryQueries: ["", "", "", ""]
    property real libraryContentY: 0
    property real artistGridY: 0
    property real albumGridY: 0
    property real directoryGridY: 0
    property int libraryBrowseMode: 0

    Theme {
        id: appTheme
        darkTheme: window.darkTheme
    }

    palette.window: appTheme.backgroundColor
    palette.windowText: appTheme.textColor
    palette.base: appTheme.fieldColor
    palette.text: appTheme.textColor
    palette.button: appTheme.surfaceColor
    palette.buttonText: appTheme.textColor
    palette.highlight: appTheme.accentColor
    palette.mid: appTheme.sidebarSelectedColor
    palette.light: appTheme.hoverColor

    AppBridge {
        id: backend
    }

    Component.onCompleted: backend.restore_session()

    onClosing: function(close) {
        backend.flush_settings()
        backend.shutdown()
        close.accepted = true
        Qt.quit()
    }

    onVisibilityChanged: function(visibility) {
        backend.playback.set_ui_visible(
            visibility !== Window.Minimized && visibility !== Window.Hidden)
    }

    font.family: "Noto Sans CJK SC"

    Shortcut {
        sequence: "Space"
        enabled: !(window.activeFocusItem instanceof TextInput)
            && !(window.activeFocusItem instanceof TextEdit)
            && !(window.activeFocusItem instanceof AbstractButton)
        onActivated: backend.playback.toggle_playback()
    }

    Item {
        anchors.fill: parent

        Loader {
            id: pageLoader
            anchors.fill: parent
            visible: window.currentView !== 2
            sourceComponent: window.currentView === 1 ? settingsPage : libraryPage
        }

        Loader {
            anchors.fill: parent
            active: window.currentView === 2
            sourceComponent: NowPlaying {
                theme: appTheme
                playerBackend: backend.playback
                fullscreen: window.visibility === Window.FullScreen
                onBackRequested: window.leaveNowPlaying()
                onFullscreenRequested: window.toggleFullscreen()
            }
        }

        PlayerBar {
            id: playerBar
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 12
            width: Math.min(parent.width - 80, 720)
            height: 80
            theme: appTheme
            playerBackend: backend.playback
            onNowPlayingRequested: window.currentView = 2
            onRevealTrackRequested: (trackId) => {
                window.leaveNowPlaying()
                Qt.callLater(() => window.revealTrack(trackId))
            }
        }

        RoundedRect {
            z: playerBar.z - 1
            anchors.fill: playerBar
            anchors.topMargin: 6
            radius: playerBar.height / 2
            color: Qt.rgba(0, 0, 0, appTheme.darkTheme ? 0.4 : 0.12)
        }
    }

    Component {
        id: libraryPage
        Library {
            id: libraryView
            Connections {
                target: window
                function onRevealTrack(trackId) { libraryView.revealTrack(trackId) }
            }
            theme: appTheme
            session: backend.library
            currentTrackId: backend.playback.current_track_id
            playbackState: backend.playback.playback_state
            viewQueries: window.libraryQueries
            onViewQueriesChanged: window.libraryQueries = viewQueries
            onSettingsRequested: window.currentView = 1
            onTogglePlaybackRequested: backend.playback.toggle_playback()
            pendingQuery: window.libraryQuery
            savedContentY: window.libraryContentY
            browseMode: window.libraryBrowseMode
            artistGridY: window.artistGridY
            albumGridY: window.albumGridY
            directoryGridY: window.directoryGridY
            onBrowseModeChanged: window.libraryBrowseMode = browseMode
            onArtistGridYChanged: window.artistGridY = artistGridY
            onAlbumGridYChanged: window.albumGridY = albumGridY
            onDirectoryGridYChanged: window.directoryGridY = directoryGridY
            onPendingQueryChanged: window.libraryQuery = pendingQuery
            onSavedContentYChanged: window.libraryContentY = savedContentY
        }
    }

    Component {
        id: settingsPage
        Settings {
            theme: appTheme
            onBackRequested: window.currentView = 0
            darkMode: backend.dark_theme
            musicFolders: backend.music_folders
            settingsError: backend.settings_error
            onThemeRequested: function(dark) {
                backend.set_dark_theme(dark)
            }
        }
    }
}
