pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property var session
    property string title: ""

    signal libraryRequested()
    signal playlistRequested(int playlistId)
    signal settingsRequested()

    readonly property int expandedWidth: 196
    readonly property int collapsedWidth: 80
    property bool collapsed: false
    property int renamingPlaylistId: 0

    onCollapsedChanged: {
        if (root.collapsed)
            root.cancelRename()
    }

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
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 1
        color: root.theme.dividerColor
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.bottomMargin: 10
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 48
            Layout.leftMargin: root.collapsed ? 0 : 16
            Layout.rightMargin: root.collapsed ? 0 : 8
            spacing: root.collapsed ? 0 : 4

            Item {
                Layout.fillWidth: true
                visible: root.collapsed
            }

            Text {
                visible: !root.collapsed
                Layout.fillWidth: true
                text: root.title
                color: root.theme.textColor
                font.pixelSize: 18
                font.weight: Font.Medium
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }

            FlatButton {
                id: sidebarToggle
                theme: root.theme
                iconName: root.collapsed ? "sidebarCompact" : "sidebar"
                iconSize: 18
                preferredWidth: 28
                preferredHeight: 28
                labelColor: sidebarToggle.hovered ? root.theme.textColor : root.theme.mutedTextColor
                Accessible.name: root.collapsed ? "展开侧边栏" : "折叠侧边栏"
                ToolTip.visible: hovered
                ToolTip.delay: 400
                ToolTip.text: Accessible.name
                onClicked: root.collapsed = !root.collapsed

                HoverHandler {
                    cursorShape: Qt.PointingHandCursor
                }
            }

            Item {
                Layout.fillWidth: true
                visible: root.collapsed
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.leftMargin: root.collapsed ? 6 : 10
            Layout.rightMargin: root.collapsed ? 6 : 10
            Layout.topMargin: 6
            spacing: 6

        SidebarRow {
            theme: root.theme
            iconName: "library"
            label: "曲库"
            compact: root.collapsed
            selected: !root.session.playlist_open
            onClicked: root.libraryRequested()
        }

        Item { Layout.preferredHeight: 8 }

        RowLayout {
            Layout.fillWidth: true

            Item {
                Layout.fillWidth: true
                visible: root.collapsed
            }

            Text {
                visible: !root.collapsed
                text: "歌单"
                color: root.theme.mutedTextColor
                font.pixelSize: root.theme.metaSize
                font.weight: Font.Medium
            }

            Item {
                Layout.fillWidth: true
                visible: !root.collapsed
            }

            FlatButton {
                theme: root.theme
                iconName: "plus"
                iconSize: 14
                preferredWidth: 28
                preferredHeight: 28
                Accessible.name: "新建歌单"
                onClicked: root.session.create_playlist()
            }

            Item {
                Layout.fillWidth: true
                visible: root.collapsed
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
                    compact: root.collapsed
                    showPencil: !root.collapsed
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
            iconName: "settings"
            label: "设置"
            compact: root.collapsed
            onClicked: root.settingsRequested()
        }
        }
    }

    component SidebarRow: Item {
        id: row

        property var theme
        property string iconName: ""
        property string label
        property string detail: ""
        property bool selected: false
        property bool compact: false
        property bool showPencil: false
        signal clicked()
        signal pencilClicked()

        readonly property bool iconOnly: row.compact && row.iconName !== ""

        implicitHeight: 36
        Layout.fillWidth: true

        RoundedRect {
            anchors.fill: parent
            radius: 8
            color: row.selected ? row.theme.subtleColor
                : (rowHover.hovered ? row.theme.hoverColor : "transparent")
        }

        Icon {
            anchors.centerIn: parent
            visible: row.iconOnly
            name: row.iconName
            size: 18
            color: row.theme.textColor
        }

        MouseArea {
            anchors.fill: parent
            visible: row.iconOnly
            onClicked: row.clicked()
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: row.compact ? 4 : 8
            anchors.rightMargin: row.compact ? 4 : 4
            spacing: 8
            visible: !row.iconOnly

            Icon {
                visible: row.iconName !== ""
                name: row.iconName
                size: 18
                color: row.theme.textColor
                Layout.alignment: Qt.AlignVCenter
            }

            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                Text {
                    id: labelText
                    anchors.fill: parent
                    verticalAlignment: Text.AlignVCenter
                    text: row.label
                    color: row.theme.textColor
                    horizontalAlignment: row.compact ? Text.AlignHCenter : Text.AlignLeft
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
                visible: !row.compact && row.detail !== "" && !pencilButton.visible
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

        ToolTip.visible: rowHover.hovered && (row.iconOnly || labelText.truncated)
        ToolTip.text: row.label
        ToolTip.delay: 400
    }
}
