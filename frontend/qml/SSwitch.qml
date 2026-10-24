import QtQuick
import QtQuick.Controls.Basic

Switch {
    id: root
    spacing: 10
    indicator: Rectangle {
        implicitWidth: 38
        implicitHeight: 20
        x: root.leftPadding
        y: (root.height - height) / 2
        radius: 10
        color: root.checked ? Theme.accent : Theme.inputFill
        border.width: root.visualFocus ? 2 : 1
        border.color: root.visualFocus ? Theme.cyan : Theme.lineStrong
        Rectangle {
            x: root.checked ? parent.width - width - 3 : 3
            anchors.verticalCenter: parent.verticalCenter
            width: 14
            height: 14
            radius: 7
            color: "#ffffff"
            Behavior on x { NumberAnimation { duration: 120 } }
        }
    }
    contentItem: SLabel {
        text: root.text
        leftPadding: root.indicator.width + root.spacing
        verticalAlignment: Text.AlignVCenter
    }
    Accessible.role: Accessible.CheckBox
    Accessible.name: text
}
