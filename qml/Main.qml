pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Effects
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
    flags: Qt.Window | Qt.FramelessWindowHint
    color: "transparent"

    function qingyinRootLoaded() {
        return true
    }

    signal revealTrack(int trackId)

    property bool capsuleMode: false
    property int previousVisibility: Window.Windowed

    function updatePlaybackVisibility() {
        backend.playback.set_ui_visible(
            (window.visible && window.visibility !== Window.Minimized)
            || (capsuleWindow.visible && capsuleWindow.visibility !== Window.Minimized))
    }

    function enterCapsule() {
        if (capsuleMode) return
        previousVisibility = window.visibility
        capsuleMode = true
        backend.playback.set_capsule_active(true)
        nowPlayingLoader.active = false
        capsuleWindow.show()
        window.hide()
        capsuleWindow.requestActivate()
        updatePlaybackVisibility()
    }

    function restoreWindow() {
        if (!capsuleMode) return
        window.visibility = previousVisibility
        capsuleMode = false
        if (currentView === 2) nowPlayingLoader.active = true
        capsuleWindow.hide()
        backend.playback.set_capsule_active(false)
        window.requestActivate()
        updatePlaybackVisibility()
    }

    CapsuleWindow {
        id: capsuleWindow
        objectName: "capsuleWindow"
        theme: appTheme
        playerBackend: backend.playback
        onRestoreRequested: window.restoreWindow()
        onActivityChanged: Qt.callLater(window.updatePlaybackVisibility)
    }

    property int currentView: 0
    property bool wasMaximized: false

    function leaveNowPlaying() {
        if (window.visibility === Window.FullScreen) {
            if (wasMaximized) window.showMaximized()
            else window.showNormal()
        }
        currentView = 0
    }

    onCurrentViewChanged: {
        if (window.currentView === 2 && !window.capsuleMode)
            nowPlayingLoader.active = true
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
        enabled: window.visible && !window.capsuleMode && window.currentView === 2
        onActivated: window.leaveNowPlaying()
    }
    Shortcut {
        sequence: "F11"
        enabled: window.visible && !window.capsuleMode
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
        objectName: "appBackend"
    }

    Component.onCompleted: backend.restore_session()

    onClosing: function(close) {
        backend.flush_settings()
        backend.shutdown()
        close.accepted = true
        Qt.quit()
    }

    onVisibilityChanged: Qt.callLater(window.updatePlaybackVisibility)

    font.family: "Noto Sans CJK SC"

    Shortcut {
        sequence: "Space"
        enabled: window.visible && !window.capsuleMode
            && !(window.activeFocusItem instanceof TextInput)
            && !(window.activeFocusItem instanceof TextEdit)
            && !(window.activeFocusItem instanceof AbstractButton)
        onActivated: backend.playback.toggle_playback()
    }

    WindowFrame {
        id: windowFrame
        objectName: "mainWindowFrame"
        anchors.fill: parent
        theme: appTheme
        windowHandle: window
    }

    Item {
        parent: windowFrame.contentItem
        anchors.fill: parent

        LibrarySidebar {
            id: librarySidebar
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: window.currentView === 0 ? 196 : 0
            visible: width > 0
            theme: appTheme
            session: backend.library
            onLibraryRequested: backend.library.close_playlist()
            onPlaylistRequested: (playlistId) => backend.library.open_playlist(playlistId)
            onSettingsRequested: window.currentView = 1
        }

        Item {
            id: pages
            anchors.left: librarySidebar.right
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: parent.bottom

            Loader {
                id: pageLoader
                objectName: "libraryPageLoader"
                anchors.fill: parent
                sourceComponent: window.currentView === 1 ? settingsPage : libraryPage
            }

            Loader {
                id: nowPlayingLoader
                objectName: "nowPlayingLoader"
                anchors.fill: parent
                z: 1
                active: false
                visible: active
                opacity: window.currentView === 2 && status === Loader.Ready ? 1 : 0
                property real slideY: window.currentView === 2 && status === Loader.Ready ? 0 : 12
                transform: Translate { y: nowPlayingLoader.slideY }
                Behavior on opacity {
                    NumberAnimation { duration: 220; easing.type: Easing.OutCubic }
                }
                Behavior on slideY {
                    NumberAnimation { duration: 220; easing.type: Easing.OutCubic }
                }
                onOpacityChanged: {
                    if (opacity < 0.02 && window.currentView !== 2)
                        active = false
                }
                sourceComponent: NowPlaying {
                    theme: appTheme
                    playerBackend: backend.playback
                    fullscreen: window.visibility === Window.FullScreen
                    onBackRequested: window.leaveNowPlaying()
                    onFullscreenRequested: window.toggleFullscreen()
                }
            }
        }

        Item {
            id: playerFrost
            z: 2
            anchors.fill: playerBar

            ShaderEffectSource {
                id: frostGrab
                anchors.fill: parent
                sourceItem: pages
                sourceRect: Qt.rect(playerBar.x, playerBar.y, playerBar.width, playerBar.height)
                textureSize: Qt.size(
                    Math.max(1, Math.round(playerBar.width / 4)),
                    Math.max(1, Math.round(playerBar.height / 4)))
                live: window.visible && !window.capsuleMode
                hideSource: false
                recursive: false
                visible: false
            }

            Item {
                id: frostMask
                anchors.fill: parent
                visible: false
                layer.enabled: true
                layer.smooth: true

                Rectangle {
                    anchors.fill: parent
                    radius: playerBar.height / 2
                    color: appTheme.maskColor
                    antialiasing: true
                }
            }

            MultiEffect {
                anchors.fill: parent
                source: frostGrab
                blurEnabled: true
                blur: 0.65
                blurMax: 24
                autoPaddingEnabled: false
                maskEnabled: true
                maskSource: frostMask
                maskThresholdMin: 0.5
                maskSpreadAtMin: 1.0
            }
        }

        MouseArea {
            anchors.fill: parent
            z: 3
            visible: playerBar.queueOpen
            onClicked: playerBar.queueOpen = false
        }

        QueuePanel {
            id: queuePanel
            z: 4
            visible: playerBar.queueOpen
            anchors.left: playerBar.left
            anchors.right: playerBar.right
            anchors.bottom: playerBar.top
            anchors.bottomMargin: 8
            height: Math.min(420, Math.max(160, parent.height * 0.5))
            theme: appTheme
            playerBackend: backend.playback
            onCloseRequested: playerBar.queueOpen = false
        }

        PlayerBar {
            id: playerBar
            z: 3
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 12
            width: Math.min(parent.width - 80, 720)
            height: 80
            theme: appTheme
            playerBackend: backend.playback
            onCapsuleRequested: window.enterCapsule()
            onNowPlayingRequested: {
                nowPlayingLoader.active = true
                window.currentView = 2
            }
            onRevealTrackRequested: (trackId) => {
                window.leaveNowPlaying()
                Qt.callLater(() => window.revealTrack(trackId))
            }
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
