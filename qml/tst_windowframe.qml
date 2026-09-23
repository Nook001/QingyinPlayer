pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Window
import QtTest

Item {
    id: root
    width: 640
    height: 400
    Theme { id: theme; darkTheme: false }
    QtObject {
        id: windowMock
        property int visibility: Window.Windowed
        property string title: "清音"
        property int moves: 0
        property int resizeEdges: 0
        property int closes: 0
        function showMinimized() { visibility = Window.Minimized }
        function showMaximized() { visibility = Window.Maximized }
        function showNormal() { visibility = Window.Windowed }
        function close() { ++closes }
        function startSystemMove() { ++moves; return true }
        function startSystemResize(edges) { resizeEdges = edges; return true }
    }
    WindowFrame {
        id: frame
        anchors.fill: parent
        theme: theme
        windowHandle: windowMock
    }
    TestCase {
        name: "FramelessWindow"
        when: windowShown
        function init() {
            windowMock.visibility = Window.Windowed
            windowMock.moves = 0
            windowMock.resizeEdges = 0
            windowMock.closes = 0
        }
        function test_caption_controls() {
            mouseClick(findChild(frame, "windowminimize"))
            compare(windowMock.visibility, Window.Minimized)
            windowMock.showNormal()
            mouseClick(findChild(frame, "windowmaximize"))
            compare(windowMock.visibility, Window.Maximized)
            mouseClick(findChild(frame, "windowmaximize"))
            compare(windowMock.visibility, Window.Windowed)
            mouseClick(findChild(frame, "windowclose"))
            compare(windowMock.closes, 1)
            compare(windowMock.moves, 0)
        }
        function test_caption_drag_and_double_click() {
            const area = findChild(frame, "windowDragArea")
            mouseDoubleClickSequence(area, 200, 20)
            tryCompare(windowMock, "visibility", Window.Maximized)
            mouseDoubleClickSequence(area, 200, 20)
            tryCompare(windowMock, "visibility", Window.Windowed)
            mousePress(area, 200, 20)
            mouseMove(area, 240, 20)
            mouseRelease(area, 240, 20)
            compare(windowMock.moves, 1)
        }
        function test_resize_edges_and_corners() {
            const edges = [Qt.TopEdge, Qt.BottomEdge, Qt.LeftEdge, Qt.RightEdge,
                Qt.TopEdge | Qt.LeftEdge, Qt.TopEdge | Qt.RightEdge,
                Qt.BottomEdge | Qt.LeftEdge, Qt.BottomEdge | Qt.RightEdge]
            for (const edge of edges) {
                mouseClick(findChild(frame, "resizeEdge" + edge))
                compare(windowMock.resizeEdges, edge)
            }
        }
        function test_geometry_for_window_states() {
            compare(frame.cornerRadius, 16)
            compare(frame.contentItem.y, 36)
            compare(frame.contentItem.height, root.height - 37)
            windowMock.showMaximized()
            compare(frame.cornerRadius, 0)
            compare(frame.contentItem.height, root.height - 36)
            verify(!findChild(frame, "resizeEdge" + Qt.TopEdge).enabled)
            windowMock.visibility = Window.FullScreen
            compare(frame.cornerRadius, 0)
            compare(frame.contentItem.height, root.height)
            verify(!findChild(frame, "windowTitleBar").visible)
            windowMock.visibility = Window.Hidden
            compare(frame.contentItem.height, root.height)
            compare(frame.cornerRadius, 0)
            windowMock.visibility = Window.Minimized
            compare(frame.contentItem.height, root.height)
            windowMock.showNormal()
            compare(frame.cornerRadius, 16)
            verify(findChild(frame, "resizeEdge" + Qt.TopEdge).enabled)
        }
    }
}
