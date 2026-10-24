import QtQuick

// Вращающееся кольцо (проверка идёт, загрузка)
Item {
    id: root
    property color color: Theme.cyan
    property int thickness: 2
    implicitWidth: 14
    implicitHeight: 14
    visible: running
    property bool running: true

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: "transparent"
        border.width: root.thickness
        border.color: Theme.alpha(root.color, 0.25)
    }
    Item {
        anchors.fill: parent
        RotationAnimator on rotation { from: 0; to: 360; duration: 900; loops: Animation.Infinite; running: root.running && root.visible }
        Canvas {
            anchors.fill: parent
            onPaint: {
                var c = getContext("2d")
                c.reset()
                c.strokeStyle = root.color
                c.lineWidth = root.thickness
                c.lineCap = "round"
                c.beginPath()
                c.arc(width / 2, height / 2, width / 2 - root.thickness / 2, -Math.PI / 2, 0)
                c.stroke()
            }
            Component.onCompleted: requestPaint()
        }
    }
}
