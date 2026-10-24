import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import QtQuick.Layouts
import Sherpa

// «Логи»: journald (log.query / log.follow с фильтрами) и файлы /var/log (log.file.tail).
UtilPane {
    id: root
    property var sources: ({ journal: true, files: [] })
    property string source: "journal"          // "journal" | путь к файлу
    property var entries: []
    property string cursor: ""
    property string sub: ""
    property bool following: false
    property bool loading: false
    property bool useSudo: false
    property string period: "1h"
    property string prio: ""
    property string boot: ""
    property string unit: ""
    property string grep: ""
    property int expanded: -1
    property bool unavailable: !!desktop.capabilities.utilities && desktop.capabilities.utilities.logs === false
    readonly property int maxRows: 3000
    readonly property var periods: [{ value: "15m", text: qsTr("15 min") }, { value: "1h", text: qsTr("1 h") }, { value: "today", text: qsTr("Today") }, { value: "24h", text: qsTr("24 h") }, { value: "", text: qsTr("All") }]
    readonly property var prios: [{ value: "", text: qsTr("Any level") }, { value: "0..3", text: qsTr("Errors") }, { value: "0..4", text: qsTr("Warnings and worse") },
                                  { value: "0..5", text: qsTr("Notice and worse") }, { value: "0..6", text: qsTr("Info and worse") }]
    readonly property var boots: [{ value: "", text: qsTr("All boots") }, { value: "0", text: qsTr("This boot") }, { value: "-1", text: qsTr("Previous boot") }]

    function sinceArg() { return { "15m": "-15min", "1h": "-1h", "today": "today", "24h": "-24h" }[period] || "" }
    function filters() {
        const p = {}
        if (unit !== "") p.unit = unit
        if (prio !== "") p.priority = prio
        if (sinceArg() !== "") p.since = sinceArg()
        if (grep !== "") p.grep = grep
        if (boot !== "") p.boot = boot
        if (useSudo) p.sudo = true
        return p
    }
    function toRow(e) {
        return { ts: e.timestamp_usec, prio: e.priority, unit: e.unit || (e.fields && (e.fields.SYSLOG_IDENTIFIER || e.fields._COMM)) || "", msg: Fmt.text(e.message), fields: e.fields || ({}), cursor: e.cursor }
    }
    function trim(list) { return list.length > maxRows ? list.slice(list.length - maxRows) : list }
    function reload() {
        if (!working || unavailable) return
        stopFollow(false)
        pending = []
        loading = true; expanded = -1
        if (source === "journal") {
            rpc("log.query", Object.assign(filters(), { lines: 500 }), (d) => {
                loading = false; entries = (d.items || []).map(toRow); cursor = d.cursor || ""
                if (wantFollow) startFollow()
            }, (t) => { loading = false; entries = []; showNotice(t) })
        } else {
            const p = { path: source, lines: 500 }
            if (useSudo) p.sudo = true
            rpc("log.file.tail", p, (d) => {
                loading = false
                const g = grep.toLowerCase()
                entries = (d.lines || []).filter(l => g === "" || l.toLowerCase().indexOf(g) >= 0).map(l => ({ ts: null, prio: null, unit: "", msg: l, fields: ({}) }))
            }, (t) => { loading = false; entries = []; showNotice(t) })
        }
    }
    property bool wantFollow: false
    function startFollow() {
        if (source !== "journal") return
        const p = Object.assign(filters(), { lines: 200, interval_ms: 1000 })
        if (cursor !== "") p.cursor = cursor
        delete p.since
        rpc("log.follow", p, (d) => { sub = d.subscription; following = true }, (t) => { following = false; wantFollow = false; showNotice(t) })
    }
    function stopFollow(clear) {
        if (sub !== "") { rpc("log.unfollow", { subscription: sub }, () => {}, () => {}); sub = "" }
        following = false
        if (clear) wantFollow = false
    }
    function toggleFollow() {
        if (following) stopFollow(true)
        else { wantFollow = true; if (sub === "") startFollow() }
    }
    onEvent: (ev) => {
        if (sub === "") return
        if (ev.event === "log.line" && ev.subscription === sub) {
            const r = toRow(ev.data)
            if (r.cursor) cursor = r.cursor
            pending.push(r)
            flushTimer.start()
        } else if (ev.event === "error" && ev.subscription === sub) {
            sub = ""; following = false
            showNotice((ev.error && ev.error.message) || qsTr("Follow stopped"))
        }
    }
    function loadSources() {
        rpc("log.sources", { sudo: useSudo }, (d) => { sources = d; if (!d.journal && source === "journal" && d.files.length) source = d.files[0] }, () => {})
    }
    function plainText() { return entries.map(e => (e.ts ? Fmt.dateTime(e.ts) + " " : "") + (e.unit ? e.unit + ": " : "") + e.msg).join("\n") }
    onWorkingChanged: if (working) { loadSources(); reload() }
    Component.onCompleted: if (working) { loadSources(); reload() }
    Component.onDestruction: if (sub !== "") desktop.call("log.unfollow", { subscription: sub })
    // строки follow копятся и вливаются пачкой: по одной перестройка списка при сотнях записей подвешивает интерфейс
    property var pending: []
    Timer {
        id: flushTimer; interval: 150
        onTriggered: {
            if (root.pending.length === 0) return
            const stick = view.atEnd
            root.entries = root.trim(root.entries.concat(root.pending))
            root.pending = []
            if (stick) Qt.callLater(() => view.positionViewAtEnd())
        }
    }
    Timer { id: debounce; interval: 450; onTriggered: root.reload() }

    SEmptyState {
        anchors.fill: parent; visible: root.unavailable || !root.working
        glyph: "≡"; title: root.working ? qsTr("Not available on this server") : qsTr("Waiting for the server…")
        description: root.working ? qsTr("journalctl was not found on this server.") : ""
    }
    RowLayout {
        anchors.fill: parent; anchors.margins: 10; spacing: 10
        visible: root.working && !root.unavailable
        // источники
        Rectangle {
            Layout.preferredWidth: 170; Layout.fillHeight: true; radius: 8; color: Theme.card; border.color: Theme.line
            ColumnLayout {
                anchors.fill: parent; anchors.margins: 6; spacing: 2
                SLabel { caps: true; text: qsTr("Sources"); Layout.leftMargin: 6; Layout.topMargin: 4 }
                ListView {
                    Layout.fillWidth: true; Layout.fillHeight: true; clip: true
                    model: (root.sources.journal ? ["journal"] : []).concat(root.sources.files || [])
                    ScrollBar.vertical: ScrollBar {}
                    delegate: Rectangle {
                        required property string modelData
                        width: ListView.view.width; height: 26; radius: 5
                        color: root.source === modelData ? Theme.selected : (sma.containsMouse ? Theme.hover : "transparent")
                        SLabel { x: 8; width: parent.width - 12; anchors.verticalCenter: parent.verticalCenter; font.pixelSize: 12
                                 text: modelData === "journal" ? qsTr("System journal") : modelData.replace("/var/log/", "") }
                        MouseArea { id: sma; anchors.fill: parent; hoverEnabled: true; onClicked: { root.source = modelData; root.reload() } }
                    }
                }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true; Layout.fillHeight: true; spacing: 8
            RowLayout {
                Layout.fillWidth: true; spacing: 8
                visible: root.source === "journal"
                SSelect { Layout.preferredWidth: 110; model: root.periods; current: root.period; onActivated: (v) => { root.period = v; root.reload() } }
                SSelect { Layout.preferredWidth: 160; model: root.prios; current: root.prio; onActivated: (v) => { root.prio = v; root.reload() } }
                SSelect { Layout.preferredWidth: 120; model: root.boots; current: root.boot; onActivated: (v) => { root.boot = v; root.reload() } }
                STextField { objectName: "logUnit"; Layout.fillWidth: true; placeholderText: qsTr("Unit (nginx.service)"); mono: true; onEdited: { root.unit = text; debounce.restart() } Accessible.name: qsTr("Unit filter") }
            }
            RowLayout {
                Layout.fillWidth: true; spacing: 8
                STextField { objectName: "logGrep"; Layout.fillWidth: true; placeholderText: qsTr("Search / regex…"); mono: true; onEdited: { root.grep = text; debounce.restart() } Accessible.name: qsTr("Search in logs") }
                SCheckBox { text: qsTr("sudo"); checked: root.useSudo; onToggled: { root.useSudo = checked; root.loadSources(); root.reload() } }
                SButton { objectName: "logFollow"; visible: root.source === "journal"; text: root.following ? qsTr("⏸ Pause") : qsTr("▶ Follow"); kind: root.following ? "primary" : "secondary"; compact: true; onClicked: root.toggleFollow() }
                SButton { text: qsTr("Copy"); compact: true; onClicked: root.desktop.copyText(root.plainText()) }
                SButton { text: qsTr("Save…"); compact: true; onClicked: saveDlg.open() }
                SButton { text: qsTr("Refresh"); compact: true; onClicked: root.reload() }
            }
            Rectangle {
                Layout.fillWidth: true; Layout.fillHeight: true; radius: 8; color: Theme.inputFill; border.color: Theme.line; clip: true
                ListView {
                    id: view
                    objectName: "logView"
                    anchors.fill: parent; anchors.margins: 2; clip: true; model: root.entries
                    readonly property bool atEnd: contentY + height >= contentHeight - 30
                    ScrollBar.vertical: ScrollBar {}
                    boundsBehavior: Flickable.StopAtBounds
                    delegate: Column {
                        id: rowCol
                        required property var modelData
                        required property int index
                        width: ListView.view.width
                        Rectangle {
                            width: parent.width; height: Math.max(20, txt.implicitHeight + 4)
                            color: root.expanded === rowCol.index ? Theme.selected : (rma.containsMouse ? Theme.hover : "transparent")
                            Rectangle { x: 4; y: 6; width: 8; height: 8; radius: 4; visible: modelData.prio !== null; color: Fmt.prioTone(modelData.prio) }
                            SLabel { x: 18; y: 2; width: 62; mono: true; font.pixelSize: 11; sub: true; text: Fmt.time(modelData.ts); visible: modelData.ts !== null }
                            SLabel { x: modelData.ts !== null ? 82 : 8; y: 2; width: modelData.unit !== "" ? 150 : 0; mono: true; font.pixelSize: 11; color: Theme.cyan; text: modelData.unit }
                            SLabel {
                                id: txt
                                x: (modelData.ts !== null ? 82 : 8) + (modelData.unit !== "" ? 156 : 0); y: 2; width: parent.width - x - 6
                                mono: true; font.pixelSize: 11; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone
                                color: modelData.prio !== null && modelData.prio <= 4 ? Fmt.prioTone(modelData.prio) : Theme.text
                                text: modelData.msg
                            }
                            MouseArea { id: rma; anchors.fill: parent; hoverEnabled: true; onClicked: root.expanded = root.expanded === rowCol.index ? -1 : rowCol.index }
                        }
                        Rectangle {
                            visible: root.expanded === rowCol.index && Object.keys(modelData.fields).length > 0
                            width: parent.width; height: visible ? fieldsCol.implicitHeight + 10 : 0; color: Theme.alpha(Theme.text, 0.05)
                            Column {
                                id: fieldsCol
                                x: 18; y: 5; width: parent.width - 24
                                Repeater {
                                    model: Object.keys(modelData.fields).sort()
                                    delegate: SLabel { required property string modelData; width: fieldsCol.width; mono: true; font.pixelSize: 10; sub: true
                                                       text: modelData + " = " + Fmt.text(rowCol.modelData.fields[modelData]) }
                                }
                            }
                        }
                    }
                }
                SSpinner { visible: root.loading; anchors.centerIn: parent; width: 22; height: 22 }
                SLabel { visible: !root.loading && root.entries.length === 0; anchors.centerIn: parent; sub: true; text: qsTr("No entries match the filter") }
            }
            RowLayout {
                Layout.fillWidth: true
                SLabel { sub: true; font.pixelSize: 11; text: qsTr("%1 entries").arg(root.entries.length) }
                SChip { visible: root.following; text: qsTr("following"); tone: Theme.green }
                Item { Layout.fillWidth: true }
                SButton { visible: root.source === "journal" && root.sources.journal; text: qsTr("Vacuum journal…"); compact: true; kind: "ghost"; onClicked: vacuum.open() }
            }
        }
    }
    FileDialog {
        id: saveDlg
        fileMode: FileDialog.SaveFile
        defaultSuffix: "log"
        onAccepted: if (!root.desktop.saveText(selectedFile, root.plainText())) root.showNotice(qsTr("Could not write the file"))
    }
    SDialog {
        id: vacuum
        title: qsTr("Vacuum journal")
        tone: Theme.red
        dialogWidth: 460
        SLabel { Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone; text: qsTr("Old journal files will be deleted until the journal is smaller than the limit. This cannot be undone.") }
        STextField { id: vacSize; objectName: "vacSize"; Layout.fillWidth: true; label: qsTr("Keep at most, MB"); text: "200"; validator: IntValidator { bottom: 1 } }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: vacuum.close() },
                   SButton { objectName: "vacuumOk"; text: qsTr("Vacuum"); kind: "danger"; enabled: parseInt(vacSize.text) > 0
                             onClicked: { const mb = parseInt(vacSize.text); vacuum.close()
                                          root.rpc("log.vacuum", { max_bytes: mb * 1048576, confirm: true }, () => root.showNotice(qsTr("Journal vacuumed"), Theme.green)) } } ]
    }
}
