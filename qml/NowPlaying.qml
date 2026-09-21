pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root
    required property var theme
    required property var playerBackend
    property bool fullscreen: false
    signal backRequested()
    signal fullscreenRequested()
    property bool following: true
    readonly property var lines: playerBackend.lyrics
    readonly property bool synced: playerBackend.lyrics_synchronized
    readonly property int activeLine: {
        if (!root.synced || !root.lines.length) return -1
        const position = root.playerBackend.playback_position
        let low = 0, high = root.lines.length
        while (low < high) {
            const middle = Math.floor((low + high) / 2)
            if (root.lines[middle].time <= position) low = middle + 1
            else high = middle
        }
        return low - 1
    }

    function followCurrent() {
        if (!following) return
        if (activeLine < 0) {
            lyricsList.positionViewAtBeginning()
            return
        }
        lyricsList.forceLayout()
        lyricsList.positionViewAtIndex(activeLine, ListView.Center)
    }
    onActiveLineChanged: Qt.callLater(root.followCurrent)
    onLinesChanged: {
        following = true
        lyricsList.positionViewAtBeginning()
        Qt.callLater(root.followCurrent)
    }
    Component.onCompleted: {
        playerBackend.set_lyrics_visible(true)
        Qt.callLater(root.followCurrent)
    }
    Component.onDestruction: playerBackend.set_lyrics_visible(false)

    RowLayout {
        id: header
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: 28
        FlatButton {
            objectName: "nowPlayingBack"
            theme: root.theme
            text: "返回曲库"
            preferredWidth: 88
            onClicked: root.backRequested()
        }
        Item { Layout.fillWidth: true }
        FlatButton {
            theme: root.theme
            text: root.fullscreen ? "退出全屏" : "全屏"
            preferredWidth: 80
            contentAlignment: Text.AlignHCenter
            onClicked: root.fullscreenRequested()
        }
    }

    GridLayout {
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.topMargin: 16
        anchors.bottomMargin: 112
        anchors.leftMargin: 48
        anchors.rightMargin: 48
        columns: root.width < 760 ? 1 : 2
        columnSpacing: 64
        rowSpacing: 16

        ColumnLayout {
            Layout.preferredWidth: parent.columns === 1 ? -1 : Math.min(340, root.width * 0.32)
            Layout.fillWidth: parent.columns === 1
            Layout.alignment: Qt.AlignVCenter
            spacing: 12
            CoverImage {
                Layout.alignment: Qt.AlignHCenter
                displaySize: root.width < 760 ? 128 : Math.min(320, Math.max(160, root.height - 370))
                theme: root.theme
                source: root.playerBackend.current_cover
            }
            Text {
                Layout.fillWidth: true
                text: root.playerBackend.current_title || "未在播放"
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                color: root.theme.textColor
                font.pixelSize: 24
                font.weight: Font.DemiBold
            }
            Text {
                Layout.fillWidth: true
                text: root.playerBackend.current_artist
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                color: root.theme.mutedTextColor
                font.pixelSize: 14
            }
            AudioDetails {
                Layout.fillWidth: true
                theme: root.theme
                audio: root.playerBackend.current_audio
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            ListView {
                id: lyricsList
                objectName: "lyricsList"
                anchors.fill: parent
                anchors.bottomMargin: resume.visible ? 42 : 0
                clip: true
                model: root.lines
                spacing: 14
                header: Item { height: root.synced ? Math.max(0, lyricsList.height / 2 - 32) : 0 }
                footer: Item { height: root.synced ? Math.max(0, lyricsList.height / 2 - 32) : 0 }
                onHeightChanged: Qt.callLater(root.followCurrent)
                reuseItems: true
                boundsBehavior: Flickable.StopAtBounds
                onMovementStarted: root.following = false
                ScrollBar.vertical: ScrollBar {
                    onPressedChanged: if (pressed) root.following = false
                }
                delegate: ItemDelegate {
                    id: lineDelegate
                    required property var modelData
                    required property int index
                    width: lyricsList.width - 14
                    padding: 12
                    hoverEnabled: root.synced
                    Accessible.name: modelData.text
                    onClicked: {
                        if (modelData.time >= 0) {
                            root.playerBackend.seek_to(modelData.time)
                            root.following = true
                            Qt.callLater(root.followCurrent)
                        }
                    }
                    contentItem: Text {
                        text: lineDelegate.modelData.text || "♪"
                        wrapMode: Text.Wrap
                        color: lineDelegate.index === root.activeLine
                            ? root.theme.accentColor : root.theme.mutedTextColor
                        font.pixelSize: root.width < 760 ? 20 : 26
                        font.weight: lineDelegate.index === root.activeLine ? Font.DemiBold : Font.Normal
                    }
                    background: RoundedRect {
                        radius: 10
                        color: lineDelegate.hovered && root.synced
                            ? root.theme.hoverColor : "transparent"
                        borderWidth: lineDelegate.visualFocus ? 1 : 0
                        borderColor: root.theme.accentColor
                    }
                }
            }
            Text {
                anchors.centerIn: parent
                width: parent.width - 24
                visible: root.lines.length === 0
                text: root.playerBackend.lyrics_loading ? "正在读取歌词…"
                    : (root.playerBackend.lyrics_error || "暂无歌词")
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                color: root.theme.mutedTextColor
                font.pixelSize: 16
            }
            FlatButton {
                id: resume
                objectName: "resumeLyrics"
                anchors.bottom: parent.bottom
                anchors.horizontalCenter: parent.horizontalCenter
                visible: root.synced && !root.following
                text: "回到当前歌词"
                preferredWidth: 128
                theme: root.theme
                contentAlignment: Text.AlignHCenter
                onClicked: {
                    root.following = true
                    root.followCurrent()
                }
            }
        }
    }
}
