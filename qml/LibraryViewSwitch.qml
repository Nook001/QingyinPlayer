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

    Keys.onLeftPressed: root.currentIndex = Math.max(0, root.currentIndex - 1)
    Keys.onRightPressed: root.currentIndex = Math.min(root.labels.length - 1, root.currentIndex + 1)

    RoundedRect {
        anchors.fill: parent
        color: root.theme.fieldColor
        radius: height / 2
    }

    Row {
        id: tabRow

        anchors.fill: parent
        anchors.margins: root.inset
        spacing: 2

        Repeater {
            model: root.labels

            Button {
                id: tab

                required property string modelData
                required property int index

                objectName: "libraryViewTab" + index
                width: (tabRow.width - tabRow.spacing * 3) / 4
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
                    color: tab.index === root.currentIndex ? root.theme.subtleColor
                        : (tab.hovered ? root.theme.hoverColor : "transparent")
                    radius: height / 2
                    borderWidth: tab.visualFocus || (root.activeFocus && tab.index === root.currentIndex) ? 1 : 0
                    borderColor: root.theme.accentColor
                }

                contentItem: Text {
                    text: tab.text
                    color: tab.index === root.currentIndex ? root.theme.textColor : root.theme.mutedTextColor
                    font.pixelSize: 13
                    font.weight: tab.index === root.currentIndex ? Font.Medium : Font.Normal
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }
    }
}
