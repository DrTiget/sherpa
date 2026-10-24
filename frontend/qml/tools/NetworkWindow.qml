import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// «Сеть»: интерфейсы, маршруты, DNS, порты и сокеты, firewall (ufw/nft/iptables) с откатом по таймеру.
ToolPane {
    id: root
    property string startTab: "ifaces"
    property string tab: startTab
    property var ifaces: []
    property var routes: []
    property var dns: ({})
    property var sockets: []
    property string socketFilter: ""
    property bool privSockets: false
    property var fwRules: []
    property bool fwActive: false
    property string fwBackend: ""
    property bool fwLoading: false
    property bool loading: false
    readonly property var backends: ["ufw", "nft", "iptables"].filter(b => !!tools[b])

    function ip4(a) { return (a.addr_info || []).filter(x => x.family === "inet").map(x => x.local + "/" + x.prefixlen).join(", ") }
    function ip6(a) { return (a.addr_info || []).filter(x => x.family === "inet6").map(x => x.local + "/" + x.prefixlen).join(", ") }

    function refresh() {
        loading = true
        if (tab === "ifaces") req("net.iface.list", {}, d => { ifaces = d.items || []; loading = false }, e => { errorText = e; loading = false })
        else if (tab === "routes") req("net.route.list", {}, d => {
            routes = (d.ipv4 || []).map(r => Object.assign({ fam: "IPv4" }, r)).concat((d.ipv6 || []).map(r => Object.assign({ fam: "IPv6" }, r))); loading = false
        }, e => { errorText = e; loading = false })
        else if (tab === "dns") req("net.dns.get", {}, d => { dns = d; loading = false }, e => { errorText = e; loading = false })
        else if (tab === "ports") req("net.sockets", { privileged: privSockets }, d => { sockets = d.items || []; loading = false }, e => { errorText = e; loading = false })
        else if (tab === "firewall") loadFirewall()
    }
    function loadFirewall() {
        const bl = backends || []
        if (fwBackend === "" && bl.length) fwBackend = bl[0]
        if (fwBackend === "") { loading = false; return }
        fwLoading = true
        req("net.firewall.list", { backend: fwBackend }, d => {
            fwActive = d.active === undefined ? true : !!d.active
            fwRules = flatten(fwBackend, d.items || [])
            fwLoading = false; loading = false
        }, e => { errorText = e; fwLoading = false; loading = false; fwRules = [] })
    }
    function flatten(be, items) {
        if (be === "ufw") return items.map(r => ({ id: r.id, action: r.action, dir: r.direction, src: r.source, dst: r.destination, comment: r.comment || "", rule_id: r.id }))
        if (be === "iptables") {
            const cnt = {}
            const out = []
            for (const it of items) {
                const tk = it.tokens || []
                if (tk[0] !== "-A") continue
                const key = it.table + "/" + tk[1]
                cnt[key] = (cnt[key] || 0) + 1
                const ji = tk.indexOf("-j")
                out.push({ id: cnt[key], action: ji >= 0 ? tk[ji + 1] : "", dir: tk[1], src: optv(tk, "-s") || "anywhere", dst: optv(tk, "-d") || "anywhere",
                           comment: (optv(tk, "-p") || "") + (optv(tk, "--dport") ? ":" + optv(tk, "--dport") : "") + " " + tk.join(" ").replace(/^-A \S+ ?/, ""),
                           rule_id: cnt[key], table: it.table, chain: tk[1] })
            }
            return out
        }
        // nft: нативный JSON nftables[]
        const out = []
        for (const o of items) {
            const r = o.rule
            if (!r) continue
            out.push({ id: r.handle, action: nftAction(r.expr || []), dir: r.chain, src: nftMatch(r.expr || [], "saddr") || "anywhere", dst: nftMatch(r.expr || [], "daddr") || "anywhere",
                       comment: (nftMatch(r.expr || [], "dport") ? "dport " + nftMatch(r.expr || [], "dport") : "") + (r.comment ? " " + r.comment : ""),
                       rule_id: r.handle, table: r.table, chain: r.chain })
        }
        return out
    }
    function optv(tk, o) { const i = tk.indexOf(o); return i >= 0 ? tk[i + 1] : "" }
    function nftAction(expr) { for (const e of expr) { if (e.accept !== undefined) return "accept"; if (e.drop !== undefined) return "drop"; if (e.reject !== undefined) return "reject" } return "" }
    function nftMatch(expr, field) {
        for (const e of expr) if (e.match && e.match.left && e.match.left.payload && e.match.left.payload.field === field) {
            const r = e.match.right
            return typeof r === "object" ? JSON.stringify(r) : String(r)
        }
        return ""
    }
    function actionTone(a) { return a === "allow" || a === "ALLOW" || a === "accept" || a === "ACCEPT" ? Theme.green : (a === "deny" || a === "DENY" || a === "DROP" || a === "drop" || a === "reject" || a === "REJECT" ? Theme.red : Theme.textSub) }

    function deleteRule(r) {
        const p = { backend: fwBackend, operation: "delete", rule_id: r.rule_id }
        if (r.table) p.table = r.table
        if (r.chain) p.chain = r.chain
        netFlow.apply(p, [qsTr("Delete rule #%1: %2 %3").arg(r.id).arg(r.action).arg(r.comment)], qsTr("Delete firewall rule"))
    }
    function openAddRule(port, proto) {
        ruleAction.current = "deny"; ruleProto.current = proto || "tcp"; rulePort.text = port ? String(port) : ""; ruleSrc.text = ""
        ruleErr.text = ""
        addRule.open()
    }
    function submitRule() {
        const port = parseInt(rulePort.text)
        if (!(port >= 1 && port <= 65535)) { ruleErr.text = qsTr("Port must be 1–65535"); return }
        const p = { backend: fwBackend, operation: "add", action: ruleAction.current, protocol: ruleProto.current, port: port }
        if (ruleSrc.text.trim() !== "") p.source = ruleSrc.text.trim()
        addRule.close()
        netFlow.apply(p, [qsTr("%1 incoming %2/%3 from %4").arg(ruleAction.current).arg(port).arg(ruleProto.current).arg(p.source || "0.0.0.0/0")])
    }

    onTabChanged: refresh()
    Component.onCompleted: refresh()
    onStartTabChanged: tab = startTab
    // openApp(id, args): {tab, port, proto} — например «закрыть порт» из окна безопасности
    function startWith(a) {
        if (a.tab) tab = a.tab
        if (a.port) openAddRule(a.port, a.proto)
    }

    property alias netFlow: netFlow
    NetApplyFlow { id: netFlow; pane: root; onFinished: (s) => { root.loadFirewall(); if (s === "expired") root.errorText = qsTr("Changes were not confirmed in time and have been rolled back.") } }

    // шапка
    RowLayout {
        Layout.fillWidth: true; Layout.margins: 10; Layout.bottomMargin: 6
        spacing: 10
        SSegmented {
            model: [{ value: "ifaces", text: qsTr("Interfaces") }, { value: "routes", text: qsTr("Routes") }, { value: "dns", text: "DNS" },
                    { value: "ports", text: qsTr("Ports") }, { value: "firewall", text: qsTr("Firewall") }]
            current: root.tab
            onActivated: (v) => root.tab = v
        }
        Item { Layout.fillWidth: true }
        SButton { text: qsTr("Refresh"); compact: true; onClicked: root.refresh() }
    }

    StackLayout {
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 10; Layout.topMargin: 0
        currentIndex: ["ifaces", "routes", "dns", "ports", "firewall"].indexOf(root.tab)

        DataTable {
            objectName: "ifaceTable"
            loading: root.loading
            rows: root.ifaces
            columns: [
                { key: "ifname", title: qsTr("Name"), w: 110, mono: true },
                { key: "operstate", title: qsTr("State"), w: 90, chip: true, tone: r => r.operstate === "UP" ? Theme.green : (r.operstate === "DOWN" ? Theme.red : Theme.textSub) },
                { key: "address", title: "MAC", w: 140, mono: true },
                { key: "v4", title: "IPv4", flex: 1.2, mono: true, text: r => root.ip4(r) },
                { key: "v6", title: "IPv6", flex: 1.6, mono: true, text: r => root.ip6(r) },
                { key: "mtu", title: "MTU", w: 60, right: true, mono: true }
            ]
        }
        DataTable {
            objectName: "routeTable"
            loading: root.loading
            rows: root.routes
            columns: [
                { key: "fam", title: qsTr("Family"), w: 70 },
                { key: "dst", title: qsTr("Destination"), flex: 1.4, mono: true },
                { key: "gateway", title: qsTr("Via"), flex: 1, mono: true },
                { key: "dev", title: qsTr("Device"), w: 110, mono: true },
                { key: "metric", title: qsTr("Metric"), w: 70, right: true, mono: true },
                { key: "table", title: qsTr("Table"), w: 80, mono: true }
            ]
        }
        Flickable {
            contentHeight: dnsCol.implicitHeight; clip: true
            ColumnLayout {
                id: dnsCol
                width: parent.width; spacing: 10
                Repeater {
                    model: [{ t: qsTr("Name servers"), v: root.dns.servers || [] }, { t: qsTr("Search domains"), v: root.dns.search || [] }, { t: qsTr("Options"), v: root.dns.options || [] }]
                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true; implicitHeight: c.implicitHeight + 20; radius: 8; color: Theme.card; border.color: Theme.line
                        ColumnLayout { id: c; anchors.fill: parent; anchors.margins: 10; spacing: 4
                            SLabel { caps: true; text: modelData.t }
                            SLabel { Layout.fillWidth: true; mono: true; text: modelData.v.length ? modelData.v.join("   ") : "—"; elide: Text.ElideNone; wrapMode: Text.Wrap } }
                    }
                }
                Rectangle {
                    visible: (root.dns.resolved || []).length > 0
                    Layout.fillWidth: true; implicitHeight: c2.implicitHeight + 20; radius: 8; color: Theme.card; border.color: Theme.line
                    ColumnLayout { id: c2; anchors.fill: parent; anchors.margins: 10; spacing: 4
                        SLabel { caps: true; text: "systemd-resolved" }
                        Repeater { model: root.dns.resolved || []
                            delegate: SLabel { required property var modelData; Layout.fillWidth: true; mono: true; text: modelData.link + ":  " + (modelData.servers || []).join("   ") } } }
                }
            }
        }
        ColumnLayout {
            spacing: 8
            RowLayout {
                spacing: 10
                STextField { Layout.preferredWidth: 240; placeholderText: qsTr("Filter by port, address or process"); onEdited: root.socketFilter = text }
                SCheckBox { text: qsTr("Include processes of other users (root)"); checked: root.privSockets; onToggled: { root.privSockets = checked; root.refresh() } }
            }
            DataTable {
                objectName: "socketTable"
                Layout.fillWidth: true; Layout.fillHeight: true
                loading: root.loading
                rows: root.sockets.filter(s => {
                    const f = root.socketFilter.toLowerCase()
                    return f === "" || (s.local.address + ":" + s.local.port + " " + (s.processes || []).map(p => p.name + p.pid).join(" ")).toLowerCase().indexOf(f) >= 0
                })
                actions: [{ id: "close", text: qsTr("Close port…"), kind: "danger", visible: r => r.protocol === "tcp" || r.protocol === "udp" }]
                columns: [
                    { key: "protocol", title: qsTr("Proto"), w: 60, mono: true },
                    { key: "state", title: qsTr("State"), w: 80 },
                    { key: "local", title: qsTr("Local address"), flex: 1.4, mono: true, text: r => (r.local.address || "*") + ":" + (r.local.port === null ? "*" : r.local.port),
                      sort: r => r.local.port },
                    { key: "proc", title: qsTr("Process"), flex: 1.2, mono: true, text: r => (r.processes || []).map(p => p.name + "/" + p.pid).join(", ") || "—" },
                    { key: "uid", title: "UID", w: 60, right: true, mono: true }
                ]
                onAction: (id, r) => { root.tab = "firewall"; root.openAddRule(r.local.port, r.protocol === "udp" ? "udp" : "tcp") }
            }
        }
        // firewall
        ColumnLayout {
            spacing: 8
            RowLayout {
                spacing: 10
                visible: root.backends.length > 0
                SSegmented {
                    model: root.backends.map(b => ({ value: b, text: b }))
                    current: root.fwBackend
                    onActivated: (v) => { root.fwBackend = v; root.loadFirewall() }
                }
                SChip { text: root.fwActive ? qsTr("enabled") : qsTr("disabled"); tone: root.fwActive ? Theme.green : Theme.amber; mono: false }
                Item { Layout.fillWidth: true }
                SButton { text: qsTr("+ Rule"); kind: "primary"; compact: true; enabled: !root.readonlyMode && !netFlow.pending; onClicked: root.openAddRule(0, "tcp") }
            }
            SEmptyState {
                visible: root.backends.length === 0
                Layout.fillWidth: true; Layout.fillHeight: true
                glyph: "▥"; title: qsTr("No firewall found"); description: qsTr("Neither ufw, nftables nor iptables is available on this server.")
            }
            DataTable {
                visible: root.backends.length > 0
                objectName: "fwTable"
                Layout.fillWidth: true; Layout.fillHeight: true
                loading: root.fwLoading
                rows: root.fwRules
                actions: [{ id: "del", text: qsTr("Delete…"), kind: "danger", enabled: r => !root.readonlyMode && !netFlow.pending }]
                emptyText: qsTr("No rules")
                columns: [
                    { key: "id", title: "№", w: 60, mono: true },
                    { key: "action", title: qsTr("Action"), w: 90, chip: true, tone: r => root.actionTone(r.action) },
                    { key: "dir", title: qsTr("Chain"), w: 90, mono: true },
                    { key: "src", title: qsTr("Source"), flex: 1, mono: true },
                    { key: "dst", title: qsTr("Destination"), flex: 1, mono: true },
                    { key: "comment", title: qsTr("Details"), flex: 1.6, mono: true }
                ]
                onAction: (id, r) => root.deleteRule(r)
            }
        }
    }

    // новое правило
    SDialog {
        id: addRule
        title: qsTr("New firewall rule")
        dialogWidth: 440
        onOpened: rulePort.forceFocus()
        onAcceptRequested: root.submitRule()
        SLabel { Layout.fillWidth: true; sub: true; wrapMode: Text.Wrap; elide: Text.ElideNone
                 text: qsTr("Incoming rule for backend “%1”. It is applied with an automatic rollback.").arg(root.fwBackend) }
        SSegmented { id: ruleAction; model: [{ value: "allow", text: qsTr("allow") }, { value: "deny", text: qsTr("deny") }, { value: "reject", text: qsTr("reject") }]; current: "deny" }
        SSegmented { id: ruleProto; model: [{ value: "tcp", text: "TCP" }, { value: "udp", text: "UDP" }]; current: "tcp" }
        STextField { id: rulePort; Layout.fillWidth: true; label: qsTr("Port"); mono: true; error: ruleErr.text; onAccepted: root.submitRule() }
        STextField { id: ruleSrc; Layout.fillWidth: true; label: qsTr("Source (IPv4 or CIDR, empty = any)"); mono: true; placeholderText: "0.0.0.0/0" }
        SLabel { id: ruleErr; visible: false }
        buttons: [
            Item { Layout.fillWidth: true },
            SButton { text: qsTr("Cancel"); onClicked: addRule.close() },
            SButton { text: qsTr("Apply…"); kind: "primary"; onClicked: root.submitRule() }
        ]
    }
}
