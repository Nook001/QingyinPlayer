pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Window

Item {
    id: root
    required property var theme
    required property var windowHandle
    readonly property var styleHints: Qt.styleHints
    readonly property int windowVisibility: windowHandle.visibility
    property int presentationState: Window.Windowed
    onWindowVisibilityChanged: {
        if (windowVisibility === Window.Windowed || windowVisibility === Window.Maximized
            || windowVisibility === Window.FullScreen)
            presentationState = windowVisibility
    }
    readonly property bool fullscreen: presentationState === Window.FullScreen
    readonly property bool maximized: presentationState === Window.Maximized
    readonly property int cornerRadius: fullscreen || maximized ? 0 : 16
    readonly property int titleBarHeight: fullscreen ? 0 : 36
    readonly property alias contentItem: body

    function toggleMaximized() {
        if (maximized) windowHandle.showNormal()
        else windowHandle.showMaximized()
    }

    RoundedRect {
        anchors.fill: parent
        radius: root.cornerRadius
        color: root.theme.backgroundColor
        borderWidth: root.cornerRadius > 0 ? 1 : 0
        borderColor: root.theme.dividerColor
    }

    // Content stays inside the straight edges of the frame. This avoids a full-window mask texture.
    Item {
        id: body
        objectName: "windowBody"
        anchors.fill: parent
        anchors.topMargin: root.titleBarHeight
        anchors.bottomMargin: root.cornerRadius
        anchors.leftMargin: root.cornerRadius > 0 ? 1 : 0
        anchors.rightMargin: root.cornerRadius > 0 ? 1 : 0
        clip: true
    }

    Item {
        id: titleBar
        objectName: "windowTitleBar"
        anchors.top: parent.top
        width: parent.width
        height: root.titleBarHeight
        visible: !root.fullscreen

        Item {
            id: dragArea
            objectName: "windowDragArea"
            anchors.left: parent.left
            anchors.right: controls.left
            height: parent.height
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton
                property point pressPosition
                property bool moveRequested: false
                onPressed: function(mouse) {
                    pressPosition = Qt.point(mouse.x, mouse.y)
                    moveRequested = false
                }
                onPositionChanged: function(mouse) {
                    if (pressed && !moveRequested
                        && Math.hypot(mouse.x - pressPosition.x, mouse.y - pressPosition.y)
                            >= root.styleHints.startDragDistance) {
                        moveRequested = true
                        root.windowHandle.startSystemMove()
                    }
                }
                onDoubleClicked: root.toggleMaximized()
            }
            Text {
                anchors.left: parent.left
                anchors.leftMargin: 20
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                text: root.windowHandle.title
                color: root.theme.mutedTextColor
                font.pixelSize: root.theme.metaSize
                elide: Text.ElideRight
            }
        }
        Row {
            id: controls
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Repeater {
                model: ["minimize", "maximize", "close"]
                ToolButton {
                    id: action
                    required property string modelData
                    objectName: "window" + modelData
                    width: 34
                    height: 28
                    padding: 0
                    hoverEnabled: true
                    readonly property string label: modelData === "close" ? "关闭"
                        : modelData === "minimize" ? "最小化" : root.maximized ? "还原" : "最大化"
                    Accessible.name: label
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: label
                    onClicked: {
                        if (modelData === "close") root.windowHandle.close()
                        else if (modelData === "minimize") root.windowHandle.showMinimized()
                        else root.toggleMaximized()
                    }
                    contentItem: Item {
                        Icon {
                            anchors.centerIn: parent
                            size: 14
                            name: action.modelData === "close" ? "close"
                                : action.modelData === "minimize" ? "windowMinimize"
                                : root.maximized ? "windowRestore" : "windowMaximize"
                            color: action.modelData === "close" && action.hovered ? "white" : root.theme.textColor
                        }
                    }
                    background: Rectangle {
                        radius: 6
                        color: action.hovered || action.down
                            ? (action.modelData === "close" ? root.theme.closeColor : root.theme.hoverColor) : "transparent"
                        border.width: action.visualFocus ? 1 : 0
                        border.color: root.theme.accentColor
                    }
                }
            }
        }
    }

    Repeater {
        model: [Qt.TopEdge | Qt.LeftEdge, Qt.TopEdge | Qt.RightEdge,
            Qt.BottomEdge | Qt.LeftEdge, Qt.BottomEdge | Qt.RightEdge,
            Qt.TopEdge, Qt.BottomEdge, Qt.LeftEdge, Qt.RightEdge]
        MouseArea {
            required property int modelData
            readonly property bool horizontal: (modelData & (Qt.LeftEdge | Qt.RightEdge)) !== 0
            readonly property bool vertical: (modelData & (Qt.TopEdge | Qt.BottomEdge)) !== 0
            readonly property bool corner: horizontal && vertical
            objectName: "resizeEdge" + modelData
            z: 10
            enabled: !root.maximized && !root.fullscreen
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton
            width: corner ? 12 : horizontal ? 5 : root.width - 24
            height: corner ? 12 : vertical ? 5 : root.height - 24
            x: (modelData & Qt.LeftEdge) ? 0 : (modelData & Qt.RightEdge) ? root.width - width : 12
            y: (modelData & Qt.TopEdge) ? 0 : (modelData & Qt.BottomEdge) ? root.height - height : 12
            cursorShape: corner
                ? ((modelData === (Qt.TopEdge | Qt.LeftEdge) || modelData === (Qt.BottomEdge | Qt.RightEdge))
                    ? Qt.SizeFDiagCursor : Qt.SizeBDiagCursor)
                : horizontal ? Qt.SizeHorCursor : Qt.SizeVerCursor
            onPressed: root.windowHandle.startSystemResize(modelData)
        }
    }
}
