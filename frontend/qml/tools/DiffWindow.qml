import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Сравнение конфигов: файл на этом сервере против другого сервера или сохранённой версии; две колонки, «только различия».
ToolPane {
    id: root
    property string against: "server"           // server | version
    property var versions: []
    property var lines: []                      // [{kind: ctx|add|del, left, right}]
    property bool equal: false
    property bool compared: false
    property bool onlyDiff: false
    property bool loading: false
    property string leftTitle: ""
    property string rightTitle: ""
    property alias pathField: pathField
    property alias otherSrv: otherSrv
    property alias otherPath: otherPath
    property alias verSel: verSel

    readonly property var serverOpts: Backend.allServers().filter(s => s.id !== serverId).map(s => ({ value: s.id, text: s.name }))
    readonly property var verOpts: versions.map(v => ({ value: v.id, text: root.srvName(v.connection_id) + ": " + v.path + " · " + String(v.id).slice(0, 6) }))
    function srvName(id) { const s = Backend.serverById(id); return s && s.name ? s.name : id }

    // Построчное выравнивание двух блоков: ctx — общие строки, chg — замена, del/add — только слева/справа
    function align(a, b) {
        const n = a.length, m = b.length, ops = []
        if (n === 0 || m === 0 || n * m > 4000000) {
            const k = Math.max(n, m)
            for (let i = 0; i < k; ++i)
                ops.push({ kind: i < n && i < m ? "chg" : (i < n ? "del" : "add"), left: i < n ? a[i] : "", right: i < m ? b[i] : "" })
            return ops
        }
        const w = m + 1
        const t = new Int32Array((n + 1) * w)
        for (let i = n - 1; i >= 0; --i)
            for (let j = m - 1; j >= 0; --j)
                t[i * w + j] = a[i] === b[j] ? t[(i + 1) * w + j + 1] + 1 : Math.max(t[(i + 1) * w + j], t[i * w + j + 1])
        let i = 0, j = 0
        const dels = [], adds = []
        const flush = () => {
            const k = Math.max(dels.length, adds.length)
            for (let x = 0; x < k; ++x)
                ops.push({ kind: x < dels.length && x < adds.length ? "chg" : (x < dels.length ? "del" : "add"), left: x < dels.length ? dels[x] : "", right: x < adds.length ? adds[x] : "" })
            dels.length = 0; adds.length = 0
        }
        while (i < n || j < m) {
            if (i < n && j < m && a[i] === b[j]) { flush(); ops.push({ kind: "ctx", left: a[i], right: b[j] }); ++i; ++j }
            else if (j < m && (i >= n || t[i * w + j + 1] >= t[(i + 1) * w + j])) adds.push(b[j++])
            else dels.push(a[i++])
        }
        flush()
        return ops
    }
    // unified (один блок) → строки левой и правой колонок
    function parseUnified(u) {
        const out = []
        for (const l of (u || "").split("\n")) {
            if (l.startsWith("---") || l.startsWith("+++") || l.startsWith("@@") || l === "\\ No newline at end of file") continue
            if (l.startsWith("+")) out.push({ kind: "add", left: "", right: l.slice(1) })
            else if (l.startsWith("-")) out.push({ kind: "del", left: l.slice(1), right: "" })
            else out.push({ kind: "ctx", left: l.slice(1), right: l.slice(1) })
        }
        // убрать пустой хвост после последнего \n
        if (out.length && out[out.length - 1].kind === "ctx" && out[out.length - 1].left === "") out.pop()
        // Ядро заменяет весь отличающийся участок одним блоком «−…+…»: уточняем его построчным LCS
        const merged = []
        for (let i = 0; i < out.length; ++i) {
            if (out[i].kind !== "del" && out[i].kind !== "add") { merged.push(out[i]); continue }
            const dels = [], adds = []
            let j = i
            while (j < out.length && out[j].kind === "del") dels.push(out[j++].left)
            while (j < out.length && out[j].kind === "add") adds.push(out[j++].right)
            for (const r of align(dels, adds)) merged.push(r)
            i = j - 1
        }
        return merged
    }
    function loadVersions() {
        req("diff.versions.list", {}, d => { versions = d.items || [] }, e => { errorText = e })
    }
    function compare() {
        const path = pathField.text.trim()
        if (path === "" || path[0] !== "/") { pathField.error = qsTr("Enter an absolute path"); return }
        pathField.error = ""
        const p = { path: path }
        if (against === "server") {
            if (otherSrv.current === "") { errorText = qsTr("Choose a server to compare with"); return }
            p.other_connection_id = otherSrv.current
            if (otherPath.text.trim() !== "") p.other_path = otherPath.text.trim()
            leftTitle = serverName + ":" + path
            rightTitle = srvName(otherSrv.current) + ":" + (p.other_path || path)
        } else {
            if (verSel.current === "") { errorText = qsTr("Choose a saved version"); return }
            p.version_id = verSel.current
            leftTitle = serverName + ":" + path
            rightTitle = qsTr("saved version")
        }
        loading = true
        req("diff.compare", p, d => { equal = !!d.equal; lines = equal ? [] : parseUnified(d.unified); compared = true; loading = false },
            e => { errorText = e; loading = false })
    }
    function saveVersion() {
        const path = pathField.text.trim()
        if (path === "" || path[0] !== "/") { pathField.error = qsTr("Enter an absolute path"); return }
        req("diff.save", { path: path }, () => { note.text = qsTr("Version saved"); loadVersions() }, e => { errorText = e })
    }
    function delVersion() {
        if (verSel.current === "") return
        dangerous({ title: qsTr("Delete saved version"), items: [verSel.current], confirmText: qsTr("Delete"), needName: false }, "diff.versions.del", { id: verSel.current },
                  () => { verSel.current = ""; loadVersions() })
    }
    readonly property var visibleLines: onlyDiff ? lines.filter(l => l.kind !== "ctx") : lines

    Component.onCompleted: loadVersions()

    ColumnLayout {
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 10; spacing: 8
        RowLayout {
            spacing: 8
            STextField { id: pathField; objectName: "diffPath"; Layout.fillWidth: true; label: qsTr("File on this server"); mono: true; placeholderText: "/etc/nginx/nginx.conf"; onAccepted: root.compare() }
            SButton { Layout.alignment: Qt.AlignBottom; text: qsTr("Save version"); enabled: !root.readonlyMode; onClicked: root.saveVersion() }
        }
        RowLayout {
            spacing: 8
            SSegmented { model: [{ value: "server", text: qsTr("Other server") }, { value: "version", text: qsTr("Saved version") }]; current: root.against; onActivated: (v) => root.against = v }
            SSelect { id: otherSrv; objectName: "diffOtherServer"; visible: root.against === "server"; Layout.preferredWidth: 200; model: root.serverOpts; current: root.serverOpts.length ? root.serverOpts[0].value : "" }
            STextField { id: otherPath; visible: root.against === "server"; Layout.fillWidth: true; mono: true; placeholderText: qsTr("path on the other server (empty = same)") }
            SSelect { id: verSel; visible: root.against === "version"; Layout.fillWidth: true; model: root.verOpts; current: "" }
            SButton { visible: root.against === "version"; compact: true; text: qsTr("Delete…"); enabled: verSel.current !== "" && !root.readonlyMode; onClicked: root.delVersion() }
            SButton { text: qsTr("Compare"); kind: "primary"; busy: root.loading; onClicked: root.compare() }
        }
        RowLayout {
            visible: root.compared
            SChip { text: root.equal ? qsTr("identical") : qsTr("%1 differences").arg(root.lines.filter(l => l.kind !== "ctx").length); tone: root.equal ? Theme.green : Theme.amber; mono: false }
            SCheckBox { text: qsTr("Only differences"); checked: root.onlyDiff; onToggled: root.onlyDiff = checked }
            SLabel { id: note; Layout.fillWidth: true; sub: true; horizontalAlignment: Text.AlignRight }
        }
        Rectangle {
            Layout.fillWidth: true; Layout.fillHeight: true
            radius: 8; color: Theme.inputFill; border.color: Theme.line; clip: true
            SLabel { visible: !root.compared; anchors.centerIn: parent; sub: true; text: qsTr("Choose a file and a source to compare with") }
            SLabel { visible: root.compared && root.equal; anchors.centerIn: parent; color: Theme.green; text: qsTr("Files are identical") }
            Row {
                visible: root.compared && !root.equal
                id: dh; width: parent.width; height: 24
                Repeater { model: [root.leftTitle, root.rightTitle]
                    delegate: Rectangle { required property var modelData; width: dh.width / 2; height: 24; color: Theme.alpha(Theme.panelFill, 0.8)
                                          SLabel { x: 8; width: parent.width - 12; anchors.verticalCenter: parent.verticalCenter; mono: true; font.pixelSize: 11; text: modelData } } }
            }
            ListView {
                objectName: "diffView"
                visible: root.compared && !root.equal
                anchors.top: dh.bottom; anchors.bottom: parent.bottom; width: parent.width
                clip: true; model: root.visibleLines
                ScrollBar.vertical: ScrollBar {}
                delegate: Row {
                    required property var modelData
                    width: ListView.view.width
                    Repeater {
                        model: [{ t: modelData.left, side: "l" }, { t: modelData.right, side: "r" }]
                        delegate: Rectangle {
                            required property var modelData
                            readonly property var ln: parent.modelData
                            width: parent.width / 2; height: 20
                            color: ln.kind === "ctx" ? "transparent"
                                   : (ln.kind === "chg" ? Theme.alpha(Theme.amber, 0.14)
                                      : (ln.kind === "del" ? (modelData.side === "l" ? Theme.alpha(Theme.red, 0.18) : Theme.alpha(Theme.text, 0.04))
                                                           : (modelData.side === "r" ? Theme.alpha(Theme.green, 0.18) : Theme.alpha(Theme.text, 0.04))))
                            SLabel { x: 8; width: parent.width - 12; anchors.verticalCenter: parent.verticalCenter; mono: true; font.pixelSize: 11; text: modelData.t; elide: Text.ElideRight }
                        }
                    }
                }
            }
        }
    }
}
