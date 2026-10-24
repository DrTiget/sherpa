import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// «Планировщик»: cron пользователя, системный cron (/etc/cron.d) и systemd-таймеры (cron.*).
UtilPane {
    id: root
    property string tab: "user"
    property string cronUser: ""
    property var table: ({ version: "", items: [] })
    property var files: []
    property var timers: []
    property bool loading: false
    property var sel: null            // выбранная строка задачи (с _file/_version)
    property bool unavailable: !!desktop.capabilities.utilities && desktop.capabilities.utilities.cron === false
    readonly property string me: desktop.capabilities.user ? desktop.capabilities.user.name : ""
    readonly property var jobs: {
        if (tab === "user") return (table.items || []).filter(i => i.kind === "job").map(i => Object.assign({ _version: table.version }, i))
        const out = []
        for (const f of files) {
            const name = (f.path || "").split("/").pop()
            for (const i of (f.items || [])) if (i.kind === "job") out.push(Object.assign({ _file: name, _version: f.version }, i))
        }
        return out
    }

    // ---- человекочитаемое расписание
    function describe(expr) {
        const al = { "@yearly": qsTr("every year"), "@annually": qsTr("every year"), "@monthly": qsTr("every month"), "@weekly": qsTr("every week"),
                     "@daily": qsTr("every day at 00:00"), "@midnight": qsTr("every day at 00:00"), "@hourly": qsTr("every hour"), "@reboot": qsTr("at boot") }
        if (!expr) return ""
        if (al[expr]) return al[expr]
        const f = expr.trim().split(/\s+/)
        if (f.length !== 5) return expr
        const [mi, h, dom, mo, dow] = f
        const two = (x) => x.length < 2 ? "0" + x : x
        const num = (x) => /^\d+$/.test(x)
        if (mi === "*" && h === "*" && dom === "*" && mo === "*" && dow === "*") return qsTr("every minute")
        let m = /^\*\/(\d+)$/.exec(mi)
        if (m && h === "*" && dom === "*" && mo === "*" && dow === "*") return qsTr("every %1 min").arg(m[1])
        if (num(mi) && h === "*" && dom === "*" && mo === "*" && dow === "*") return qsTr("every hour at :%1").arg(two(mi))
        m = /^\*\/(\d+)$/.exec(h)
        if (num(mi) && m && dom === "*" && mo === "*" && dow === "*") return qsTr("every %1 h at :%2").arg(m[1]).arg(two(mi))
        if (num(mi) && num(h) && mo === "*") {
            const t = two(h) + ":" + two(mi)
            if (dom === "*" && dow === "*") return qsTr("every day at %1").arg(t)
            if (dom === "*" && num(dow)) return qsTr("every %1 at %2").arg([qsTr("Sun"), qsTr("Mon"), qsTr("Tue"), qsTr("Wed"), qsTr("Thu"), qsTr("Fri"), qsTr("Sat"), qsTr("Sun")][Number(dow)]).arg(t)
            if (dow === "*" && num(dom)) return qsTr("on day %1 of the month at %2").arg(dom).arg(t)
            if (dom === "*" && dow === "1-5") return qsTr("weekdays at %1").arg(t)
        }
        return expr
    }
    // ---- ближайшие запуски
    function parseField(s, lo, hi) {
        const set = {}
        for (const part of s.split(",")) {
            const m = /^(\*|\d+(?:-\d+)?)(?:\/(\d+))?$/.exec(part)
            if (!m) return null
            let a = lo, b = hi
            if (m[1] !== "*") { const r = m[1].split("-"); a = Number(r[0]); b = r.length > 1 ? Number(r[1]) : (m[2] ? hi : a) }
            const step = m[2] ? Number(m[2]) : 1
            if (a < lo || b > hi || a > b || step < 1) return null
            for (let v = a; v <= b; v += step) set[v] = true
        }
        return set
    }
    function nextRuns(expr, n) {
        const f = (expr || "").trim().split(/\s+/)
        if (f.length !== 5) return []
        const mi = parseField(f[0], 0, 59), h = parseField(f[1], 0, 23), dom = parseField(f[2], 1, 31), mo = parseField(f[3], 1, 12), dw = parseField(f[4].replace(/7/g, "0"), 0, 6)
        if (!mi || !h || !dom || !mo || !dw) return []
        const domStar = f[2] === "*", dowStar = f[4] === "*"
        const out = []
        const d = new Date(); d.setSeconds(0, 0)
        for (let day = 0; day < 800 && out.length < n; ++day) {
            const c = new Date(d.getFullYear(), d.getMonth(), d.getDate() + day)
            if (!mo[c.getMonth() + 1]) continue
            const okDom = dom[c.getDate()], okDow = dw[c.getDay()]
            const ok = domStar || dowStar ? (okDom && okDow) : (okDom || okDow)
            if (!ok) continue
            for (let hh = 0; hh < 24 && out.length < n; ++hh) {
                if (!h[hh]) continue
                for (let mm = 0; mm < 60 && out.length < n; ++mm) {
                    if (!mi[mm]) continue
                    const t = new Date(c.getFullYear(), c.getMonth(), c.getDate(), hh, mm)
                    if (t > d) out.push(t)
                }
            }
        }
        return out
    }
    function validExpr(expr) {
        const t = (expr || "").trim()
        if (/^@(reboot|yearly|annually|monthly|weekly|daily|midnight|hourly)$/.test(t)) return true
        return nextRuns(t, 1).length > 0 || /^\S+\s+\S+\s+\S+\s+\S+\s+\S+$/.test(t) && parseField(t.split(/\s+/)[0], 0, 59) !== null
    }

    function target(row) {
        if (tab === "system") return { file: row ? row._file : editor.file }
        return { user: cronUser !== "" ? cronUser : me }
    }
    function refresh() {
        if (!working || unavailable) return
        loading = true
        const fail = (t) => { loading = false; showNotice(t) }
        if (tab === "user") {
            const p = cronUser !== "" && cronUser !== me ? { user: cronUser, sudo: true } : {}
            rpc("cron.list", p, (d) => { loading = false; table = d }, fail)
        } else if (tab === "system") {
            rpc("cron.list", { system: true, sudo: true }, (d) => { loading = false; files = d.files || [] }, fail)
        } else {
            rpc("cron.timer.list", {}, (d) => { loading = false; timers = d.items || [] }, fail)
        }
    }
    onTabChanged: { sel = null; refresh() }
    onWorkingChanged: if (working) refresh()
    Component.onCompleted: refresh()

    function mutate(method, extra, okMsg) {
        const row = extra._row
        const p = Object.assign({}, target(row), { version: row ? row._version : table.version }, extra)
        delete p._row
        rpc(method, p, () => { showNotice(okMsg, Theme.green); sel = null; refresh() },
            (t, code) => { showNotice(t); if (code === "cron.changed") refresh() })
    }
    function toggle(row) { mutate("cron.toggle", { _row: row, line: row.line, enabled: !row.enabled }, row.enabled ? qsTr("Job disabled") : qsTr("Job enabled")) }
    function removeJob(row) {
        confirmDel.row = row; confirmDel.open()
    }

    SEmptyState {
        anchors.fill: parent; visible: root.unavailable || !root.working
        glyph: "◷"; title: root.working ? qsTr("Not available on this server") : qsTr("Waiting for the server…")
        description: root.working ? qsTr("cron was not found on this server.") : ""
    }
    ColumnLayout {
        anchors.fill: parent; anchors.margins: 12; spacing: 10
        visible: root.working && !root.unavailable
        RowLayout {
            Layout.fillWidth: true; spacing: 8
            SSegmented {
                current: root.tab
                model: [{ value: "user", text: qsTr("User cron") }, { value: "system", text: qsTr("System cron") }, { value: "timers", text: qsTr("systemd timers") }]
                onActivated: (v) => root.tab = v
            }
            RowLayout {
                visible: root.tab === "user"; spacing: 6
                SLabel { sub: true; text: qsTr("User") }
                STextField { id: userField; objectName: "cronUser"; Layout.preferredWidth: 130; placeholderText: root.me; mono: true; onAccepted: { root.cronUser = text; root.refresh() } Accessible.name: qsTr("Crontab user") }
            }
            Item { Layout.fillWidth: true }
            SSpinner { visible: root.loading; width: 18; height: 18 }
            SButton { text: qsTr("Refresh"); compact: true; onClicked: root.refresh() }
            SButton { objectName: "cronAdd"; visible: root.tab !== "timers"; text: qsTr("New job"); kind: "primary"; compact: true; onClicked: editor.openNew() }
        }
        StackLayout {
            Layout.fillWidth: true; Layout.fillHeight: true
            currentIndex: root.tab === "timers" ? 1 : 0
            RowLayout {
                spacing: 10
                UTable {
                    id: jobTable; objectName: "cronTable"
                    Layout.fillWidth: true; Layout.fillHeight: true
                    rows: root.jobs
                    emptyText: root.loading ? "" : qsTr("No jobs")
                    onRowClicked: (i, r) => root.sel = r
                    onRowDoubleClicked: (i, r) => editor.openEdit(r)
                    columns: [
                        { key: "enabled", title: qsTr("State"), w: 90, kind: "dot", fmt: (r) => r.enabled ? qsTr("active") : qsTr("disabled"), color: (r) => r.enabled ? Theme.green : Theme.gray },
                        { key: "schedule", title: qsTr("Schedule"), w: 130, mono: true },
                        { key: "desc", title: qsTr("Meaning"), w: 190, fmt: (r) => root.describe(r.schedule), color: (r) => Theme.textSub },
                        { key: "command", title: qsTr("Command"), w: 0, mono: true },
                        { key: "user", title: qsTr("User"), w: root.tab === "system" ? 80 : 1, fmt: (r) => root.tab === "system" ? (r.user || "") : "" },
                        { key: "_file", title: qsTr("File"), w: root.tab === "system" ? 110 : 1, fmt: (r) => root.tab === "system" ? (r._file || "") : "" }
                    ]
                }
                Rectangle {
                    visible: root.sel !== null
                    Layout.preferredWidth: 230; Layout.fillHeight: true; radius: 8; color: Theme.card; border.color: Theme.line
                    ColumnLayout {
                        anchors.fill: parent; anchors.margins: 10; spacing: 8
                        SLabel { caps: true; text: qsTr("Schedule") }
                        SLabel { font.bold: true; mono: true; text: root.sel ? root.sel.schedule : "" }
                        SLabel { Layout.fillWidth: true; sub: true; wrapMode: Text.Wrap; elide: Text.ElideNone; text: root.sel ? root.describe(root.sel.schedule) : "" }
                        SLabel { caps: true; text: qsTr("Next runs") }
                        Repeater { model: root.sel && root.sel.enabled ? root.nextRuns(root.sel.schedule, 4) : []
                                   delegate: SLabel { required property var modelData; mono: true; font.pixelSize: 11; text: Qt.formatDateTime(modelData, "dd.MM.yyyy HH:mm") } }
                        Item { Layout.fillHeight: true }
                        SButton { Layout.fillWidth: true; text: qsTr("Edit…"); onClicked: editor.openEdit(root.sel) }
                        SButton { objectName: "cronToggle"; Layout.fillWidth: true; text: root.sel && root.sel.enabled ? qsTr("Disable") : qsTr("Enable"); onClicked: root.toggle(root.sel) }
                        SButton { objectName: "cronDelete"; Layout.fillWidth: true; text: qsTr("Delete…"); kind: "danger"; onClicked: root.removeJob(root.sel) }
                    }
                }
            }
            UTable {
                objectName: "timerTable"
                rows: root.timers
                emptyText: root.loading ? "" : qsTr("No timers")
                onRowClicked: (i, r) => timerMenu.row = r
                columns: [
                    { key: "ActiveState", title: qsTr("State"), w: 90, kind: "dot", color: (r) => r.ActiveState === "active" ? Theme.green : Theme.gray },
                    { key: "Id", title: qsTr("Timer"), w: 240, mono: true },
                    { key: "UnitFileState", title: qsTr("Autostart"), w: 90, color: (r) => r.UnitFileState === "enabled" ? Theme.green : Theme.textSub },
                    { key: "NextElapseUSecRealtime", title: qsTr("Next run"), w: 210, mono: true, fmt: (r) => r.NextElapseUSecRealtime || "—" },
                    { key: "LastTriggerUSec", title: qsTr("Last run"), w: 0, mono: true, fmt: (r) => r.LastTriggerUSec || "—" }
                ]
            }
        }
        RowLayout {
            visible: root.tab === "timers" && timerMenu.row !== null
            Layout.fillWidth: true; spacing: 8
            SLabel { Layout.fillWidth: true; mono: true; text: timerMenu.row ? timerMenu.row.Id : "" }
            SButton { objectName: "timerEnable"; text: qsTr("Enable"); compact: true; onClicked: timerMenu.act("enable") }
            SButton { objectName: "timerDisable"; text: qsTr("Disable"); compact: true; kind: "danger"; onClicked: timerMenu.act("disable") }
        }
    }
    QtObject {
        id: timerMenu
        property var row: null
        function act(verb) {
            const r = row
            root.rpc("cron.timer." + verb, { unit: r.Id, confirm: true }, () => { root.showNotice(r.Id + ": " + (verb === "enable" ? qsTr("enabled") : qsTr("disabled")), Theme.green); root.refresh() })
        }
    }

    ConfirmDialog {
        id: confirmDel
        property var row: null
        title: qsTr("Delete job?")
        message: row ? qsTr("The job will be removed from the crontab:\n%1  %2").arg(row.schedule).arg(row.command) : ""
        confirmText: qsTr("Delete")
        danger: true
        onConfirmed: root.mutate("cron.del", { _row: row, line: row.line, confirm: true }, qsTr("Job deleted"))
    }

    // ---- редактор задачи
    SDialog {
        id: editor
        objectName: "cronEditor"
        title: line > 0 ? qsTr("Edit job") : qsTr("New job")
        dialogWidth: 580
        property int line: 0
        property var row: null
        property string file: ""
        property string err: ""
        function openNew() { line = 0; row = null; file = ""; fFile.text = ""; fRun.text = "root"; fExpr.text = "0 3 * * *"; fCmd.text = ""; err = ""; syncFields(); open(); fCmd.forceFocus() }
        function openEdit(r) {
            line = r.line; row = r; file = r._file || ""; fFile.text = file; fRun.text = r.user || "root"; fExpr.text = r.schedule; fCmd.text = r.command; err = ""; syncFields(); open()
        }
        function syncFields() {
            const f = fExpr.text.trim().split(/\s+/)
            if (f.length === 5) { fMin.text = f[0]; fHour.text = f[1]; fDom.text = f[2]; fMon.text = f[3]; fDow.text = f[4] }
        }
        function fromFields() { fExpr.text = [fMin.text, fHour.text, fDom.text, fMon.text, fDow.text].map(x => x === "" ? "*" : x).join(" ") }
        readonly property bool exprOk: root.validExpr(fExpr.text)
        function submit() {
            if (fCmd.text.trim() === "") { err = qsTr("Enter a command"); return }
            if (!exprOk) { err = qsTr("Invalid schedule"); return }
            if (root.tab === "system" && !/^[A-Za-z0-9_-]+$/.test(fFile.text)) { err = qsTr("File name: letters, digits, _ and - only"); return }
            const base = Object.assign({}, root.tab === "system" ? { file: fFile.text, run_as: fRun.text } : { user: root.cronUser !== "" ? root.cronUser : root.me },
                                       { schedule: fExpr.text.trim(), command: fCmd.text.trim(), confirm: true })
            if (line > 0) base.line = line
            base.version = row ? row._version : (root.tab === "user" ? root.table.version : (root.files.find(f => (f.path || "").split("/").pop() === fFile.text) || {}).version || "")
            root.rpc(line > 0 ? "cron.set" : "cron.add", base, () => { close(); root.showNotice(qsTr("Job saved"), Theme.green); root.sel = null; root.refresh() },
                     (t, code) => { err = t; if (code === "cron.changed") root.refresh() })
        }
        // конструктор
        GridLayout {
            Layout.fillWidth: true; columns: 5; columnSpacing: 8
            SLabel { caps: true; text: qsTr("Minute") } SLabel { caps: true; text: qsTr("Hour") } SLabel { caps: true; text: qsTr("Day") } SLabel { caps: true; text: qsTr("Month") } SLabel { caps: true; text: qsTr("Weekday") }
            STextField { id: fMin; Layout.fillWidth: true; Layout.preferredWidth: 1; mono: true; onEdited: editor.fromFields() }
            STextField { id: fHour; Layout.fillWidth: true; Layout.preferredWidth: 1; mono: true; onEdited: editor.fromFields() }
            STextField { id: fDom; Layout.fillWidth: true; Layout.preferredWidth: 1; mono: true; onEdited: editor.fromFields() }
            STextField { id: fMon; Layout.fillWidth: true; Layout.preferredWidth: 1; mono: true; onEdited: editor.fromFields() }
            STextField { id: fDow; Layout.fillWidth: true; Layout.preferredWidth: 1; mono: true; onEdited: editor.fromFields() }
        }
        Flow {
            Layout.fillWidth: true; spacing: 6
            Repeater {
                model: [{ t: qsTr("every minute"), e: "* * * * *" }, { t: qsTr("every 5 min"), e: "*/5 * * * *" }, { t: qsTr("hourly"), e: "0 * * * *" },
                        { t: qsTr("daily 03:00"), e: "0 3 * * *" }, { t: qsTr("weekly"), e: "0 3 * * 1" }, { t: qsTr("monthly"), e: "0 3 1 * *" }, { t: qsTr("at boot"), e: "@reboot" }]
                delegate: SChip { required property var modelData; text: modelData.t; mono: false; active: fExpr.text === modelData.e; onClicked: { fExpr.text = modelData.e; editor.syncFields() } }
            }
        }
        STextField { id: fExpr; objectName: "cronExpr"; Layout.fillWidth: true; label: qsTr("Cron expression"); mono: true; onEdited: editor.syncFields()
                     error: !editor.exprOk ? qsTr("Invalid schedule") : ""; hint: editor.exprOk && root.describe(text) !== text.trim() ? root.describe(text) : "" }
        SLabel {
            Layout.fillWidth: true; sub: true; font.pixelSize: 11; elide: Text.ElideNone; wrapMode: Text.Wrap
            visible: editor.exprOk && fExpr.text.charAt(0) !== "@"
            text: qsTr("Next runs:") + " " + root.nextRuns(fExpr.text, 5).map(d => Qt.formatDateTime(d, "dd.MM HH:mm")).join(", ")
        }
        SLabel { visible: fMin.text === "*" && editor.exprOk && fExpr.text.charAt(0) !== "@"; Layout.fillWidth: true; color: Theme.amber; wrapMode: Text.Wrap; elide: Text.ElideNone
                 text: qsTr("The job runs every minute — make sure this is intended.") }
        STextField { id: fCmd; objectName: "cronCmd"; Layout.fillWidth: true; label: qsTr("Command"); mono: true; onAccepted: editor.submit() }
        RowLayout {
            visible: root.tab === "system"; Layout.fillWidth: true; spacing: 8
            STextField { id: fFile; Layout.fillWidth: true; label: qsTr("File in /etc/cron.d"); mono: true; readOnly: editor.line > 0 }
            STextField { id: fRun; Layout.fillWidth: true; label: qsTr("Run as"); mono: true }
        }
        SLabel { visible: editor.err !== ""; Layout.fillWidth: true; color: Theme.red; wrapMode: Text.Wrap; elide: Text.ElideNone; text: editor.err }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: editor.close() },
                   SButton { objectName: "cronSave"; text: qsTr("Save"); kind: "primary"; onClicked: editor.submit() } ]
    }
}
