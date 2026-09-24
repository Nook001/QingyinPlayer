pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts

Item {
    id: root

    required property var theme
    required property string colorTheme
    required property string musicFolders
    property string settingsError
    signal themeRequested(string themeId)
    signal backRequested()

    Shortcut { sequence: "Escape"; onActivated: root.backRequested() }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 18
        spacing: 18

        RowLayout {
            Layout.fillWidth: true
            FlatButton {
                theme: root.theme
                iconName: "chevronLeft"
                Accessible.name: "返回曲库"
                onClicked: root.backRequested()
            }
            Text {
                text: "设置"
                color: root.theme.textColor
                font.pixelSize: root.theme.titleSize
                font.weight: Font.DemiBold
            }
            Item { Layout.fillWidth: true }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.dividerColor
        }

        Text {
            text: "主题"
            color: root.theme.textColor
            font.pixelSize: root.theme.bodySize
            font.weight: Font.DemiBold
        }

        Row {
            spacing: 14

            Repeater {
                model: root.theme.choices

                delegate: Item {
                    id: themeChoice
                    required property var modelData
                    width: 88
                    height: 78

                    readonly property bool selected: root.colorTheme === themeChoice.modelData.id

                    Rectangle {
                        id: swatch
                        width: 88
                        height: 52
                        radius: 12
                        antialiasing: true
                        gradient: Gradient {
                            orientation: Gradient.Vertical
                            GradientStop {
                                position: 0.0
                                color: root.theme.colorOf(themeChoice.modelData.id, "backgroundTop")
                            }
                            GradientStop {
                                position: 1.0
                                color: root.theme.colorOf(themeChoice.modelData.id, "backgroundBottom")
                            }
                        }
                        border.width: 2
                        border.color: themeChoice.selected
                            ? root.theme.accentColor
                            : (choiceHover.hovered ? root.theme.textColor : root.theme.dividerColor)

                        Rectangle {
                            width: 18
                            height: 18
                            radius: 9
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            anchors.margins: 8
                            color: root.theme.colorOf(themeChoice.modelData.id, "accentColor")
                        }
                    }

                    Text {
                        anchors.top: swatch.bottom
                        anchors.topMargin: 8
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: themeChoice.modelData.name
                        color: themeChoice.selected ? root.theme.textColor : root.theme.mutedTextColor
                        font.pixelSize: root.theme.metaSize
                        font.weight: themeChoice.selected ? Font.DemiBold : Font.Normal
                    }

                    HoverHandler {
                        id: choiceHover
                        cursorShape: Qt.PointingHandCursor
                    }

                    TapHandler {
                        onTapped: root.themeRequested(themeChoice.modelData.id)
                    }

                    Accessible.role: Accessible.Button
                    Accessible.name: "主题 " + themeChoice.modelData.name
                }
            }
        }

        Text {
            text: "音乐文件夹"
            color: root.theme.textColor
            font.pixelSize: root.theme.bodySize
            font.weight: Font.DemiBold
        }

        Text {
            Layout.fillWidth: true
            visible: root.musicFolders.trim() !== ""
            text: root.musicFolders
            color: root.theme.mutedTextColor
            font.pixelSize: root.theme.metaSize
            wrapMode: Text.Wrap
        }

        Text {
            Layout.fillWidth: true
            visible: root.musicFolders.trim() === ""
            text: "尚未添加音乐文件夹"
            color: root.theme.mutedTextColor
            font.pixelSize: root.theme.metaSize
        }

        Text {
            Layout.fillWidth: true
            visible: root.settingsError.trim() !== ""
            text: root.settingsError
            color: root.theme.accentPressedColor
            font.pixelSize: root.theme.metaSize
            wrapMode: Text.Wrap
        }

        Item { Layout.fillHeight: true }
    }
}
