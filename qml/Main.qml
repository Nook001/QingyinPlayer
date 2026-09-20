pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Qingyin 1.0
import "pointer.js" as Pointer

// Pointer policy: see qml/pointer.js.
// System chrome owns move/resize. Lists are ItemDelegate rows in a
// ScrollView. Play and page switches are deferred with Qt.callLater.
ApplicationWindow {
    id: window

    width: 1180
    height: 760
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    title: backend.application_name()
    color: window.backgroundColor

    property int currentView: 0
    property bool sidebarCollapsed: false
    property bool darkTheme: backend.dark_theme
    readonly property bool pointerDebug: backend.pointer_debug_enabled()

    readonly property color backgroundColor: darkTheme ? "#151817" : "#F5F6F3"
    readonly property color surfaceColor: darkTheme ? "#1D2220" : "#FAFBF8"
    readonly property color sidebarColor: darkTheme ? "#101312" : "#202A27"
    readonly property color sidebarSelectedColor: darkTheme ? "#29312E" : "#2F403A"
    readonly property color textColor: darkTheme ? "#F1F4F2" : "#1D2523"
    readonly property color mutedTextColor: darkTheme ? "#A4AFAA" : "#68736F"
    readonly property color sidebarTextColor: "#C5CEC9"
    readonly property color accentColor: darkTheme ? "#51A98D" : "#24745F"
    readonly property color accentPressedColor: darkTheme ? "#3D876F" : "#1B5E4D"
    readonly property color dividerColor: darkTheme ? "#343B38" : "#DEE2DC"
    readonly property color fieldColor: darkTheme ? "#222825" : "#FFFFFF"
    readonly property color subtleColor: darkTheme ? "#29312E" : "#E4ECE7"
    readonly property color artworkColor: darkTheme ? "#312D29" : "#E8E3DA"
    readonly property color hoverColor: darkTheme ? "#2A302E" : "#EDF0EC"

    palette.window: backgroundColor
    palette.windowText: textColor
    palette.base: fieldColor
    palette.text: textColor
    palette.button: surfaceColor
    palette.buttonText: textColor
    palette.highlight: accentColor
    palette.mid: sidebarSelectedColor
    palette.light: hoverColor

    AppBridge {
        id: backend
    }

    Component.onCompleted: {
        backend.restore_session()
        Pointer.setDebug(window.pointerDebug, function(kind, target, extra) {
            backend.log_pointer(kind, target, extra)
        })
        if (window.pointerDebug)
            Pointer.hookButtons(window.contentItem)
    }

    onClosing: function(close) {
        backend.shutdown()
        close.accepted = true
        Qt.quit()
    }

    font.family: "Noto Sans CJK SC"

    function navButtonBackground(button, selected) {
        if (button.down)
            return window.sidebarSelectedColor
        if (selected || button.hovered)
            return window.sidebarSelectedColor
        return "transparent"
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.preferredWidth: window.sidebarCollapsed ? 64 : 210
            Layout.fillHeight: true
            color: window.sidebarColor

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

                    Button {
                        id: collapseButton
                        objectName: "collapseSidebarButton"

                        Layout.alignment: Qt.AlignHCenter
                        Layout.preferredWidth: 40
                        Layout.preferredHeight: 40
                        flat: true
                        hoverEnabled: true
                        Accessible.name: window.sidebarCollapsed ? "展开侧边栏" : "折叠侧边栏"
                        onClicked: window.sidebarCollapsed = !window.sidebarCollapsed

                        background: Rectangle {
                            radius: 5
                            color: window.navButtonBackground(collapseButton, false)
                        }

                        contentItem: Text {
                            text: window.sidebarCollapsed ? ">" : "<"
                            color: window.sidebarTextColor
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                            font.pixelSize: 19
                        }
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
                        objectName: "navigationButton-" + navigationButton.modelData.label

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
                                    ? "#FFFFFF" : window.sidebarTextColor
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
                            color: window.accentColor
                            visible: window.currentView === navigationButton.index
                            radius: 2
                        }
                    }
                }

                Item { Layout.fillHeight: true }

                Button {
                    id: settingsButton
                    objectName: "settingsButton"

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
                            color: window.currentView === 3 ? "#FFFFFF" : window.sidebarTextColor
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                            font.pixelSize: 17
                        }

                        Text {
                            Layout.fillWidth: true
                            visible: !window.sidebarCollapsed
                            text: "设置"
                            color: window.currentView === 3 ? "#FFFFFF" : window.sidebarTextColor
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
                        color: window.accentColor
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
                onLoaded: if (window.pointerDebug)
                    Pointer.hookButtons(window.contentItem)
            }

            PlayerBar {
                Layout.fillWidth: true
                Layout.preferredHeight: 104
                theme: window
                playerBackend: backend.playback
            }
        }
    }

    Component {
        id: libraryPage
        Library {
            theme: window
            session: backend
        }
    }

    Component {
        id: artistPage
        Artist {
            theme: window
            libraryModel: backend
        }
    }

    Component {
        id: albumPage
        Album {
            theme: window
            libraryModel: backend
        }
    }

    Component {
        id: settingsPage
        Settings {
            theme: window
            darkMode: backend.dark_theme
            musicFolders: backend.music_folders
            onThemeRequested: function(dark) {
                backend.set_dark_theme(dark)
            }
        }
    }
}
