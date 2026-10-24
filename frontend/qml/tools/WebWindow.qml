import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Веб-серверы и SSL: сайты nginx/apache, сертификаты со сроком, проверка конфигурации и reload.
ToolPane {
    id: root
    property string tab: "sites"
    property string engine: "nginx"
    property var detected: []
    property var sites: []
    property var certs: []
    property bool loading: false
    property bool testRan: false
    property bool testValid: false
    property var testLines: []
    property string unavailable: ""
    readonly property var engines: [].concat(detected.indexOf("nginx") >= 0 ? [{ value: "nginx", text: "nginx" }] : [],
                                             detected.indexOf("apachectl") >= 0 ? [{ value: "apache", text: "Apache" }] : [])

    function daysLeft(iso) { return iso ? Math.floor((new Date(iso).getTime() - Date.now()) / 86400000) : null }
    function certTone(r) { const d = daysLeft(r.expires_at); return d === null ? Theme.textSub : (d < 7 ? Theme.red : (d < 30 ? Theme.amber : Theme.green)) }
    function detect() {
        req("web.detect", {}, d => {
            detected = d.tools || []
            if (engines.length && !engines.some(e => e.value === engine)) engine = engines[0].value
            unavailable = engines.length === 0 && detected.indexOf("certbot") < 0 && detected.indexOf("openssl") < 0 ? qsTr("No web server (nginx, Apache) was found on this server.") : ""
            refresh()
        }, e => { unavailable = e })
    }
    function refresh() {
        loading = true
        if (tab === "sites") {
            if (!engines.length) { sites = []; loading = false; return }
            req("web.sites.list", { engine: engine }, d => { sites = d.items || []; loading = false }, e => { errorText = e; loading = false })
        } else {
            req("web.certs.list", {}, d => { certs = d.items || []; loading = false }, e => { errorText = e; loading = false })
        }
    }
    function runTest() {
        testRan = false
        req("web.test", { engine: engine }, d => { testValid = !!d.valid; testLines = d.lines || []; testRan = true }, e => { errorText = e })
    }
    function reload() {
        dangerousReq({ title: qsTr("Reload %1").arg(engine), message: qsTr("The configuration is re-checked on the server; the web server is reloaded only if the check passes."),
                       items: [engine], confirmText: qsTr("Reload") }, "web.reload", { engine: engine }, () => { testRan = false })
    }
    function viewSite(r) {
        const path = engine === "nginx" ? r.path : (r.source || "").split(":")[0]
        if (!path) return
        siteTitle.text = path
        siteText.text = ""
        siteDlg.open()
        req("web.sites.get", { engine: engine, path: path }, d => { siteText.text = d.content || "" }, e => { siteText.text = e })
    }
    function renew(dry) {
        dangerousReq({ title: dry ? qsTr("Test certificate renewal") : qsTr("Renew certificates"), items: ["certbot renew" + (dry ? " --dry-run" : "")],
                       confirmText: dry ? qsTr("Run test") : qsTr("Renew") }, "web.certs.renew", { dry_run: dry }, () => refresh())
    }

    onTabChanged: refresh()
    onEngineChanged: { testRan = false; if (tab === "sites") refresh() }
    Component.onCompleted: detect()

    RowLayout {
        Layout.fillWidth: true; Layout.margins: 10; Layout.bottomMargin: 6; spacing: 10
        SSegmented { model: [{ value: "sites", text: qsTr("Sites") }, { value: "certs", text: qsTr("Certificates") }]; current: root.tab; onActivated: (v) => root.tab = v }
        SSegmented { visible: root.engines.length > 1; model: root.engines; current: root.engine; onActivated: (v) => root.engine = v }
        Item { Layout.fillWidth: true }
        SButton { visible: root.tab === "sites" && root.engines.length > 0; compact: true; text: root.engine === "nginx" ? "nginx -t" : qsTr("Check config"); onClicked: root.runTest() }
        SButton { visible: root.tab === "sites" && root.engines.length > 0; compact: true; kind: "primary"; text: qsTr("Reload")
                  enabled: root.testRan && root.testValid && !root.readonlyMode; onClicked: root.reload() }
        SButton { visible: root.tab === "certs"; compact: true; text: qsTr("Test renewal"); enabled: !root.readonlyMode; onClicked: root.renew(true) }
        SButton { visible: root.tab === "certs"; compact: true; kind: "primary"; text: qsTr("Renew"); enabled: !root.readonlyMode; onClicked: root.renew(false) }
        SButton { compact: true; text: qsTr("Refresh"); onClicked: root.detect() }
    }
    SEmptyState {
        visible: root.unavailable !== ""
        Layout.fillWidth: true; Layout.fillHeight: true
        glyph: "◎"; title: qsTr("Web servers are not available"); description: root.unavailable
    }
    ColumnLayout {
        visible: root.unavailable === ""
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 10; Layout.topMargin: 0
        spacing: 8
        // результат проверки
        Rectangle {
            visible: root.tab === "sites" && root.testRan
            Layout.fillWidth: true; implicitHeight: Math.min(130, testCol.implicitHeight + 16); radius: 8
            color: Theme.alpha(root.testValid ? Theme.green : Theme.red, 0.1); border.color: Theme.alpha(root.testValid ? Theme.green : Theme.red, 0.5)
            clip: true
            Flickable {
                anchors.fill: parent; anchors.margins: 8; contentHeight: testCol.implicitHeight
                Column {
                    id: testCol; width: parent.width
                    SLabel { font.bold: true; color: root.testValid ? Theme.green : Theme.red; text: root.testValid ? qsTr("Configuration is valid") : qsTr("Configuration has errors — reload is blocked") }
                    Repeater { model: root.testLines; delegate: SLabel { required property var modelData; width: testCol.width; mono: true; font.pixelSize: 11; text: modelData } }
                }
            }
        }
        StackLayout {
            Layout.fillWidth: true; Layout.fillHeight: true
            currentIndex: root.tab === "sites" ? 0 : 1
            DataTable {
                objectName: "siteTable"
                loading: root.loading; rows: root.sites
                actions: [{ id: "view", text: qsTr("Config") }]
                emptyText: qsTr("No sites found")
                columns: [
                    { key: "names", title: qsTr("Domains"), flex: 1.6, mono: true, text: r => (r.names || []).join(", ") },
                    { key: "root", title: qsTr("Root"), flex: 1.2, mono: true, text: r => r.root || "—" },
                    { key: "listen", title: qsTr("Listen"), w: 110, mono: true, text: r => (r.listen || []).join(", ") || "—" },
                    { key: "ssl", title: "SSL", w: 70, chip: true, text: r => r.ssl === undefined ? "—" : (r.ssl ? "SSL" : "—"), tone: r => r.ssl ? Theme.green : Theme.textSub },
                    { key: "path", title: qsTr("File"), flex: 1.2, mono: true, text: r => r.path || r.source || "—" }
                ]
                onAction: (id, r) => root.viewSite(r)
            }
            DataTable {
                objectName: "certTable"
                loading: root.loading; rows: root.certs
                emptyText: qsTr("No certificates found")
                columns: [
                    { key: "name", title: qsTr("Certificate"), flex: 1.2, mono: true, text: r => r.name || (r.domains ? r.domains[0] : r.subject) || "—" },
                    { key: "domains", title: qsTr("Domains"), flex: 1.6, mono: true, text: r => (r.domains || []).join(", ") || r.subject || "—" },
                    { key: "expires_at", title: qsTr("Expires"), w: 150, text: r => root.fmtTime(r.expires_at) },
                    { key: "days", title: qsTr("Left"), w: 90, chip: true, text: r => { const d = root.daysLeft(r.expires_at); return d === null ? "—" : qsTr("%1 d").arg(d) },
                      sort: r => root.daysLeft(r.expires_at), tone: r => root.certTone(r) },
                    { key: "path", title: qsTr("Path"), flex: 1.4, mono: true }
                ]
            }
        }
    }

    SDialog {
        id: siteDlg
        dialogWidth: 760
        title: qsTr("Site configuration")
        SLabel { id: siteTitle; mono: true; sub: true; Layout.fillWidth: true }
        STextArea { id: siteText; Layout.fillWidth: true; Layout.preferredHeight: 380; readOnly: true; wrap: false }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Copy"); onClicked: ToolUtil.copy(siteText.text) }, SButton { text: qsTr("Close"); onClicked: siteDlg.close() } ]
    }
}
