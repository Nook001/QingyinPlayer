pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import QtQuick.Layouts

Item {
    id: root
    required property var theme
    required property var playerBackend
    property bool fullscreen: false
    signal backRequested()
    signal fullscreenRequested()
    property bool following: true
    property int followDuration: 260
    property bool pageAlive: true
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
        if (!root.pageAlive || !root.following || !lyricsList || lyricsList.moving)
            return
        followAnim.stop()
        lyricsList.forceLayout()
        if (root.activeLine < 0) {
            root.animateContentY(lyricsList.originY)
            return
        }
        const item = lyricsList.itemAtIndex(root.activeLine)
        if (!item) {
            lyricsList.positionViewAtIndex(root.activeLine, ListView.Center)
            return
        }
        const target = item.y - (lyricsList.height - item.height) / 2
        const minY = lyricsList.originY
        const maxY = minY + Math.max(0, lyricsList.contentHeight - lyricsList.height)
        root.animateContentY(Math.max(minY, Math.min(maxY, target)))
    }

    function animateContentY(value) {
        if (!root.pageAlive || !lyricsList)
            return
        if (Math.abs(lyricsList.contentY - value) < 1 || root.followDuration <= 0) {
            lyricsList.contentY = value
            return
        }
        followAnim.to = value
        followAnim.duration = root.followDuration
        followAnim.start()
    }

    onActiveLineChanged: Qt.callLater(() => { if (root.pageAlive) root.followCurrent() })
    onLinesChanged: {
        if (!root.pageAlive)
            return
        following = true
        followAnim.stop()
        root.followDuration = 0
        lyricsList.positionViewAtBeginning()
        Qt.callLater(() => {
            if (!root.pageAlive)
                return
            root.followDuration = 260
            root.followCurrent()
        })
    }
    Component.onCompleted: {
        playerBackend.set_lyrics_visible(true)
        Qt.callLater(() => { if (root.pageAlive) root.followCurrent() })
    }
    Component.onDestruction: {
        pageAlive = false
        followAnim.stop()
        playerBackend.set_lyrics_visible(false)
    }

    NumberAnimation {
        id: followAnim
        target: lyricsList
        property: "contentY"
        easing.type: Easing.OutCubic
    }

    Item {
        id: backdrop
        anchors.fill: parent
        z: -1

        Image {
            id: backCover
            width: 96
            height: 96
            visible: false
            asynchronous: true
            cache: true
            fillMode: Image.PreserveAspectCrop
            source: root.playerBackend.current_cover
        }

        MultiEffect {
            anchors.fill: parent
            source: backCover
            blurEnabled: true
            blur: 1.0
            blurMax: 48
            brightness: root.theme.darkTheme ? -0.32 : -0.12
            saturation: 0.4
            autoPaddingEnabled: false
            visible: backCover.status === Image.Ready
            opacity: backCover.status === Image.Ready ? 1 : 0
            Behavior on opacity {
                NumberAnimation { duration: 280; easing.type: Easing.OutCubic }
            }
        }

        Rectangle {
            anchors.fill: parent
            color: root.theme.backgroundColor
            opacity: backCover.status === Image.Ready ? 0.28 : 1
            Behavior on opacity {
                NumberAnimation { duration: 280; easing.type: Easing.OutCubic }
            }
        }

        Rectangle {
            anchors.fill: parent
            color: root.theme.scrimColor
        }
    }

    RowLayout {
        id: header
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: 28
        FlatButton {
            objectName: "nowPlayingBack"
            theme: root.theme
            filled: true
            iconName: "chevronLeft"
            iconSize: 18
            preferredWidth: 36
            preferredHeight: 36
            Accessible.name: "返回曲库"
            ToolTip.visible: hovered
            ToolTip.text: "返回曲库"
            onClicked: root.backRequested()
        }
        Item { Layout.fillWidth: true }
        FlatButton {
            objectName: "nowPlayingFullscreen"
            theme: root.theme
            filled: true
            iconName: root.fullscreen ? "minimize" : "expand"
            iconSize: 16
            preferredWidth: 36
            preferredHeight: 36
            Accessible.name: root.fullscreen ? "退出全屏" : "全屏"
            ToolTip.visible: hovered
            ToolTip.text: Accessible.name
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
                font.pixelSize: root.theme.titleSize
                font.weight: Font.DemiBold
            }
            Text {
                Layout.fillWidth: true
                text: root.playerBackend.current_artist
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                color: root.theme.mutedTextColor
                font.pixelSize: root.theme.bodySize
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
                onHeightChanged: Qt.callLater(() => { if (root.pageAlive) root.followCurrent() })
                reuseItems: true
                boundsBehavior: Flickable.StopAtBounds
                onMovementStarted: {
                    followAnim.stop()
                    root.following = false
                }
                ScrollBar.vertical: ScrollBar {
                    onPressedChanged: if (pressed) {
                        followAnim.stop()
                        root.following = false
                    }
                }
                delegate: ItemDelegate {
                    id: lineDelegate
                    required property var modelData
                    required property int index
                    width: lyricsList.width - 14
                    padding: 12
                    hoverEnabled: root.synced
                    transformOrigin: Item.Center
                    scale: lineDelegate.index === root.activeLine ? 1.04 : 1
                    opacity: lineDelegate.index === root.activeLine ? 1 : 0.72
                    Accessible.name: modelData.text
                    Behavior on scale {
                        NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
                    }
                    Behavior on opacity {
                        NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
                    }
                    onClicked: {
                        if (modelData.time >= 0) {
                            root.playerBackend.seek_to(modelData.time)
                            root.following = true
                            Qt.callLater(() => { if (root.pageAlive) root.followCurrent() })
                        }
                    }
                    contentItem: Text {
                        text: lineDelegate.modelData.text || "♪"
                        wrapMode: Text.Wrap
                        color: lineDelegate.index === root.activeLine
                            ? root.theme.accentColor : root.theme.mutedTextColor
                        font.pixelSize: root.theme.titleSize
                        font.weight: lineDelegate.index === root.activeLine ? Font.DemiBold : Font.Normal
                        Behavior on color {
                            ColorAnimation { duration: 180; easing.type: Easing.OutCubic }
                        }
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
                font.pixelSize: root.theme.bodySize
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
                filled: true
                contentAlignment: Text.AlignHCenter
                onClicked: {
                    root.following = true
                    root.followCurrent()
                }
            }
        }
    }
}
