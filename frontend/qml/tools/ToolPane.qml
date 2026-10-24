import QtQuick
import QtQuick.Layouts
import Sherpa

// Каркас окна утилиты: вызовы демона с учётом занятости и ошибок, опасные действия через
// danger.prepare → диалог подтверждения → метод с confirm_token, запрос пароля sudo при priv.required.
ToolRpc {
    id: root
    readonly property bool prod: (Backend.serverById(desk.connectionId).group_name || "").toLowerCase().indexOf("prod") >= 0
    readonly property bool readonlyMode: desk.readOnlyMode
    readonly property var utilities: (desk.capabilities && desk.capabilities.utilities) || ({})
    readonly property var tools: (desk.capabilities && desk.capabilities.tools) || ({})
    default property alias data: body.data
    property alias body: body
    property alias dangerDialog: dlg
    onPrivRequired: (e) => sudo.ask(e)

    // Опасное действие. opts: {title, message, items: [строки], confirmText, needName (по умолчанию — на Prod), word}.
    function dangerous(opts, method, params, ok, fail) {
        if (opts.hostname) {
            // ядро сверяет введённое имя с hostname сервера (confirm_hostname), узнаём его через sys.info
            req("sys.info", {}, (info) => {
                dangerous(Object.assign({}, opts, { hostname: false, needName: true, expectName: info.hostname, nameKind: "hostname" }), method,
                          Object.assign({}, params, { confirm_hostname: info.hostname }), ok, fail)
            }, fail)
            return
        }
        dlg.ask(opts, () => {
            call("danger.prepare", { method: method, params: params }, (r) => {
                call(method, Object.assign({}, params, { confirm_token: r.confirm_token }), ok, fail)
            }, fail)
        })
    }
    // Подтверждение действия сервера: connection_id + confirm: true + токен
    function dangerousReq(opts, method, params, ok, fail) {
        dangerous(opts, method, Object.assign({ connection_id: serverId, confirm: true }, params || {}), ok, fail)
    }
    function fmtBytes(b) {
        if (b === undefined || b === null) return "—"
        const u = ["B", "KB", "MB", "GB", "TB", "PB"]
        let i = 0, v = b
        while (v >= 1024 && i < 5) { v /= 1024; i++ }
        return v.toFixed(i > 1 ? 1 : 0) + " " + u[i]
    }
    function fmtTime(iso) { return iso ? Backend.formatTime(iso) : "—" }

    ToolDangerDialog { id: dlg; serverName: root.serverName; prod: root.prod }

    // пароль sudo
    SDialog {
        id: sudo
        property var entry: null
        title: qsTr("Administrator password")
        dialogWidth: 420
        function ask(e) { entry = e; pass.text = ""; err.text = ""; open() }
        onOpened: pass.forceFocus()
        onAcceptRequested: go()
        function go() {
            if (pass.text === "") return
            root.call("priv.sudo.auth", { connection_id: root.serverId, password: pass.text }, () => {
                const e = entry; entry = null; close()
                if (e) root.call(e.method, e.params, e.ok, e.fail)
            }, (m) => { err.text = m })
        }
        SLabel { Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone; sub: true
                 text: qsTr("This action needs root rights. Enter the sudo password of the SSH user; it is kept in memory only.") }
        STextField { id: pass; Layout.fillWidth: true; secret: true; label: qsTr("Password"); error: err.text; onAccepted: sudo.go() }
        SLabel { id: err; visible: false }
        buttons: [
            Item { Layout.fillWidth: true },
            SButton { text: qsTr("Cancel"); onClicked: sudo.close() },
            SButton { text: qsTr("Continue"); kind: "primary"; onClicked: sudo.go() }
        ]
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        SBanner {
            Layout.fillWidth: true; Layout.margins: 8; Layout.bottomMargin: 0
            visible: root.errorText !== ""
            text: root.errorText
            onDismissed: root.errorText = ""
        }
        SBanner {
            Layout.fillWidth: true; Layout.margins: 8; Layout.bottomMargin: 0
            visible: root.readonlyMode
            tone: Theme.amber
            text: qsTr("Read-only mode: changes are blocked")
            onDismissed: root.desk.setReadonly(false)
        }
        ColumnLayout { id: body; Layout.fillWidth: true; Layout.fillHeight: true; spacing: 0 }
    }
    SSpinner { visible: root.busy > 0; anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 10; width: 16; height: 16; z: 5 }
}
