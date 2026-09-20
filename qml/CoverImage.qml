pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Window

Item {
    id: root

    property url source
    property int displaySize: 44
    required property var theme

    readonly property int bucket: {
        if (root.displaySize <= 48)
            return 44
        if (root.displaySize <= 56)
            return 54
        if (root.displaySize <= 80)
            return 64
        return 160
    }
    readonly property int pixelSize: root.bucket * (Screen.devicePixelRatio >= 1.5 ? 2 : 1)

    width: root.displaySize
    height: root.displaySize

    Rectangle {
        anchors.fill: parent
        color: root.theme.artworkColor
        radius: Math.min(8, root.displaySize / 8)
        clip: true

        Image {
            id: cover
            anchors.fill: parent
            source: root.source
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            sourceSize.width: root.pixelSize
            sourceSize.height: root.pixelSize
            visible: status === Image.Ready
        }

        Text {
            anchors.centerIn: parent
            text: "♫"
            color: root.theme.accentColor
            font.pixelSize: Math.max(14, root.displaySize / 2.4)
            visible: cover.status !== Image.Ready
        }
    }
}
