import QtQuick
import QtQuick.Layouts
import Sherpa

// Подтверждение опасного действия: перечень затрагиваемого, ввод имени сервера (Prod / по требованию),
// слово «выполнить» для мультисерверных запусков. По умолчанию (Enter) — «Отмена».
SDialog {
    id: root
    property string serverName: ""
    property bool prod: false
    property string message: ""
    property var items: []
    property string confirmText: qsTr("Confirm")
    property bool needName: false
    property string word: ""
    property var _then: null
    property alias nameField: nameField
    property alias wordField: wordField
    readonly property bool okToGo: (!needName || nameField.text === serverName) && (word === "" || wordField.text.trim() === word)

    dialogWidth: 480
    tone: Theme.red

    function ask(opts, then) {
        title = opts.title || qsTr("Confirm action")
        message = opts.message || ""
        items = opts.items || []
        confirmText = opts.confirmText || qsTr("Confirm")
        needName = opts.needName !== undefined ? opts.needName : prod
        word = opts.word || ""
        nameField.text = ""
        wordField.text = ""
        _then = then
        open()
    }
    function accept() { if (okToGo) go.clicked() }
    onOpened: cancelBtn.forceActiveFocus()

    SLabel { Layout.fillWidth: true; visible: root.message !== ""; text: root.message; wrapMode: Text.Wrap; elide: Text.ElideNone }
    Rectangle {
        visible: root.items.length > 0
        Layout.fillWidth: true
        implicitHeight: Math.min(150, itemsCol.implicitHeight + 16)
        radius: Theme.radius; color: Theme.inputFill; border.color: Theme.line
        clip: true
        Flickable {
            anchors.fill: parent; anchors.margins: 8
            contentHeight: itemsCol.implicitHeight
            Column {
                id: itemsCol
                width: parent.width
                Repeater { model: root.items; delegate: SLabel { required property var modelData; width: itemsCol.width; text: modelData; mono: true; font.pixelSize: 11 } }
            }
        }
    }
    STextField {
        id: nameField
        visible: root.needName
        Layout.fillWidth: true
        label: qsTr("Type the server name “%1” to confirm").arg(root.serverName)
        mono: true
        onAccepted: if (root.okToGo) go.clicked()
    }
    STextField {
        id: wordField
        visible: root.word !== ""
        Layout.fillWidth: true
        label: qsTr("Type “%1” to confirm").arg(root.word)
        onAccepted: if (root.okToGo) go.clicked()
    }

    buttons: [
        Item { Layout.fillWidth: true },
        SButton { id: cancelBtn; text: qsTr("Cancel"); onClicked: root.close() },
        SButton {
            id: go
            text: root.confirmText; kind: "danger"; enabled: root.okToGo
            onClicked: { const f = root._then; root._then = null; root.close(); if (f) f() }
        }
    ]
}
