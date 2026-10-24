import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Журнал действий Sherpa: что и когда делали через приложение (без секретов); фильтры, поиск, экспорт CSV.
ToolPane {
    id: root
    property var items: []
    property bool loading: false
    property string scope: "this"
    property string query: ""
    property int limit: 200
    property string note: ""

    function srvName(id) { if (!id) return "—"; const s = Backend.serverById(id); return s && s.name ? s.name : id }
    function params() {
        const p = { limit: limit }
        if (scope === "this") p.connection_id = serverId
        if (query.trim() !== "") p.q = query.trim()
        return p
    }
    function refresh() {
        loading = true
        call("audit.list", params(), d => { items = d.items || []; loading = false }, e => { errorText = e; loading = false })
    }
    function exportCsv() {
        call("audit.export", params(), d => {
            const path = ToolUtil.defaultExportDir() + "/sherpa-audit-" + new Date().toISOString().slice(0, 10) + ".csv"
            note = ToolUtil.writeText(path, d.csv || "") ? qsTr("Saved: %1").arg(path) : qsTr("Could not write %1").arg(path)
        }, e => { errorText = e })
    }
    function outcomeTone(r) { return r.outcome === "ok" ? Theme.green : Theme.red }

    onScopeChanged: refresh()
    Component.onCompleted: refresh()
    Timer { id: debounce; interval: 350; onTriggered: root.refresh() }

    RowLayout {
        Layout.fillWidth: true; Layout.margins: 10; Layout.bottomMargin: 6; spacing: 10
        SSegmented { model: [{ value: "this", text: qsTr("This server") }, { value: "all", text: qsTr("All servers") }]; current: root.scope; onActivated: (v) => root.scope = v }
        STextField { objectName: "auditSearch"; Layout.fillWidth: true; placeholderText: qsTr("Search by action or outcome"); onEdited: { root.query = text; debounce.restart() } }
        SSelect { Layout.preferredWidth: 100; current: String(root.limit); model: ["100", "200", "500", "1000"].map(v => ({ value: v, text: v })); onActivated: (v) => { root.limit = parseInt(v); root.refresh() } }
        SButton { compact: true; text: qsTr("Export CSV"); onClicked: root.exportCsv() }
        SButton { compact: true; text: qsTr("Refresh"); onClicked: root.refresh() }
    }
    DataTable {
        objectName: "auditTable"
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 10; Layout.topMargin: 0
        loading: root.loading; rows: root.items
        emptyText: qsTr("The log is empty")
        columns: [
            { key: "timestamp", title: qsTr("Time"), w: 160, text: r => root.fmtTime(r.timestamp), sort: r => r.timestamp },
            { key: "connection_id", title: qsTr("Server"), w: 130, text: r => root.srvName(r.connection_id) },
            { key: "method", title: qsTr("Action"), flex: 1.2, mono: true },
            { key: "outcome", title: qsTr("Outcome"), flex: 1, chip: true, tone: r => root.outcomeTone(r) },
            { key: "client", title: qsTr("Client"), w: 70, mono: true }
        ]
        sortKey: "timestamp"; sortDesc: true
    }
    SLabel { visible: root.note !== ""; Layout.margins: 10; Layout.topMargin: 0; sub: true; text: root.note }
}
