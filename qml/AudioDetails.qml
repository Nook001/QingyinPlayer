pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root
    implicitHeight: summaryButton.implicitHeight
    required property var theme
    required property var audio
    readonly property string summary: {
        const parts = []
        if (audio.format) parts.push(audio.format)
        if (audio.bitDepth > 0) parts.push(audio.bitDepth + " bit")
        else if (audio.bitrate > 0) parts.push(audio.bitrate + " kbps")
        if (audio.sampleRate > 0) parts.push((audio.sampleRate / 1000) + " kHz")
        if (audio.channels > 0) parts.push(audio.channels + " 声道")
        return parts.join(" · ")
    }
    readonly property var rows: [
        ["文件格式", audio.format || ""],
        ["采样率", audio.sampleRate > 0 ? audio.sampleRate + " Hz" : ""],
        ["位深", audio.bitDepth > 0 ? audio.bitDepth + " bit" : ""],
        ["声道", audio.channels > 0 ? String(audio.channels) : ""],
        ["音频码率", audio.bitrate > 0 ? audio.bitrate + " kbps" : ""],
        ["文件大小", audio.fileSize > 0 ? (audio.fileSize / 1048576).toFixed(2) + " MiB" : ""]
    ]

    Button {
        id: summaryButton
        objectName: "audioDetailsButton"
        width: parent.width
        text: root.summary || "源文件信息"
        flat: true
        padding: 8
        Accessible.name: "查看源文件音频信息"
        contentItem: Text {
            text: summaryButton.text
            color: root.theme.mutedTextColor
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: root.theme.metaSize
        }
        onClicked: details.open()
    }

    Popup {
        id: details
        objectName: "audioDetailsPopup"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(480, parent ? parent.width - 40 : 480)
        padding: 24
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: RoundedRect {
            color: root.theme.surfaceColor
            radius: 16
            borderWidth: 1
            borderColor: root.theme.dividerColor
        }
        contentItem: ColumnLayout {
            spacing: 12
            Text {
                text: "源文件信息"
                color: root.theme.textColor
                font.pixelSize: root.theme.bodySize
                font.weight: Font.DemiBold
            }
            Repeater {
                model: root.rows
                RowLayout {
                    required property var modelData
                    visible: modelData[1] !== ""
                    Layout.fillWidth: true
                    Text { text: parent.modelData[0]; color: root.theme.mutedTextColor; Layout.fillWidth: true }
                    Text { text: parent.modelData[1]; color: root.theme.textColor }
                }
            }
            TextArea {
                Layout.fillWidth: true
                text: root.audio.path || ""
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.WrapAnywhere
                color: root.theme.mutedTextColor
                font.pixelSize: root.theme.metaSize
                background: null
                padding: 0
            }
            FlatButton {
                theme: root.theme
                text: "关闭"
                Layout.alignment: Qt.AlignRight
                contentAlignment: Text.AlignHCenter
                onClicked: details.close()
            }
        }
    }
}
