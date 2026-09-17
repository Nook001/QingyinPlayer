import QtQuick
import QtQuick.Layouts

Item {
    id: root

    required property var theme

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 34
        spacing: 18

        Text {
            text: "专辑"
            color: root.theme.textColor
            font.pixelSize: 28
            font.weight: Font.DemiBold
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.dividerColor
        }

        Text {
            Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter
            Layout.fillHeight: true
            text: "添加音乐后，专辑将显示在这里"
            color: root.theme.mutedTextColor
            font.pixelSize: 14
        }
    }
}
