import QtQuick
import QtQuick.Layouts
import Sherpa

// Основа окна утилиты: запросы к демону через DesktopState.call с колбэками, обработка priv.required
// (диалог sudo и повтор запроса), опасные действия (danger.prepare → подтверждение → метод с confirm_token),
// полоса уведомлений, события подписок. Содержимое окна — обычные дочерние элементы.
Item {
    id: root
    required property var desktop
    property string notice: ""
    property color noticeTone: Theme.red
    property int busyCount: 0
    property string hostname: ""
    property var calls: ({})
    property var failures: ({})
    property var retryAfterSudo: null
    readonly property bool working: desktop.state === "ready"
    default property alias content: body.data
    signal event(var ev)

    function showNotice(text, tone) { noticeTone = tone || Theme.red; notice = text }
    function clearNotice() { notice = "" }

    // method, params, ok(data), fail(errorText, code) — оба необязательны; без fail ошибка уходит в полосу.
    // quiet — при priv.required не открывать диалог sudo, а вернуть ошибку в fail (необязательное чтение вроде журнала в карточке)
    function rpc(method, params, ok, fail, quiet) {
        const token = desktop.call(method, params || ({}))
        calls[token] = { method: method, params: params, ok: ok, fail: fail, quiet: quiet === true }
        busyCount++
        return token
    }
    function sudoAuthenticate(password, done) {
        rpc("priv.sudo.auth", { password: password }, () => done(true, ""), (text) => done(false, text))
    }

    // Опасное действие. opts: {title, message, confirmText, needName, details}
    function danger(method, params, opts, ok) {
        dangerDialog.start(method, params, opts || ({}), ok)
    }
    function runDanger(method, params, hostnameTyped, ok, fail) {
        const p = Object.assign({ connection_id: desktop.connectionId }, params)
        if (hostnameTyped !== undefined && hostnameTyped !== "") p.confirm_hostname = hostnameTyped
        rpc("danger.prepare", { method: method, params: p }, (d) => {
            rpc(method, Object.assign({}, p, { confirm_token: d.confirm_token }), ok, fail)
        }, fail)
    }
    function loadHostname(cb) {
        if (hostname !== "") { if (cb) cb(hostname); return }
        rpc("sys.info", {}, (d) => { hostname = d.hostname || ""; if (cb) cb(hostname) })
    }

    Connections {
        target: root.desktop
        function onRequestFailed(token, code, params) { if (root.calls[token]) root.failures[token] = code }
        function onRequestDone(token, ok, data, errorText) {
            const c = root.calls[token]
            if (!c) return
            delete root.calls[token]
            root.busyCount = Math.max(0, root.busyCount - 1)
            if (ok) { if (c.ok) c.ok(data); return }
            const code = root.failures[token] || ""
            delete root.failures[token]
            if (code === "priv.required" && !c.quiet) {
                root.retryAfterSudo = c
                sudoDialog.openFor()
                return
            }
            if (c.fail) c.fail(errorText, code); else root.showNotice(errorText)
        }
        function onUtilityEvent(ev) { root.event(ev) }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        SBanner {
            Layout.fillWidth: true; Layout.margins: root.notice !== "" ? 8 : 0
            Layout.preferredHeight: root.notice !== "" ? implicitHeight : 0
            visible: root.notice !== ""
            text: root.notice
            tone: root.noticeTone
            onDismissed: root.notice = ""
        }
        Item { id: body; Layout.fillWidth: true; Layout.fillHeight: true }
    }

    // ---- sudo
    SDialog {
        id: sudoDialog
        objectName: "sudoDialog"
        title: qsTr("Administrator rights")
        dialogWidth: 440
        property string err: ""
        property bool busy: false
        function openFor() { err = ""; pw.text = ""; busy = false; open() }
        function submit() {
            if (pw.text === "") return
            busy = true
            root.sudoAuthenticate(pw.text, (ok, text) => {
                busy = false
                if (ok) {
                    close()
                    const r = root.retryAfterSudo; root.retryAfterSudo = null
                    if (r) root.rpc(r.method, r.params, r.ok, r.fail)
                } else { err = text; pw.text = ""; pw.forceFocus() }
            })
        }
        onOpened: pw.forceFocus()
        onClosed: { if (root.retryAfterSudo) { const r = root.retryAfterSudo; root.retryAfterSudo = null; if (r.fail) r.fail(qsTr("Root rights are required for this action"), "priv.required"); else root.showNotice(qsTr("Root rights are required for this action")) } }
        SLabel {
            Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone
            text: qsTr("This action needs sudo. The password is kept only in the daemon memory until the connection closes.")
        }
        STextField { id: pw; Layout.fillWidth: true; secret: true; label: qsTr("sudo password"); error: sudoDialog.err; onAccepted: sudoDialog.submit() }
        buttons: [
            Item { Layout.fillWidth: true },
            SButton { text: qsTr("Cancel"); onClicked: sudoDialog.close() },
            SButton { text: qsTr("Authenticate"); kind: "primary"; busy: sudoDialog.busy; onClicked: sudoDialog.submit() }
        ]
    }

    // ---- опасное действие
    SDialog {
        id: dangerDialog
        objectName: "dangerDialog"
        tone: Theme.red
        dialogWidth: 500
        property string method
        property var params
        property var opts: ({})
        property var okCb
        property string err: ""
        property bool busy: false
        title: opts.title || qsTr("Confirm action")
        function start(m, p, o, cb) {
            method = m; params = p; opts = o; okCb = cb; err = ""; busy = false; nameField.text = ""
            if (o.needName) root.loadHostname()
            open()
        }
        readonly property bool nameOk: !opts.needName || (root.hostname !== "" && nameField.text === root.hostname)
        onOpened: opts.needName ? nameField.forceFocus() : cancelBtn.forceActiveFocus()
        function go() {
            if (!nameOk || busy) return
            busy = true
            root.runDanger(method, params, opts.needName ? nameField.text : "", (d) => { busy = false; close(); if (okCb) okCb(d) },
                           (text) => { busy = false; err = text })
        }
        SLabel { Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone; text: dangerDialog.opts.message || "" }
        SLabel {
            visible: !!dangerDialog.opts.details; Layout.fillWidth: true; mono: true; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone
            text: dangerDialog.opts.details || ""; color: Theme.amber
        }
        SLabel {
            visible: !!dangerDialog.opts.needName; Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone; sub: true
            text: qsTr("To confirm, type the server name: %1").arg(root.hostname)
        }
        STextField {
            id: nameField; objectName: "dangerName"; visible: !!dangerDialog.opts.needName; Layout.fillWidth: true; mono: true
            placeholderText: root.hostname; error: dangerDialog.err
            onAccepted: dangerDialog.go()
        }
        SLabel { visible: !dangerDialog.opts.needName && dangerDialog.err !== ""; Layout.fillWidth: true; color: Theme.red; wrapMode: Text.Wrap; elide: Text.ElideNone; text: dangerDialog.err }
        buttons: [
            Item { Layout.fillWidth: true },
            SButton { id: cancelBtn; text: qsTr("Cancel"); onClicked: dangerDialog.close() },
            SButton { objectName: "dangerConfirm"; text: dangerDialog.opts.confirmText || qsTr("Confirm"); kind: "danger"; busy: dangerDialog.busy; enabled: dangerDialog.nameOk; onClicked: dangerDialog.go() }
        ]
    }
}
