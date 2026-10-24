import QtQuick
import QtQuick.Layouts
import Sherpa

// Вопросы демона: host key (новый / сменился) и пароль/фраза. source — DesktopState или TerminalSession
// (оба дают `prompt` и методы answerHostKey / answerAuth / cancelAuth).
SDialog {
    id: root
    property var source: null
    readonly property var p: source ? source.prompt : ({})
    readonly property string ptype: p && p.type ? p.type : ""
    readonly property bool isHostKey: ptype === "hostkey"
    readonly property bool mismatch: isHostKey && p.kind === "mismatch"
    property bool confirmedReplace: false

    dialogWidth: 560
    showClose: false
    tone: mismatch ? Theme.red : Theme.accent
    title: isHostKey ? (mismatch ? qsTr("Server key has changed") : qsTr("New server key"))
                     : (p.kind === "passphrase" ? qsTr("Key passphrase required") : qsTr("Password required"))

    onPtypeChanged: {
        if (ptype !== "") { confirmedReplace = false; pass.text = ""; saveBox.checked = false; open() }
        else if (opened) close()
    }
    onOpened: isHostKey ? cancelBtn.forceActiveFocus() : pass.forceFocus()
    onClosed: {
        // закрыли Esc'ом, не ответив: считаем отменой
        if (source && source.prompt && source.prompt.type) {
            if (isHostKey) source.answerHostKey("cancel"); else source.cancelAuth()
        }
    }
    onAcceptRequested: if (!isHostKey && ptype !== "") { source.answerAuth(pass.text, saveBox.checked) }

    // --- host key
    SLabel {
        visible: root.isHostKey
        Layout.fillWidth: true
        wrapMode: Text.Wrap; elide: Text.ElideNone
        color: root.mismatch ? Theme.red : Theme.text
        text: root.mismatch
              ? qsTr("The host key of %1 differs from the one you trusted before. Someone may be intercepting the connection (man-in-the-middle). Do not continue unless you know the server was reinstalled.").arg((root.p.host || "") + ":" + (root.p.port || ""))
              : qsTr("You are connecting to %1 for the first time. Check the fingerprint with the server administrator before trusting it.").arg((root.p.host || "") + ":" + (root.p.port || ""))
    }
    GridLayout {
        visible: root.isHostKey
        Layout.fillWidth: true
        columns: 2; columnSpacing: 14; rowSpacing: 8
        SLabel { text: qsTr("Algorithm"); caps: true }
        SLabel { Layout.fillWidth: true; mono: true; text: root.p.key_type || "" }
        SLabel { visible: root.mismatch; text: qsTr("Was"); caps: true }
        SLabel { visible: root.mismatch; Layout.fillWidth: true; mono: true; color: Theme.green; text: root.p.known_fingerprint || ""; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone }
        SLabel { text: root.mismatch ? qsTr("Now") : qsTr("Fingerprint"); caps: true }
        SLabel { Layout.fillWidth: true; mono: true; color: root.mismatch ? Theme.red : Theme.cyan; text: root.p.fingerprint || ""; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone }
    }
    SCheckBox {
        visible: root.mismatch
        Layout.fillWidth: true
        text: qsTr("I understand the risk and want to replace the saved key")
        checked: root.confirmedReplace
        onToggled: root.confirmedReplace = checked
    }

    // --- пароль
    SLabel {
        visible: !root.isHostKey
        Layout.fillWidth: true
        wrapMode: Text.Wrap; elide: Text.ElideNone
        text: (root.p.prompts && root.p.prompts.length ? root.p.prompts[0].text : qsTr("Enter the secret"))
              + (root.p.attempt > 1 ? "\n" + qsTr("Attempt %1").arg(root.p.attempt) : "")
    }
    STextField { id: pass; visible: !root.isHostKey; Layout.fillWidth: true; secret: true; label: qsTr("Secret"); onAccepted: root.source.answerAuth(text, saveBox.checked) }
    SCheckBox { id: saveBox; visible: !root.isHostKey; text: qsTr("Remember for this server") }

    buttons: [
        SButton { visible: root.isHostKey; text: qsTr("Reject and remember"); kind: "ghost"; onClicked: root.source.answerHostKey("reject") },
        SButton { visible: root.isHostKey && !root.mismatch; text: qsTr("Trust once"); kind: "ghost"; onClicked: root.source.answerHostKey("accept_once") },
        Item { Layout.fillWidth: true },
        SButton {
            id: cancelBtn
            text: qsTr("Cancel"); kind: root.mismatch ? "primary" : "secondary"
            onClicked: root.isHostKey ? root.source.answerHostKey("cancel") : root.source.cancelAuth()
        },
        SButton {
            visible: root.isHostKey
            text: root.mismatch ? qsTr("Accept new key") : qsTr("Trust and connect")
            kind: root.mismatch ? "danger" : "primary"
            enabled: !root.mismatch || root.confirmedReplace
            onClicked: root.source.answerHostKey("accept")
        },
        SButton { visible: !root.isHostKey; text: qsTr("Sign in"); kind: "primary"; onClicked: root.source.answerAuth(pass.text, saveBox.checked) }
    ]
}
