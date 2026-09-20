pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls

ScrollBar {
    id: root

    required property var theme
    required property Flickable scroller

    implicitWidth: 12
    padding: 2
    z: 2
    orientation: Qt.Vertical
    policy: ScrollBar.AlwaysOn
    hoverEnabled: true
    interactive: true
    minimumSize: 0.08
    visible: root.scroller && root.scroller.visible
        && root.scroller.contentHeight > root.scroller.height + 1
    size: {
        if (!root.scroller || root.scroller.contentHeight <= 0)
            return 1
        return Math.min(1, root.scroller.height / root.scroller.contentHeight)
    }

    readonly property real largeJump: root.scroller
        ? Math.max(root.scroller.height * 2.5, 1)
        : 1

    property bool ignorePressMove: false

    Binding {
        target: root
        property: "position"
        value: root.scroller ? root.scroller.visibleArea.yPosition : 0
        when: !root.pressed && !jumpAnim.running
        restoreMode: Binding.RestoreNone
    }

    NumberAnimation {
        id: jumpAnim
        target: root.scroller
        property: "contentY"
        duration: 180
        easing.type: Easing.OutCubic
    }

    function clampedY() {
        if (!root.scroller)
            return 0
        const maxY = Math.max(0, root.scroller.contentHeight - root.scroller.height)
        return Math.max(0, Math.min(root.position * maxY, maxY))
    }

    function moveTo(y, allowAnimate) {
        if (!root.scroller)
            return
        jumpAnim.stop()
        root.scroller.cancelFlick()
        const distance = Math.abs(y - root.scroller.contentY)
        if (!allowAnimate || distance > root.largeJump)
            root.scroller.contentY = y
        else if (distance > 1) {
            jumpAnim.to = y
            jumpAnim.start()
        }
    }

    onPressedChanged: {
        if (!root.scroller)
            return
        if (!root.pressed)
            return
        root.ignorePressMove = true
        root.scroller.cancelFlick()
        Qt.callLater(function() {
            if (!root.scroller) {
                root.ignorePressMove = false
                return
            }
            const y = root.clampedY()
            if (Math.abs(y - root.scroller.contentY) > 2)
                root.moveTo(y, true)
            root.ignorePressMove = false
        })
    }

    onPositionChanged: {
        if (!root.pressed || !root.scroller || root.ignorePressMove)
            return
        root.moveTo(root.clampedY(), false)
    }

    contentItem: Rectangle {
        implicitWidth: 6
        implicitHeight: 48
        radius: 3
        antialiasing: true
        color: root.pressed ? root.theme.accentColor
            : (root.hovered ? root.theme.mutedTextColor : root.theme.dividerColor)
    }

    background: Item {
        implicitWidth: 12
    }
}
