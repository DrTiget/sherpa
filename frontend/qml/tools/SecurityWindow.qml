import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Безопасность: оценка, fail2ban, входы и сессии, SSH-ключи, открытые порты, security-обновления.
ToolPane {
    id: root
    property string tab: "overview"
    property var score: ({})
    property var jails: []
    property string jail: ""
    property var bans: []
    property var logins: []
    property var sessions: []
    property var keys: []
    property var ports: []
    property var updates: []
    property bool loading: false
    readonly property var fwBackends: ["ufw", "nft", "iptables"].filter(b => !!tools[b])

    function fail(e) { errorText = e; loading = false }
    function refresh() {
        loading = true
        const ok = () => { loading = false }
        if (tab === "overview") req("sec.score", {}, d => { score = d; ok() }, fail)
        else if (tab === "f2b") req("sec.fail2ban.jails", {}, d => {
            jails = d.items || []
            if (jails.indexOf(jail) < 0) jail = jails.length ? jails[0] : ""
            loadBans()
        }, e => { jails = []; bans = []; fail(e) })
        else if (tab === "logins") {
            req("sec.logins", {}, d => { logins = d.items || []; ok() }, fail)
            req("sec.sessions", {}, d => { sessions = d.items || [] }, () => {})
        }
        else if (tab === "keys") req("sec.keys.list", {}, d => { keys = d.items || []; ok() }, fail)
        else if (tab === "ports") req("sec.ports", {}, d => { ports = d.items || []; ok() }, fail)
        else if (tab === "updates") req("sec.updates", {}, d => { updates = d.items || []; ok() }, fail)
    }
    function loadBans() {
        if (jail === "") { bans = []; loading = false; return }
        req("sec.fail2ban.bans", { jail: jail }, d => { bans = (d.items || []).map(ip => ({ ip: ip })); loading = false }, fail)
    }
    function unban(r) {
        dangerous({ title: qsTr("Unban IP"), items: [r.ip + " (" + jail + ")"], confirmText: qsTr("Unban") }, "sec.fail2ban.unban",
                  { connection_id: serverId, jail: jail, ip: r.ip }, () => loadBans())
    }
    function ban(ip, j) {
        const jj = j || jail
        if (jj === "") { errorText = qsTr("No fail2ban jail is available"); return }
        dangerous({ title: qsTr("Ban IP"), items: [ip + " (" + jj + ")"], confirmText: qsTr("Ban") }, "sec.fail2ban.ban",
                  { connection_id: serverId, jail: jj, ip: ip }, () => { if (tab === "f2b") loadBans(); else refresh() })
    }
    function delKey(k) {
        dangerous({ title: qsTr("Delete SSH key"), message: qsTr("The key is removed from authorized_keys; the owner can no longer sign in with it."),
                    items: [(k.user || "?") + "  " + k.key_type + "  " + k.fingerprint + "  " + (k.comment || "")], confirmText: qsTr("Delete"),
                    needName: prod || k.user === "root" }, "sec.keys.del", { connection_id: serverId, user: k.user, fingerprint: k.fingerprint }, () => refresh())
    }
    function addKey() {
        const u = keyUser.text.trim(), k = keyText.text.trim()
        if (u === "" || k === "") { keyErr.text = qsTr("User and key are required"); return }
        keyErr.text = ""
        addKeyDlg.close()
        dangerous({ title: qsTr("Add SSH key"), items: [u + "  " + k.slice(0, 60) + "…"], confirmText: qsTr("Add"), needName: prod || u === "root" },
                  "sec.keys.add", { connection_id: serverId, user: u, public_key: k }, () => refresh())
    }
    function closePort(r) {
        if (fwBackends.length === 0) { errorText = qsTr("No firewall (ufw, nftables, iptables) is available"); return }
        netFlow.apply({ backend: fwBackends[0], operation: "add", action: "deny", protocol: r.protocol === "udp" ? "udp" : "tcp", port: r.port },
                      [qsTr("deny incoming %1/%2 (%3)").arg(r.port).arg(r.protocol).arg(fwBackends[0])], qsTr("Close port %1").arg(r.port))
    }
    function riskTone(r) { return r.risk === "high" ? Theme.red : Theme.amber }
    function scoreTone() { const s = score.score; return s >= 80 ? Theme.green : (s >= 50 ? Theme.amber : Theme.red) }
    readonly property var recoText: ({
        enable_fail2ban: qsTr("Install and enable fail2ban with an sshd jail"),
        install_security_updates: qsTr("Install pending security updates"),
        replace_weak_keys: qsTr("Replace weak SSH keys (DSA or RSA shorter than 2048 bits)"),
        review_listeners: qsTr("Review database/Redis ports reachable from outside and close them in the firewall")
    })
    readonly property var checkText: ({
        fail2ban_jails: qsTr("fail2ban jails"), security_updates: qsTr("Security updates pending"),
        weak_keys: qsTr("Weak SSH keys"), high_risk_listeners: qsTr("High-risk open ports")
    })

    onTabChanged: refresh()
    onJailChanged: if (tab === "f2b") loadBans()
    Component.onCompleted: refresh()
    NetApplyFlow { id: netFlow; pane: root; onFinished: (s) => { root.refresh(); if (s === "expired") root.errorText = qsTr("Changes were not confirmed in time and have been rolled back.") } }

    RowLayout {
        Layout.fillWidth: true; Layout.margins: 10; Layout.bottomMargin: 6
        SSegmented {
            model: [{ value: "overview", text: qsTr("Overview") }, { value: "f2b", text: "fail2ban" }, { value: "logins", text: qsTr("Logins") },
                    { value: "keys", text: qsTr("Keys") }, { value: "ports", text: qsTr("Ports") }, { value: "updates", text: qsTr("Updates") }]
            current: root.tab
            onActivated: (v) => root.tab = v
        }
        Item { Layout.fillWidth: true }
        SButton { text: qsTr("Refresh"); compact: true; onClicked: root.refresh() }
    }

    StackLayout {
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 10; Layout.topMargin: 0
        currentIndex: ["overview", "f2b", "logins", "keys", "ports", "updates"].indexOf(root.tab)

        // обзор
        Flickable {
            contentHeight: ovCol.implicitHeight; clip: true
            ColumnLayout {
                id: ovCol
                width: parent.width; spacing: 12
                RowLayout {
                    spacing: 20
                    Rectangle {
                        width: 110; height: 110; radius: 55; color: "transparent"; border.width: 8; border.color: Theme.alpha(root.scoreTone(), 0.35)
                        SLabel { anchors.centerIn: parent; text: root.score.score === undefined ? "—" : root.score.score; font.pixelSize: 34; font.bold: true; color: root.scoreTone() }
                        objectName: "scoreCircle"
                    }
                    ColumnLayout {
                        spacing: 4
                        SLabel { font.pixelSize: 16; font.bold: true; text: qsTr("Security score") }
                        SLabel { sub: true; text: qsTr("A heuristic estimate, not a compliance check.") }
                    }
                }
                SLabel { caps: true; text: qsTr("Checks") }
                Repeater {
                    model: Object.keys(root.score.checks || {})
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        SLabel { Layout.fillWidth: true; text: root.checkText[modelData] || modelData }
                        SLabel { mono: true; text: root.score.checks[modelData] }
                    }
                }
                SLabel { caps: true; text: qsTr("Recommendations"); Layout.topMargin: 6 }
                Repeater {
                    model: Object.keys(root.score.recommendations || {}).filter(k => root.score.recommendations[k])
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true; spacing: 8
                        Rectangle { width: 8; height: 8; radius: 4; color: Theme.amber; Layout.alignment: Qt.AlignTop; Layout.topMargin: 5 }
                        SLabel { Layout.fillWidth: true; text: root.recoText[modelData] || modelData; wrapMode: Text.Wrap; elide: Text.ElideNone }
                    }
                }
                SLabel { visible: root.score.recommendations && Object.keys(root.score.recommendations).every(k => !root.score.recommendations[k]); color: Theme.green; text: qsTr("No recommendations") }
            }
        }
        // fail2ban
        ColumnLayout {
            spacing: 8
            RowLayout {
                spacing: 10
                SSelect { visible: root.jails.length > 0; Layout.preferredWidth: 200; current: root.jail; model: root.jails.map(j => ({ value: j, text: j })); onActivated: (v) => root.jail = v }
                Item { Layout.fillWidth: true }
                SButton { compact: true; text: qsTr("Ban IP…"); enabled: root.jail !== "" && !root.readonlyMode; onClicked: { banIp.text = ""; banDlg.open() } }
            }
            DataTable {
                objectName: "banTable"
                Layout.fillWidth: true; Layout.fillHeight: true
                loading: root.loading; rows: root.bans
                actions: [{ id: "unban", text: qsTr("Unban…"), enabled: r => !root.readonlyMode }]
                emptyText: root.jails.length ? qsTr("No banned addresses") : qsTr("fail2ban is not available or has no jails")
                columns: [{ key: "ip", title: qsTr("Banned IP"), flex: 1, mono: true }]
                onAction: (id, r) => root.unban(r)
            }
        }
        // входы
        ColumnLayout {
            spacing: 8
            SLabel { caps: true; text: qsTr("Failed sign-ins in the last 24 hours") }
            DataTable {
                objectName: "loginTable"
                Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredHeight: 2
                loading: root.loading; rows: root.logins
                actions: [{ id: "ban", text: qsTr("Ban…"), visible: r => !!r.ip, enabled: r => !root.readonlyMode && root.jails.length > 0 }]
                emptyText: qsTr("No failed sign-ins")
                columns: [{ key: "ip", title: "IP", w: 150, mono: true }, { key: "message", title: qsTr("Message"), flex: 1, mono: true }]
                onAction: (id, r) => root.ban(r.ip, root.jails[0])
            }
            SLabel { caps: true; text: qsTr("Active sessions") }
            DataTable {
                Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredHeight: 1
                rows: root.sessions; compactRows: true
                emptyText: qsTr("No active sessions")
                columns: [{ key: "user", title: qsTr("User"), w: 140, mono: true }, { key: "terminal", title: qsTr("Terminal"), w: 100, mono: true },
                          { key: "login_at", title: qsTr("Since"), w: 160, mono: true }, { key: "source", title: qsTr("From"), flex: 1, mono: true, text: r => r.source || "—" }]
            }
        }
        // ключи
        ColumnLayout {
            spacing: 8
            RowLayout { Item { Layout.fillWidth: true }
                SButton { compact: true; text: qsTr("Add key…"); enabled: !root.readonlyMode; onClicked: { keyUser.text = ""; keyText.text = ""; keyErr.text = ""; addKeyDlg.open() } } }
            DataTable {
                objectName: "keyTable"
                Layout.fillWidth: true; Layout.fillHeight: true
                loading: root.loading; rows: root.keys
                actions: [{ id: "del", text: qsTr("Delete…"), kind: "danger", enabled: r => !root.readonlyMode }]
                emptyText: qsTr("No keys found")
                columns: [
                    { key: "user", title: qsTr("User"), w: 100, mono: true },
                    { key: "key_type", title: qsTr("Type"), w: 110, mono: true },
                    { key: "bits", title: qsTr("Bits"), w: 60, right: true, mono: true },
                    { key: "weak", title: qsTr("Strength"), w: 90, chip: true, text: r => r.weak ? qsTr("weak") : qsTr("ok"), tone: r => r.weak ? Theme.red : Theme.green },
                    { key: "fingerprint", title: qsTr("Fingerprint"), flex: 1.4, mono: true },
                    { key: "comment", title: qsTr("Comment"), flex: 1, mono: true }
                ]
                onAction: (id, r) => root.delKey(r)
            }
        }
        // порты
        DataTable {
            objectName: "portTable"
            loading: root.loading; rows: root.ports
            actions: [{ id: "close", text: qsTr("Close…"), kind: "danger", enabled: r => !root.readonlyMode }]
            columns: [
                { key: "protocol", title: qsTr("Proto"), w: 60, mono: true },
                { key: "address", title: qsTr("Address"), flex: 1, mono: true },
                { key: "port", title: qsTr("Port"), w: 70, right: true, mono: true },
                { key: "process", title: qsTr("Process"), flex: 1.4, mono: true, text: r => r.process || "—" },
                { key: "risk", title: qsTr("Risk"), w: 90, chip: true, tone: r => root.riskTone(r), text: r => r.risk === "high" ? qsTr("high") : qsTr("review") }
            ]
            onAction: (id, r) => root.closePort(r)
        }
        // обновления
        DataTable {
            objectName: "updateTable"
            loading: root.loading; rows: root.updates
            emptyText: qsTr("No pending security updates")
            columns: [{ key: "name", title: qsTr("Package"), flex: 1, mono: true }, { key: "details", title: qsTr("Details"), flex: 2, mono: true }]
        }
    }

    SDialog {
        id: banDlg
        title: qsTr("Ban IP"); dialogWidth: 400
        onOpened: banIp.forceFocus()
        function go() { const ip = banIp.text.trim(); if (ip === "") return; close(); root.ban(ip) }
        onAcceptRequested: go()
        STextField { id: banIp; Layout.fillWidth: true; label: qsTr("IP address"); mono: true; onAccepted: banDlg.go() }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: banDlg.close() }, SButton { text: qsTr("Next…"); kind: "primary"; onClicked: banDlg.go() } ]
    }
    SDialog {
        id: addKeyDlg
        title: qsTr("Add SSH key"); dialogWidth: 560
        onOpened: keyUser.forceFocus()
        STextField { id: keyUser; Layout.fillWidth: true; label: qsTr("User"); mono: true; error: keyErr.text }
        STextArea { id: keyText; Layout.fillWidth: true; Layout.preferredHeight: 90; placeholderText: "ssh-ed25519 AAAA… comment"; wrap: true }
        SLabel { id: keyErr; visible: false }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: addKeyDlg.close() }, SButton { text: qsTr("Next…"); kind: "primary"; onClicked: root.addKey() } ]
    }
}
