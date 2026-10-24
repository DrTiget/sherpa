import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// «Службы»: systemd-юниты (svc.list/status/logs), запуск/остановка/перезапуск/автозапуск с подтверждением по имени сервера.
UtilPane {
    id: root
    property var all: []
    property var rows: []
    property string filterMode: "all"
    property string query: ""
    property string sortKey: "name"
    property bool sortDesc: false
    property var sel: null
    property var journal: []
    property bool journalNeedsSudo: false
    property bool loading: false
    property bool unavailable: !!desktop.capabilities.utilities && desktop.capabilities.utilities.services === false
    readonly property int failedCount: all.filter(s => s.active_state === "failed").length

    function stateTone(s) { return s.active_state === "active" ? Theme.green : (s.active_state === "failed" ? Theme.red : (s.active_state === "activating" || s.active_state === "deactivating" ? Theme.amber : Theme.gray)) }
    function apply() {
        const q = query.toLowerCase()
        let l = all.filter(s => (filterMode === "all" || (filterMode === "failed" && s.active_state === "failed")
                                  || (filterMode === "running" && s.sub_state === "running") || (filterMode === "enabled" && s.enabled === "enabled"))
                                 && (q === "" || s.name.toLowerCase().indexOf(q) >= 0 || (s.description || "").toLowerCase().indexOf(q) >= 0))
        const k = sortKey, d = sortDesc ? -1 : 1
        l.sort((a, b) => {
            const x = a[k], y = b[k]
            if (x === y) return 0
            if (x === null || x === undefined) return 1
            if (y === null || y === undefined) return -1
            return (typeof x === "string" ? x.localeCompare(y) : x - y) * d
        })
        rows = l
    }
    onAllChanged: apply()
    onFilterModeChanged: apply()
    onQueryChanged: apply()
    onSortKeyChanged: apply()
    onSortDescChanged: apply()
    function refresh() {
        if (!working || unavailable) return
        loading = true
        rpc("svc.list", {}, (d) => {
            loading = false; all = d.items || []
            if (sel) { const n = all.find(s => s.name === sel.name); sel = n || null }
        }, (t) => { loading = false; showNotice(t) })
    }
    function select(s) {
        sel = s; journal = []; journalNeedsSudo = false
        if (s) loadJournal(false)
    }
    function loadJournal(askSudo) {
        const s = sel
        rpc("svc.logs", { unit: s.name, lines: 40 }, (d) => { if (sel && sel.name === s.name) { journal = d.items || []; journalNeedsSudo = false } },
            (t, code) => { if (sel && sel.name === s.name) { journal = []; journalNeedsSudo = code === "priv.required" } }, !askSudo)
    }
    function act(verb, s) {
        const names = { start: qsTr("Start"), stop: qsTr("Stop"), restart: qsTr("Restart"), reload: qsTr("Reload"), enable: qsTr("Enable autostart"), disable: qsTr("Disable autostart") }
        danger("svc." + verb, { unit: s.name }, {
            title: names[verb] + ": " + s.name,
            message: qsTr("The action is applied on the server immediately. Stopping or disabling network, SSH or systemd services may cut off access."),
            confirmText: names[verb], needName: true
        }, () => { showNotice(names[verb] + ": " + s.name + " — OK", Theme.green); refresh(); select(s) })
    }
    onWorkingChanged: if (working) refresh()
    Component.onCompleted: refresh()

    SEmptyState {
        anchors.fill: parent; visible: root.unavailable || !root.working
        glyph: "⚙"; title: root.working ? qsTr("Not available on this server") : qsTr("Waiting for the server…")
        description: root.working ? qsTr("systemd was not found on this server.") : ""
    }
    ColumnLayout {
        anchors.fill: parent; anchors.margins: 12; spacing: 10
        visible: root.working && !root.unavailable
        RowLayout {
            Layout.fillWidth: true; spacing: 8
            SSegmented {
                current: root.filterMode
                model: [{ value: "all", text: qsTr("All") }, { value: "running", text: qsTr("Running") },
                        { value: "failed", text: qsTr("Failed") + (root.failedCount ? " (" + root.failedCount + ")" : "") }, { value: "enabled", text: qsTr("Autostart") }]
                onActivated: (v) => root.filterMode = v
            }
            STextField { objectName: "svcSearch"; Layout.fillWidth: true; placeholderText: qsTr("Search services…"); onEdited: root.query = text; Accessible.name: qsTr("Search services") }
            SSpinner { visible: root.loading; width: 18; height: 18 }
            SButton { text: qsTr("Refresh"); compact: true; onClicked: root.refresh() }
        }
        RowLayout {
            Layout.fillWidth: true; Layout.fillHeight: true; spacing: 10
            UTable {
                objectName: "svcTable"
                Layout.fillWidth: true; Layout.fillHeight: true
                rows: root.rows
                sortKey: root.sortKey; sortDesc: root.sortDesc
                emptyText: root.loading ? "" : qsTr("No services match the filter")
                onSortClicked: (k) => { if (root.sortKey === k) root.sortDesc = !root.sortDesc; else { root.sortKey = k; root.sortDesc = false } }
                onRowClicked: (i, r) => root.select(r)
                columns: [
                    { key: "active_state", title: qsTr("State"), w: 96, kind: "dot", sortable: true, fmt: (r) => r.active_state, color: (r) => root.stateTone(r) },
                    { key: "name", title: qsTr("Unit"), w: 190, sortable: true, mono: true },
                    { key: "description", title: qsTr("Description"), w: 0 },
                    { key: "enabled", title: qsTr("Autostart"), w: 82, sortable: true, color: (r) => r.enabled === "enabled" ? Theme.green : Theme.textSub },
                    { key: "memory_bytes", title: qsTr("Memory"), w: 76, right: true, sortable: true, fmt: (r) => r.memory_bytes === null || r.memory_bytes === undefined ? "—" : Fmt.bytes(r.memory_bytes) }
                ]
            }
            // карточка службы
            Rectangle {
                visible: root.sel !== null
                Layout.preferredWidth: 300; Layout.fillHeight: true
                radius: 8; color: Theme.card; border.color: Theme.line
                ColumnLayout {
                    anchors.fill: parent; anchors.margins: 10; spacing: 8
                    RowLayout {
                        Layout.fillWidth: true
                        Rectangle { width: 10; height: 10; radius: 5; color: root.sel ? root.stateTone(root.sel) : Theme.gray }
                        SLabel { Layout.fillWidth: true; font.bold: true; mono: true; text: root.sel ? root.sel.name : "" }
                        SLabel { text: "×"; font.pixelSize: 18; sub: true; MouseArea { anchors.fill: parent; anchors.margins: -6; onClicked: root.select(null) } }
                    }
                    SLabel { Layout.fillWidth: true; sub: true; text: root.sel ? (root.sel.description || "") : "" }
                    GridLayout {
                        Layout.fillWidth: true; columns: 2; columnSpacing: 10; rowSpacing: 3
                        SLabel { caps: true; text: qsTr("Active") }
                        SLabel { Layout.fillWidth: true; mono: true; text: root.sel ? root.sel.active_state + " (" + root.sel.sub_state + ")" : "" }
                        SLabel { caps: true; text: qsTr("Autostart") }
                        SLabel { Layout.fillWidth: true; mono: true; text: root.sel ? root.sel.enabled : "" }
                        SLabel { caps: true; text: "PID" }
                        SLabel { Layout.fillWidth: true; mono: true; text: root.sel && root.sel.pid ? root.sel.pid : "—" }
                        SLabel { caps: true; text: qsTr("User") }
                        SLabel { Layout.fillWidth: true; mono: true; text: root.sel && root.sel.user ? root.sel.user : "—" }
                        SLabel { caps: true; text: qsTr("Memory") }
                        SLabel { Layout.fillWidth: true; mono: true; text: root.sel ? Fmt.bytes(root.sel.memory_bytes) : "" }
                    }
                    Flow {
                        Layout.fillWidth: true; spacing: 6
                        SButton { text: qsTr("Start"); compact: true; enabled: root.sel && root.sel.active_state !== "active"; onClicked: root.act("start", root.sel) }
                        SButton { text: qsTr("Stop"); compact: true; enabled: root.sel && root.sel.active_state === "active"; onClicked: root.act("stop", root.sel) }
                        SButton { text: qsTr("Restart"); compact: true; onClicked: root.act("restart", root.sel) }
                        SButton { text: qsTr("Reload"); compact: true; onClicked: root.act("reload", root.sel) }
                        SButton { text: root.sel && root.sel.enabled === "enabled" ? qsTr("Disable autostart") : qsTr("Enable autostart"); compact: true
                                  onClicked: root.act(root.sel.enabled === "enabled" ? "disable" : "enable", root.sel) }
                    }
                    SLabel { caps: true; text: qsTr("Journal") }
                    Rectangle {
                        Layout.fillWidth: true; Layout.fillHeight: true; radius: 6; color: Theme.inputFill; border.color: Theme.line; clip: true
                        ListView {
                            anchors.fill: parent; anchors.margins: 4; model: root.journal; clip: true
                            ScrollBar.vertical: ScrollBar {}
                            delegate: SLabel {
                                required property var modelData
                                width: ListView.view.width; mono: true; font.pixelSize: 10; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone
                                color: Fmt.prioTone(modelData.priority) === Theme.gray ? Theme.text : Fmt.prioTone(modelData.priority)
                                text: Fmt.time(modelData.timestamp_usec) + " " + Fmt.text(modelData.message)
                            }
                        }
                        SLabel { visible: root.journal.length === 0 && !root.journalNeedsSudo; anchors.centerIn: parent; sub: true; text: qsTr("No journal entries") }
                        ColumnLayout {
                            visible: root.journalNeedsSudo; anchors.centerIn: parent; width: parent.width - 20; spacing: 8
                            SLabel { Layout.fillWidth: true; sub: true; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; elide: Text.ElideNone
                                     text: qsTr("The journal of this service needs administrator rights") }
                            SButton { Layout.alignment: Qt.AlignHCenter; objectName: "svcJournalSudo"; text: qsTr("Show with sudo"); compact: true; onClicked: root.loadJournal(true) }
                        }
                    }
                }
            }
        }
    }
}
