import QtQuick
import QtQuick.Controls.Basic

// Выпадающий список. model: [{value, text}], current — value выбранного.
Item {
    id: root
    property var model: []
    property string current: ""
    property string label: ""
    signal activated(string value)

    implicitWidth: 200
    implicitHeight: (label !== "" ? 22 : 0) + Theme.fieldHeight

    readonly property string currentText: {
        for (var i = 0; i < model.length; ++i)
            if (model[i].value === current)
                return model[i].text
        return ""
    }

    SLabel {
        id: cap
        visible: root.label !== ""
        text: root.label
        caps: true
        anchors.top: parent.top
    }

    Rectangle {
        id: box
        anchors.top: cap.visible ? cap.bottom : parent.top
        anchors.topMargin: cap.visible ? 4 : 0
        width: parent.width
        height: Theme.fieldHeight
        radius: Theme.radius
        color: Theme.inputFill
        border.width: root.activeFocus || pop.opened ? 2 : 1
        border.color: root.activeFocus || pop.opened ? Theme.cyan : Theme.line
        SLabel {
            anchors.left: parent.left
            anchors.right: arrow.left
            anchors.leftMargin: 10
            anchors.verticalCenter: parent.verticalCenter
            text: root.currentText
        }
        SLabel {
            id: arrow
            text: "▾"
            sub: true
            anchors.right: parent.right
            anchors.rightMargin: 10
            anchors.verticalCenter: parent.verticalCenter
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: { root.forceActiveFocus(); pop.opened ? pop.close() : pop.open() }
        }
    }
    focus: false
    activeFocusOnTab: true
    Keys.onSpacePressed: pop.open()
    Keys.onDownPressed: pop.open()

    Popup {
        id: pop
        y: box.y + box.height + 2
        width: box.width
        padding: 4
        implicitHeight: Math.min(list.contentHeight + 8, 260)
        background: Rectangle {
            radius: Theme.radius
            color: Theme.light ? "#ffffff" : "#161a26"
            border.color: Theme.lineStrong
        }
        contentItem: ListView {
            id: list
            clip: true
            model: root.model
            delegate: Rectangle {
                required property var modelData
                width: list.width
                height: 30
                radius: 4
                color: ma.containsMouse ? Theme.hover : (modelData.value === root.current ? Theme.selected : "transparent")
                SLabel {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    verticalAlignment: Text.AlignVCenter
                    text: modelData.text
                }
                MouseArea {
                    id: ma
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: { root.current = modelData.value; root.activated(modelData.value); pop.close() }
                }
            }
        }
    }
    Accessible.role: Accessible.ComboBox
    Accessible.name: label !== "" ? label : currentText
}
