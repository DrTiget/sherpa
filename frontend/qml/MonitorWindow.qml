import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// «Монитор ресурсов»: графики CPU/RAM/load, память и swap, таблица процессов (данные monitor.subscribe).
Item {
    id: root
    required property var desktop
    readonly property var m: desktop.monitor
    readonly property bool has: !!m.cpu
    property string sortKey: "cpu_pct"
    property bool sortDesc: true
    property string filter: ""

    function fmtBytes(b) {
        if (b === undefined || b === null) return "—"
        const u = ["B", "KB", "MB", "GB", "TB"]
        let i = 0, v = b
        while (v >= 1024 && i < 4) { v /= 1024; i++ }
        return v.toFixed(i > 1 ? 1 : 0) + " " + u[i]
    }
    function fmtUptime(s) {
        if (!s && s !== 0) return "—"
        const d = Math.floor(s / 86400), h = Math.floor(s % 86400 / 3600), mi = Math.floor(s % 3600 / 60)
        return (d ? d + qsTr("d ") : "") + h + qsTr("h ") + mi + qsTr("m")
    }
    function procs() {
        const list = (m.processes || []).slice()
        const f = filter.toLowerCase()
        const out = f === "" ? list : list.filter(p => (p.command || "").toLowerCase().indexOf(f) >= 0 || (p.user || "").toLowerCase().indexOf(f) >= 0 || String(p.pid) === f)
        const k = sortKey, d = sortDesc ? -1 : 1
        out.sort((a, b) => (typeof a[k] === "string" ? a[k].localeCompare(b[k]) : (a[k] - b[k])) * d)
        return out
    }
    property var rows: has ? procs() : []
    onMChanged: rows = has ? procs() : []
    onSortKeyChanged: rows = has ? procs() : []
    onSortDescChanged: rows = has ? procs() : []
    onFilterChanged: rows = has ? procs() : []

    // график-линия
    component Chart: Canvas {
        id: ch
        property var values: []
        property color line: Theme.cyan
        property real maxV: 100
        onValuesChanged: requestPaint()
        onPaint: {
            const c = getContext("2d")
            c.reset()
            c.strokeStyle = Theme.alpha(Theme.text, 0.08)
            c.lineWidth = 1
            for (let i = 1; i < 4; ++i) { c.beginPath(); c.moveTo(0, height * i / 4); c.lineTo(width, height * i / 4); c.stroke() }
            const v = values
            if (!v || v.length < 2) return
            const n = 90, step = width / (n - 1)
            const x0 = width - (v.length - 1) * step
            c.beginPath()
            for (let i = 0; i < v.length; ++i) {
                const x = x0 + i * step, y = height - Math.min(1, v[i] / maxV) * (height - 2) - 1
                i === 0 ? c.moveTo(x, y) : c.lineTo(x, y)
            }
            c.strokeStyle = line
            c.lineWidth = 1.6
            c.stroke()
            c.lineTo(x0 + (v.length - 1) * step, height); c.lineTo(x0, height); c.closePath()
            c.fillStyle = Theme.alpha(line, 0.16)
            c.fill()
        }
    }
    component Meter: Item {
        property real pct: 0
        property color tone: Theme.green
        implicitHeight: 8
        Rectangle { anchors.fill: parent; radius: 4; color: Theme.alpha(Theme.text, 0.1) }
        Rectangle { width: parent.width * Math.min(1, pct / 100); height: parent.height; radius: 4; color: tone }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 10
        visible: root.has

        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            // CPU
            Rectangle {
                Layout.fillWidth: true; Layout.preferredWidth: 1; Layout.preferredHeight: 120
                radius: 8; color: Theme.card; border.color: Theme.line
                SLabel { x: 10; y: 8; caps: true; text: "CPU" }
                SLabel { anchors.right: parent.right; anchors.rightMargin: 10; y: 6; mono: true; font.pixelSize: 16; font.bold: true; color: Theme.cyan
                         text: (m.cpu && m.cpu.usage_pct !== null && m.cpu.usage_pct !== undefined ? m.cpu.usage_pct.toFixed(0) : "—") + "%" }
                Chart { x: 8; y: 30; width: parent.width - 16; height: parent.height - 38; values: root.desktop.cpuHistory; line: Theme.cyan }
            }
            // RAM
            Rectangle {
                Layout.fillWidth: true; Layout.preferredWidth: 1; Layout.preferredHeight: 120
                radius: 8; color: Theme.card; border.color: Theme.line
                SLabel { x: 10; y: 8; caps: true; text: qsTr("Memory") }
                SLabel { anchors.right: parent.right; anchors.rightMargin: 10; y: 6; mono: true; font.pixelSize: 16; font.bold: true; color: Theme.purple
                         text: (m.memory && m.memory.usage_pct !== null && m.memory.usage_pct !== undefined ? m.memory.usage_pct.toFixed(0) : "—") + "%" }
                Chart { x: 8; y: 30; width: parent.width - 16; height: parent.height - 38; values: root.desktop.memHistory; line: Theme.purple }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            // ядра
            Rectangle {
                Layout.fillWidth: true; Layout.preferredWidth: 1
                implicitHeight: coreCol.implicitHeight + 20
                radius: 8; color: Theme.card; border.color: Theme.line
                ColumnLayout {
                    id: coreCol
                    anchors.fill: parent; anchors.margins: 10; spacing: 5
                    SLabel { caps: true; text: qsTr("Cores: %1").arg(m.cpu ? m.cpu.cores : "—") }
                    Repeater {
                        model: m.cpu && m.cpu.per_core ? m.cpu.per_core.slice(0, 8) : []
                        delegate: RowLayout {
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            SLabel { text: index; mono: true; sub: true; font.pixelSize: 11; Layout.preferredWidth: 16 }
                            Meter { Layout.fillWidth: true; pct: modelData; tone: modelData > 85 ? Theme.red : (modelData > 60 ? Theme.amber : Theme.cyan) }
                            SLabel { text: modelData.toFixed(0) + "%"; mono: true; font.pixelSize: 11; Layout.preferredWidth: 34; horizontalAlignment: Text.AlignRight }
                        }
                    }
                }
            }
            // память, swap, load
            Rectangle {
                Layout.fillWidth: true; Layout.preferredWidth: 1
                implicitHeight: memCol.implicitHeight + 20
                radius: 8; color: Theme.card; border.color: Theme.line
                ColumnLayout {
                    id: memCol
                    anchors.fill: parent; anchors.margins: 10; spacing: 6
                    SLabel { caps: true; text: qsTr("Memory") }
                    Meter { Layout.fillWidth: true; pct: m.memory ? (m.memory.usage_pct || 0) : 0; tone: Theme.purple }
                    SLabel { mono: true; font.pixelSize: 11; text: m.memory ? root.fmtBytes(m.memory.used_bytes) + " / " + root.fmtBytes(m.memory.total_bytes) : "" }
                    SLabel { caps: true; text: "Swap"; Layout.topMargin: 4 }
                    Meter { Layout.fillWidth: true; pct: m.swap ? (m.swap.usage_pct || 0) : 0; tone: Theme.amber }
                    SLabel { mono: true; font.pixelSize: 11; text: m.swap ? root.fmtBytes(m.swap.used_bytes) + " / " + root.fmtBytes(m.swap.total_bytes) : "" }
                    SLabel { mono: true; font.pixelSize: 11; Layout.topMargin: 4
                             text: m.load ? qsTr("load") + " " + m.load["1m"] + "  " + m.load["5m"] + "  " + m.load["15m"] + "   ·   " + qsTr("up") + " " + root.fmtUptime(m.uptime_sec) : "" }
                }
            }
        }

        // таблица процессов
        RowLayout {
            Layout.fillWidth: true
            SLabel { caps: true; text: qsTr("Processes"); Layout.fillWidth: true }
            STextField { Layout.preferredWidth: 220; placeholderText: qsTr("Filter"); onEdited: root.filter = text; Accessible.name: qsTr("Filter processes") }
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            radius: 8; color: Theme.inputFill; border.color: Theme.line
            clip: true
            Row {
                id: head
                width: parent.width
                height: 24
                Repeater {
                    model: [{ k: "pid", t: "PID", w: 60 }, { k: "user", t: qsTr("User"), w: 112 }, { k: "cpu_pct", t: "CPU%", w: 62 },
                            { k: "mem_pct", t: "MEM%", w: 62 }, { k: "rss_bytes", t: "RSS", w: 80 }, { k: "command", t: qsTr("Command"), w: 0 }]
                    delegate: Rectangle {
                        required property var modelData
                        width: modelData.w ? modelData.w : head.width - 376
                        height: 24
                        color: Theme.alpha(Theme.panelFill, 0.8)
                        SLabel { x: 8; anchors.verticalCenter: parent.verticalCenter; caps: true; text: modelData.t + (root.sortKey === modelData.k ? (root.sortDesc ? " ▼" : " ▲") : "") }
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                                    onClicked: { if (root.sortKey === modelData.k) root.sortDesc = !root.sortDesc; else { root.sortKey = modelData.k; root.sortDesc = modelData.k !== "user" && modelData.k !== "command" } } }
                    }
                }
            }
            ListView {
                anchors.top: head.bottom
                anchors.bottom: parent.bottom
                width: parent.width
                clip: true
                model: root.rows
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    required property var modelData
                    required property int index
                    width: ListView.view.width
                    height: 22
                    color: index % 2 ? Theme.alpha(Theme.text, 0.03) : "transparent"
                    Row {
                        anchors.fill: parent
                        SLabel { width: 60; leftPadding: 8; mono: true; font.pixelSize: 11; text: modelData.pid; anchors.verticalCenter: parent.verticalCenter }
                        SLabel { width: 112; leftPadding: 8; mono: true; font.pixelSize: 11; text: modelData.user; anchors.verticalCenter: parent.verticalCenter }
                        SLabel { width: 62; leftPadding: 8; mono: true; font.pixelSize: 11; text: modelData.cpu_pct.toFixed(1); color: modelData.cpu_pct > 50 ? Theme.red : Theme.text; anchors.verticalCenter: parent.verticalCenter }
                        SLabel { width: 62; leftPadding: 8; mono: true; font.pixelSize: 11; text: modelData.mem_pct.toFixed(1); anchors.verticalCenter: parent.verticalCenter }
                        SLabel { width: 80; leftPadding: 8; mono: true; font.pixelSize: 11; text: root.fmtBytes(modelData.rss_bytes); anchors.verticalCenter: parent.verticalCenter }
                        SLabel { width: parent.width - 376; leftPadding: 8; mono: true; font.pixelSize: 11; text: modelData.command; anchors.verticalCenter: parent.verticalCenter }
                    }
                }
            }
        }
    }

    // скелетон / ошибка
    ColumnLayout {
        visible: !root.has
        anchors.centerIn: parent
        spacing: 10
        SSpinner { Layout.alignment: Qt.AlignHCenter; width: 22; height: 22; visible: root.desktop.state === "ready" && root.desktop.capabilities.utilities && root.desktop.capabilities.utilities.monitor !== false }
        SLabel {
            Layout.alignment: Qt.AlignHCenter
            sub: true
            text: root.desktop.state !== "ready" ? qsTr("Waiting for the server…")
                  : (root.desktop.capabilities.utilities && root.desktop.capabilities.utilities.monitor === false ? qsTr("Not available on this server") : qsTr("Collecting data…"))
        }
    }
}
