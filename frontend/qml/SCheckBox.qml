import QtQuick
import QtQuick.Controls.Basic

CheckBox {
    id: root
    spacing: 8
    padding: 2
    indicator: Rectangle {
        implicitWidth: 18
        implicitHeight: 18
        x: root.leftPadding
        y: (root.height - height) / 2
        radius: 4
        color: root.checked ? Theme.accent : Theme.inputFill
        border.width: root.visualFocus ? 2 : 1
        border.color: root.visualFocus ? Theme.cyan : (root.checked ? Theme.cyan : Theme.lineStrong)
        Canvas {
            anchors.fill: parent
            visible: root.checked
            onVisibleChanged: requestPaint()
            onPaint: {
                var c = getContext("2d")
                c.reset()
                c.strokeStyle = "#ffffff"
                c.lineWidth = 2
                c.lineCap = "round"
                c.lineJoin = "round"
                c.beginPath()
                c.moveTo(4.5, 9.5)
                c.lineTo(7.5, 12.5)
                c.lineTo(13.5, 5.5)
                c.stroke()
            }
        }
    }
    contentItem: SLabel {
        text: root.text
        leftPadding: root.indicator.width + root.spacing
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    Accessible.role: Accessible.CheckBox
    Accessible.name: text
}
