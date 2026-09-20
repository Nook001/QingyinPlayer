pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
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

    property int currentView: 0
    property bool sidebarCollapsed: false
    property bool darkTheme: backend.dark_theme
    property string libraryQuery: ""
    property real libraryContentY: 0
    property real artistGridY: 0
    property real albumGridY: 0

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

    function navButtonBackground(button, selected) {
        if (button.down)
            return appTheme.sidebarSelectedColor
        if (selected || button.hovered)
            return appTheme.sidebarSelectedColor
        return "transparent"
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.preferredWidth: window.sidebarCollapsed ? 64 : 210
            Layout.fillHeight: true
            color: appTheme.sidebarColor

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 10
                spacing: 5

                RowLayout {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 42
                    spacing: 4

                    Text {
                        Layout.fillWidth: true
                        Layout.leftMargin: 8
                        visible: !window.sidebarCollapsed
                        text: backend.application_name()
                        color: "#FFFFFF"
                        elide: Text.ElideRight
                        font.pixelSize: 20
                        font.weight: Font.DemiBold
                    }

                    FlatButton {
                        Layout.alignment: Qt.AlignHCenter
                        preferredWidth: 40
                        preferredHeight: 40
                        theme: appTheme
                        text: window.sidebarCollapsed ? ">" : "<"
                        Accessible.name: window.sidebarCollapsed ? "展开侧边栏" : "折叠侧边栏"
                        onClicked: window.sidebarCollapsed = !window.sidebarCollapsed
                    }
                }

                Repeater {
                    model: [
                        { label: "曲库", icon: "♫" },
                        { label: "歌手", icon: "♬" },
                        { label: "专辑", icon: "▣" }
                    ]

                    delegate: Button {
                        id: navigationButton

                        required property int index
                        required property var modelData

                        Layout.fillWidth: true
                        Layout.preferredHeight: 42
                        flat: true
                        hoverEnabled: true
                        Accessible.name: navigationButton.modelData.label
                        onClicked: {
                            const view = navigationButton.index
                            Qt.callLater(function() { window.currentView = view })
                        }

                        background: Rectangle {
                            radius: 6
                            color: window.navButtonBackground(
                                navigationButton,
                                window.currentView === navigationButton.index)
                        }

                        contentItem: RowLayout {
                            spacing: 8

                            Text {
                                Layout.preferredWidth: 34
                                text: navigationButton.modelData.icon
                                color: window.currentView === navigationButton.index
                                    ? "#FFFFFF" : appTheme.sidebarTextColor
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                                font.pixelSize: 17
                            }

                            Text {
                                Layout.fillWidth: true
                                visible: !window.sidebarCollapsed
                                text: navigationButton.modelData.label
                                color: window.currentView === navigationButton.index
                                    ? "#FFFFFF" : "#C5CEC9"
                                elide: Text.ElideRight
                                font.pixelSize: 14
                                font.weight: window.currentView === navigationButton.index
                                    ? Font.DemiBold : Font.Normal
                                verticalAlignment: Text.AlignVCenter
                            }
                        }

                        Rectangle {
                            width: 3
                            height: 20
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            color: appTheme.accentColor
                            visible: window.currentView === navigationButton.index
                            radius: 2
                        }
                    }
                }

                Item { Layout.fillHeight: true }

                Button {
                    id: settingsButton

                    Layout.fillWidth: true
                    Layout.preferredHeight: 42
                    flat: true
                    hoverEnabled: true
                    Accessible.name: "设置"
                    onClicked: Qt.callLater(function() { window.currentView = 3 })

                    background: Rectangle {
                        radius: 6
                        color: window.navButtonBackground(settingsButton, window.currentView === 3)
                    }

                    contentItem: RowLayout {
                        spacing: 8

                        Text {
                            Layout.preferredWidth: 34
                            text: "⚙"
                            color: window.currentView === 3 ? "#FFFFFF" : appTheme.sidebarTextColor
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                            font.pixelSize: 17
                        }

                        Text {
                            Layout.fillWidth: true
                            visible: !window.sidebarCollapsed
                            text: "设置"
                            color: window.currentView === 3 ? "#FFFFFF" : appTheme.sidebarTextColor
                            font.pixelSize: 14
                            font.weight: window.currentView === 3 ? Font.DemiBold : Font.Normal
                            verticalAlignment: Text.AlignVCenter
                        }
                    }

                    Rectangle {
                        width: 3
                        height: 20
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        color: appTheme.accentColor
                        visible: window.currentView === 3
                        radius: 2
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            Loader {
                id: pageLoader
                Layout.fillWidth: true
                Layout.fillHeight: true
                sourceComponent: [libraryPage, artistPage, albumPage, settingsPage][window.currentView]
            }

            PlayerBar {
                Layout.fillWidth: true
                Layout.preferredHeight: 104
                theme: appTheme
                playerBackend: backend.playback
            }
        }
    }

    Component {
        id: libraryPage
        Library {
            theme: appTheme
            session: backend.library
            pendingQuery: window.libraryQuery
            savedContentY: window.libraryContentY
            onPendingQueryChanged: window.libraryQuery = pendingQuery
            onSavedContentYChanged: window.libraryContentY = savedContentY
        }
    }

    Component {
        id: artistPage
        Artist {
            theme: appTheme
            libraryModel: backend.library
            savedGridY: window.artistGridY
            onSavedGridYChanged: window.artistGridY = savedGridY
        }
    }

    Component {
        id: albumPage
        Album {
            theme: appTheme
            libraryModel: backend.library
            savedGridY: window.albumGridY
            onSavedGridYChanged: window.albumGridY = savedGridY
        }
    }

    Component {
        id: settingsPage
        Settings {
            theme: appTheme
            darkMode: backend.dark_theme
            musicFolders: backend.music_folders
            settingsError: backend.settings_error
            onThemeRequested: function(dark) {
                backend.set_dark_theme(dark)
            }
        }
    }
}
