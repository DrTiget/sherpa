import QtQuick

// Сегментированный переключатель. model: [{value, text}], current — выбранное value.
Rectangle {
    id: root
    property var model: []
    property string current: ""
    signal activated(string value)

    implicitHeight: 30
    implicitWidth: row.implicitWidth + 2
    radius: Theme.radius
    color: Theme.inputFill
    border.color: Theme.line
    clip: true

    Row {
        id: row
        anchors.fill: parent
        anchors.margins: 1
        Repeater {
            model: root.model
            delegate: Rectangle {
                required property var modelData
                required property int index
                readonly property bool on: root.current === modelData.value
                height: row.height
                width: lbl.implicitWidth + 24
                radius: 5
                color: on ? Theme.accent : (ma.containsMouse ? Theme.hover : "transparent")
                SLabel {
                    id: lbl
                    anchors.centerIn: parent
                    text: modelData.text
                    font.bold: true
                    color: parent.on ? "#ffffff" : Theme.textSub
                }
                MouseArea {
                    id: ma
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: { root.current = modelData.value; root.activated(modelData.value) }
                }
                Accessible.role: Accessible.RadioButton
                Accessible.name: modelData.text
                Accessible.checked: on
            }
        }
    }
}
