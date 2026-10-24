import QtQuick
import QtQuick.Layouts
import Sherpa

// Подтверждение опасного действия (DESKTOP.md 2.6): красный заголовок, перечень затронутого, имя сервера крупно,
// действие по умолчанию — «Отмена». Если requireName — «Выполнить» доступно после ввода имени сервера.
SDialog {
    id: root
    property string serverName: ""
    property string message: ""
    property var details: []                 // строки: что именно затронуто
    property bool requireName: false
    property string confirmText: qsTr("Delete")
    property var onConfirm: null             // function()
    tone: Theme.red
    dialogWidth: 520

    function ask(opts) {
        title = opts.title || qsTr("Dangerous action")
        message = opts.message || ""
        details = opts.details || []
        requireName = !!opts.requireName
        confirmText = opts.confirmText || qsTr("Delete")
        onConfirm = opts.onConfirm || null
        nameField.text = ""
        open()
    }
    onOpened: cancelBtn.forceActiveFocus()

    SLabel {
        Layout.fillWidth: true
        text: root.serverName
        mono: true
        font.pixelSize: 15
        font.bold: true
        color: Theme.red
    }
    SLabel { Layout.fillWidth: true; text: root.message; wrapMode: Text.Wrap; elide: Text.ElideNone }
    Rectangle {
        visible: root.details.length > 0
        Layout.fillWidth: true
        implicitHeight: Math.min(150, dcol.implicitHeight + 16)
        radius: Theme.radius
        color: Theme.inputFill
        border.color: Theme.line
        clip: true
        Flickable {
            anchors.fill: parent; anchors.margins: 8
            contentHeight: dcol.implicitHeight
            Column {
                id: dcol
                width: parent.width
                spacing: 2
                Repeater {
                    model: root.details.slice(0, 40)
                    delegate: SLabel { required property string modelData; width: dcol.width; text: modelData; mono: true; font.pixelSize: 11 }
                }
                SLabel { visible: root.details.length > 40; text: qsTr("…and %1 more").arg(root.details.length - 40); sub: true; font.pixelSize: 11 }
            }
        }
    }
    STextField {
        id: nameField
        visible: root.requireName
        Layout.fillWidth: true
        label: qsTr("Type the server name to confirm")
        placeholderText: root.serverName
        mono: true
        Accessible.name: qsTr("Server name confirmation")
    }

    buttons: [
        Item { Layout.fillWidth: true },
        SButton { id: cancelBtn; text: qsTr("Cancel"); onClicked: root.close() },
        SButton {
            text: root.confirmText
            kind: "danger"
            enabled: !root.requireName || nameField.text === root.serverName
            onClicked: { const f = root.onConfirm; root.close(); if (f) f() }
        }
    ]
}
