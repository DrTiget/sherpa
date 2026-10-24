import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Базы данных: MySQL/MariaDB, PostgreSQL, Redis — список баз, активные запросы (Kill), дамп в /var/backups.
ToolPane {
    id: root
    property string tab: "dbs"
    property string engine: ""
    property var detected: []
    property var dbs: []
    property var queries: []
    property bool loading: false
    property string unavailable: ""
    readonly property var engineList: {
        const l = []
        if (detected.indexOf("mysql") >= 0) l.push({ value: "mysql", text: "MySQL" })
        if (detected.indexOf("mariadb") >= 0) l.push({ value: "mariadb", text: "MariaDB" })
        if (detected.indexOf("psql") >= 0) l.push({ value: "postgres", text: "PostgreSQL" })
        if (detected.indexOf("redis-cli") >= 0) l.push({ value: "redis", text: "Redis" })
        return l
    }
    readonly property bool isRedis: engine === "redis"

    function detect() {
        req("db.detect", {}, d => {
            detected = d.tools || []
            if (engineList.length === 0) { unavailable = qsTr("No database clients (mysql, psql, redis-cli) were found on this server."); return }
            unavailable = ""
            if (!engineList.some(e => e.value === engine)) engine = engineList[0].value
            else refresh()
        }, e => { unavailable = e })
    }
    function fail(e) { errorText = e; loading = false; dbs = []; queries = [] }
    function refresh() {
        if (engine === "") return
        loading = true
        if (tab === "dbs") req("db.list", { engine: engine }, d => { dbs = d.items || []; loading = false }, fail)
        else req("db.queries", { engine: engine }, d => { queries = d.items || []; loading = false }, fail)
    }
    function kill(q) {
        dangerousReq({ title: isRedis ? qsTr("Disconnect client") : qsTr("Cancel query"), items: [(q.user || "") + " @ " + (q.database || "") + " · " + (q.query || "").slice(0, 120)],
                       confirmText: qsTr("Kill") }, "db.kill", { engine: engine, id: parseInt(q.id) }, () => refresh())
    }
    function openDump(r) {
        dumpName = r.name
        const ext = isRedis ? "rdb" : (engine === "postgres" ? "dump" : "sql")
        dumpPath.text = "/var/backups/" + (isRedis ? "redis" : r.name) + "-" + new Date().toISOString().slice(0, 10) + "." + ext
        dumpDlg.open()
    }
    property string dumpName: ""
    property alias dumpPath: dumpPath
    function doDump() {
        const p = dumpPath.text.trim()
        if (p.indexOf("/var/backups/") !== 0) { dumpPath.error = qsTr("The file must be inside /var/backups/"); return }
        dumpPath.error = ""
        dumpDlg.close()
        dangerousReq({ title: qsTr("Create dump"), message: qsTr("The dump is written on the server with permissions 600; large databases can take a while and load the server."),
                       items: [dumpName + " → " + p], confirmText: qsTr("Create dump") }, "db.dump", { engine: engine, database: dumpName, path: p },
                     d => { toast.text = qsTr("Dump saved: %1").arg(d.path || p) })
    }

    onTabChanged: refresh()
    onEngineChanged: refresh()
    Component.onCompleted: detect()

    RowLayout {
        Layout.fillWidth: true; Layout.margins: 10; Layout.bottomMargin: 6; spacing: 10
        SSegmented { visible: root.unavailable === ""; model: root.engineList; current: root.engine; onActivated: (v) => root.engine = v }
        SSegmented { visible: root.unavailable === ""; model: [{ value: "dbs", text: qsTr("Databases") }, { value: "queries", text: root.isRedis ? qsTr("Clients") : qsTr("Queries") }]
                     current: root.tab; onActivated: (v) => root.tab = v }
        SLabel { id: toast; sub: true; Layout.fillWidth: true; color: Theme.green }
        SButton { compact: true; text: qsTr("Refresh"); onClicked: root.unavailable === "" ? root.refresh() : root.detect() }
    }
    SEmptyState {
        visible: root.unavailable !== ""
        Layout.fillWidth: true; Layout.fillHeight: true
        glyph: "▤"; title: qsTr("Databases are not available"); description: root.unavailable
    }
    StackLayout {
        visible: root.unavailable === ""
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 10; Layout.topMargin: 0
        currentIndex: root.tab === "dbs" ? 0 : 1
        DataTable {
            objectName: "dbTable"
            loading: root.loading; rows: root.dbs
            actions: [{ id: "dump", text: qsTr("Dump…"), enabled: r => !root.readonlyMode }]
            columns: root.isRedis ? [
                { key: "name", title: qsTr("Database"), flex: 1, mono: true },
                { key: "keys", title: qsTr("Keys"), w: 90, right: true, mono: true },
                { key: "expires", title: qsTr("With TTL"), w: 90, right: true, mono: true },
                { key: "avg_ttl", title: qsTr("Avg TTL, ms"), w: 110, right: true, mono: true }
            ] : [
                { key: "name", title: qsTr("Database"), flex: 1.4, mono: true },
                { key: "size_bytes", title: qsTr("Size"), w: 110, right: true, mono: true, text: r => root.fmtBytes(r.size_bytes), sort: r => r.size_bytes },
                { key: "owner", title: qsTr("Owner"), w: 140, mono: true, text: r => r.owner || "—" }
            ]
            onAction: (id, r) => root.openDump(r)
        }
        DataTable {
            objectName: "queryTable"
            loading: root.loading; rows: root.queries
            actions: [{ id: "kill", text: "Kill…", kind: "danger", enabled: r => !root.readonlyMode }]
            emptyText: qsTr("Nothing is running")
            columns: root.isRedis ? [
                { key: "id", title: "ID", w: 70, mono: true },
                { key: "addr", title: qsTr("Address"), flex: 1, mono: true },
                { key: "name", title: qsTr("Name"), flex: 1, mono: true },
                { key: "age", title: "Age, s", w: 80, right: true, mono: true },
                { key: "cmd", title: qsTr("Command"), flex: 1, mono: true }
            ] : [
                { key: "id", title: "ID", w: 70, mono: true },
                { key: "user", title: qsTr("User"), w: 110, mono: true },
                { key: "database", title: qsTr("Database"), w: 120, mono: true },
                { key: "elapsed_sec", title: qsTr("Seconds"), w: 80, right: true, mono: true },
                { key: "state", title: qsTr("State"), w: 100 },
                { key: "query", title: qsTr("Query"), flex: 2, mono: true }
            ]
            onAction: (id, r) => root.kill(r)
        }
    }

    SDialog {
        id: dumpDlg
        title: qsTr("Create dump")
        dialogWidth: 520
        onOpened: dumpPath.forceFocus()
        onAcceptRequested: root.doDump()
        SLabel { Layout.fillWidth: true; sub: true; wrapMode: Text.Wrap; elide: Text.ElideNone
                 text: root.isRedis ? qsTr("A full RDB snapshot of the Redis instance will be saved on the server.") : qsTr("Database “%1” will be saved on the server (the content is not transferred to this computer).").arg(root.dumpName) }
        STextField { id: dumpPath; Layout.fillWidth: true; label: qsTr("File on the server"); mono: true; onAccepted: root.doDump() }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: dumpDlg.close() }, SButton { text: qsTr("Next…"); kind: "primary"; onClicked: root.doDump() } ]
    }
}
