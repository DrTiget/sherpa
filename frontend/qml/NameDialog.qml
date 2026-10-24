import QtQuick
import QtQuick.Layouts

// Запрос одного имени (создать/переименовать группу).
SDialog {
    id: root
    property string label: qsTr("Name")
    property string value: ""
    property string error: ""
    property string confirmText: qsTr("Save")
    signal submitted(string name)

    dialogWidth: 400
    onAboutToShow: { field.text = value; error = "" }
    onOpened: { field.forceFocus(); field.inputItem.selectAll() }

    function submit() {
        if (field.text.trim() === "") {
            error = qsTr("This field is required")
            return
        }
        submitted(field.text.trim())
    }
    onAcceptRequested: submit()

    STextField {
        id: field
        Layout.fillWidth: true
        label: root.label
        error: root.error
        onAccepted: root.submit()
    }

    buttons: [
        Item { Layout.fillWidth: true },
        SButton { text: qsTr("Cancel"); onClicked: root.close() },
        SButton { text: root.confirmText; kind: "primary"; onClicked: root.submit() }
    ]
}
