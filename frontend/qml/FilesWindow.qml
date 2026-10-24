import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import QtQuick.Layouts
import Sherpa

// Файловый менеджер (SFTP): дерево, список, права, поиск, редактор, передачи с очередью.
Item {
    id: root
    required property var desktop
    signal openApp(string id, var args)

    // ---------------------------------------------------------------- состояние
    property string cwd: "/"
    property var entries: []                 // записи текущего каталога или результатов поиска
    property var shown: []                   // после фильтра и сортировки
    property var selection: ({})             // path -> true
    property string anchorPath: ""
    property bool loading: false
    property bool truncated: false
    property string errorText: ""
    property bool showHidden: false
    property string sortKey: "name"
    property bool sortDesc: false
    property bool searchMode: false
    property string searchQuery: ""
    property var history: []
    property int histIndex: -1
    property bool showProps: true
    property bool editingPath: false
    property int navSeq: 0                   // устаревшие ответы list/search игнорируются
    property var pendingJobs: []             // очередь передач, ожидающих решения о конфликте
    property var meta: ({})                  // id передачи -> {direction, name, local, remote, total, last, lastBytes}
    readonly property bool ro: desktop.readOnlyMode
    readonly property string home: {
        const u = desktop.capabilities.user
        if (!u || !u.name) return "/"
        return u.name === "root" ? "/root" : "/home/" + u.name
    }
    readonly property var selected: shown.filter(e => selection[e.path] === true)
    property alias dangerDialog: danger
    property alias conflictDialog: conflict
    property alias permsDialog: perms
    property alias nameDialog: nameDlg
    property alias transfersModel: tmodel
    property alias transfersPanel: transfers

    // ---------------------------------------------------------------- утилиты
    function call(method, params, ok, fail) {
        desktop.rpc(method, params, function (good, data, code, text, eparams) {
            if (good) { if (ok) ok(data) }
            else if (fail) fail(code, text, eparams)
            else root.errorText = text
        })
    }
    // Опасный вызов: danger.prepare → тот же метод с confirm_token (после подтверждения в диалоге)
    function dangerCall(method, params, ok, fail) {
        const full = Object.assign({ connection_id: desktop.connectionId }, params)
        desktop.rpcRaw("danger.prepare", { method: method, params: full }, function (good, data, code, text) {
            if (!good) { (fail || ((c, t) => { root.errorText = t }))(code, text); return }
            desktop.rpcRaw(method, Object.assign({}, full, { confirm_token: data.confirm_token }), function (g2, d2, c2, t2) {
                if (g2) { if (ok) ok(d2) } else (fail || ((c, t) => { root.errorText = t }))(c2, t2)
            })
        })
    }
    function joinPath(dir, name) { return (dir === "/" ? "" : dir) + "/" + name }
    function parentOf(p) { const i = p.lastIndexOf("/"); return i <= 0 ? "/" : p.substring(0, i) }
    function fmtSize(b) {
        if (b === undefined || b === null) return "—"
        const u = ["B", "KB", "MB", "GB", "TB"]
        let i = 0, v = b
        while (v >= 1024 && i < 4) { v /= 1024; i++ }
        return v.toFixed(i > 0 && v < 10 ? 1 : 0) + " " + u[i]
    }
    function fmtTime(s) { return s === null || s === undefined ? "—" : Qt.formatDateTime(new Date(s * 1000), "yyyy-MM-dd HH:mm") }
    function fmtPerm(mode) {
        if (mode === null || mode === undefined) return "—"
        const m = mode & 4095
        const ch = ["r", "w", "x"]
        let out = ""
        for (let i = 8; i >= 0; --i) out += (m >> i) & 1 ? ch[(8 - i) % 3] : "-"
        if (m & 2048) out = out.substring(0, 2) + ((m & 64) ? "s" : "S") + out.substring(3)
        if (m & 1024) out = out.substring(0, 5) + ((m & 8) ? "s" : "S") + out.substring(6)
        if (m & 512) out = out.substring(0, 8) + ((m & 1) ? "t" : "T")
        return out
    }
    function isSystemPath(p) {
        if (p === "/") return true
        for (const r of ["/etc", "/boot", "/usr", "/bin", "/sbin", "/lib", "/lib64", "/var", "/root", "/sys", "/proc", "/dev", "/opt", "/srv"])
            if (p === r || p.indexOf(r + "/") === 0) return true
        return false
    }
    function glyphOf(e) { return e.type === "directory" ? "▸" : (e.type === "symlink" ? "↪" : "·") }

    function applyView() {
        let l = entries.slice()
        if (!showHidden && !searchMode) l = l.filter(e => e.name.charAt(0) !== ".")
        const k = sortKey, d = sortDesc ? -1 : 1
        l.sort((a, b) => {
            const da = a.type === "directory" ? 0 : 1, db = b.type === "directory" ? 0 : 1
            if (da !== db) return da - db
            let r
            if (k === "name") r = a.name.toLowerCase().localeCompare(b.name.toLowerCase())
            else r = ((a[k] || 0) - (b[k] || 0)) || a.name.localeCompare(b.name)
            return r * d
        })
        shown = l
        // оставляем в выделении только то, что есть в списке
        const sel = {}
        for (const e of l) if (selection[e.path]) sel[e.path] = true
        selection = sel
    }
    onShowHiddenChanged: applyView()
    onSortKeyChanged: applyView()
    onSortDescChanged: applyView()

    // ---------------------------------------------------------------- навигация
    function navigate(path, push) {
        if (push === undefined) push = true
        loading = true
        errorText = ""
        const seq = ++navSeq
        call("files.list", { path: path, limit: 5000 }, function (data) {
            if (seq !== navSeq) return
            loading = false
            searchMode = false
            searchQuery = ""
            searchField.text = ""
            cwd = path
            entries = data.items || []
            truncated = !!data.truncated
            selection = ({})
            anchorPath = ""
            applyView()
            if (push) {
                history = history.slice(0, histIndex + 1).concat([path])
                histIndex = history.length - 1
            }
            revealInTree(path)
        }, function (code, text) {
            if (seq !== navSeq) return
            loading = false
            errorText = text
        })
    }
    function refresh() { searchMode ? runSearch(searchQuery) : navigate(cwd, false) }
    function goBack() { if (histIndex > 0) { histIndex--; navigate(history[histIndex], false) } }
    function goForward() { if (histIndex < history.length - 1) { histIndex++; navigate(history[histIndex], false) } }
    function goUp() { if (cwd !== "/") navigate(parentOf(cwd)) }

    function runSearch(q) {
        q = q.trim()
        if (q === "") { if (searchMode) navigate(cwd, false); return }
        loading = true
        errorText = ""
        const seq = ++navSeq
        call("files.search", { path: cwd, query: q, limit: 2000 }, function (data) {
            if (seq !== navSeq) return
            loading = false
            searchMode = true
            searchQuery = q
            entries = data.items || []
            truncated = !!data.truncated
            selection = ({})
            applyView()
        }, function (code, text) { if (seq !== navSeq) return; loading = false; errorText = text })
    }

    function openEntry(e) {
        if (e.type === "directory") { navigate(e.path); return }
        if (e.type === "symlink") {
            // симлинк: пробуем как каталог, иначе как файл
            call("files.list", { path: e.path, limit: 1 }, function () { navigate(e.path) }, function () { openEditor(e) })
            return
        }
        openEditor(e)
    }
    function openEditor(e) { openApp("editor", { path: e.path, desktop: desktop }) }

    // ---------------------------------------------------------------- выделение
    function selectOnly(path) { const s = {}; s[path] = true; selection = s; anchorPath = path }
    function selectClick(e, mods) {
        if (mods & Qt.ShiftModifier && anchorPath !== "") {
            const a = shown.findIndex(x => x.path === anchorPath), b = shown.findIndex(x => x.path === e.path)
            const s = {}
            for (let i = Math.min(a, b); i <= Math.max(a, b); ++i) s[shown[i].path] = true
            selection = s
        } else if (mods & Qt.ControlModifier) {
            const s = Object.assign({}, selection)
            if (s[e.path]) delete s[e.path]; else s[e.path] = true
            selection = s
            anchorPath = e.path
        } else selectOnly(e.path)
    }
    function selectAll() { const s = {}; for (const e of shown) s[e.path] = true; selection = s }
    function moveSelection(delta, mods) {
        if (shown.length === 0) return
        let i = shown.findIndex(x => x.path === anchorPath)
        i = Math.max(0, Math.min(shown.length - 1, (i < 0 ? (delta > 0 ? -1 : shown.length) : i) + delta))
        selectClick(shown[i], mods & Qt.ShiftModifier ? Qt.ShiftModifier : 0)
        if (!(mods & Qt.ShiftModifier)) anchorPath = shown[i].path
        listView.positionViewAtIndex(i, ListView.Contain)
    }

    // ---------------------------------------------------------------- действия
    function mutating() {
        if (ro) { errorText = qsTr("Read-only mode is on: changes are blocked"); return false }
        return true
    }
    function newFolder() {
        if (!mutating()) return
        nameDlg.mode = "mkdir"; nameDlg.title = qsTr("New folder"); nameDlg.label = qsTr("Folder name"); nameDlg.value = ""
        nameDlg.confirmText = qsTr("Create"); nameDlg.open()
    }
    function renameSelected() {
        if (!mutating() || selected.length !== 1) return
        nameDlg.mode = "rename"; nameDlg.target = selected[0]; nameDlg.title = qsTr("Rename"); nameDlg.label = qsTr("New name")
        nameDlg.value = selected[0].name; nameDlg.confirmText = qsTr("Rename"); nameDlg.open()
    }
    function moveSelected() {
        if (!mutating() || selected.length !== 1) return
        nameDlg.mode = "move"; nameDlg.target = selected[0]; nameDlg.title = qsTr("Move to"); nameDlg.label = qsTr("New full path")
        nameDlg.value = selected[0].path; nameDlg.confirmText = qsTr("Move"); nameDlg.open()
    }
    function submitName(name) {
        if (nameDlg.mode === "mkdir") {
            if (name.indexOf("/") >= 0) { nameDlg.error = qsTr("The name must not contain “/”"); return }
            call("files.mkdir", { path: joinPath(cwd, name) }, function () { nameDlg.close(); refresh() }, function (c, t) { nameDlg.error = t })
        } else {
            const to = nameDlg.mode === "move" ? name : joinPath(parentOf(nameDlg.target.path), name)
            if (nameDlg.mode === "rename" && name.indexOf("/") >= 0) { nameDlg.error = qsTr("The name must not contain “/”"); return }
            call("files.rename", { path: nameDlg.target.path, to: to }, function () { nameDlg.close(); refresh() }, function (c, t) { nameDlg.error = t })
        }
    }
    function deleteSelected() {
        if (!mutating() || selected.length === 0) return
        const items = selected.slice()
        const dirs = items.filter(e => e.type === "directory")
        const sys = dirs.some(e => isSystemPath(e.path)) || items.some(e => e.path === "/")
        danger.ask({
            title: qsTr("Delete"),
            message: dirs.length > 0 ? qsTr("Delete %1 item(s), including folders with all their contents? This cannot be undone.").arg(items.length)
                                     : qsTr("Delete %1 file(s)? This cannot be undone.").arg(items.length),
            details: items.map(e => e.path + (e.type === "directory" ? "/" : "")),
            requireName: sys || items.length > 20,
            confirmText: qsTr("Delete"),
            onConfirm: function () { runDeletes(items, 0) }
        })
    }
    function runDeletes(items, i) {
        if (i >= items.length) { refresh(); return }
        const e = items[i]
        const next = () => runDeletes(items, i + 1)
        const fail = (c, t) => { errorText = t; refresh() }
        if (e.type === "directory") dangerCall("files.delete", { path: e.path, recursive: true }, next, fail)
        else call("files.delete", { path: e.path }, next, fail)
    }
    function permissionsOfSelected() {
        if (!mutating() || selected.length !== 1) return
        perms.openFor(selected[0])
    }
    function applyPermissions(e, mode, uid, gid, sudo) {
        const done = () => refresh()
        const fail = (c, t) => { errorText = t }
        const doOwner = () => {
            if (uid == null && gid == null) { done(); return }
            const o = { path: e.path, uid: uid == null ? e.uid : uid, gid: gid == null ? e.gid : gid, privileged: sudo }
            call("files.chown", o, done, fail)
        }
        if (mode >= 0) call("files.chmod", { path: e.path, mode: mode, privileged: sudo }, doOwner, fail)
        else doOwner()
    }
    function copyPath() { if (selected.length) { hiddenClip.text = selected.map(e => e.path).join("\n"); hiddenClip.selectAll(); hiddenClip.copy() } }

    // ---------------------------------------------------------------- дерево
    ListModel { id: tree }
    function treeIndex(path) { for (let i = 0; i < tree.count; ++i) if (tree.get(i).path === path) return i; return -1 }
    function treeLoad(i, cb) {
        const n = tree.get(i)
        if (n.loaded) { if (cb) cb(); return }
        tree.setProperty(i, "busy", true)
        call("files.list", { path: n.path, limit: 5000 }, function (data) {
            const dirs = (data.items || []).filter(e => e.type === "directory").sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()))
            const idx = treeIndex(n.path)
            if (idx < 0) return
            tree.setProperty(idx, "loaded", true)
            tree.setProperty(idx, "busy", false)
            for (let k = 0; k < dirs.length; ++k)
                tree.insert(idx + 1 + k, { path: dirs[k].path, name: dirs[k].name, depth: n.depth + 1, expanded: false, loaded: false, busy: false })
            tree.setProperty(idx, "expanded", true)
            if (cb) cb()
        }, function () { const idx = treeIndex(n.path); if (idx >= 0) { tree.setProperty(idx, "busy", false); tree.setProperty(idx, "loaded", true) } if (cb) cb() })
    }
    function treeCollapse(i) {
        const d = tree.get(i).depth
        while (i + 1 < tree.count && tree.get(i + 1).depth > d) tree.remove(i + 1)
        tree.setProperty(i, "expanded", false)
        tree.setProperty(i, "loaded", false)
    }
    function treeToggle(i) { tree.get(i).expanded ? treeCollapse(i) : treeLoad(i) }
    function revealInTree(path) {
        if (tree.count === 0) return
        const parts = path.split("/").filter(p => p !== "")
        let cur = ""
        const step = (k) => {
            const i = treeIndex(cur === "" ? "/" : cur)
            if (i < 0) return
            if (k >= parts.length) { treeView.currentIndex = i; treeView.positionViewAtIndex(i, ListView.Contain); return }
            const idx = i
            const go = () => { cur = cur + "/" + parts[k]; step(k + 1) }
            if (!tree.get(idx).expanded) treeLoad(idx, go); else go()
        }
        step(0)
    }

    // ---------------------------------------------------------------- передачи
    ListModel { id: tmodel }
    // Копия строки модели обычным объектом (для тестов и внешнего кода)
    function transferAt(i) { if (i < 0 || i >= tmodel.count) return ({}); const t = tmodel.get(i); return { tid: t.tid, direction: t.direction, name: t.name, status: t.status, bytes: t.bytes, total: t.total, speed: t.speed, error: t.error } }
    function tIndex(id) { for (let i = 0; i < tmodel.count; ++i) if (tmodel.get(i).tid === id) return i; return -1 }
    function addTransfer(id, m) {
        meta[id] = m
        tmodel.insert(0, { tid: id, direction: m.direction, name: m.name, status: "queued", bytes: 0, total: m.total || 0, speed: 0, error: "", errorCode: "" })
        transfers.expanded = true
    }
    function applyTransferEvent(id, data) {
        let i = tIndex(id)
        const m = meta[id] || {}
        if (i < 0) {
            // передача, начатая другим клиентом или до перезапуска окна
            tmodel.insert(0, { tid: id, direction: data.direction || m.direction || "download", name: m.name || data.remote || id,
                               status: data.status || "queued", bytes: data.bytes || 0, total: m.total || 0, speed: 0, error: "", errorCode: "" })
            i = 0
            if (data.remote) { meta[id] = { direction: data.direction, name: data.remote, local: data.local, remote: data.remote, total: 0 }; tmodel.setProperty(0, "name", data.remote.split("/").pop()) }
        }
        const now = Date.now()
        const prev = meta[id] || {}
        let speed = tmodel.get(i).speed
        if (data.status === "running" && data.bytes !== undefined) {
            if (prev.last) {
                const dt = (now - prev.last) / 1000
                if (dt > 0.05) {
                    const inst = (data.bytes - (prev.lastBytes || 0)) / dt
                    speed = speed > 0 ? speed * 0.6 + inst * 0.4 : inst
                    prev.last = now; prev.lastBytes = data.bytes
                }
            } else { prev.last = now; prev.lastBytes = data.bytes }
            meta[id] = prev
        }
        tmodel.setProperty(i, "status", data.status || "queued")
        if (data.bytes !== undefined) tmodel.setProperty(i, "bytes", data.bytes)
        tmodel.setProperty(i, "speed", data.status === "running" ? speed : 0)
        if (data.status === "completed") {
            if (tmodel.get(i).total === 0) tmodel.setProperty(i, "total", data.bytes || 0)
            tmodel.setProperty(i, "bytes", Math.max(tmodel.get(i).total, data.bytes || 0))
            if (prev.direction === "upload" && !searchMode && parentOf(prev.remote || "") === cwd) refresh()
        }
        if (data.status === "failed") {
            const err = data.error || {}
            tmodel.setProperty(i, "error", desktopError(err))
            tmodel.setProperty(i, "errorCode", err.code || "")
        }
    }
    function desktopError(err) { return err && err.code ? Backend.errorText(err.code, err.params || {}, err.message || "") : "" }
    Connections {
        target: root.desktop
        function onEventReceived(ev) {
            if (ev.event === "transfer" && ev.id) root.applyTransferEvent(ev.id, ev.data || {})
        }
    }
    function loadTransfers() {
        call("transfer.list", {}, function (data) {
            for (const it of (data.items || []).slice().reverse())
                if (it.connection_id === desktop.connectionId && tIndex(it.id) < 0) applyTransferEvent(it.id, it.data || {})
        })
    }

    function startTransfer(job, mode) {
        // job: { direction, local, remote, name, total }
        const finish = () => processPending()
        if (mode === "skip") { finish(); return }
        let j = Object.assign({}, job)
        const begin = () => {
            call("transfer.start", { direction: j.direction, local: j.local, remote: j.remote, resume: mode === "resume" }, function (data) {
                addTransfer(data.id, j)
                finish()
            }, function (code, text) { errorText = text; finish() })
        }
        if (mode === "both") {
            if (j.direction === "download") {
                const dir = parentOf(j.local.replace(/\\/g, "/"))
                const nm = LocalFs.uniqueName(dir, LocalFs.baseName(j.local))
                j.local = LocalFs.join(dir, nm); j.name = nm
            } else {
                const names = {}
                for (const e of entries) names[e.name] = true
                let nm = j.name, n = 1
                const dot = nm.lastIndexOf(".")
                const base = dot > 0 ? nm.substring(0, dot) : nm, ext = dot > 0 ? nm.substring(dot) : ""
                while (names[nm]) nm = base + " (" + (n++) + ")" + ext
                j.remote = joinPath(parentOf(j.remote), nm); j.name = nm
            }
            begin()
        } else if (mode === "replace") {
            if (j.direction === "download") { LocalFs.remove(j.local); begin() }
            else call("files.delete", { path: j.remote }, begin, function (c, t) { errorText = t; finish() })
        } else begin()
    }
    function processPending() {
        if (pendingJobs.length === 0) return
        const job = pendingJobs.shift()
        pendingJobs = pendingJobs.slice()
        const exists = job.direction === "download" ? LocalFs.exists(job.local) : entries.some(e => e.path === job.remote)
        if (exists) {
            conflict.name = job.name; conflict.download = job.direction === "download"; conflict.job = job
            conflict.open()
        } else startTransfer(job, "")
    }
    function enqueue(jobs) { pendingJobs = pendingJobs.concat(jobs); if (!conflict.opened) processPending() }

    function uploadPaths(paths) {
        if (!mutating()) return
        const jobs = []
        for (const p of paths) {
            const info = LocalFs.info(p)
            if (!info.exists) continue
            jobs.push({ direction: "upload", local: info.path, remote: joinPath(cwd, info.name), name: info.name, total: info.size })
        }
        enqueue(jobs)
    }
    function uploadUrls(urls) { uploadPaths(urls.map(u => LocalFs.toPath(u.toString()))) }
    function downloadTo(dir) {
        const jobs = selected.map(e => ({ direction: "download", local: LocalFs.join(dir, e.name), remote: e.path, name: e.name, total: e.size || 0 }))
        enqueue(jobs)
    }
    function retryTransfer(id) {
        const m = meta[id]
        if (!m) return
        const i = tIndex(id)
        if (i >= 0) tmodel.remove(i)
        startTransfer(m, "resume")
    }
    function clearFinished() {
        for (let i = tmodel.count - 1; i >= 0; --i) { const s = tmodel.get(i).status; if (s !== "running" && s !== "queued") tmodel.remove(i) }
    }

    // ---------------------------------------------------------------- диалоги
    DangerDialog { id: danger; objectName: "danger"; serverName: root.desktop.name }
    PermissionsDialog { id: perms; objectName: "perms"; onApply: (mode, uid, gid, sudo) => root.applyPermissions(root.selected[0], mode, uid, gid, sudo) }
    NameDialog {
        id: nameDlg
        objectName: "nameDlg"
        property string mode: "mkdir"
        property var target: null
        onSubmitted: (n) => root.submitName(n)
    }
    TransferConflictDialog {
        id: conflict
        objectName: "conflict"
        property var job: null
        onChosen: (mode) => root.startTransfer(job, mode)
    }
    FileDialog {
        id: uploadPick
        title: qsTr("Upload files")
        fileMode: FileDialog.OpenFiles
        onAccepted: root.uploadUrls(selectedFiles)
    }
    FolderDialog {
        id: uploadFolderPick
        title: qsTr("Upload folder")
        onAccepted: root.uploadUrls([selectedFolder])
    }
    FolderDialog {
        id: downloadPick
        title: qsTr("Download to…")
        currentFolder: "file://" + LocalFs.downloads()
        onAccepted: root.downloadTo(LocalFs.toPath(selectedFolder.toString()))
    }
    TextEdit { id: hiddenClip; visible: false }

    SMenu {
        id: menu
        MenuItem { text: qsTr("Open"); enabled: root.selected.length === 1; onTriggered: root.openEntry(root.selected[0]) }
        MenuItem { text: qsTr("Edit"); enabled: root.selected.length === 1 && root.selected[0].type !== "directory"; onTriggered: root.openEditor(root.selected[0]) }
        MenuItem { text: qsTr("Open terminal here"); onTriggered: root.openApp("terminal", { cwd: root.selected.length === 1 && root.selected[0].type === "directory" ? root.selected[0].path : root.cwd }) }
        MenuSeparator { }
        MenuItem { text: qsTr("Download…"); enabled: root.selected.length > 0; onTriggered: downloadPick.open() }
        MenuItem { text: qsTr("Rename…"); enabled: root.selected.length === 1 && !root.ro; onTriggered: root.renameSelected() }
        MenuItem { text: qsTr("Move to…"); enabled: root.selected.length === 1 && !root.ro; onTriggered: root.moveSelected() }
        MenuItem { text: qsTr("Permissions…"); enabled: root.selected.length === 1 && !root.ro; onTriggered: root.permissionsOfSelected() }
        MenuItem { text: qsTr("Copy path"); enabled: root.selected.length > 0; onTriggered: root.copyPath() }
        MenuSeparator { }
        MenuItem { text: qsTr("New folder"); enabled: !root.ro && !root.searchMode; onTriggered: root.newFolder() }
        MenuItem { text: qsTr("Delete"); enabled: root.selected.length > 0 && !root.ro; onTriggered: root.deleteSelected() }
        MenuItem { text: qsTr("Refresh"); onTriggered: root.refresh() }
    }

    // ---------------------------------------------------------------- разметка
    Component.onCompleted: {
        tree.append({ path: "/", name: "/", depth: 0, expanded: false, loaded: false, busy: false })
        navigate(home)
        loadTransfers()
    }
    Connections {
        target: root.desktop
        function onStateChanged() { if (root.desktop.state === "ready" && root.entries.length === 0 && !root.loading) root.navigate(root.cwd, false) }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // панель инструментов
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            color: Theme.alpha(Theme.panelFill, Theme.blurOn ? 0.4 : 1)
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10; anchors.rightMargin: 10
                spacing: 6
                SButton { compact: true; kind: "ghost"; text: "←"; enabled: root.histIndex > 0; onClicked: root.goBack(); Accessible.name: qsTr("Back") }
                SButton { compact: true; kind: "ghost"; text: "→"; enabled: root.histIndex < root.history.length - 1; onClicked: root.goForward(); Accessible.name: qsTr("Forward") }
                SButton { compact: true; kind: "ghost"; text: "↑"; enabled: root.cwd !== "/"; onClicked: root.goUp(); Accessible.name: qsTr("Up") }
                SButton { compact: true; kind: "ghost"; text: "⟳"; onClicked: root.refresh(); Accessible.name: qsTr("Refresh") }

                // путь: хлебные крошки или поле ввода
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 28
                    radius: Theme.radius
                    color: Theme.inputFill
                    border.width: root.editingPath ? 2 : 1
                    border.color: root.editingPath ? Theme.cyan : Theme.line
                    Row {
                        visible: !root.editingPath
                        anchors.verticalCenter: parent.verticalCenter
                        x: 8
                        spacing: 2
                        Repeater {
                            model: {
                                const parts = root.searchMode ? [] : root.cwd.split("/").filter(p => p !== "")
                                const out = [{ name: "/", path: "/" }]
                                let acc = ""
                                for (const p of parts) { acc += "/" + p; out.push({ name: p, path: acc }) }
                                return out
                            }
                            delegate: Row {
                                required property var modelData
                                required property int index
                                spacing: 2
                                SLabel { visible: index > 1; text: "›"; sub: true; anchors.verticalCenter: parent.verticalCenter }
                                SLabel {
                                    text: modelData.name; mono: true; font.pixelSize: 12
                                    color: Theme.cyan
                                    MouseArea { anchors.fill: parent; anchors.margins: -3; cursorShape: Qt.PointingHandCursor; onClicked: root.navigate(modelData.path) }
                                    Accessible.role: Accessible.Button
                                    Accessible.name: modelData.path
                                }
                            }
                        }
                        SLabel { visible: root.searchMode; mono: true; font.pixelSize: 12; text: qsTr("Search “%1” in %2").arg(root.searchQuery).arg(root.cwd) }
                    }
                    MouseArea {
                        anchors.fill: parent
                        visible: !root.editingPath
                        z: -1
                        onClicked: { root.editingPath = true; pathField.text = root.cwd; pathField.forceActiveFocus(); pathField.selectAll() }
                    }
                    TextInput {
                        id: pathField
                        visible: root.editingPath
                        anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 8
                        verticalAlignment: TextInput.AlignVCenter
                        color: Theme.text
                        font.family: Theme.fontMono; font.pixelSize: 12
                        selectByMouse: true
                        clip: true
                        onAccepted: { root.editingPath = false; root.navigate(text.length > 1 && text.endsWith("/") ? text.slice(0, -1) : text) }
                        onActiveFocusChanged: if (!activeFocus) root.editingPath = false
                        Keys.onEscapePressed: { root.editingPath = false; root.forceActiveFocus() }
                        Accessible.name: qsTr("Path")
                    }
                }
                STextField {
                    id: searchField
                    Layout.preferredWidth: 190
                    placeholderText: qsTr("Search by name")
                    onAccepted: root.runSearch(text)
                    Keys.onEscapePressed: { text = ""; root.runSearch("") }
                    Accessible.name: qsTr("Search files")
                }
                SButton { compact: true; kind: "ghost"; text: root.showHidden ? qsTr("Hide dotfiles") : qsTr("Show hidden"); onClicked: root.showHidden = !root.showHidden }
                SButton { compact: true; text: qsTr("Upload"); enabled: !root.ro && !root.searchMode; onClicked: uploadPick.open() }
                SButton { compact: true; text: qsTr("New folder"); enabled: !root.ro && !root.searchMode; onClicked: root.newFolder() }
                SButton { compact: true; kind: "ghost"; text: root.showProps ? "▮" : "▯"; onClicked: root.showProps = !root.showProps; Accessible.name: qsTr("Properties panel") }
            }
        }
        SBanner {
            Layout.fillWidth: true; Layout.margins: 6; Layout.bottomMargin: 0
            visible: root.errorText !== ""
            text: root.errorText
            actionText: qsTr("Retry")
            onAction: root.refresh()
            onDismissed: root.errorText = ""
        }
        SBanner {
            Layout.fillWidth: true; Layout.margins: 6; Layout.bottomMargin: 0
            visible: root.ro
            tone: Theme.amber
            text: qsTr("Read-only mode: browsing and downloads only.")
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            // дерево
            Rectangle {
                Layout.preferredWidth: 190
                Layout.fillHeight: true
                color: Theme.alpha(Theme.panelFill, Theme.blurOn ? 0.35 : 1)
                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 6
                    spacing: 2
                    SLabel { caps: true; text: qsTr("Favorites"); Layout.leftMargin: 6; Layout.topMargin: 2 }
                    Repeater {
                        model: [{ n: qsTr("Home"), p: root.home }, { n: "/", p: "/" }, { n: "/etc", p: "/etc" }, { n: "/var/log", p: "/var/log" }, { n: "/tmp", p: "/tmp" }]
                        delegate: Rectangle {
                            required property var modelData
                            Layout.fillWidth: true; Layout.preferredHeight: 24; radius: 5
                            color: fma.containsMouse ? Theme.hover : "transparent"
                            SLabel { x: 8; anchors.verticalCenter: parent.verticalCenter; text: modelData.n; color: Theme.cyan; font.pixelSize: Theme.sizeSmall }
                            MouseArea { id: fma; anchors.fill: parent; hoverEnabled: true; onClicked: root.navigate(modelData.p) }
                            Accessible.role: Accessible.Button
                            Accessible.name: modelData.n
                        }
                    }
                    SLabel { caps: true; text: qsTr("Folders"); Layout.leftMargin: 6; Layout.topMargin: 8 }
                    ListView {
                        id: treeView
                        Layout.fillWidth: true; Layout.fillHeight: true
                        clip: true
                        model: tree
                        ScrollBar.vertical: ScrollBar {}
                        delegate: Rectangle {
                            id: tn
                            required property int index
                            required property string path
                            required property string name
                            required property int depth
                            required property bool expanded
                            required property bool busy
                            width: ListView.view.width; height: 24; radius: 5
                            color: path === root.cwd ? Theme.selected : (tma.containsMouse ? Theme.hover : "transparent")
                            Row {
                                anchors.verticalCenter: parent.verticalCenter
                                x: 4 + tn.depth * 12
                                spacing: 3
                                SLabel {
                                    width: 14; text: tn.busy ? "…" : (tn.expanded ? "▾" : "▸"); sub: true; font.pixelSize: 11; horizontalAlignment: Text.AlignHCenter
                                    MouseArea { anchors.fill: parent; anchors.margins: -3; onClicked: root.treeToggle(tn.index) }
                                }
                                SLabel { text: tn.name; width: 150 - tn.depth * 12; font.pixelSize: Theme.sizeSmall; elide: Text.ElideRight }
                            }
                            MouseArea { id: tma; anchors.fill: parent; anchors.leftMargin: 20 + tn.depth * 12; hoverEnabled: true; onClicked: root.navigate(tn.path) }
                            Accessible.role: Accessible.TreeItem
                            Accessible.name: tn.path
                        }
                    }
                }
            }
            Rectangle { Layout.fillHeight: true; width: 1; color: Theme.line }

            // список
            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true
                focus: true
                id: listArea
                Keys.onPressed: (e) => {
                    if (e.key === Qt.Key_Down) { root.moveSelection(1, e.modifiers); e.accepted = true }
                    else if (e.key === Qt.Key_Up) { root.moveSelection(-1, e.modifiers); e.accepted = true }
                    else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) { if (root.selected.length === 1) root.openEntry(root.selected[0]); e.accepted = true }
                    else if (e.key === Qt.Key_Backspace) { root.goUp(); e.accepted = true }
                    else if (e.key === Qt.Key_Delete) { root.deleteSelected(); e.accepted = true }
                    else if (e.key === Qt.Key_F2) { root.renameSelected(); e.accepted = true }
                    else if (e.key === Qt.Key_F5) { root.refresh(); e.accepted = true }
                    else if (e.key === Qt.Key_A && (e.modifiers & Qt.ControlModifier)) { root.selectAll(); e.accepted = true }
                    else if (e.key === Qt.Key_L && (e.modifiers & Qt.ControlModifier)) { root.editingPath = true; pathField.text = root.cwd; pathField.forceActiveFocus(); pathField.selectAll(); e.accepted = true }
                }

                // заголовок колонок
                Row {
                    id: header
                    width: parent.width
                    height: 26
                    Repeater {
                        model: [{ k: "name", t: qsTr("Name"), w: 0 }, { k: "size", t: qsTr("Size"), w: 80 }, { k: "mtime", t: qsTr("Modified"), w: 130 },
                                { k: "uid", t: qsTr("Owner"), w: 84 }, { k: "permissions", t: qsTr("Permissions"), w: 92 }]
                        delegate: Rectangle {
                            required property var modelData
                            width: modelData.w ? modelData.w : header.width - 386
                            height: 26
                            color: Theme.alpha(Theme.panelFill, 0.7)
                            SLabel { x: 8; anchors.verticalCenter: parent.verticalCenter; caps: true; text: modelData.t + (root.sortKey === modelData.k ? (root.sortDesc ? " ▼" : " ▲") : "") }
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                                onClicked: { if (root.sortKey === modelData.k) root.sortDesc = !root.sortDesc; else { root.sortKey = modelData.k; root.sortDesc = false } } }
                        }
                    }
                }
                ListView {
                    id: listView
                    anchors.top: header.bottom; anchors.bottom: parent.bottom
                    width: parent.width
                    clip: true
                    model: root.shown
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar {}
                    delegate: Rectangle {
                        id: fr
                        required property var modelData
                        required property int index
                        readonly property bool sel: root.selection[modelData.path] === true
                        width: ListView.view.width; height: 28
                        color: sel ? Theme.selected : (fma2.containsMouse ? Theme.hover : (index % 2 ? Theme.alpha(Theme.text, 0.025) : "transparent"))
                        Row {
                            anchors.fill: parent
                            Item {
                                width: parent.width - 386; height: parent.height
                                Row {
                                    anchors.verticalCenter: parent.verticalCenter; x: 8; spacing: 8; width: parent.width - 12
                                    SLabel { text: root.glyphOf(fr.modelData); color: fr.modelData.type === "directory" ? Theme.cyan : Theme.textSub; font.bold: true; anchors.verticalCenter: parent.verticalCenter }
                                    SLabel {
                                        text: root.searchMode ? fr.modelData.path : fr.modelData.name
                                        width: parent.width - 24
                                        anchors.verticalCenter: parent.verticalCenter
                                        elide: root.searchMode ? Text.ElideMiddle : Text.ElideRight
                                        color: fr.modelData.name.charAt(0) === "." ? Theme.textSub : Theme.text
                                    }
                                }
                            }
                            SLabel { width: 80; height: parent.height; verticalAlignment: Text.AlignVCenter; leftPadding: 8; mono: true; font.pixelSize: 11; text: fr.modelData.type === "directory" ? "—" : root.fmtSize(fr.modelData.size) }
                            SLabel { width: 130; height: parent.height; verticalAlignment: Text.AlignVCenter; leftPadding: 8; mono: true; font.pixelSize: 11; sub: true; text: root.fmtTime(fr.modelData.mtime) }
                            SLabel { width: 84; height: parent.height; verticalAlignment: Text.AlignVCenter; leftPadding: 8; mono: true; font.pixelSize: 11; sub: true; text: fr.modelData.uid + ":" + fr.modelData.gid }
                            SLabel { width: 92; height: parent.height; verticalAlignment: Text.AlignVCenter; leftPadding: 8; mono: true; font.pixelSize: 11; sub: true; text: root.fmtPerm(fr.modelData.permissions) }
                        }
                        MouseArea {
                            id: fma2
                            anchors.fill: parent
                            hoverEnabled: true
                            acceptedButtons: Qt.LeftButton | Qt.RightButton
                            onPressed: (m) => {
                                listArea.forceActiveFocus()
                                if (m.button === Qt.RightButton) { if (!fr.sel) root.selectOnly(fr.modelData.path); menu.popup() }
                                else root.selectClick(fr.modelData, m.modifiers)
                            }
                            onDoubleClicked: (m) => { if (m.button === Qt.LeftButton) root.openEntry(fr.modelData) }
                        }
                        Accessible.role: Accessible.ListItem
                        Accessible.name: modelData.name
                    }
                    MouseArea {
                        anchors.fill: parent
                        z: -1
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onPressed: (m) => { listArea.forceActiveFocus(); root.selection = ({}); if (m.button === Qt.RightButton) menu.popup() }
                    }
                }
                BusyIndicatorLike { visible: root.loading }
                SEmptyState {
                    anchors.top: header.bottom; anchors.bottom: parent.bottom; width: parent.width
                    visible: !root.loading && root.shown.length === 0 && root.errorText === ""
                    glyph: root.searchMode ? "?" : "▸"
                    title: root.searchMode ? qsTr("Nothing found") : qsTr("Folder is empty")
                    description: root.searchMode ? "" : qsTr("Drop files here to upload them.")
                }
                SLabel {
                    visible: root.truncated
                    anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: 8
                    color: Theme.amber; font.pixelSize: 11
                    text: qsTr("List is truncated")
                }

                // перетаскивание файлов из ОС = загрузка
                DropArea {
                    id: drop
                    anchors.fill: parent
                    onDropped: (d) => { if (d.hasUrls) root.uploadUrls(d.urls) }
                }
                Rectangle {
                    visible: drop.containsDrag
                    anchors.fill: parent
                    color: Theme.alpha(Theme.accent, 0.18)
                    border.color: Theme.cyan; border.width: 2
                    radius: 8
                    SLabel { anchors.centerIn: parent; text: qsTr("Drop to upload to %1").arg(root.cwd); font.pixelSize: 15; font.bold: true }
                }
            }

            // свойства
            Rectangle {
                visible: root.showProps
                Layout.preferredWidth: root.showProps ? 230 : 0
                Layout.fillHeight: true
                id: props
                color: Theme.alpha(Theme.panelFill, Theme.blurOn ? 0.35 : 1)
                readonly property var e: root.selected.length === 1 ? root.selected[0] : null
                Rectangle { width: 1; height: parent.height; color: Theme.line }
                ColumnLayout {
                    anchors.fill: parent; anchors.margins: 14; spacing: 8
                    SLabel { caps: true; text: qsTr("Properties") }
                    SLabel {
                        visible: root.selected.length === 0
                        Layout.fillWidth: true; sub: true; wrapMode: Text.Wrap; elide: Text.ElideNone
                        text: root.shown.length + " " + qsTr("items") + (root.searchMode ? "" : " · " + root.cwd)
                    }
                    SLabel { visible: root.selected.length > 1; text: qsTr("%1 selected").arg(root.selected.length); font.bold: true }
                    ColumnLayout {
                        visible: props.e !== null
                        Layout.fillWidth: true
                        spacing: 6
                        SLabel { Layout.fillWidth: true; text: props.e ? props.e.name : ""; font.bold: true; font.pixelSize: 14; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone }
                        Repeater {
                            model: {
                                const e = root.selected.length === 1 ? root.selected[0] : null
                                if (!e) return []
                                return [{ k: qsTr("Path"), v: e.path }, { k: qsTr("Type"), v: e.type }, { k: qsTr("Size"), v: e.type === "directory" ? "—" : root.fmtSize(e.size) + " (" + e.size + " B)" },
                                        { k: qsTr("Modified"), v: root.fmtTime(e.mtime) }, { k: qsTr("Owner"), v: e.uid + ":" + e.gid },
                                        { k: qsTr("Permissions"), v: root.fmtPerm(e.permissions) + "  " + ((e.permissions & 4095).toString(8)) }]
                            }
                            delegate: Column {
                                required property var modelData
                                Layout.fillWidth: true
                                SLabel { text: modelData.k; caps: true }
                                SLabel { width: 200; text: modelData.v; mono: true; font.pixelSize: 11; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone }
                            }
                        }
                        SButton { compact: true; text: qsTr("Permissions…"); enabled: !root.ro; onClicked: root.permissionsOfSelected() }
                    }
                    Item { Layout.fillHeight: true }
                }
            }
        }

        FilesTransfers {
            id: transfers
            Layout.fillWidth: true
            Layout.preferredHeight: implicitHeight
            model: tmodel
            expanded: false
            onCancelRequested: (id) => root.desktop.rpcRaw("transfer.cancel", { id: id }, function () {})
            onRetryRequested: (id) => root.retryTransfer(id)
            onRemoveRequested: (id) => { const i = root.tIndex(id); if (i >= 0) tmodel.remove(i) }
            onClearFinished: root.clearFinished()
        }
    }
    component BusyIndicatorLike: Item {
        anchors.centerIn: parent
        SSpinner { anchors.centerIn: parent; width: 22; height: 22 }
    }
}
