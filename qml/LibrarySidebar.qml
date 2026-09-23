pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property var session

    signal libraryRequested()
    signal playlistRequested(int playlistId)
    signal settingsRequested()

    property int renamingPlaylistId: 0

    function commitRename(name) {
        const playlistId = root.renamingPlaylistId
        root.renamingPlaylistId = 0
        const trimmed = name.trim()
        if (playlistId > 0 && trimmed !== "")
            root.session.rename_playlist(playlistId, trimmed)
    }

    function cancelRename() {
        root.renamingPlaylistId = 0
    }

    Rectangle {
        anchors.fill: parent
        color: root.theme.backgroundColor
    }

    Rectangle {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 1
        color: root.theme.dividerColor
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 6

        SidebarRow {
            theme: root.theme
            label: "曲库"
            selected: !root.session.playlist_open
            onClicked: root.libraryRequested()
        }

        Item { Layout.preferredHeight: 8 }

        RowLayout {
            Layout.fillWidth: true

            Text {
                text: "歌单"
                color: root.theme.mutedTextColor
                font.pixelSize: root.theme.metaSize
                font.weight: Font.Medium
            }

            Item { Layout.fillWidth: true }

            FlatButton {
                theme: root.theme
                iconName: "plus"
                iconSize: 14
                preferredWidth: 28
                preferredHeight: 28
                Accessible.name: "新建歌单"
                onClicked: root.session.create_playlist()
            }
        }

        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: root.session.playlist_list
            spacing: 2

            delegate: Item {
                id: playlistEntry
                required property int playlistId
                required property string name
                required property int trackCount
                width: ListView.view ? ListView.view.width : 0
                height: 36

                readonly property bool renaming: root.renamingPlaylistId === playlistEntry.playlistId

                SidebarRow {
                    anchors.fill: parent
                    visible: !playlistEntry.renaming
                    theme: root.theme
                    label: playlistEntry.name
                    detail: playlistEntry.trackCount + " 首"
                    showPencil: true
                    selected: root.session.playlist_open
                        && playlistEntry.playlistId === root.session.selected_playlist_id
                    onClicked: root.playlistRequested(playlistEntry.playlistId)
                    onPencilClicked: root.renamingPlaylistId = playlistEntry.playlistId
                }

                RowLayout {
                    anchors.fill: parent
                    visible: playlistEntry.renaming
                    spacing: 4

                    TextField {
                        id: renameField
                        Layout.fillWidth: true
                        Layout.preferredHeight: 32
                        text: playlistEntry.name
                        color: root.theme.textColor
                        font.pixelSize: root.theme.bodySize
                        selectByMouse: true
                        leftPadding: 8
                        rightPadding: 8
                        background: RoundedRect {
                            color: root.theme.fieldColor
                            radius: 8
                            borderWidth: 1
                            borderColor: renameField.activeFocus
                                ? root.theme.accentColor : root.theme.dividerColor
                        }
                        Keys.onReturnPressed: root.commitRename(renameField.text)
                        Keys.onEscapePressed: root.cancelRename()
                    }

                    FlatButton {
                        theme: root.theme
                        text: "确定"
                        fontPixelSize: root.theme.metaSize
                        contentAlignment: Text.AlignHCenter
                        preferredWidth: 44
                        preferredHeight: 28
                        Accessible.name: "确定歌单名称"
                        onClicked: root.commitRename(renameField.text)
                    }
                }

                onRenamingChanged: {
                    if (!renaming)
                        return
                    Qt.callLater(() => {
                        renameField.forceActiveFocus()
                        renameField.selectAll()
                    })
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.dividerColor
        }

        SidebarRow {
            objectName: "librarySettingsButton"
            theme: root.theme
            label: "设置"
            onClicked: root.settingsRequested()
        }
    }

    component SidebarRow: Item {
        id: row

        property var theme
        property string label
        property string detail: ""
        property bool selected: false
        property bool showPencil: false
        signal clicked()
        signal pencilClicked()

        implicitHeight: 36
        Layout.fillWidth: true

        RoundedRect {
            anchors.fill: parent
            radius: 8
            color: row.selected ? row.theme.subtleColor
                : (rowHover.hovered ? row.theme.hoverColor : "transparent")
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 10
            anchors.rightMargin: 4
            spacing: 4

            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                Text {
                    anchors.fill: parent
                    verticalAlignment: Text.AlignVCenter
                    text: row.label
                    color: row.theme.textColor
                    elide: Text.ElideRight
                    font.pixelSize: row.theme.bodySize
                    font.weight: row.selected ? Font.DemiBold : Font.Normal
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: row.clicked()
                }
            }

            Text {
                visible: row.detail !== "" && !pencilButton.visible
                text: row.detail
                color: row.theme.mutedTextColor
                font.pixelSize: row.theme.metaSize
            }

            FlatButton {
                id: pencilButton
                visible: row.showPencil && rowHover.hovered
                theme: row.theme
                iconName: "pencil"
                iconSize: 14
                preferredWidth: 28
                preferredHeight: 28
                Accessible.name: "重命名歌单"
                onClicked: row.pencilClicked()
            }
        }

        HoverHandler { id: rowHover }
    }
}
