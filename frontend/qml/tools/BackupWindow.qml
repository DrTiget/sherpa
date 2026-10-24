import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Бэкапы: задания restic/rsync, запуск, снимки, восстановление (пробный прогон по умолчанию), проверка, prune.
ToolPane {
    id: root
    property var jobs: []
    property bool allServers: false
    property bool loading: false
    property var jobSel: null                   // выбранное задание
    property var snaps: []
    property bool snapsLoading: false
    property string note: ""
    property string editId: ""
    property alias rTarget: rTarget
    property alias rDry: rDry

    function fail(e) { errorText = e; loading = false; snapsLoading = false }
    function refresh() {
        loading = true
        const p = allServers ? {} : { connection_id: serverId }
        call("backup.jobs.list", p, d => {
            jobs = d.items || []
            loading = false
            if (jobSel) jobSel = jobs.find(j => j.id === jobSel.id) || null
        }, fail)
    }
    function loadSnaps() {
        snaps = []
        if (!jobSel || jobSel.engine !== "restic") return
        snapsLoading = true
        call("backup.snapshots", { id: jobSel.id }, d => { snaps = (d.items || []).slice().reverse(); snapsLoading = false }, fail)
    }
    function srvName(id) { const s = Backend.serverById(id); return s && s.name ? s.name : id }
    function select(j) { jobSel = j; note = ""; loadSnaps() }

    function openEdit(j) {
        editId = j ? j.id : ""
        fTitle.text = j ? j.title : ""
        fEngine.current = j ? j.engine : "restic"
        fSource.text = j ? j.source : ""
        fRepo.text = j ? j.repository : ""
        fPass.text = j && j.password_file ? j.password_file : ""
        fSched.text = j && j.schedule ? j.schedule : ""
        fPriv.checked = j ? !!j.privileged : false
        fErr.text = ""
        editDlg.open()
    }
    function saveJob() {
        if (fTitle.text.trim() === "" || fSource.text.trim() === "" || fRepo.text.trim() === "") { fErr.text = qsTr("Title, source and repository are required"); return }
        if (fSource.text.trim()[0] !== "/") { fErr.text = qsTr("Source must be an absolute path"); return }
        if (fEngine.current === "restic" && fPass.text.trim() === "") { fErr.text = qsTr("restic needs a password file on the server"); return }
        const p = { connection_id: serverId, title: fTitle.text.trim(), engine: fEngine.current, source: fSource.text.trim(), repository: fRepo.text.trim(), privileged: fPriv.checked }
        if (editId !== "") p.id = editId
        if (fPass.text.trim() !== "") p.password_file = fPass.text.trim()
        if (fSched.text.trim() !== "") p.schedule = fSched.text.trim()
        call("backup.jobs.set", p, () => { editDlg.close(); refresh() }, e => { fErr.text = e })
    }
    function runJob(j) {
        note = qsTr("Backup is running…")
        call("backup.run", { id: j.id }, () => { note = qsTr("Backup finished"); refresh() }, e => { note = ""; errorText = e; refresh() })
    }
    function checkJob(j) {
        note = qsTr("Checking the repository…")
        call("backup.check", { id: j.id }, () => { note = qsTr("Repository check passed") }, e => { note = ""; errorText = e })
    }
    function pruneJob(j) {
        dangerous({ title: qsTr("Prune repository"), message: qsTr("Old snapshots are removed according to the retention policy and cannot be recovered."),
                    items: [j.title + "  →  " + j.repository], confirmText: qsTr("Prune"), needName: prod }, "backup.prune", { id: j.id }, () => { note = qsTr("Prune finished"); loadSnaps() })
    }
    function delJob(j) {
        dangerous({ title: qsTr("Delete backup job"), message: qsTr("Only the job is deleted; the backups in the repository are kept."),
                    items: [j.title], confirmText: qsTr("Delete"), needName: false }, "backup.jobs.del", { id: j.id }, () => { jobSel = null; refresh() })
    }
    function openRestore(s) {
        rSnap.text = s ? s.short_id || s.id : ""
        rTarget.text = ""
        rDry.checked = true
        rErr.text = ""
        restoreDlg.open()
    }
    function doRestore() {
        const t = rTarget.text.trim()
        if (rSnap.text.trim() === "" || t === "" || t[0] !== "/") { rErr.text = qsTr("Snapshot and an absolute target path are required"); return }
        const p = { id: jobSel.id, snapshot: rSnap.text.trim(), target: t, dry_run: rDry.checked }
        restoreDlg.close()
        if (rDry.checked) {
            note = qsTr("Dry run…")
            call("backup.restore", p, () => { note = qsTr("Dry run finished: nothing was written") }, e => { note = ""; errorText = e })
        } else {
            dangerous({ title: qsTr("Restore from backup"), message: qsTr("Files in the target directory can be overwritten. Make sure the target is correct."),
                        items: [jobSel.title + ": " + p.snapshot + " → " + t], confirmText: qsTr("Restore"), needName: true, hostname: true }, "backup.restore", p,
                      () => { note = qsTr("Restore finished") })
        }
    }

    onAllServersChanged: refresh()
    Component.onCompleted: refresh()

    RowLayout {
        Layout.fillWidth: true; Layout.margins: 10; Layout.bottomMargin: 6; spacing: 10
        SSegmented { model: [{ value: "this", text: qsTr("This server") }, { value: "all", text: qsTr("All servers") }]; current: root.allServers ? "all" : "this"
                     onActivated: (v) => root.allServers = v === "all" }
        SLabel { Layout.fillWidth: true; sub: true; text: root.note }
        SButton { compact: true; kind: "primary"; text: qsTr("New job…"); enabled: !root.readonlyMode; onClicked: root.openEdit(null) }
        SButton { compact: true; text: qsTr("Refresh"); onClicked: root.refresh() }
    }
    ColumnLayout {
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 10; Layout.topMargin: 0
        spacing: 8
        DataTable {
            objectName: "jobTable"
            Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredHeight: 3
            loading: root.loading; rows: root.jobs
            current: -1
            actions: [
                { id: "run", text: qsTr("Run"), kind: "primary", enabled: r => !root.readonlyMode },
                { id: "check", text: qsTr("Check"), visible: r => r.engine === "restic" },
                { id: "prune", text: qsTr("Prune…"), visible: r => r.engine === "restic", enabled: r => !root.readonlyMode },
                { id: "edit", text: qsTr("Edit") },
                { id: "del", text: qsTr("Delete…"), kind: "danger", enabled: r => !root.readonlyMode }
            ]
            emptyText: qsTr("No backup jobs yet")
            columns: [
                { key: "title", title: qsTr("Job"), flex: 1, },
                { key: "srv", title: qsTr("Server"), w: 110, visible: root.allServers, text: r => root.srvName(r.connection_id) },
                { key: "engine", title: qsTr("Engine"), w: 70, chip: true },
                { key: "source", title: qsTr("Source"), flex: 1, mono: true },
                { key: "repository", title: qsTr("Repository"), flex: 1.2, mono: true },
                { key: "schedule", title: qsTr("Schedule"), w: 100, mono: true, text: r => r.schedule || "—" },
                { key: "last_run_at", title: qsTr("Last run"), w: 130, text: r => r.last_run_at ? root.fmtTime(r.last_run_at) : "—",
                  tone: r => r.last_run_ok === false ? Theme.red : (r.last_run_ok === true ? Theme.green : Theme.textSub) }
            ].filter(c => c.visible !== false)
            onRowClicked: (i, r) => root.select(r)
            onAction: (id, r) => {
                if (id === "run") root.runJob(r); else if (id === "check") root.checkJob(r); else if (id === "prune") root.pruneJob(r)
                else if (id === "edit") root.openEdit(r); else if (id === "del") root.delJob(r)
            }
        }
        Rectangle {
            visible: root.jobSel !== null
            Layout.fillWidth: true; Layout.preferredHeight: 190; radius: 8; color: Theme.card; border.color: Theme.line
            ColumnLayout {
                anchors.fill: parent; anchors.margins: 8; spacing: 6
                RowLayout {
                    SLabel { caps: true; Layout.fillWidth: true; text: qsTr("Snapshots") + (root.jobSel ? " · " + root.jobSel.title : "") }
                    SButton { compact: true; text: qsTr("Restore…"); visible: root.jobSel && root.jobSel.engine === "restic"; enabled: !root.readonlyMode
                              onClicked: root.openRestore(snapTable.selectedRow()) }
                }
                SLabel { visible: root.jobSel && root.jobSel.engine !== "restic"; sub: true; text: qsTr("rsync jobs have no snapshots: only “Run” is supported.") }
                DataTable {
                    id: snapTable
                    objectName: "snapTable"
                    visible: root.jobSel && root.jobSel.engine === "restic"
                    Layout.fillWidth: true; Layout.fillHeight: true
                    loading: root.snapsLoading; rows: root.snaps; compactRows: true
                    emptyText: qsTr("No snapshots")
                    columns: [
                        { key: "short_id", title: "ID", w: 100, mono: true },
                        { key: "time", title: qsTr("Time"), w: 150, text: r => root.fmtTime(r.time) },
                        { key: "hostname", title: qsTr("Host"), w: 120, mono: true },
                        { key: "paths", title: qsTr("Paths"), flex: 1, mono: true, text: r => (r.paths || []).join(", ") }
                    ]
                    onRowDoubleClicked: (i, r) => root.openRestore(r)
                }
            }
        }
    }

    SDialog {
        id: editDlg
        title: root.editId === "" ? qsTr("New backup job") : qsTr("Edit backup job")
        dialogWidth: 560
        onOpened: fTitle.forceFocus()
        STextField { id: fTitle; Layout.fillWidth: true; label: qsTr("Title") }
        SSegmented { id: fEngine; model: [{ value: "restic", text: "restic" }, { value: "rsync", text: "rsync" }]; current: "restic" }
        STextField { id: fSource; Layout.fillWidth: true; label: qsTr("Source (absolute path on the server)"); mono: true }
        STextField { id: fRepo; Layout.fillWidth: true; label: qsTr("Repository / destination"); mono: true; placeholderText: "/mnt/backup/repo  ·  sftp:user@host:/repo  ·  s3:…" }
        STextField { id: fPass; visible: fEngine.current === "restic"; Layout.fillWidth: true; label: qsTr("Password file on the server"); mono: true; placeholderText: "/root/.restic-pass" }
        STextField { id: fSched; Layout.fillWidth: true; label: qsTr("Schedule (description only)"); mono: true; placeholderText: "0 3 * * *" }
        SCheckBox { id: fPriv; text: qsTr("Run with root rights (sudo)") }
        SLabel { id: fErr; Layout.fillWidth: true; color: Theme.red; wrapMode: Text.Wrap; elide: Text.ElideNone; visible: text !== "" }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: editDlg.close() }, SButton { text: qsTr("Save"); kind: "primary"; onClicked: root.saveJob() } ]
    }
    SDialog {
        id: restoreDlg
        title: qsTr("Restore from backup")
        dialogWidth: 500
        STextField { id: rSnap; Layout.fillWidth: true; label: qsTr("Snapshot"); mono: true }
        STextField { id: rTarget; Layout.fillWidth: true; label: qsTr("Target directory on the server"); mono: true; placeholderText: "/tmp/restore" }
        SCheckBox { id: rDry; text: qsTr("Dry run (do not write anything)"); checked: true }
        SLabel { visible: !rDry.checked; Layout.fillWidth: true; color: Theme.red; wrapMode: Text.Wrap; elide: Text.ElideNone; text: qsTr("A real restore can overwrite existing files and needs confirmation.") }
        SLabel { id: rErr; Layout.fillWidth: true; color: Theme.red; wrapMode: Text.Wrap; elide: Text.ElideNone; visible: text !== "" }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: restoreDlg.close() },
                   SButton { text: rDry.checked ? qsTr("Run dry run") : qsTr("Restore…"); kind: rDry.checked ? "primary" : "danger"; onClicked: root.doRestore() } ]
    }
}
