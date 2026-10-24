import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Туннели SSH: local (-L), remote (-R), dynamic (-D, SOCKS5). Статусы приходят событиями tunnel.
Item {
    id: root
    required property var desktop
    property string errorText: ""
    property bool formOpen: false
    property string formError: ""
    property bool starting: false
    readonly property bool ro: desktop.readOnlyMode

    ListModel { id: tunnels }   // tid, kind, bind_host, bind_port, target, status, error

    function tunnelAt(i) { if (i < 0 || i >= tunnels.count) return ({}); const t = tunnels.get(i); return { tid: t.tid, kind: t.kind, bind_host: t.bind_host, bind_port: t.bind_port, target: t.target, status: t.status, error: t.error } }
    function idx(id) { for (let i = 0; i < tunnels.count; ++i) if (tunnels.get(i).tid === id) return i; return -1 }
    function targetText(t) { return t && t.length === 2 ? t[0] + ":" + t[1] : (typeof t === "string" ? t : "") }
    function apply(id, data) {
        let i = idx(id)
        if (i < 0) {
            tunnels.insert(0, { tid: id, kind: data.kind || "local", bind_host: data.bind_host || "", bind_port: data.bind_port || 0,
                                target: targetText(data.target), status: data.status || "starting", error: "" })
            i = 0
        }
        const cur = tunnels.get(i)
        // состояние от демона заменяет прежнее целиком (running приходит без kind/target) — старые поля сохраняем
        if (data.kind) tunnels.setProperty(i, "kind", data.kind)
        if (data.bind_host) tunnels.setProperty(i, "bind_host", data.bind_host)
        if (data.bind_port) tunnels.setProperty(i, "bind_port", data.bind_port)
        if (data.target !== undefined && data.target !== null) tunnels.setProperty(i, "target", targetText(data.target))
        if (data.status) tunnels.setProperty(i, "status", data.status)
        tunnels.setProperty(i, "error", data.error ? Backend.errorText(data.error.code || "", data.error.params || {}, data.error.message || "") : (data.status === "failed" ? cur.error : ""))
    }
    function load() {
        desktop.rpcRaw("tunnel.list", {}, function (ok, data) {
            if (!ok) return
            for (const it of (data.items || []).slice().reverse())
                if (it.connection_id === desktop.connectionId) apply(it.id, it.data || {})
        })
    }
    Component.onCompleted: load()
    Connections {
        target: root.desktop
        function onEventReceived(ev) { if (ev.event === "tunnel" && ev.id) root.apply(ev.id, ev.data || {}) }
        function onStateChanged() { if (root.desktop.state === "ready") root.load() }
    }

    function openForm() {
        formError = ""
        kindSel.current = "local"
        bindHost.text = "127.0.0.1"; bindPort.text = ""; targetHost.text = "localhost"; targetPort.text = ""
        formOpen = true
    }
    function start() {
        formError = ""
        const kind = kindSel.current
        const bp = bindPort.text === "" ? 0 : parseInt(bindPort.text)
        if (!(bp >= 0 && bp <= 65535)) { formError = qsTr("Port must be between 1 and 65535"); return }
        if (kind === "remote" && bp === 0) { formError = qsTr("Remote tunnels need an explicit port"); return }
        const params = { kind: kind, bind_host: bindHost.text.trim() || "127.0.0.1", bind_port: bp }
        if (kind !== "dynamic") {
            const tp = parseInt(targetPort.text)
            if (targetHost.text.trim() === "" || !(tp >= 1 && tp <= 65535)) { formError = qsTr("Enter the target host and port"); return }
            params.target_host = targetHost.text.trim(); params.target_port = tp
        }
        launch(kind, params)
    }
    // Запуск по готовым параметрам (из формы и из тестов); нелокальный bind — через danger.prepare
    function launch(kind, params) {
        const bp = params.bind_port
        const nonLocal = !/^(127\.|::1$)/.test(params.bind_host)
        const run = (token) => {
            starting = true
            const p = Object.assign({ connection_id: desktop.connectionId }, params)
            if (token) p.confirm_token = token
            desktop.rpcRaw("tunnel.start", p, function (ok, data, code, text) {
                starting = false
                if (!ok) { formError = text; return }
                formOpen = false
                root.apply(data.id, { status: data.status, kind: kind, bind_host: data.bind_host, bind_port: data.bind_port,
                                      target: kind === "dynamic" ? null : [params.target_host, params.target_port] })
            })
        }
        if (nonLocal) {
            danger.ask({
                title: qsTr("Open a tunnel to the network"),
                message: qsTr("The tunnel listens on %1, so other machines can use it to reach the target through this server.").arg(params.bind_host),
                details: [kind + " " + params.bind_host + ":" + bp + (kind === "dynamic" ? "" : " → " + params.target_host + ":" + params.target_port)],
                requireName: true, confirmText: qsTr("Open tunnel"),
                onConfirm: function () {
                    const full = Object.assign({ connection_id: desktop.connectionId }, params)
                    desktop.rpcRaw("danger.prepare", { method: "tunnel.start", params: full }, function (ok, data, code, text) {
                        if (!ok) { formError = text; return }
                        run(data.confirm_token)
                    })
                }
            })
        } else run(null)
    }
    function stop(id) { desktop.rpcRaw("tunnel.stop", { id: id }, function (ok, d, c, text) { if (!ok) errorText = text }) }
    function describe(t) {
        if (t.kind === "dynamic") return qsTr("SOCKS5 proxy on %1:%2").arg(t.bind_host).arg(t.bind_port)
        const dir = t.kind === "local" ? t.bind_host + ":" + t.bind_port + "  →  " + t.target : t.target + "  ←  " + qsTr("server") + " " + t.bind_host + ":" + t.bind_port
        return dir
    }
    function kindLabel(k) { return k === "local" ? "L" : (k === "remote" ? "R" : "D") }
    function statusColor(s) { return s === "running" ? Theme.green : (s === "failed" ? Theme.red : (s === "starting" ? Theme.amber : Theme.gray)) }

    property alias dangerDialog: danger
    property alias tunnelModel: tunnels
    DangerDialog { id: danger; serverName: root.desktop.name }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 10
        RowLayout {
            Layout.fillWidth: true
            SLabel { caps: true; text: qsTr("Tunnels"); Layout.fillWidth: true }
            SButton { text: qsTr("New tunnel…"); kind: "primary"; compact: true; enabled: !root.ro; onClicked: root.openForm() }
        }
        SBanner { Layout.fillWidth: true; visible: root.errorText !== ""; text: root.errorText; onDismissed: root.errorText = "" }
        SBanner { Layout.fillWidth: true; visible: root.ro; tone: Theme.amber; text: qsTr("Read-only mode: new tunnels are blocked.") }

        // форма
        Rectangle {
            visible: root.formOpen
            Layout.fillWidth: true
            implicitHeight: fcol.implicitHeight + 24
            radius: 8; color: Theme.card; border.color: Theme.line
            ColumnLayout {
                id: fcol
                anchors.fill: parent; anchors.margins: 12; spacing: 10
                SSegmented {
                    id: kindSel
                    current: "local"
                    model: [{ value: "local", text: qsTr("Local (-L)") }, { value: "remote", text: qsTr("Remote (-R)") }, { value: "dynamic", text: qsTr("SOCKS (-D)") }]
                }
                SLabel {
                    Layout.fillWidth: true; sub: true; font.pixelSize: 11; wrapMode: Text.Wrap; elide: Text.ElideNone
                    text: kindSel.current === "local" ? qsTr("Listens on this computer and forwards connections through the server to the target.")
                          : kindSel.current === "remote" ? qsTr("Listens on the server and forwards connections to a target reachable from this computer.")
                          : qsTr("Starts a SOCKS5 proxy on this computer; connections go out from the server.")
                }
                RowLayout {
                    Layout.fillWidth: true; spacing: 10
                    STextField { id: bindHost; Layout.fillWidth: true; Layout.preferredWidth: 2; label: kindSel.current === "remote" ? qsTr("Listen address (on server)") : qsTr("Listen address"); mono: true }
                    STextField { id: bindPort; Layout.fillWidth: true; Layout.preferredWidth: 1; label: qsTr("Port"); mono: true; placeholderText: kindSel.current === "remote" ? "" : qsTr("auto"); validator: IntValidator { bottom: 0; top: 65535 } }
                    STextField { id: targetHost; visible: kindSel.current !== "dynamic"; Layout.fillWidth: true; Layout.preferredWidth: 2; label: qsTr("Target host"); mono: true }
                    STextField { id: targetPort; visible: kindSel.current !== "dynamic"; Layout.fillWidth: true; Layout.preferredWidth: 1; label: qsTr("Target port"); mono: true; validator: IntValidator { bottom: 0; top: 65535 } }
                }
                SLabel { visible: root.formError !== ""; Layout.fillWidth: true; color: Theme.red; text: root.formError; wrapMode: Text.Wrap; elide: Text.ElideNone }
                RowLayout {
                    Layout.alignment: Qt.AlignRight; spacing: 8
                    SButton { text: qsTr("Cancel"); onClicked: root.formOpen = false }
                    SButton { text: qsTr("Start"); kind: "primary"; busy: root.starting; onClicked: root.start() }
                }
            }
        }

        // список
        Rectangle {
            Layout.fillWidth: true; Layout.fillHeight: true
            radius: 8; color: Theme.inputFill; border.color: Theme.line; clip: true
            ListView {
                anchors.fill: parent; anchors.margins: 6
                model: tunnels
                spacing: 4
                clip: true
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    id: tr
                    required property string tid
                    required property string kind
                    required property string bind_host
                    required property int bind_port
                    required property string target
                    required property string status
                    required property string error
                    width: ListView.view.width; height: 52; radius: 8
                    color: Theme.card; border.color: Theme.line
                    opacity: status === "stopped" ? 0.6 : 1
                    RowLayout {
                        anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 12
                        Rectangle {
                            width: 28; height: 28; radius: 7
                            color: Theme.alpha(Theme.cyan, 0.16); border.color: Theme.alpha(Theme.cyan, 0.5)
                            SLabel { anchors.centerIn: parent; text: root.kindLabel(tr.kind); mono: true; color: Theme.cyan; font.bold: true }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true; spacing: 1
                            SLabel { Layout.fillWidth: true; mono: true; text: root.describe(tr) }
                            SLabel {
                                Layout.fillWidth: true; font.pixelSize: 11
                                color: tr.status === "failed" ? Theme.red : Theme.textSub
                                text: tr.status === "failed" && tr.error !== "" ? tr.error : tr.tid
                            }
                        }
                        Row {
                            spacing: 6
                            Rectangle { width: 8; height: 8; radius: 4; color: root.statusColor(tr.status); anchors.verticalCenter: parent.verticalCenter }
                            SLabel { text: tr.status === "running" ? qsTr("running") : tr.status === "starting" ? qsTr("starting") : tr.status === "stopped" ? qsTr("stopped") : qsTr("failed"); font.pixelSize: 11; color: root.statusColor(tr.status) }
                        }
                        SButton { visible: tr.status === "running" || tr.status === "starting"; compact: true; text: qsTr("Stop"); onClicked: root.stop(tr.tid) }
                        SLabel {
                            visible: tr.status === "stopped" || tr.status === "failed"
                            text: "×"; sub: true; font.pixelSize: 16
                            MouseArea { anchors.fill: parent; anchors.margins: -5; cursorShape: Qt.PointingHandCursor; onClicked: tunnels.remove(root.idx(tr.tid)) }
                            Accessible.role: Accessible.Button
                            Accessible.name: qsTr("Remove from list")
                        }
                    }
                }
            }
            SEmptyState {
                anchors.fill: parent
                visible: tunnels.count === 0
                glyph: "⇌"
                title: qsTr("No tunnels")
                description: qsTr("Forward a port through this server: for example reach a database that listens only on the server.")
            }
        }
    }
}
