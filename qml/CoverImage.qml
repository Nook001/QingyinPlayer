pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Effects
import QtQuick.Shapes
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
    readonly property real dpr: Math.max(1, Screen.devicePixelRatio)
    readonly property int pixelSize: Math.round(root.bucket * root.dpr)
    readonly property int cornerRadius: Math.min(16, Math.max(8, Math.round(root.displaySize * 0.2)))

    width: root.displaySize
    height: root.displaySize

    RoundedRect {
        anchors.fill: parent
        color: root.theme.artworkColor
        radius: root.cornerRadius
    }

    Image {
        id: cover
        anchors.fill: parent
        source: root.source
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        smooth: true
        mipmap: true
        sourceSize.width: root.pixelSize
        sourceSize.height: root.pixelSize
        visible: false
    }

    Shape {
        id: coverMask
        anchors.fill: parent
        visible: false
        preferredRendererType: Shape.CurveRenderer
        antialiasing: true
        layer.enabled: true
        layer.smooth: true
        layer.textureSize: Qt.size(
            Math.max(1, Math.round(width * root.dpr)),
            Math.max(1, Math.round(height * root.dpr)))

        ShapePath {
            fillColor: "#FFFFFF"
            strokeWidth: 0

            PathRectangle {
                width: coverMask.width
                height: coverMask.height
                radius: Math.min(root.cornerRadius, coverMask.width / 2, coverMask.height / 2)
            }
        }
    }

    MultiEffect {
        anchors.fill: parent
        source: cover
        maskEnabled: true
        maskSource: coverMask
        maskThresholdMin: 0.35
        maskSpreadAtMin: 0.15
        visible: cover.status === Image.Ready
    }

    Icon {
        anchors.centerIn: parent
        name: "library"
        size: Math.max(14, Math.round(root.displaySize / 2.4))
        color: root.theme.accentColor
        visible: cover.status !== Image.Ready
    }
}
