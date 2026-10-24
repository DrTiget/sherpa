import QtQuick
import QtQuick.Layouts

// Подтверждение. По умолчанию (Enter, фокус) — безопасное действие «Отмена».
SDialog {
    id: root
    property string message: ""
    property string confirmText: qsTr("OK")
    property bool danger: false
    signal confirmed()

    dialogWidth: 440
    tone: danger ? Theme.red : Theme.accent

    onOpened: cancelBtn.forceActiveFocus()

    SLabel {
        Layout.fillWidth: true
        text: root.message
        wrapMode: Text.Wrap
        elide: Text.ElideNone
    }

    buttons: [
        Item { Layout.fillWidth: true },
        SButton { id: cancelBtn; text: qsTr("Cancel"); onClicked: root.close() },
        SButton {
            text: root.confirmText
            kind: root.danger ? "danger" : "primary"
            onClicked: { root.close(); root.confirmed() }
        }
    ]
}
