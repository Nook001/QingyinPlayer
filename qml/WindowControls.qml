pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtQuick.Window
import "pointer.js" as Pointer

Rectangle {
    id: root

    required property var targetWindow
    required property var theme
    required property real cornerRadius

    color: root.theme.surfaceColor
    radius: root.cornerRadius

    Rectangle {
        anchors.left: parent.left
        width: root.cornerRadius
        height: parent.height
        color: parent.color
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: root.cornerRadius
        color: parent.color
    }

    Item {
        id: moveArea

        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: windowButtons.left

        DragHandler {
            id: moveHandler

            target: null
            acceptedButtons: Qt.LeftButton
            grabPermissions: PointerHandler.CanTakeOverFromAnything
                             | PointerHandler.ApprovesTakeOverByAnything
            onActiveChanged: {
                Pointer.debug(active ? "move-grab" : "move-ungrab", "window", "")
                if (active) {
                    Pointer.handOff(moveHandler, function() {
                        root.targetWindow.startSystemMove()
                    })
                }
            }
        }
    }

    RowLayout {
        id: windowButtons
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        spacing: 0

        TapControl {
            id: minimizeButton
            objectName: "minimizeButton"

            Layout.preferredWidth: 44
            Layout.fillHeight: true
            hoverFill: root.theme.hoverColor
            tooltip: "最小化"
            onTapped: root.targetWindow.showMinimized()

            Text {
                anchors.centerIn: parent
                text: "−"
                color: root.theme.textColor
                font.pixelSize: 17
            }
        }

        TapControl {
            id: maximizeButton
            objectName: "maximizeButton"

            Layout.preferredWidth: 44
            Layout.fillHeight: true
            hoverFill: root.theme.hoverColor
            tooltip: root.targetWindow.visibility === Window.Maximized ? "还原" : "最大化"
            onTapped: {
                if (root.targetWindow.visibility === Window.Maximized)
                    root.targetWindow.showNormal()
                else
                    root.targetWindow.showMaximized()
            }

            Text {
                anchors.centerIn: parent
                text: root.targetWindow.visibility === Window.Maximized ? "❐" : "□"
                color: root.theme.textColor
                font.pixelSize: 15
            }
        }

        TapControl {
            id: closeButton
            objectName: "closeButton"

            Layout.preferredWidth: 44
            Layout.fillHeight: true
            hoverFill: root.theme.hoverColor
            tooltip: "关闭"
            onTapped: {
                root.targetWindow.close()
                Qt.quit()
            }

            Text {
                anchors.centerIn: parent
                text: "×"
                color: root.theme.textColor
                font.pixelSize: 18
            }
        }
    }
}
