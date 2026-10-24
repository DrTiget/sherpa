import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// «Пакеты»: обновления, установленные, поиск; установка/удаление/обновление с потоковым выводом (pkg.output/pkg.done).
UtilPane {
    id: root
    property string tab: "updates"
    property string manager: ""
    property var updates: []
    property var installed: []
    property var found: []
    property string query: ""
    property string searchText: ""
    property bool loading: false
    property string opId: ""
    property string opTitle: ""
    property bool opRunning: false
    property string opResult: ""     // "" | ok | error
    property string logText: ""
    property bool unavailable: !!desktop.capabilities.utilities && desktop.capabilities.utilities.packages === false
    readonly property var current: tab === "updates" ? updates : (tab === "installed" ? shownInstalled : found)
    readonly property var shownInstalled: installed.filter(p => query === "" || p.name.toLowerCase().indexOf(query.toLowerCase()) >= 0 || (p.description || "").toLowerCase().indexOf(query.toLowerCase()) >= 0)

    function stripAnsi(t) { return t.replace(/\x1b\[[0-9;?]*[A-Za-z]/g, "").replace(/\r(?!\n)/g, "\n") }
    function appendLog(t) {
        logText += stripAnsi(t)
        if (logText.length > 200000) logText = logText.slice(logText.length - 150000)
    }
    function load(kind) {
        if (!working || unavailable) return
        const m = { updates: "pkg.updates", installed: "pkg.installed" }[kind]
        if (!m) return
        loading = true
        rpc(m, {}, (d) => { loading = false; if (kind === "updates") updates = d.items || []; else installed = d.items || [] },
            (t) => { loading = false; showNotice(t) })
    }
    function search() {
        if (searchText.length < 1) return
        loading = true
        rpc("pkg.search", { query: searchText }, (d) => { loading = false; found = d.items || [] }, (t) => { loading = false; found = []; showNotice(t) })
    }
    function init() {
        if (!working || unavailable) return
        rpc("pkg.detect", {}, (d) => manager = d.manager || "", () => {})
        load("updates"); load("installed")
    }
    onWorkingChanged: if (working) init()
    Component.onCompleted: init()

    function runOp(method, names, title, needName, msg) {
        if (opRunning) return
        const params = names && names.length ? { packages: names } : {}
        danger(method, params, { title: title, message: msg, details: names && names.length ? names.join(" ") : "", confirmText: title, needName: needName }, (d) => {
            opId = d.operation; opTitle = title; opRunning = true; opResult = ""; logText = ""
        })
    }
    function install(names) { runOp("pkg.install", names, qsTr("Install"), false, qsTr("The packages below will be installed with their dependencies.")) }
    function remove(names) { runOp("pkg.remove", names, qsTr("Remove"), true, qsTr("The packages below will be removed from the server together with unused dependencies, if the manager decides so.")) }
    function upgrade(names) {
        runOp("pkg.upgrade", names, names && names.length ? qsTr("Upgrade selected") : qsTr("Upgrade all"), true,
              names && names.length ? qsTr("The selected packages will be upgraded to the latest versions.") : qsTr("All installed packages will be upgraded. Services may restart."))
    }

    onEvent: (ev) => {
        if (ev.operation === undefined || ev.operation !== opId) return
        if (ev.event === "pkg.output") {
            appendLog(desktop.decodeBase64(ev.data_b64))
        } else if (ev.event === "pkg.done") {
            opRunning = false; opResult = "ok"
            appendLog("\n" + qsTr("Done") + "\n")
            load("updates"); load("installed"); checkedClear()
        } else if (ev.event === "error") {
            opRunning = false; opResult = "error"
            const e = ev.error || ({})
            appendLog("\n" + qsTr("Error") + ": " + (e.message || e.code || "") + "\n")
        }
    }
    function checkedClear() { updTable.checked = ({}); insTable.checked = ({}); srcTable.checked = ({}) }

    SEmptyState {
        anchors.fill: parent; visible: root.unavailable || !root.working
        glyph: "▦"; title: root.working ? qsTr("Not available on this server") : qsTr("Waiting for the server…")
        description: root.working ? qsTr("No supported package manager (apt, dnf, yum, pacman, apk) was found.") : ""
    }
    ColumnLayout {
        anchors.fill: parent; anchors.margins: 12; spacing: 10
        visible: root.working && !root.unavailable
        RowLayout {
            Layout.fillWidth: true; spacing: 8
            SSegmented {
                current: root.tab
                model: [{ value: "updates", text: qsTr("Updates") + (root.updates.length ? " (" + root.updates.length + ")" : "") },
                        { value: "installed", text: qsTr("Installed") }, { value: "search", text: qsTr("Search") }]
                onActivated: (v) => root.tab = v
            }
            SChip { visible: root.manager !== ""; text: root.manager; tone: Theme.cyan }
            STextField {
                Layout.fillWidth: true
                visible: root.tab !== "updates"
                placeholderText: root.tab === "search" ? qsTr("Package name to search, then Enter") : qsTr("Filter installed packages…")
                Accessible.name: qsTr("Package search")
                objectName: "pkgSearch"
                onEdited: { if (root.tab === "installed") root.query = text; else root.searchText = text }
                onAccepted: if (root.tab === "search") root.search()
            }
            Item { Layout.fillWidth: true; visible: root.tab === "updates" }
            SSpinner { visible: root.loading; width: 18; height: 18 }
            SButton { text: qsTr("Refresh"); compact: true; onClicked: root.tab === "search" ? root.search() : root.load(root.tab) }
        }
        RowLayout {
            Layout.fillWidth: true; spacing: 8
            SButton {
                objectName: "pkgUpgradeSel"; visible: root.tab === "updates"; text: qsTr("Upgrade selected"); compact: true; kind: "primary"
                enabled: !root.opRunning && updTable.checkedKeys().length > 0; onClicked: root.upgrade(updTable.checkedKeys())
            }
            SButton { objectName: "pkgUpgradeAll"; visible: root.tab === "updates"; text: qsTr("Upgrade all…"); compact: true; enabled: !root.opRunning && root.updates.length > 0; onClicked: root.upgrade([]) }
            SButton {
                objectName: "pkgInstall"; visible: root.tab === "search"; text: qsTr("Install selected"); compact: true; kind: "primary"
                enabled: !root.opRunning && srcTable.checkedKeys().length > 0; onClicked: root.install(srcTable.checkedKeys())
            }
            SButton {
                objectName: "pkgRemove"; visible: root.tab === "installed"; text: qsTr("Remove selected…"); compact: true; kind: "danger"
                enabled: !root.opRunning && insTable.checkedKeys().length > 0; onClicked: root.remove(insTable.checkedKeys())
            }
            SLabel { Layout.fillWidth: true; sub: true; font.pixelSize: 11; text: root.tab === "updates" ? qsTr("%1 updates available").arg(root.updates.length) : (root.tab === "installed" ? qsTr("%1 packages").arg(root.shownInstalled.length) : qsTr("%1 found").arg(root.found.length)) }
        }
        StackLayout {
            Layout.fillWidth: true; Layout.fillHeight: true
            currentIndex: ["updates", "installed", "search"].indexOf(root.tab)
            UTable {
                id: updTable; objectName: "updTable"; checkKey: "name"; rows: root.updates
                emptyText: root.loading ? "" : qsTr("System is up to date")
                columns: [ { key: "name", title: qsTr("Package"), w: 220, mono: true }, { key: "installed_version", title: qsTr("Now"), w: 140, mono: true },
                           { key: "version", title: qsTr("New"), w: 150, mono: true, color: (r) => Theme.green }, { key: "arch", title: qsTr("Arch"), w: 70 },
                           { key: "description", title: qsTr("Description"), w: 0 } ]
            }
            UTable {
                id: insTable; objectName: "insTable"; checkKey: "name"; rows: root.shownInstalled
                emptyText: root.loading ? "" : qsTr("Nothing found")
                columns: [ { key: "name", title: qsTr("Package"), w: 240, mono: true }, { key: "version", title: qsTr("Version"), w: 180, mono: true },
                           { key: "arch", title: qsTr("Arch"), w: 0 } ]
            }
            UTable {
                id: srcTable; objectName: "srcTable"; checkKey: "name"; rows: root.found
                emptyText: root.loading ? "" : qsTr("Type a name and press Enter")
                columns: [ { key: "name", title: qsTr("Package"), w: 240, mono: true }, { key: "version", title: qsTr("Version"), w: 140, mono: true },
                           { key: "installed_version", title: qsTr("Installed"), w: 140, mono: true, color: (r) => Theme.green }, { key: "description", title: qsTr("Description"), w: 0 } ]
            }
        }
        // журнал операции
        Rectangle {
            visible: root.opId !== ""
            Layout.fillWidth: true; Layout.preferredHeight: 170; radius: 8; color: Theme.inputFill; border.color: root.opResult === "error" ? Theme.red : Theme.line
            ColumnLayout {
                anchors.fill: parent; anchors.margins: 8; spacing: 4
                RowLayout {
                    Layout.fillWidth: true
                    SSpinner { visible: root.opRunning; width: 14; height: 14 }
                    SLabel { Layout.fillWidth: true; caps: true; text: qsTr("Operation log") + " · " + root.opTitle + (root.opResult === "ok" ? " ✓" : (root.opResult === "error" ? " ✗" : "")) }
                    SLabel { text: "×"; font.pixelSize: 16; sub: true; visible: !root.opRunning; MouseArea { anchors.fill: parent; anchors.margins: -6; onClicked: { root.opId = ""; root.logText = "" } } }
                }
                Flickable {
                    id: logFlick
                    Layout.fillWidth: true; Layout.fillHeight: true; clip: true
                    contentWidth: width; contentHeight: logView.contentHeight
                    onContentHeightChanged: contentY = Math.max(0, contentHeight - height)
                    ScrollBar.vertical: ScrollBar {}
                    TextEdit {
                        id: logView; objectName: "pkgLog"
                        width: logFlick.width - 10; readOnly: true; selectByMouse: true; wrapMode: TextEdit.WrapAnywhere
                        color: Theme.text; font.family: Theme.fontMono; font.pixelSize: 11; text: root.logText
                    }
                }
            }
        }
    }
}
