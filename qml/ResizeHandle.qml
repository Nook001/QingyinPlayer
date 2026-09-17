pragma ComponentBehavior: Bound

import QtQuick

Item {
    id: root

    required property var targetWindow
    required property int resizeEdges
    required property int resizeCursor

    z: 1000
    visible: root.targetWindow.visibility === Window.Windowed

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: root.resizeCursor
        onPressed: root.targetWindow.startSystemResize(root.resizeEdges)
    }
}