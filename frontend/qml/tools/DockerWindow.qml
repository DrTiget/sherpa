import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Docker: контейнеры (с CPU/RAM), образы, тома, сети; журнал контейнера потоком; инспекция.
ToolPane {
    id: root
    property string tab: "containers"
    property var containers: []
    property var images: []
    property var volumes: []
    property var networks: []
    property var stats: ({})                    // имя → {cpu_pct, memory_used_bytes, ...}
    property string statsSub: ""
    property string logSub: ""
    property string panel: ""                   // "" | logs | inspect
    property string panelTitle: ""
    property string unavailable: ""             // причина, если Docker недоступен
    property bool loading: false
    property alias panelText: panelText

    function stateTone(s) { return s === "running" ? Theme.green : (s === "paused" ? Theme.amber : (s === "exited" || s === "dead" ? Theme.red : Theme.textSub)) }
    function cname(c) { return (c.Names || c.ID || "").split(",")[0] }
    function fail(e, d) {
        loading = false
        unavailable = e
    }
    function refresh() {
        loading = true
        const done = () => { loading = false; unavailable = "" }
        if (tab === "containers") req("docker.ps", {}, d => { containers = d.items || []; done(); subscribeStats() }, fail)
        else if (tab === "images") req("docker.images", {}, d => { images = d.items || []; done() }, fail)
        else if (tab === "volumes") req("docker.volumes", {}, d => { volumes = d.items || []; done() }, fail)
        else if (tab === "networks") req("docker.networks", {}, d => { networks = d.items || []; done() }, fail)
    }
    function subscribeStats() {
        if (statsSub !== "") return
        req("docker.stats", { subscribe: true, interval_ms: 3000 }, d => { statsSub = d.subscription || "" }, () => {})
    }
    function unsubscribe(sub) { if (sub !== "") call("docker.unsubscribe", { subscription: sub }, () => {}, () => {}) }
    function openLogs(c) {
        closePanel()
        panel = "logs"; panelTitle = cname(c)
        panelText.text = ""
        req("docker.logs", { id: c.ID, tail: 200, follow: true }, d => { logSub = d.subscription || ""; if (d.lines) panelText.text = d.lines.join("\n") },
            e => { panelText.text = e })
    }
    function openInspect(c) {
        closePanel()
        panel = "inspect"; panelTitle = cname(c)
        panelText.text = ""
        req("docker.inspect", { id: c.ID }, d => { panelText.text = JSON.stringify((d.items || [])[0] || d, null, 2) }, e => { panelText.text = e })
    }
    function closePanel() {
        if (logSub !== "") { unsubscribe(logSub); logSub = "" }
        panel = ""
    }
    function act(kind, c) {
        const name = cname(c)
        const titles = { start: qsTr("Start container"), stop: qsTr("Stop container"), restart: qsTr("Restart container"), pause: qsTr("Pause container"), rm: qsTr("Remove container") }
        const msg = kind === "rm" ? qsTr("The container will be removed. Data in anonymous volumes may be lost.") : ""
        dangerousReq({ title: titles[kind], message: msg, items: [name + "  (" + c.Image + ")"], confirmText: titles[kind],
                       needName: prod && (kind === "stop" || kind === "rm" || kind === "restart") },
                     "docker." + kind, { id: c.ID }, () => refresh())
    }

    onTabChanged: { current = -1; refresh() }
    property int current: -1
    Component.onCompleted: refresh()
    Component.onDestruction: { unsubscribe(statsSub); unsubscribe(logSub) }
    onEvent: (ev) => {
        if (ev.event === "docker.stats" && ev.subscription === statsSub) {
            const m = {}
            for (const s of (ev.data && ev.data.items) || []) m[s.name] = s
            stats = m
        } else if (ev.event === "docker.logs" && ev.subscription === logSub) {
            panelText.text += ToolUtil.fromBase64(ev.data_b64)
            if (panelText.text.length > 200000) panelText.text = panelText.text.slice(-150000)
            panelText.area.cursorPosition = panelText.text.length
        } else if (ev.event === "docker.logs.done" && ev.subscription === logSub) {
            logSub = ""
        } else if (ev.event === "error" && ev.subscription === statsSub) {
            statsSub = ""
        }
    }

    RowLayout {
        Layout.fillWidth: true; Layout.margins: 10; Layout.bottomMargin: 6
        SSegmented {
            model: [{ value: "containers", text: qsTr("Containers") }, { value: "images", text: qsTr("Images") },
                    { value: "volumes", text: qsTr("Volumes") }, { value: "networks", text: qsTr("Networks") }]
            current: root.tab
            onActivated: (v) => root.tab = v
        }
        Item { Layout.fillWidth: true }
        SButton { text: qsTr("Refresh"); compact: true; onClicked: root.refresh() }
    }
    SEmptyState {
        visible: root.unavailable !== ""
        Layout.fillWidth: true; Layout.fillHeight: true
        glyph: "◫"; title: qsTr("Docker is not available"); description: root.unavailable
        primaryText: qsTr("Retry"); onPrimary: root.refresh()
    }
    StackLayout {
        visible: root.unavailable === ""
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 10; Layout.topMargin: 0
        currentIndex: ["containers", "images", "volumes", "networks"].indexOf(root.tab)
        ColumnLayout {
            spacing: 8
            DataTable {
                objectName: "containerTable"
                Layout.fillWidth: true; Layout.fillHeight: true
                loading: root.loading
                rows: root.containers
                actions: [
                    { id: "start", text: qsTr("Start"), visible: r => r.State !== "running" && r.State !== "paused", enabled: r => !root.readonlyMode },
                    { id: "stop", text: qsTr("Stop"), visible: r => r.State === "running", enabled: r => !root.readonlyMode },
                    { id: "restart", text: qsTr("Restart"), visible: r => r.State === "running", enabled: r => !root.readonlyMode },
                    { id: "pause", text: qsTr("Pause"), visible: r => r.State === "running", enabled: r => !root.readonlyMode },
                    { id: "logs", text: qsTr("Logs") },
                    { id: "inspect", text: qsTr("Inspect") },
                    { id: "rm", text: qsTr("Remove…"), kind: "danger", enabled: r => !root.readonlyMode }
                ]
                columns: [
                    { key: "Names", title: qsTr("Name"), flex: 1, mono: true },
                    { key: "Image", title: qsTr("Image"), flex: 1.2, mono: true },
                    { key: "State", title: qsTr("State"), w: 86, chip: true, tone: r => root.stateTone(r.State) },
                    { key: "Status", title: qsTr("Status"), w: 130 },
                    { key: "Ports", title: qsTr("Ports"), flex: 1.1, mono: true, text: r => r.Ports || "—" },
                    { key: "cpu", title: "CPU", w: 60, right: true, mono: true,
                      text: r => root.stats[root.cname(r)] ? root.stats[root.cname(r)].cpu_pct.toFixed(1) + "%" : "—",
                      sort: r => root.stats[root.cname(r)] ? root.stats[root.cname(r)].cpu_pct : -1 },
                    { key: "mem", title: "RAM", w: 80, right: true, mono: true,
                      text: r => root.stats[root.cname(r)] ? root.fmtBytes(root.stats[root.cname(r)].memory_used_bytes) : "—",
                      sort: r => root.stats[root.cname(r)] ? root.stats[root.cname(r)].memory_used_bytes : -1 }
                ]
                onAction: (id, r) => { if (id === "logs") root.openLogs(r); else if (id === "inspect") root.openInspect(r); else root.act(id, r) }
            }
            // панель журнала / инспекции
            Rectangle {
                visible: root.panel !== ""
                Layout.fillWidth: true; Layout.preferredHeight: 220
                radius: 8; color: Theme.card; border.color: Theme.line
                ColumnLayout {
                    anchors.fill: parent; anchors.margins: 8; spacing: 6
                    RowLayout {
                        SLabel { caps: true; text: (root.panel === "logs" ? qsTr("Logs") : qsTr("Inspect")) + " · " + root.panelTitle; Layout.fillWidth: true }
                        SSpinner { visible: root.logSub !== ""; width: 12; height: 12 }
                        SButton { compact: true; text: qsTr("Copy"); onClicked: ToolUtil.copy(panelText.text) }
                        SButton { compact: true; text: qsTr("Close"); onClicked: root.closePanel() }
                    }
                    STextArea { id: panelText; objectName: "dockerPanelText"; Layout.fillWidth: true; Layout.fillHeight: true; readOnly: true; wrap: root.panel === "logs" }
                }
            }
        }
        DataTable {
            loading: root.loading; rows: root.images
            columns: [
                { key: "Repository", title: qsTr("Repository"), flex: 1.4, mono: true },
                { key: "Tag", title: qsTr("Tag"), w: 110, mono: true },
                { key: "ID", title: "ID", w: 120, mono: true },
                { key: "Size", title: qsTr("Size"), w: 90, right: true, mono: true },
                { key: "CreatedSince", title: qsTr("Created"), w: 130 }
            ]
        }
        DataTable {
            loading: root.loading; rows: root.volumes
            columns: [
                { key: "Name", title: qsTr("Name"), flex: 1.4, mono: true },
                { key: "Driver", title: qsTr("Driver"), w: 90 },
                { key: "Mountpoint", title: qsTr("Mountpoint"), flex: 1.6, mono: true }
            ]
        }
        DataTable {
            loading: root.loading; rows: root.networks
            columns: [
                { key: "Name", title: qsTr("Name"), flex: 1.4, mono: true },
                { key: "Driver", title: qsTr("Driver"), w: 90 },
                { key: "Scope", title: qsTr("Scope"), w: 80 },
                { key: "ID", title: "ID", w: 130, mono: true }
            ]
        }
    }
}
