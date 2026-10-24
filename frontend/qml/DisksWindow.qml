import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// «Анализ дисков»: разделы (disk.list), карта размеров каталога — treemap/sunburst/список (disk.scan), крупные файлы (disk.large).
UtilPane {
    id: root
    property string tab: "parts"
    property var parts: []
    property var blockdevices: []
    property bool loading: false
    // карта
    property string scanPath: "/"
    property int scanDepth: 2
    property bool oneFs: true
    property string mapMode: "treemap"
    property string scanSub: ""
    property bool scanning: false
    property var tree: null                // корень текущей карты
    property var crumbs: []                // пути от корня сканирования
    property bool partial: false
    property bool truncated: false
    property var largest: []
    // крупные файлы
    property string bigPath: "/"
    property string bigMin: "100"
    property string bigExt: ""
    property string bigAge: ""
    property var big: []
    property bool bigLoading: false
    property bool bigPartial: false
    property bool unavailable: !!desktop.capabilities.utilities && desktop.capabilities.utilities.disks === false

    function baseName(p) { if (p === "/") return "/"; const i = p.lastIndexOf("/"); return i >= 0 ? p.slice(i + 1) : p }
    function parentOf(p) { const i = p.lastIndexOf("/"); return i <= 0 ? "/" : p.slice(0, i) }
    // плоский список {path,size_bytes} → дерево
    function buildTree(items, rootPath) {
        const nodes = {}
        for (const it of items) nodes[it.path] = { path: it.path, name: baseName(it.path), size: it.size_bytes, children: [] }
        let rootNode = nodes[rootPath]
        if (!rootNode) rootNode = nodes[rootPath] = { path: rootPath, name: baseName(rootPath), size: items.reduce((m, i) => Math.max(m, i.size_bytes), 0), children: [] }
        for (const p of Object.keys(nodes)) {
            if (p === rootPath) continue
            const par = nodes[parentOf(p)]
            if (par) par.children.push(nodes[p])
        }
        // доля «файлы прямо в каталоге»: разница между размером и суммой детей
        for (const p of Object.keys(nodes)) {
            const n = nodes[p]
            if (!n.children.length) continue
            const sum = n.children.reduce((s, k) => s + k.size, 0)
            if (n.size - sum > n.size * 0.02) n.children.push({ path: n.path, name: qsTr("(files)"), size: n.size - sum, children: [], files: true })
        }
        return rootNode
    }
    function find(node, path) {
        if (node.path === path && !node.files) return node
        for (const k of node.children) { if (path === k.path || path.indexOf(k.path + "/") === 0) { const f = find(k, path); if (f) return f } }
        return null
    }
    property var scanRoot: null

    function loadParts() {
        if (!working || unavailable) return
        loading = true
        rpc("disk.list", {}, (d) => { loading = false; parts = d.items || []; blockdevices = d.blockdevices || [] }, (t) => { loading = false; showNotice(t) })
    }
    function startScan(path) {
        if (!working) return
        cancelScan()
        scanPath = path
        scanning = true; tree = null; scanRoot = null
        const p = { path: path, depth: scanDepth, one_filesystem: oneFs, timeout_sec: 60, limit: 2000 }
        rpc("disk.scan", p, (d) => { scanSub = d.subscription }, (t) => { scanning = false; showNotice(t) })
    }
    function cancelScan() {
        if (scanSub !== "") { rpc("disk.scan.cancel", { subscription: scanSub }, () => {}, () => {}); scanSub = "" }
        scanning = false
    }
    onEvent: (ev) => {
        if (scanSub !== "" && ev.subscription === scanSub) {
            if (ev.event === "disk.scan.done") {
                scanSub = ""; scanning = false
                const data = ev.data || ({})
                partial = !!data.partial; truncated = !!data.truncated
                scanRoot = buildTree(data.items || [], scanPath)
                tree = scanRoot
                crumbs = [scanPath]
            } else if (ev.event === "error") {
                scanSub = ""; scanning = false
                showNotice((ev.error && ev.error.message) || qsTr("Scan failed"))
            }
        }
    }
    function drill(node) {
        if (node.files) return
        if (node.children.length) { tree = node; crumbs = pathCrumbs(node.path) }
        else startScan(node.path)
    }
    function pathCrumbs(p) {
        const out = [scanPath]
        if (p === scanPath) return out
        const rel = scanPath === "/" ? p.slice(1) : p.slice(scanPath.length + 1)
        let cur = scanPath
        for (const part of rel.split("/")) { cur = (cur === "/" ? "" : cur) + "/" + part; out.push(cur) }
        return out
    }
    function goCrumb(p) { const n = scanRoot ? find(scanRoot, p) : null; if (n && n.children.length) { tree = n; crumbs = pathCrumbs(p) } else startScan(p) }
    function runBig() {
        if (!working) return
        bigLoading = true
        const p = { path: bigPath, min_size: Math.max(1, parseFloat(bigMin) || 100) * 1048576, one_filesystem: oneFs, limit: 300, timeout_sec: 60 }
        if (bigExt !== "") p.extension = bigExt
        if (bigAge !== "" && parseInt(bigAge) > 0) p.min_age_days = parseInt(bigAge)
        rpc("disk.large", p, (d) => { bigLoading = false; big = d.items || []; bigPartial = !!d.partial }, (t) => { bigLoading = false; showNotice(t) })
    }
    onWorkingChanged: if (working) loadParts()
    Component.onCompleted: if (working) loadParts()
    Component.onDestruction: if (scanSub !== "") desktop.call("disk.scan.cancel", { subscription: scanSub })

    readonly property var listRows: tree ? tree.children.slice().sort((a, b) => b.size - a.size) : []

    SEmptyState {
        anchors.fill: parent; visible: root.unavailable || !root.working
        glyph: "◍"; title: root.working ? qsTr("Not available on this server") : qsTr("Waiting for the server…")
        description: root.working ? qsTr("The required tools (df, lsblk, du, find) were not found.") : ""
    }
    ColumnLayout {
        anchors.fill: parent; anchors.margins: 12; spacing: 10
        visible: root.working && !root.unavailable
        RowLayout {
            Layout.fillWidth: true; spacing: 8
            SSegmented {
                current: root.tab
                model: [{ value: "parts", text: qsTr("Partitions") }, { value: "map", text: qsTr("Map") }, { value: "big", text: qsTr("Large files") }]
                onActivated: (v) => root.tab = v
            }
            Item { Layout.fillWidth: true }
            SSpinner { visible: root.loading; width: 18; height: 18 }
            SButton { visible: root.tab === "parts"; text: qsTr("Refresh"); compact: true; onClicked: root.loadParts() }
        }
        StackLayout {
            Layout.fillWidth: true; Layout.fillHeight: true
            currentIndex: ["parts", "map", "big"].indexOf(root.tab)

            // ---------------- разделы
            ColumnLayout {
                spacing: 8
                UTable {
                    id: partTable; objectName: "partTable"
                    Layout.fillWidth: true; Layout.fillHeight: true
                    rows: root.parts
                    emptyText: root.loading ? "" : qsTr("No partitions")
                    onRowDoubleClicked: (i, r) => { root.tab = "map"; root.startScan(r.mount) }
                    columns: [
                        { key: "device", title: qsTr("Device"), w: 190, mono: true },
                        { key: "mount", title: qsTr("Mount"), w: 190, mono: true },
                        { key: "total_bytes", title: qsTr("Size"), w: 84, right: true, fmt: (r) => Fmt.bytes(r.total_bytes) },
                        { key: "used_bytes", title: qsTr("Used"), w: 84, right: true, fmt: (r) => Fmt.bytes(r.used_bytes) },
                        { key: "usage", title: qsTr("Usage"), w: 0, kind: "meter", value: (r) => r.usage_pct || 0, color: (r) => Fmt.usageTone(r.usage_pct) },
                        { key: "usage_pct", title: "%", w: 52, right: true, fmt: (r) => Fmt.pct(r.usage_pct), color: (r) => Fmt.usageTone(r.usage_pct) },
                        { key: "inodes_usage_pct", title: "inode", w: 60, right: true, fmt: (r) => Fmt.pct(r.inodes_usage_pct), color: (r) => Fmt.usageTone(r.inodes_usage_pct) }
                    ]
                }
                RowLayout {
                    Layout.fillWidth: true
                    SLabel { Layout.fillWidth: true; sub: true; font.pixelSize: 11; text: qsTr("Double-click a partition to analyze its contents.") }
                    SButton { text: qsTr("Analyze selected"); compact: true; enabled: partTable.currentIndex >= 0
                              onClicked: { root.tab = "map"; root.startScan(root.parts[partTable.currentIndex].mount) } }
                }
            }

            // ---------------- карта
            ColumnLayout {
                spacing: 8
                RowLayout {
                    Layout.fillWidth: true; spacing: 8
                    STextField { id: pathField; objectName: "scanPath"; Layout.fillWidth: true; mono: true; text: root.scanPath; placeholderText: "/var"; onAccepted: root.startScan(text) ; Accessible.name: qsTr("Path to analyze") }
                    SSelect { Layout.preferredWidth: 90; current: String(root.scanDepth); model: [{ value: "1", text: qsTr("depth 1") }, { value: "2", text: qsTr("depth 2") }, { value: "3", text: qsTr("depth 3") }]
                              onActivated: (v) => root.scanDepth = parseInt(v) }
                    SCheckBox { text: qsTr("one filesystem"); checked: root.oneFs; onToggled: root.oneFs = checked }
                    SButton { objectName: "scanBtn"; text: root.scanning ? qsTr("Cancel") : qsTr("Scan"); kind: root.scanning ? "secondary" : "primary"; onClicked: root.scanning ? root.cancelScan() : root.startScan(pathField.text) }
                }
                RowLayout {
                    Layout.fillWidth: true; spacing: 6
                    SSegmented {
                        current: root.mapMode
                        model: [{ value: "treemap", text: "Treemap" }, { value: "sunburst", text: "Sunburst" }, { value: "list", text: qsTr("List") }]
                        onActivated: (v) => root.mapMode = v
                    }
                    Flow {
                        Layout.fillWidth: true; spacing: 2
                        Repeater {
                            model: root.crumbs
                            delegate: SLabel {
                                required property string modelData
                                required property int index
                                color: index === root.crumbs.length - 1 ? Theme.text : Theme.cyan
                                text: (index > 0 ? " › " : "") + root.baseName(modelData)
                                elide: Text.ElideNone
                                MouseArea { anchors.fill: parent; onClicked: root.goCrumb(modelData) }
                            }
                        }
                    }
                    SChip { visible: root.partial; text: qsTr("partial"); tone: Theme.amber; mono: false }
                    SChip { visible: root.truncated; text: qsTr("truncated"); tone: Theme.amber; mono: false }
                }
                Rectangle {
                    Layout.fillWidth: true; Layout.fillHeight: true; radius: 8; color: Theme.inputFill; border.color: Theme.line; clip: true
                    RowLayout {
                        anchors.fill: parent; anchors.margins: 6; spacing: 8
                        DiskMap {
                            id: diskMap; objectName: "diskMap"
                            Layout.fillWidth: true; Layout.fillHeight: true; Layout.minimumWidth: 120
                            visible: root.mapMode !== "list" && root.tree !== null
                            node: root.tree; mode: root.mapMode
                            onDrill: (n) => root.drill(n)
                        }
                        UTable {
                            objectName: "mapList"
                            Layout.fillWidth: true; Layout.fillHeight: true
                            visible: root.mapMode === "list" && root.tree !== null
                            rows: root.listRows
                            onRowDoubleClicked: (i, r) => root.drill(r)
                            columns: [
                                { key: "name", title: qsTr("Name"), w: 0, mono: true },
                                { key: "size", title: qsTr("Size"), w: 90, right: true, fmt: (r) => Fmt.bytes(r.size) },
                                { key: "share", title: qsTr("Share"), w: 160, kind: "meter", value: (r) => root.tree && root.tree.size ? r.size / root.tree.size * 100 : 0, color: (r) => Theme.cyan },
                                { key: "pct", title: "%", w: 52, right: true, fmt: (r) => root.tree && root.tree.size ? (r.size / root.tree.size * 100).toFixed(0) + "%" : "" }
                            ]
                        }
                        // крупнейшие
                        ColumnLayout {
                            visible: root.mapMode !== "list" && root.tree !== null
                            Layout.preferredWidth: 230; Layout.minimumWidth: 230; Layout.maximumWidth: 230; Layout.fillHeight: true; spacing: 4
                            SLabel { caps: true; text: qsTr("Largest") }
                            Repeater {
                                model: root.listRows.slice(0, 12)
                                delegate: RowLayout {
                                    required property var modelData
                                    required property int index
                                    Layout.fillWidth: true; spacing: 6
                                    Rectangle { width: 10; height: 10; radius: 2; color: diskMap.color(index, 0) }
                                    SLabel { Layout.fillWidth: true; mono: true; font.pixelSize: 11; text: modelData.name; color: diskMap.hovered && diskMap.hovered.path === modelData.path ? Theme.cyan : Theme.text }
                                    SLabel { mono: true; font.pixelSize: 11; sub: true; text: Fmt.bytes(modelData.size) }
                                }
                            }
                            Item { Layout.fillHeight: true }
                            SLabel { Layout.fillWidth: true; visible: diskMap.hovered !== null; mono: true; font.pixelSize: 11; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone
                                     text: diskMap.hovered ? diskMap.hovered.path + "\n" + Fmt.bytes(diskMap.hovered.size) : "" }
                        }
                    }
                    ColumnLayout {
                        anchors.centerIn: parent; visible: root.tree === null; spacing: 10
                        SSpinner { Layout.alignment: Qt.AlignHCenter; width: 24; height: 24; visible: root.scanning }
                        SLabel { Layout.alignment: Qt.AlignHCenter; sub: true
                                 text: root.scanning ? qsTr("Scanning %1…").arg(root.scanPath) : qsTr("Pick a path and press Scan") }
                        SLabel { Layout.alignment: Qt.AlignHCenter; sub: true; font.pixelSize: 11; visible: root.scanning; text: qsTr("Low priority (nice/ionice), up to 60 s") }
                    }
                }
            }

            // ---------------- крупные файлы
            ColumnLayout {
                spacing: 8
                RowLayout {
                    Layout.fillWidth: true; spacing: 8
                    STextField { id: bigPathField; objectName: "bigPath"; Layout.fillWidth: true; mono: true; text: root.bigPath; label: qsTr("Path"); onEdited: root.bigPath = text; onAccepted: root.runBig() }
                    STextField { Layout.preferredWidth: 100; label: qsTr("Min size, MB"); text: root.bigMin; onEdited: root.bigMin = text; validator: DoubleValidator { bottom: 0 } onAccepted: root.runBig() }
                    STextField { Layout.preferredWidth: 90; label: qsTr("Extension"); placeholderText: "log"; mono: true; onEdited: root.bigExt = text; onAccepted: root.runBig() }
                    STextField { Layout.preferredWidth: 100; label: qsTr("Older, days"); onEdited: root.bigAge = text; validator: IntValidator { bottom: 0 } onAccepted: root.runBig() }
                    SButton { Layout.alignment: Qt.AlignBottom; objectName: "bigBtn"; text: qsTr("Find"); kind: "primary"; busy: root.bigLoading; onClicked: root.runBig() }
                }
                UTable {
                    objectName: "bigTable"
                    Layout.fillWidth: true; Layout.fillHeight: true
                    rows: root.big
                    emptyText: root.bigLoading ? qsTr("Searching…") : qsTr("Press Find to search for large files")
                    columns: [
                        { key: "size_bytes", title: qsTr("Size"), w: 90, right: true, fmt: (r) => Fmt.bytes(r.size_bytes), color: (r) => r.size_bytes > 1073741824 ? Theme.red : (r.size_bytes > 524288000 ? Theme.amber : Theme.text) },
                        { key: "path", title: qsTr("Path"), w: 0, mono: true }
                    ]
                    onRowDoubleClicked: (i, r) => root.desktop.copyText(r.path)
                }
                RowLayout {
                    Layout.fillWidth: true
                    SChip { visible: root.bigPartial; text: qsTr("partial: timeout"); tone: Theme.amber; mono: false }
                    SLabel { Layout.fillWidth: true; sub: true; font.pixelSize: 11; text: qsTr("%1 files · double-click copies the path").arg(root.big.length) }
                }
            }
        }
    }
}
