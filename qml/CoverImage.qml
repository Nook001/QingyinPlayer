pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Effects
import QtQuick.Window

Item {
    id: root

    property url source
    property int displaySize: 44
    property bool overlayVisible: false
    property string overlayIcon: ""
    property int overlayIconSize: Math.max(14, Math.round(root.displaySize * 0.4))
    required property var theme

    readonly property int bucket: {
        if (root.displaySize <= 48)
            return 44
        if (root.displaySize <= 56)
            return 54
        if (root.displaySize <= 80)
            return 64
        if (root.displaySize <= 160) return 160
        return 512
    }
    readonly property real dpr: Math.max(1, Screen.devicePixelRatio)
    readonly property int pixelSize: Math.round(root.bucket * root.dpr)
    readonly property int cornerRadius: Math.min(16, Math.max(8, Math.round(root.displaySize * 0.2)))

    width: root.displaySize
    height: root.displaySize

    Rectangle {
        anchors.fill: parent
        color: root.theme.artworkColor
        radius: root.cornerRadius
        antialiasing: true
        visible: cover.status !== Image.Ready
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

    Item {
        id: coverMask
        anchors.fill: parent
        visible: false
        layer.enabled: true
        layer.smooth: true

        Rectangle {
            anchors.fill: parent
            radius: root.cornerRadius
            color: "#FFFFFF"
            antialiasing: true
        }
    }

    MultiEffect {
        anchors.fill: parent
        source: cover
        maskEnabled: true
        maskSource: coverMask
        autoPaddingEnabled: false
        maskThresholdMin: 0.5
        maskSpreadAtMin: 1.0
        visible: cover.status === Image.Ready
    }

    Icon {
        anchors.centerIn: parent
        name: "library"
        size: Math.max(14, Math.round(root.displaySize / 2.4))
        color: root.theme.accentColor
        visible: cover.status !== Image.Ready
            && !(root.overlayVisible && root.overlayIcon !== "")
    }

    Item {
        anchors.fill: parent
        visible: root.overlayVisible && root.overlayIcon !== ""

        Rectangle {
            anchors.fill: parent
            radius: root.cornerRadius
            color: "#6B000000"
            antialiasing: true
        }

        Icon {
            anchors.centerIn: parent
            name: root.overlayIcon
            size: root.overlayIconSize
            color: "#FFFFFF"
        }
    }
}
