pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Shapes

Item {
    id: root

    property color color: "transparent"
    property color borderColor: "transparent"
    property real borderWidth: 0
    property real radius: 0

    Shape {
        id: shape

        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        antialiasing: true
        vendorExtensionsEnabled: true

        ShapePath {
            fillColor: root.color
            strokeColor: root.borderWidth > 0 ? root.borderColor : "transparent"
            strokeWidth: root.borderWidth
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin

            PathRectangle {
                x: 0
                y: 0
                width: shape.width
                height: shape.height
                radius: Math.min(root.radius, shape.width / 2, shape.height / 2)
                strokeAdjustment: root.borderWidth
            }
        }
    }
}
