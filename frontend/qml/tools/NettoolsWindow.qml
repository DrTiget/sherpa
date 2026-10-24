import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// «Сетевые инструменты»: ping, traceroute, dig, curl -I, проверка порта — выполняются на сервере, вывод потоком.
ToolPane {
    id: root
    property string tool: "ping"
    property string sub: ""                     // подписка текущего запуска
    property bool running: false
    property string status: ""
    property int exitCode: -1
    property alias targetField: targetField
    property alias out: out
    property alias countField: countField
    property alias portField: portField
    property string startedAt: ""

    readonly property var toolList: [
        { value: "ping", text: "ping" }, { value: "trace", text: "traceroute" }, { value: "dig", text: "dig" },
        { value: "curl", text: "curl -I" }, { value: "port", text: qsTr("Port check") }
    ]
    function placeholder() {
        return tool === "curl" ? "https://example.com/" : "example.com"
    }
    function run() {
        const target = targetField.text.trim()
        if (target === "") { targetField.error = qsTr("This field is required"); return }
        targetField.error = ""
        const p = { target: target }
        if (tool === "ping") p.count = parseInt(countField.text) || 4
        if (tool === "dig") p.record_type = digType.current
        if (tool === "port") {
            const port = parseInt(portField.text)
            if (!(port >= 1 && port <= 65535)) { portField.error = qsTr("Port must be 1–65535"); return }
            portField.error = ""
            p.port = port
        }
        out.text = ""
        status = qsTr("Running…")
        running = true
        exitCode = -1
        req("nettools." + tool, p, d => { sub = d.subscription }, e => { running = false; status = ""; errorText = e })
    }
    function cancel() {
        if (sub === "") return
        call("nettools.cancel", { subscription: sub }, () => {}, () => {})
        running = false
        status = qsTr("Cancelled")
        sub = ""
    }
    onEvent: (ev) => {
        if (ev.subscription !== sub || sub === "") return
        if (ev.event === "nettools.output") {
            const lines = (ev.records || []).map(r => r.text)
            out.text += (out.text === "" ? "" : "\n") + lines.join("\n")
            out.area.cursorPosition = out.text.length
        } else if (ev.event === "nettools.done") {
            running = false
            sub = ""
            exitCode = ev.exit_status === null || ev.exit_status === undefined ? -1 : ev.exit_status
            status = ev.exit_status === 124 ? qsTr("Timed out on the server (60 s)")
                   : (ev.exit_status === 0 ? qsTr("Done")
                   : qsTr("Finished with exit code %1").arg(ev.exit_status)) + (ev.truncated ? " · " + qsTr("output truncated") : "")
        } else if (ev.event === "error") {
            running = false
            sub = ""
            status = ""
            errorText = ev.error && ev.error.message ? ev.error.message : qsTr("Diagnostics failed")
        }
    }
    Component.onDestruction: cancel()

    ColumnLayout {
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 12
        spacing: 10
        SSegmented { model: root.toolList; current: root.tool; onActivated: (v) => { root.tool = v; targetField.error = "" } }
        RowLayout {
            Layout.fillWidth: true; spacing: 10
            STextField {
                id: targetField
                Layout.fillWidth: true
                label: root.tool === "curl" ? qsTr("URL") : qsTr("Host or IP")
                placeholderText: root.placeholder()
                mono: true
                onAccepted: root.running ? root.cancel() : root.run()
            }
            STextField { id: countField; visible: root.tool === "ping"; Layout.preferredWidth: 80; label: qsTr("Count"); text: "4"; mono: true }
            SSelect { id: digType; visible: root.tool === "dig"; Layout.preferredWidth: 100; label: qsTr("Record"); current: "A"
                      model: ["A", "AAAA", "MX", "TXT", "NS"].map(v => ({ value: v, text: v })) }
            STextField { id: portField; visible: root.tool === "port"; Layout.preferredWidth: 90; label: qsTr("Port"); text: "443"; mono: true }
            SButton { Layout.alignment: Qt.AlignBottom; text: root.running ? qsTr("Cancel") : qsTr("Run"); kind: root.running ? "danger" : "primary"
                      onClicked: root.running ? root.cancel() : root.run() }
        }
        STextArea { id: out; objectName: "nettoolsOutput"; Layout.fillWidth: true; Layout.fillHeight: true; readOnly: true; wrap: false
                    placeholderText: qsTr("The command runs on the server %1; the output appears here.").arg(root.serverName) }
        RowLayout {
            Layout.fillWidth: true
            SSpinner { visible: root.running; width: 14; height: 14 }
            SLabel { text: root.status; color: root.exitCode > 0 ? Theme.amber : (root.exitCode === 0 ? Theme.green : Theme.textSub); Layout.fillWidth: true }
            SButton { compact: true; text: qsTr("Copy"); enabled: out.text !== ""; onClicked: ToolUtil.copy(out.text) }
        }
    }
}
