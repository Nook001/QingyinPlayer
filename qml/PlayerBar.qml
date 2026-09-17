pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root

    required property var theme
    required property real cornerRadius

    color: root.theme.surfaceColor
    radius: root.cornerRadius

    Rectangle {
        anchors.left: parent.left
        width: root.cornerRadius
        height: parent.height
        color: parent.color
    }

    Rectangle {
        anchors.top: parent.top
        width: parent.width
        height: root.cornerRadius
        color: parent.color
    }

    Rectangle {
        anchors.top: parent.top
        width: parent.width
        height: 1
        color: root.theme.dividerColor
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 22
        anchors.rightMargin: 22
        spacing: 16

        Rectangle {
            Layout.preferredWidth: 54
            Layout.preferredHeight: 54
            color: root.theme.artworkColor
            radius: 5

            Text {
                anchors.centerIn: parent
                text: "♫"
                color: root.theme.accentColor
                font.pixelSize: 23
            }
        }

        ColumnLayout {
            Layout.preferredWidth: 210
            spacing: 3

            Text {
                Layout.fillWidth: true
                text: "未在播放"
                elide: Text.ElideRight
                color: root.theme.textColor
                font.pixelSize: 14
                font.weight: Font.DemiBold
            }

            Text {
                Layout.fillWidth: true
                text: "从曲库中选择一首歌曲"
                elide: Text.ElideRight
                color: root.theme.mutedTextColor
                font.pixelSize: 12
            }
        }

        Item { Layout.fillWidth: true }

        RowLayout {
            spacing: 8

            ToolButton {
                text: "◀|"
                enabled: false
                ToolTip.visible: hovered
                ToolTip.text: "上一首"
            }

            RoundButton {
                Layout.preferredWidth: 46
                Layout.preferredHeight: 46
                text: "▶"
                enabled: false
                ToolTip.visible: hovered
                ToolTip.text: "播放"
            }

            ToolButton {
                text: "|▶"
                enabled: false
                ToolTip.visible: hovered
                ToolTip.text: "下一首"
            }
        }

        Item { Layout.fillWidth: true }

        Text {
            text: "0:00 / 0:00"
            color: root.theme.mutedTextColor
            font.pixelSize: 12
        }

        Slider {
            Layout.preferredWidth: 110
            from: 0
            to: 1
            value: 0.7
            enabled: false
        }
    }
}