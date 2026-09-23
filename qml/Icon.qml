pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property string name
    property color color: "#000000"
    property int size: 20

    width: root.size
    height: root.size
    implicitWidth: root.size
    implicitHeight: root.size

    readonly property bool filled: root.name === "play" || root.name === "pause"

    function pathFor(iconName) {
        switch (iconName) {
        case "library":
            return "M9 18V5l12-2v13M9 18a3 3 0 1 1-6 0 3 3 0 1 1 6 0M21 16a3 3 0 1 1-6 0 3 3 0 1 1 6 0"
        case "artist":
            return "M12 2a3 3 0 0 1 3 3v7a3 3 0 0 1-6 0V5a3 3 0 0 1 3-3zM19 10v2a7 7 0 0 1-14 0v-2M12 19v3M8 22h8"
        case "album":
            return "M22 12a10 10 0 1 1-20 0 10 10 0 1 1 20 0M14 12a2 2 0 1 1-4 0 2 2 0 1 1 4 0"
        case "settings":
            return "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2zM15 12a3 3 0 1 1-6 0 3 3 0 1 1 6 0"
        case "play":
            return "M9.5 6.2v11.6L18.5 12z"
        case "pause":
            return "M7 6h3.5v12H7zM13.5 6H17v12h-3.5z"
        case "skipBack":
            return "M6 5v14M18 5v14L8 12z"
        case "skipForward":
            return "M18 5v14M6 5v14l10-7z"
        case "volume":
            return "M11 5L6 9H2v6h4l5 4zM15.54 8.46a5 5 0 0 1 0 7.07M19.07 4.93a10 10 0 0 1 0 14.14"
        case "volumeMuted":
            return "M11 5L6 9H2v6h4l5 4zM22 9l-6 6M16 9l6 6"
        case "close":
            return "M18 6L6 18M6 6l12 12"
        case "search":
            return "M19 11a8 8 0 1 1-16 0 8 8 0 1 1 16 0M21 21l-4.35-4.35"
        case "folder":
            return "M3 5h6l2 2h10v12H3z"
        case "folderPlus":
            return "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2zM12 10v6M9 13h6"
        case "chevronLeft":
            return "M15 6l-6 6 6 6"
        case "chevronRight":
            return "M9 6l6 6-6 6"
        case "chevronUp":
            return "M6 15l6-6 6 6"
        case "chevronDown":
            return "M6 9l6 6 6-6"
        case "repeat":
            return "M17 2l4 4-4 4M3 11V9a4 4 0 0 1 4-4h14M7 22l-4-4 4-4M21 13v2a4 4 0 0 1-4 4H3"
        case "shuffle":
            return "M16 3h5v5M4 20L21 3M21 16v5h-5M15 15l6 6M4 4l5 5"
        case "repeatOne":
            return "M17 2l4 4-4 4M3 11V9a4 4 0 0 1 4-4h14M7 22l-4-4 4-4M21 13v2a4 4 0 0 1-4 4H3M11 10h1v4"
        case "windowMinimize":
            return "M5 12h14"
        case "windowMaximize":
            return "M5 5h14v14H5z"
        case "windowRestore":
            return "M8 5h11v11M5 8h11v11H5z"
        case "plus":
            return "M12 5v14M5 12h14"
        case "pencil":
            return "M12 20h9M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4z"
        case "check":
            return "M20 6L9 17l-5-5"
        case "list":
            return "M8 6h13M8 12h13M8 18h13M4 6h.01M4 12h.01M4 18h.01"
        case "capsule":
            return "M8 7h8a5 5 0 0 1 0 10H8A5 5 0 0 1 8 7zM8 10v4"
        case "expand":
            return "M8 3H5a2 2 0 0 0-2 2v3M21 8V5a2 2 0 0 0-2-2h-3M3 16v3a2 2 0 0 0 2 2h3M16 21h3a2 2 0 0 0 2-2v-3"
        case "minimize":
            return "M8 3v3a2 2 0 0 1-2 2H3M21 8h-3a2 2 0 0 1-2-2V3M3 16h3a2 2 0 0 1 2 2v3M16 21v-3a2 2 0 0 1 2-2h3"
        default:
            return ""
        }
    }

    Shape {
        width: 24
        height: 24
        scale: root.size / 24
        transformOrigin: Item.TopLeft
        preferredRendererType: Shape.CurveRenderer
        antialiasing: true
        vendorExtensionsEnabled: true
        fillMode: Shape.NoResize

        ShapePath {
            fillColor: root.filled ? root.color : "transparent"
            strokeColor: root.filled ? "transparent" : root.color
            strokeWidth: root.filled ? 0 : 1.75
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            fillRule: ShapePath.WindingFill

            PathSvg {
                path: root.pathFor(root.name)
            }
        }
    }
}
