import QtQuick
import Sherpa
import QtQuick.Layouts

// Диалог host key (SCREENS.md 2.6). Данные берёт из Backend.pendingHostKey.
// Unknown: «Доверять» не по умолчанию. Mismatch: красный, по умолчанию «Отмена»,
// принять новый ключ можно только после явного подтверждения.
SDialog {
    id: root
    readonly property var hk: Backend.pendingHostKey
    readonly property bool mismatch: hk.kind === "mismatch"
    property bool confirmedReplace: false

    dialogWidth: 560
    tone: mismatch ? Theme.red : Theme.accent
    title: mismatch ? qsTr("Server key has changed") : qsTr("New server key")
    showClose: false

    Connections {
        target: Backend
        function onPendingHostKeyChanged() {
            if (Object.keys(Backend.pendingHostKey).length > 0) {
                root.confirmedReplace = false
                root.open()
            } else if (root.visible) {
                root.close()
            }
        }
    }
    onOpened: cancelBtn.forceActiveFocus()
    onClosed: if (Object.keys(Backend.pendingHostKey).length > 0) Backend.answerHostKey("cancel")

    SLabel {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        elide: Text.ElideNone
        color: root.mismatch ? Theme.red : Theme.text
        text: root.mismatch
              ? qsTr("The host key of %1 differs from the one you trusted before. Someone may be intercepting the connection (man-in-the-middle). Do not continue unless you know the server was reinstalled.").arg((root.hk.host || "") + ":" + (root.hk.port || ""))
              : qsTr("You are connecting to %1 for the first time. Check the fingerprint with the server administrator before trusting it.").arg((root.hk.host || "") + ":" + (root.hk.port || ""))
    }

    GridLayout {
        Layout.fillWidth: true
        columns: 2
        columnSpacing: 14
        rowSpacing: 8
        SLabel { text: qsTr("Server"); caps: true }
        SLabel { Layout.fillWidth: true; mono: true; text: (root.hk.name || "") + "  —  " + (root.hk.host || "") + ":" + (root.hk.port || "") }
        SLabel { text: qsTr("Algorithm"); caps: true }
        SLabel { Layout.fillWidth: true; mono: true; text: root.hk.keyType || "" }
        SLabel { visible: root.mismatch; text: qsTr("Was"); caps: true }
        SLabel {
            visible: root.mismatch
            Layout.fillWidth: true
            mono: true
            color: Theme.green
            text: (root.hk.knownFingerprint || "") + (root.hk.knownAcceptedAt ? "\n" + qsTr("accepted %1").arg(Backend.formatTime(root.hk.knownAcceptedAt)) : "")
            elide: Text.ElideMiddle
            wrapMode: Text.WrapAnywhere
            maximumLineCount: 3
        }
        SLabel { text: root.mismatch ? qsTr("Now") : qsTr("Fingerprint"); caps: true }
        SLabel {
            Layout.fillWidth: true
            mono: true
            color: root.mismatch ? Theme.red : Theme.cyan
            text: root.hk.fingerprint || ""
            wrapMode: Text.WrapAnywhere
            elide: Text.ElideNone
        }
    }

    SCheckBox {
        visible: root.mismatch
        Layout.fillWidth: true
        text: qsTr("I understand the risk and want to replace the saved key")
        checked: root.confirmedReplace
        onToggled: root.confirmedReplace = checked
    }

    buttons: [
        SButton { text: qsTr("Reject and remember"); kind: "ghost"; onClicked: { Backend.answerHostKey("reject") } },
        Item { Layout.fillWidth: true },
        SButton { id: cancelBtn; text: qsTr("Cancel"); kind: root.mismatch ? "primary" : "secondary"; onClicked: { Backend.answerHostKey("cancel") } },
        SButton {
            text: root.mismatch ? qsTr("Accept new key") : qsTr("Trust and connect")
            kind: root.mismatch ? "danger" : "primary"
            enabled: !root.mismatch || root.confirmedReplace
            onClicked: Backend.answerHostKey("accept")
        }
    ]
}
