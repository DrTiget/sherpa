import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Изменение firewall с откатом по таймеру: danger.prepare → net.apply (серверный watchdog) →
// обязательный диалог «подтвердить изменения за N секунд» → net.confirm / net.rollback.
Item {
    id: flow
    visible: false
    required property var pane                  // ToolPane
    property int timeout: 60
    property string transaction: ""
    property int remaining: 0
    property string description: ""
    property string result: ""                  // "" | confirmed | rolled_back | expired
    readonly property bool pending: transaction !== ""
    signal finished(string state)

    // params: backend, operation, ... ; items — что показать в подтверждении
    function apply(params, items, title) {
        const p = Object.assign({ connection_id: pane.serverId, confirm: true, confirm_timeout: timeout }, params)
        description = (items || []).join("; ")
        pane.dangerous({ title: title || qsTr("Change firewall rules"), items: items,
                         message: qsTr("The change will be applied with an automatic rollback: if you do not confirm it within %1 s (for example, the connection drops), the server restores the previous rules.").arg(timeout),
                         confirmText: qsTr("Apply"), needName: pane.prod }, "net.apply", p, (r) => {
            showPending(r.transaction, r.confirm_timeout || timeout)
        })
    }
    function showPending(tx, secs) {
        transaction = tx
        remaining = secs
        result = ""
        tick.restart()
        dlg.open()
    }
    function confirmNow() {
        const t = transaction
        pane.req("net.confirm", { transaction: t }, () => end("confirmed"), (e) => { pane.errorText = e; end("expired") })
    }
    function rollbackNow() {
        const t = transaction
        pane.req("net.rollback", { transaction: t }, () => end("rolled_back"), (e) => { pane.errorText = e; end("expired") })
    }
    function end(state) {
        tick.stop()
        dlg.close()
        transaction = ""
        result = state
        finished(state)
    }
    Timer {
        id: tick
        interval: 1000; repeat: true
        onTriggered: { flow.remaining--; if (flow.remaining <= -3) flow.end("expired") }
    }

    SDialog {
        id: dlg
        parent: Overlay.overlay
        title: qsTr("Confirm the changes")
        tone: flow.remaining <= 10 ? Theme.red : Theme.amber
        guardClose: true
        showClose: false
        dialogWidth: 480
        onCloseRequested: {}
        SLabel { Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone; font.bold: true
                 text: qsTr("Confirm the changes within %1 s").arg(Math.max(0, flow.remaining)) }
        SLabel { Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone; sub: true
                 text: qsTr("If the connection is lost or you do nothing, the changes will be rolled back automatically.") }
        SLabel { Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone; mono: true; text: flow.description }
        Rectangle {
            Layout.fillWidth: true; height: 6; radius: 3; color: Theme.alpha(Theme.text, 0.1)
            Rectangle { height: parent.height; radius: 3; width: parent.width * Math.max(0, Math.min(1, flow.remaining / flow.timeout))
                        color: flow.remaining <= 10 ? Theme.red : Theme.amber }
        }
        buttons: [
            Item { Layout.fillWidth: true },
            SButton { text: qsTr("Roll back now"); onClicked: flow.rollbackNow() },
            SButton { text: qsTr("Confirm"); kind: "primary"; onClicked: flow.confirmNow() }
        ]
    }
}
