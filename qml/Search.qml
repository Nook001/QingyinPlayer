pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var theme

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 34
        spacing: 18

        Text {
            text: "搜索"
            color: root.theme.textColor
            font.pixelSize: 28
            font.weight: Font.DemiBold
        }

        TextField {
            id: searchField

            Layout.fillWidth: true
            Layout.maximumWidth: 620
            Layout.preferredHeight: 44
            placeholderText: "搜索歌曲、歌手或专辑，也可输入拼音首字母"
            leftPadding: 14
            rightPadding: 14
            font.pixelSize: 14
            color: root.theme.textColor

            background: Rectangle {
                color: root.theme.fieldColor
                border.color: searchField.activeFocus
                    ? root.theme.accentColor : root.theme.dividerColor
                border.width: searchField.activeFocus ? 2 : 1
                radius: 6
            }
        }

        Text {
            Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter
            Layout.fillHeight: true
            text: "输入关键词开始搜索"
            color: root.theme.mutedTextColor
            font.pixelSize: 14
        }
    }
}
