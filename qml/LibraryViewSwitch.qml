pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls

Item {
    id: root

    required property var theme
    property int currentIndex: 0

    implicitWidth: 252
    implicitHeight: 32
    activeFocusOnTab: true
    Accessible.role: Accessible.PageTabList

    readonly property var labels: ["全部", "歌手", "专辑", "目录"]
    readonly property int inset: 2
    readonly property real tabWidth: tabRow.width > 0
        ? (tabRow.width - tabRow.spacing * 3) / 4 : 0

    Keys.onLeftPressed: root.currentIndex = Math.max(0, root.currentIndex - 1)
    Keys.onRightPressed: root.currentIndex = Math.min(root.labels.length - 1, root.currentIndex + 1)

    RoundedRect {
        anchors.fill: parent
        color: root.theme.fieldColor
        radius: height / 2
    }

    RoundedRect {
        id: selectedPill
        x: tabRow.x + root.currentIndex * (root.tabWidth + tabRow.spacing)
        y: tabRow.y
        width: root.tabWidth
        height: tabRow.height
        radius: height / 2
        color: root.theme.subtleColor
        borderWidth: root.activeFocus ? 1 : 0
        borderColor: root.theme.accentColor
        Behavior on x {
            NumberAnimation { duration: 200; easing.type: Easing.OutCubic }
        }
        Behavior on width {
            NumberAnimation { duration: 200; easing.type: Easing.OutCubic }
        }
    }

    Row {
        id: tabRow

        anchors.fill: parent
        anchors.margins: root.inset
        spacing: 2
        z: 1

        Repeater {
            model: root.labels

            Button {
                id: tab

                required property string modelData
                required property int index

                objectName: "libraryViewTab" + index
                width: root.tabWidth
                height: tabRow.height
                padding: 0
                leftInset: 0
                rightInset: 0
                topInset: 0
                bottomInset: 0
                flat: true
                hoverEnabled: true
                text: modelData
                Accessible.role: Accessible.PageTab
                Accessible.name: text
                Accessible.checkable: true
                Accessible.checked: tab.index === root.currentIndex
                onClicked: root.currentIndex = tab.index

                background: RoundedRect {
                    color: tab.index === root.currentIndex ? "transparent"
                        : (tab.hovered ? root.theme.hoverColor : "transparent")
                    radius: height / 2
                    borderWidth: tab.visualFocus ? 1 : 0
                    borderColor: root.theme.accentColor

                    Behavior on color {
                        ColorAnimation { duration: 120; easing.type: Easing.OutCubic }
                    }
                }

                contentItem: Text {
                    text: tab.text
                    color: tab.index === root.currentIndex ? root.theme.textColor : root.theme.mutedTextColor
                    font.pixelSize: root.theme.metaSize
                    font.weight: tab.index === root.currentIndex ? Font.Medium : Font.Normal
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    Behavior on color {
                        ColorAnimation { duration: 160; easing.type: Easing.OutCubic }
                    }
                }
            }
        }
    }
}
