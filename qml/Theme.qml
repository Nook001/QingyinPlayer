pragma ComponentBehavior: Bound

import QtQuick

QtObject {
    id: root

    property string colorTheme: "qingci"

    readonly property var choices: [
        { id: "qingci", name: "青瓷" },
        { id: "jilan", name: "霁蓝" },
        { id: "songyan", name: "松烟" },
        { id: "mushan", name: "暮山" }
    ]

    readonly property var palettes: ({
        qingci: {
            backgroundTop: "#F4F7F3",
            backgroundColor: "#EBF1EC",
            backgroundBottom: "#CFE2D6",
            light1Color: "#FDFFFA",
            light1Strength: 0.65,
            light2Color: "#B2D8C0",
            light2Strength: 0.75,
            grainStrength: 0.022,
            surfaceColor: "#FBFCFA",
            textColor: "#14231E",
            mutedTextColor: "#4F645C",
            accentColor: "#1E6C57",
            accentPressedColor: "#155644",
            dividerColor: "#BFD4C8",
            fieldColor: "#FFFFFF",
            subtleColor: "#D4E7DC",
            artworkColor: "#E2DCD2",
            hoverColor: "#DCEDE3",
            glassColor: "#D0FBFCFA",
            scrimColor: "#2E000000",
            sidebarSelectedColor: "#2A3C35"
        },
        jilan: {
            backgroundTop: "#EEF4FC",
            backgroundColor: "#E4EDF8",
            backgroundBottom: "#C6DBF0",
            light1Color: "#FAFDFF",
            light1Strength: 0.65,
            light2Color: "#A5C8EC",
            light2Strength: 0.75,
            grainStrength: 0.022,
            surfaceColor: "#F7FAFD",
            textColor: "#12202E",
            mutedTextColor: "#4C6278",
            accentColor: "#236592",
            accentPressedColor: "#194E72",
            dividerColor: "#BAD2E7",
            fieldColor: "#FFFFFF",
            subtleColor: "#CEE2F4",
            artworkColor: "#DFDBD2",
            hoverColor: "#D6E8F7",
            glassColor: "#D0F7FAFD",
            scrimColor: "#2E000000",
            sidebarSelectedColor: "#283C4E"
        },
        songyan: {
            backgroundTop: "#1A221E",
            backgroundColor: "#131816",
            backgroundBottom: "#0C100E",
            light1Color: "#28352F",
            light1Strength: 0.60,
            light2Color: "#0A0D0C",
            light2Strength: 0.50,
            grainStrength: 0.024,
            surfaceColor: "#1C2421",
            textColor: "#F1F4F2",
            mutedTextColor: "#A1ADA7",
            accentColor: "#51A98D",
            accentPressedColor: "#3D876F",
            dividerColor: "#2E3733",
            fieldColor: "#202825",
            subtleColor: "#26302C",
            artworkColor: "#322E29",
            hoverColor: "#28332E",
            glassColor: "#C01C2421",
            scrimColor: "#73000000",
            sidebarSelectedColor: "#29312E"
        },
        mushan: {
            backgroundTop: "#282220",
            backgroundColor: "#1D1917",
            backgroundBottom: "#13100F",
            light1Color: "#3A2E2A",
            light1Strength: 0.60,
            light2Color: "#0F0C0B",
            light2Strength: 0.50,
            grainStrength: 0.024,
            surfaceColor: "#282220",
            textColor: "#F6F1EC",
            mutedTextColor: "#B4A59C",
            accentColor: "#C4846A",
            accentPressedColor: "#A86A52",
            dividerColor: "#3A322F",
            fieldColor: "#2A2421",
            subtleColor: "#38302C",
            artworkColor: "#3A332C",
            hoverColor: "#362E2B",
            glassColor: "#C0282220",
            scrimColor: "#73000000",
            sidebarSelectedColor: "#3A312C"
        }
    })

    readonly property var active: root.palettes[root.colorTheme] || root.palettes.qingci
    readonly property bool darkTheme: root.colorTheme === "songyan" || root.colorTheme === "mushan"

    readonly property color backgroundTop: root.active.backgroundTop
    readonly property color backgroundColor: root.active.backgroundColor
    readonly property color backgroundBottom: root.active.backgroundBottom
    readonly property color light1Color: root.active.light1Color
    readonly property real light1Strength: root.active.light1Strength
    readonly property color light2Color: root.active.light2Color
    readonly property real light2Strength: root.active.light2Strength
    readonly property real grainStrength: root.active.grainStrength

    readonly property color surfaceColor: root.active.surfaceColor
    readonly property color sidebarColor: root.active.backgroundBottom
    readonly property color sidebarSelectedColor: root.active.sidebarSelectedColor
    readonly property color textColor: root.active.textColor
    readonly property color mutedTextColor: root.active.mutedTextColor
    readonly property color sidebarTextColor: "#C5CEC9"
    readonly property color accentColor: root.active.accentColor
    readonly property color accentPressedColor: root.active.accentPressedColor
    readonly property color dividerColor: root.active.dividerColor
    readonly property color fieldColor: root.active.fieldColor
    readonly property color subtleColor: Qt.rgba(root.textColor.r, root.textColor.g, root.textColor.b, root.darkTheme ? 0.12 : 0.08)
    readonly property color hoverColor: Qt.rgba(root.textColor.r, root.textColor.g, root.textColor.b, root.darkTheme ? 0.08 : 0.055)
    readonly property color pressedColor: Qt.rgba(root.textColor.r, root.textColor.g, root.textColor.b, root.darkTheme ? 0.16 : 0.11)
    readonly property color artworkColor: root.active.artworkColor
    readonly property color glassColor: root.active.glassColor
    readonly property color scrimColor: root.active.scrimColor
    readonly property color accentTextColor: "#FFFFFF"
    readonly property color closeColor: "#C74242"
    readonly property color maskColor: "#FFFFFF"
    readonly property color coverScrimColor: "#6B000000"

    readonly property int titleSize: 28
    readonly property int bodySize: 14
    readonly property int metaSize: 12
    readonly property int digitSize: 12
    readonly property string digitFamily: "monospace"

    function colorOf(themeId, key) {
        const palette = root.palettes[themeId] || root.palettes.qingci
        return palette[key]
    }
}
