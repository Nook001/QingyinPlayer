pragma ComponentBehavior: Bound

import QtQuick

QtObject {
    id: root

    required property bool darkTheme

    readonly property color backgroundColor: root.darkTheme ? "#151817" : "#F5F6F3"
    readonly property color surfaceColor: root.darkTheme ? "#1D2220" : "#FAFBF8"
    readonly property color sidebarColor: root.darkTheme ? "#101312" : "#202A27"
    readonly property color sidebarSelectedColor: root.darkTheme ? "#29312E" : "#2F403A"
    readonly property color textColor: root.darkTheme ? "#F1F4F2" : "#1D2523"
    readonly property color mutedTextColor: root.darkTheme ? "#A4AFAA" : "#68736F"
    readonly property color sidebarTextColor: "#C5CEC9"
    readonly property color accentColor: root.darkTheme ? "#51A98D" : "#24745F"
    readonly property color accentPressedColor: root.darkTheme ? "#3D876F" : "#1B5E4D"
    readonly property color dividerColor: root.darkTheme ? "#343B38" : "#DEE2DC"
    readonly property color fieldColor: root.darkTheme ? "#222825" : "#FFFFFF"
    readonly property color subtleColor: root.darkTheme ? "#29312E" : "#E4ECE7"
    readonly property color artworkColor: root.darkTheme ? "#312D29" : "#E8E3DA"
    readonly property color hoverColor: root.darkTheme ? "#2A302E" : "#EDF0EC"
}
