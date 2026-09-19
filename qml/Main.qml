pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Qingyin 1.0

ApplicationWindow {
    id: window

    width: 1180
    height: 760
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    flags: Qt.Window | Qt.FramelessWindowHint
    title: backend.application_name()
    color: "transparent"

    property int currentView: 0
    property bool sidebarCollapsed: false
    property bool darkTheme: backend.dark_theme

    readonly property real cornerRadius: visibility === Window.Maximized ? 0 : 10
    readonly property real resizeBorderWidth: 6
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

    AppBridge {
        id: backend
    }

    Component.onCompleted: backend.restore_session()

    background: Rectangle {
        color: window.backgroundColor
        radius: window.cornerRadius
    }

    font.family: "Noto Sans CJK SC"

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.preferredWidth: window.sidebarCollapsed ? 64 : 210
            Layout.fillHeight: true
            color: window.sidebarColor
            radius: window.cornerRadius

            Rectangle {
                anchors.right: parent.right
                width: window.cornerRadius
                height: parent.height
                color: parent.color
            }

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

                    ToolButton {
                        id: collapseButton

                        Layout.alignment: Qt.AlignHCenter
                        Layout.preferredWidth: 40
                        Layout.preferredHeight: 40
                        text: window.sidebarCollapsed ? ">" : "<"
                        onClicked: window.sidebarCollapsed = !window.sidebarCollapsed
                        ToolTip.visible: hovered
                        ToolTip.text: window.sidebarCollapsed ? "展开侧边栏" : "折叠侧边栏"

                        contentItem: Text {
                            text: collapseButton.text
                            color: window.sidebarTextColor
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                            font.pixelSize: 19
                        }

                        background: Rectangle {
                            color: collapseButton.hovered ? window.sidebarSelectedColor : "transparent"
                            radius: 5
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

                        required property int index
                        required property var modelData

                        Layout.fillWidth: true
                        Layout.preferredHeight: 42
                        text: navigationButton.modelData.label
                        font.pixelSize: 14
                        font.weight: window.currentView === navigationButton.index
                            ? Font.DemiBold : Font.Normal
                        onClicked: window.currentView = navigationButton.index
                        ToolTip.visible: hovered && window.sidebarCollapsed
                        ToolTip.text: navigationButton.text

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
                                text: navigationButton.text
                                color: window.currentView === navigationButton.index
                                    ? "#FFFFFF" : "#C5CEC9"
                                elide: Text.ElideRight
                                font: navigationButton.font
                                verticalAlignment: Text.AlignVCenter
                            }
                        }

                        background: Rectangle {
                            color: window.currentView === navigationButton.index
                                ? window.sidebarSelectedColor : "transparent"
                            radius: 6

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
                }

                Item { Layout.fillHeight: true }

                Button {
                    id: settingsButton

                    Layout.fillWidth: true
                    Layout.preferredHeight: 42
                    text: "设置"
                    onClicked: window.currentView = 3
                    ToolTip.visible: hovered && window.sidebarCollapsed
                    ToolTip.text: settingsButton.text

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
                            text: settingsButton.text
                            color: window.currentView === 3 ? "#FFFFFF" : window.sidebarTextColor
                            font.pixelSize: 14
                            font.weight: window.currentView === 3 ? Font.DemiBold : Font.Normal
                            verticalAlignment: Text.AlignVCenter
                        }
                    }

                    background: Rectangle {
                        color: window.currentView === 3
                            ? window.sidebarSelectedColor : "transparent"
                        radius: 6

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
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            WindowControls {
                Layout.fillWidth: true
                Layout.preferredHeight: 38
                targetWindow: window
                theme: window
                cornerRadius: window.cornerRadius
            }

            StackLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                currentIndex: window.currentView

                Library {
                    theme: window
                    libraryModel: backend
                }
                Artist {
                    theme: window
                    libraryModel: backend
                }
                Album {
                    theme: window
                    libraryModel: backend
                }
                Settings {
                    theme: window
                    darkMode: backend.dark_theme
                    musicFolders: backend.music_folders
                    onThemeRequested: function(dark) {
                        backend.set_dark_theme(dark)
                    }
                }
            }

            PlayerBar {
                Layout.fillWidth: true
                Layout.preferredHeight: 104
                theme: window
                cornerRadius: window.cornerRadius
                playerBackend: backend
            }
        }
    }

    ResizeHandle {
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: window.resizeBorderWidth
        anchors.rightMargin: window.resizeBorderWidth
        height: window.resizeBorderWidth
        targetWindow: window
        resizeEdges: Qt.TopEdge
        resizeCursor: Qt.SizeVerCursor
    }

    ResizeHandle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: window.resizeBorderWidth
        anchors.rightMargin: window.resizeBorderWidth
        height: window.resizeBorderWidth
        targetWindow: window
        resizeEdges: Qt.BottomEdge
        resizeCursor: Qt.SizeVerCursor
    }

    ResizeHandle {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.topMargin: window.resizeBorderWidth
        anchors.bottomMargin: window.resizeBorderWidth
        anchors.left: parent.left
        width: window.resizeBorderWidth
        targetWindow: window
        resizeEdges: Qt.LeftEdge
        resizeCursor: Qt.SizeHorCursor
    }

    ResizeHandle {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.topMargin: window.resizeBorderWidth
        anchors.bottomMargin: window.resizeBorderWidth
        anchors.right: parent.right
        width: window.resizeBorderWidth
        targetWindow: window
        resizeEdges: Qt.RightEdge
        resizeCursor: Qt.SizeHorCursor
    }

    ResizeHandle {
        anchors.top: parent.top
        anchors.left: parent.left
        width: window.resizeBorderWidth * 2
        height: window.resizeBorderWidth * 2
        targetWindow: window
        resizeEdges: Qt.TopEdge | Qt.LeftEdge
        resizeCursor: Qt.SizeFDiagCursor
    }

    ResizeHandle {
        anchors.top: parent.top
        anchors.right: parent.right
        width: window.resizeBorderWidth * 2
        height: window.resizeBorderWidth * 2
        targetWindow: window
        resizeEdges: Qt.TopEdge | Qt.RightEdge
        resizeCursor: Qt.SizeBDiagCursor
    }

    ResizeHandle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        width: window.resizeBorderWidth * 2
        height: window.resizeBorderWidth * 2
        targetWindow: window
        resizeEdges: Qt.BottomEdge | Qt.LeftEdge
        resizeCursor: Qt.SizeBDiagCursor
    }

    ResizeHandle {
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        width: window.resizeBorderWidth * 2
        height: window.resizeBorderWidth * 2
        targetWindow: window
        resizeEdges: Qt.BottomEdge | Qt.RightEdge
        resizeCursor: Qt.SizeFDiagCursor
    }
}
