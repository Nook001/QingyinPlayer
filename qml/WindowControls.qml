pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

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

    DragHandler {
        target: null
        acceptedButtons: Qt.LeftButton
        onActiveChanged: {
            if (active) {
                root.targetWindow.startSystemMove()
            }
        }
    }

    RowLayout {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        spacing: 0

        ToolButton {
            id: minimizeButton

            Layout.preferredWidth: 44
            Layout.fillHeight: true
            text: "−"
            onClicked: root.targetWindow.showMinimized()
            ToolTip.visible: hovered
            ToolTip.text: "最小化"

            contentItem: Text {
                text: minimizeButton.text
                color: root.theme.textColor
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                font.pixelSize: 17
            }

            background: Rectangle {
                color: minimizeButton.hovered ? root.theme.hoverColor : "transparent"
            }
        }

        ToolButton {
            id: maximizeButton

            Layout.preferredWidth: 44
            Layout.fillHeight: true
            text: root.targetWindow.visibility === Window.Maximized ? "❐" : "□"
            onClicked: {
                if (root.targetWindow.visibility === Window.Maximized) {
                    root.targetWindow.showNormal()
                } else {
                    root.targetWindow.showMaximized()
                }
            }
            ToolTip.visible: hovered
            ToolTip.text: root.targetWindow.visibility === Window.Maximized ? "还原" : "最大化"

            contentItem: Text {
                text: maximizeButton.text
                color: root.theme.textColor
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                font.pixelSize: 15
            }

            background: Rectangle {
                color: maximizeButton.hovered ? root.theme.hoverColor : "transparent"
            }
        }

        ToolButton {
            id: closeButton

            Layout.preferredWidth: 44
            Layout.fillHeight: true
            text: "×"
            onClicked: root.targetWindow.close()
            ToolTip.visible: hovered
            ToolTip.text: "关闭"

            contentItem: Text {
                text: closeButton.text
                color: root.theme.textColor
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                font.pixelSize: 18
            }

            background: Rectangle {
                color: closeButton.hovered ? root.theme.hoverColor : "transparent"
            }
        }
    }
}
