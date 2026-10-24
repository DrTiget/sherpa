import QtQuick
import QtQuick.Layouts
import Sherpa

// Виджеты на обоях: CPU (график), диски, сервер (uptime, load, RAM, ОС), алерты. Перетаскиваются,
// набор и положение хранятся в раскладке рабочего стола (desktop.state, ключ widgets).
ToolRpc {
    id: root
    property var config: ({})                   // id → {on, x, y}
    property var disks: []
    readonly property var m: desk.monitor
    readonly property var defs: [
        { id: "cpu", title: qsTr("CPU"), w: 250, h: 130, x: 0, y: 0 },
        { id: "server", title: qsTr("Server"), w: 250, h: 130, x: 0, y: 1 },
        { id: "disks", title: qsTr("Disks"), w: 250, h: 130, x: 1, y: 0 },
        { id: "alerts", title: qsTr("Alerts"), w: 250, h: 130, x: 1, y: 1 }
    ]
    signal changed(var config)

    function isOn(id) { return !!(config[id] && config[id].on) }
    function setOn(id, on) {
        const c = Object.assign({}, config)
        const d = defs.find(x => x.id === id)
        const cur = c[id] || { x: width - 270 - d.x * 270, y: 20 + d.y * 150 }
        c[id] = Object.assign({}, cur, { on: on })
        config = c
        changed(c)
    }
    function moved(id, x, y) {
        const c = Object.assign({}, config)
        c[id] = Object.assign({}, c[id], { x: x, y: y })
        config = c
        changed(c)
    }
    function refreshDisks() {
        if (!isOn("disks") || desk.state !== "ready") return
        req("disk.list", {}, d => {
            disks = (d.items || []).filter(p => p.mount && (p.mount === "/" || p.mount.indexOf("/mnt") === 0 || p.mount.indexOf("/home") === 0 || p.mount.indexOf("/var") === 0 || p.mount.indexOf("/srv") === 0 || p.mount.indexOf("/data") === 0)
                                              && p.total_bytes > 0).slice(0, 5)
        }, () => {})
    }
    readonly property var alerts: {
        const l = []
        if (desk.state !== "ready") l.push({ text: qsTr("No connection to the server"), tone: Theme.red })
        const mem = m.memory ? m.memory.usage_pct : 0
        if (mem > 90) l.push({ text: qsTr("Memory %1%").arg(mem.toFixed(0)), tone: Theme.red })
        const cpu = m.cpu ? m.cpu.usage_pct : 0
        if (cpu > 90) l.push({ text: qsTr("CPU %1%").arg(cpu.toFixed(0)), tone: Theme.amber })
        if (m.load && m.cpu && m.cpu.cores && m.load["5m"] > m.cpu.cores * 1.5) l.push({ text: qsTr("High load %1").arg(m.load["5m"]), tone: Theme.amber })
        for (const d of disks) if (d.usage_pct > 90) l.push({ text: qsTr("Disk %1 is %2% full").arg(d.mount).arg(d.usage_pct.toFixed(0)), tone: Theme.red })
        return l
    }
    function fmtUp(s) {
        if (!s && s !== 0) return "—"
        const d = Math.floor(s / 86400), h = Math.floor(s % 86400 / 3600), mi = Math.floor(s % 3600 / 60)
        return (d ? d + qsTr("d ") : "") + h + qsTr("h ") + mi + qsTr("m")
    }

    Timer { interval: 60000; repeat: true; running: root.desk.state === "ready" && root.isOn("disks"); triggeredOnStart: true; onTriggered: root.refreshDisks() }
    onConfigChanged: refreshDisks()

    Repeater {
        model: root.defs
        delegate: Rectangle {
            id: card
            required property var modelData
            readonly property var cfg: root.config[modelData.id]
            visible: root.isOn(modelData.id)
            objectName: "widget-" + modelData.id
            x: cfg ? Math.max(0, Math.min(cfg.x, root.width - width)) : 0
            y: cfg ? Math.max(0, Math.min(cfg.y, root.height - height)) : 0
            width: modelData.w; height: modelData.h
            radius: Theme.radiusLarge
            color: Theme.alpha(Theme.dialogFill, 0.78)
            border.color: Theme.alpha(Theme.cyan, 0.35)
            SLabel { x: 12; y: 8; caps: true; text: modelData.title }
            MouseArea {
                anchors.fill: parent
                drag.target: card; drag.minimumX: 0; drag.minimumY: 0; drag.maximumX: root.width - card.width; drag.maximumY: root.height - card.height
                onReleased: root.moved(card.modelData.id, card.x, card.y)
                cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                z: -1
            }
            SLabel { anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 8; text: "×"; font.pixelSize: 16; sub: true
                     MouseArea { anchors.fill: parent; anchors.margins: -5; onClicked: root.setOn(card.modelData.id, false) } }
            Loader {
                anchors.fill: parent; anchors.topMargin: 28; anchors.margins: 12
                sourceComponent: card.modelData.id === "cpu" ? cpuC : (card.modelData.id === "server" ? srvC : (card.modelData.id === "disks" ? diskC : alertC))
            }
        }
    }
    Component {
        id: cpuC
        Item {
            SLabel { anchors.right: parent.right; anchors.top: parent.top; mono: true; font.pixelSize: 20; font.bold: true; color: Theme.cyan
                     text: (root.m.cpu && root.m.cpu.usage_pct !== undefined && root.m.cpu.usage_pct !== null ? root.m.cpu.usage_pct.toFixed(0) : "—") + "%" }
            Canvas {
                id: cv
                anchors.fill: parent; anchors.topMargin: 28
                property var vals: root.desk.cpuHistory
                onValsChanged: requestPaint()
                onPaint: {
                    const c = getContext("2d"); c.reset()
                    const v = vals
                    if (!v || v.length < 2) return
                    const step = width / 89, x0 = width - (v.length - 1) * step
                    c.beginPath()
                    for (let i = 0; i < v.length; ++i) { const x = x0 + i * step, y = height - Math.min(1, v[i] / 100) * (height - 2) - 1; i === 0 ? c.moveTo(x, y) : c.lineTo(x, y) }
                    c.strokeStyle = Theme.cyan; c.lineWidth = 1.5; c.stroke()
                    c.lineTo(x0 + (v.length - 1) * step, height); c.lineTo(x0, height); c.closePath()
                    c.fillStyle = Theme.alpha(Theme.cyan, 0.16); c.fill()
                }
            }
        }
    }
    Component {
        id: srvC
        ColumnLayout {
            spacing: 3
            SLabel { Layout.fillWidth: true; font.bold: true; text: root.desk.capabilities && root.desk.capabilities.os ? root.desk.capabilities.os.name || root.desk.capabilities.os.id : "—" }
            SLabel { Layout.fillWidth: true; mono: true; font.pixelSize: 11; text: qsTr("up") + " " + root.fmtUp(root.m.uptime_sec) }
            SLabel { Layout.fillWidth: true; mono: true; font.pixelSize: 11; text: root.m.load ? qsTr("load") + " " + root.m.load["1m"] + "  " + root.m.load["5m"] + "  " + root.m.load["15m"] : "" }
            SLabel { Layout.fillWidth: true; mono: true; font.pixelSize: 11
                     text: root.m.memory ? qsTr("RAM") + " " + (root.m.memory.usage_pct || 0).toFixed(0) + "%" : "" }
        }
    }
    Component {
        id: diskC
        ColumnLayout {
            spacing: 5
            Repeater {
                model: root.disks
                delegate: ColumnLayout {
                    required property var modelData
                    Layout.fillWidth: true; spacing: 1
                    RowLayout { SLabel { Layout.fillWidth: true; mono: true; font.pixelSize: 11; text: modelData.mount }
                                SLabel { mono: true; font.pixelSize: 11; text: modelData.usage_pct.toFixed(0) + "%" } }
                    Rectangle { Layout.fillWidth: true; height: 5; radius: 2.5; color: Theme.alpha(Theme.text, 0.1)
                                Rectangle { height: parent.height; radius: 2.5; width: parent.width * Math.min(1, modelData.usage_pct / 100)
                                            color: modelData.usage_pct > 90 ? Theme.red : (modelData.usage_pct > 75 ? Theme.amber : Theme.green) } }
                }
            }
            SLabel { visible: root.disks.length === 0; sub: true; text: qsTr("Loading…") }
        }
    }
    Component {
        id: alertC
        ColumnLayout {
            spacing: 4
            Repeater {
                model: root.alerts
                delegate: RowLayout { required property var modelData; Layout.fillWidth: true; spacing: 6
                    Rectangle { width: 8; height: 8; radius: 4; color: modelData.tone }
                    SLabel { Layout.fillWidth: true; font.pixelSize: 12; text: modelData.text } }
            }
            SLabel { visible: root.alerts.length === 0; color: Theme.green; text: qsTr("All is well") }
        }
    }
}
