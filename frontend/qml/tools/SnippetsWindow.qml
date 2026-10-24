import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Сниппеты: библиотека команд (название, команда, теги), запуск на выбранных серверах, история запусков.
ToolPane {
    id: root
    property string tab: "library"
    property var snippets: []
    property var history: []
    property string selId: ""                   // "" — новый
    property bool dirty: false
    property bool loading: false
    property string filter: ""
    readonly property var shown: snippets.filter(s => filter === "" || (s.title + " " + (s.tags || []).join(" ")).toLowerCase().indexOf(filter.toLowerCase()) >= 0)

    function refresh() {
        loading = true
        req("snippet.list", {}, d => { snippets = d.items || []; loading = false }, e => { errorText = e; loading = false })
    }
    function loadHistory() {
        req("snippet.history", {}, d => { history = (d.items || []).slice().sort((a, b) => (b.timestamp || "").localeCompare(a.timestamp || "")) }, e => { errorText = e })
    }
    function load(s) {
        selId = s ? s.id : ""
        fTitle.text = s ? s.title : ""
        fCmd.text = s ? s.command : ""
        fTags.text = s ? (s.tags || []).join(", ") : ""
        dirty = false
    }
    function save() {
        if (fTitle.text.trim() === "" || fCmd.text.trim() === "") { errorText = qsTr("Title and command are required"); return }
        const p = { title: fTitle.text.trim(), command: fCmd.text, tags: fTags.text.split(",").map(t => t.trim()).filter(t => t !== "") }
        if (selId !== "") p.id = selId
        call("snippet.set", p, s => { selId = s.id; dirty = false; refresh() }, e => { errorText = e })
    }
    function del() {
        dangerous({ title: qsTr("Delete snippet"), items: [fTitle.text], confirmText: qsTr("Delete"), needName: false }, "snippet.del", { id: selId }, () => { load(null); refresh() })
    }
    function srvName(id) { const s = Backend.serverById(id); return s && s.name ? s.name : id }

    onTabChanged: if (tab === "history") loadHistory()
    Component.onCompleted: refresh()
    property alias runner: runner
    MultiRunner { id: runner; pane: root }

    RowLayout {
        Layout.fillWidth: true; Layout.margins: 10; Layout.bottomMargin: 6
        SSegmented { model: [{ value: "library", text: qsTr("Library") }, { value: "history", text: qsTr("History") }]; current: root.tab; onActivated: (v) => root.tab = v }
        Item { Layout.fillWidth: true }
    }
    StackLayout {
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 10; Layout.topMargin: 0
        currentIndex: root.tab === "library" ? 0 : 1
        RowLayout {
            spacing: 10
            ColumnLayout {
                Layout.preferredWidth: 210; Layout.maximumWidth: 210; Layout.fillHeight: true; spacing: 6
                RowLayout {
                    STextField { Layout.fillWidth: true; placeholderText: qsTr("Search"); onEdited: root.filter = text }
                    SButton { compact: true; text: "+"; onClicked: root.load(null); Accessible.name: qsTr("New snippet") }
                }
                ListView {
                    objectName: "snippetList"
                    Layout.fillWidth: true; Layout.fillHeight: true; clip: true; spacing: 2
                    model: root.shown
                    ScrollBar.vertical: ScrollBar {}
                    delegate: Rectangle {
                        required property var modelData
                        width: ListView.view.width; height: 44; radius: 6
                        color: root.selId === modelData.id ? Theme.selected : (ma.containsMouse ? Theme.hover : "transparent")
                        Column { anchors.fill: parent; anchors.margins: 6; spacing: 2
                            SLabel { width: parent.width; text: modelData.title; font.bold: true }
                            SLabel { width: parent.width; sub: true; mono: true; font.pixelSize: 11; text: (modelData.tags || []).join(" · ") || modelData.command } }
                        MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; onClicked: root.load(modelData) }
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true; Layout.fillHeight: true; spacing: 8
                RowLayout {
                    spacing: 8
                    STextField { id: fTitle; Layout.fillWidth: true; label: qsTr("Title"); onEdited: root.dirty = true }
                    STextField { id: fTags; Layout.preferredWidth: 200; label: qsTr("Tags (comma separated)"); onEdited: root.dirty = true }
                }
                STextArea { id: fCmd; objectName: "snippetCommand"; Layout.fillWidth: true; Layout.preferredHeight: 90; placeholderText: qsTr("Shell command"); onEdited: root.dirty = true }
                RowLayout {
                    spacing: 8
                    SButton { text: qsTr("Save"); kind: "primary"; compact: true; enabled: root.dirty; onClicked: root.save() }
                    SButton { text: qsTr("Delete…"); kind: "danger"; compact: true; visible: root.selId !== ""; enabled: !root.readonlyMode; onClicked: root.del() }
                    Item { Layout.fillWidth: true }
                    SButton { objectName: "snippetRun"; text: qsTr("Run on selected servers"); kind: "primary"; compact: true; busy: runner.running
                              enabled: fCmd.text.trim() !== "" && picker.selectedIds.length > 0 && !root.readonlyMode && !runner.running
                              onClicked: runner.start(root.selId !== "" && !root.dirty
                                                      ? { snippetId: root.selId, preview: fCmd.text, targets: picker.selectedIds, title: qsTr("Run snippet “%1”").arg(fTitle.text) }
                                                      : { command: fCmd.text, targets: picker.selectedIds, title: qsTr("Run command") }) }
                }
                RowLayout {
                    Layout.fillWidth: true; Layout.fillHeight: true; spacing: 8
                    ServerPicker { id: picker; Layout.fillHeight: true; Layout.preferredWidth: 190; currentId: root.serverId }
                    RunResults { runner: runner; Layout.fillWidth: true; Layout.fillHeight: true }
                }
            }
        }
        DataTable {
            objectName: "historyTable"
            rows: root.history
            emptyText: qsTr("No runs yet")
            columns: [
                { key: "timestamp", title: qsTr("Time"), w: 160, text: r => root.fmtTime(r.timestamp) },
                { key: "connection_id", title: qsTr("Server"), w: 160, text: r => root.srvName(r.connection_id) },
                { key: "outcome", title: qsTr("Outcome"), flex: 1, chip: true, tone: r => r.outcome === "ok" ? Theme.green : Theme.red },
                { key: "run", title: qsTr("Run"), w: 130, mono: true }
            ]
        }
    }
}
