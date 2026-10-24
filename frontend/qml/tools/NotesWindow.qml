import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Заметки и секреты: Markdown-заметки сервера и хранилище паролей (значение — только после подтверждения, на 10 секунд).
ToolPane {
    id: root
    property string tab: "notes"
    property var notes: []
    property var secrets: []
    property string noteId: ""
    property bool dirty: false
    property bool preview: false
    property bool loading: false
    property string editSecretId: ""
    property string revealed: ""                // значение показывается
    property string revealTitle: ""
    property int revealLeft: 0
    property alias nTitle: nTitle
    property alias nBody: nBody

    function fail(e) { errorText = e; loading = false }
    function refresh() {
        loading = true
        if (tab === "notes") req("notes.list", { connection_id: serverId }, d => { notes = d.items || []; loading = false }, fail)
        else req("vault.list", { connection_id: serverId }, d => { secrets = d.items || []; loading = false }, fail)
    }
    function loadNote(n) {
        noteId = n ? n.id : ""
        nTitle.text = n ? n.title : ""
        nBody.text = n ? n.markdown : ""
        dirty = false; preview = false
    }
    function saveNote() {
        if (nTitle.text.trim() === "") { errorText = qsTr("Title is required"); return }
        const p = { connection_id: serverId, title: nTitle.text.trim(), markdown: nBody.text }
        if (noteId !== "") p.id = noteId
        call("notes.set", p, n => { noteId = n.id; dirty = false; refresh() }, fail)
    }
    function delNote() {
        dangerous({ title: qsTr("Delete note"), items: [nTitle.text], confirmText: qsTr("Delete"), needName: false }, "notes.del", { id: noteId }, () => { loadNote(null); refresh() })
    }
    function openSecret(s) {
        editSecretId = s ? s.id : ""
        sTitle.text = s ? s.title : ""
        sValue.text = ""
        sErr.text = ""
        secretDlg.open()
    }
    function saveSecret() {
        if (sTitle.text.trim() === "") { sErr.text = qsTr("Title is required"); return }
        if (sValue.text === "" && editSecretId === "") { sErr.text = qsTr("Value is required"); return }
        const p = { connection_id: serverId, title: sTitle.text.trim(), value: sValue.text }
        if (editSecretId !== "") p.id = editSecretId
        call("vault.set", p, () => { secretDlg.close(); sValue.text = ""; refresh() }, e => { sErr.text = e })
    }
    function delSecret(s) {
        dangerous({ title: qsTr("Delete secret"), items: [s.title], confirmText: qsTr("Delete"), needName: false }, "vault.del", { id: s.id }, () => refresh())
    }
    function reveal(s) {
        dangerous({ title: qsTr("Show secret"), message: qsTr("The value is shown for 10 seconds. The access is written to the action log."), items: [s.title], confirmText: qsTr("Show"), needName: false },
                  "vault.reveal", { id: s.id }, d => {
            revealed = d.value; revealTitle = s.title; revealLeft = d.visible_for_sec || 10
            revealTimer.restart(); revealDlg.open()
        })
    }
    function hideSecret() {
        revealTimer.stop()
        ToolUtil.clearClipboardIf(revealed)
        revealed = ""; revealLeft = 0
        revealDlg.close()
    }
    Timer { id: revealTimer; interval: 1000; repeat: true; onTriggered: { root.revealLeft--; if (root.revealLeft <= 0) root.hideSecret() } }

    onTabChanged: refresh()
    Component.onCompleted: refresh()
    Component.onDestruction: ToolUtil.clearClipboardIf(revealed)

    RowLayout {
        Layout.fillWidth: true; Layout.margins: 10; Layout.bottomMargin: 6
        SSegmented { model: [{ value: "notes", text: qsTr("Notes") }, { value: "secrets", text: qsTr("Secrets") }]; current: root.tab; onActivated: (v) => root.tab = v }
        Item { Layout.fillWidth: true }
        SButton { compact: true; text: qsTr("Refresh"); onClicked: root.refresh() }
    }
    StackLayout {
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 10; Layout.topMargin: 0
        currentIndex: root.tab === "notes" ? 0 : 1
        RowLayout {
            spacing: 10
            ColumnLayout {
                Layout.preferredWidth: 200; Layout.maximumWidth: 200; Layout.fillHeight: true; spacing: 6
                SButton { Layout.fillWidth: true; compact: true; text: qsTr("+ New note"); onClicked: root.loadNote(null) }
                ListView {
                    objectName: "noteList"
                    Layout.fillWidth: true; Layout.fillHeight: true; clip: true; spacing: 2
                    model: root.notes
                    ScrollBar.vertical: ScrollBar {}
                    delegate: Rectangle {
                        required property var modelData
                        width: ListView.view.width; height: 40; radius: 6
                        color: root.noteId === modelData.id ? Theme.selected : (ma.containsMouse ? Theme.hover : "transparent")
                        Column { anchors.fill: parent; anchors.margins: 6
                            SLabel { width: parent.width; text: modelData.title; font.bold: true }
                            SLabel { width: parent.width; sub: true; font.pixelSize: 11; text: root.fmtTime(modelData.updated_at) } }
                        MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; onClicked: root.loadNote(modelData) }
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true; Layout.fillHeight: true; spacing: 8
                STextField { id: nTitle; Layout.fillWidth: true; label: qsTr("Title"); onEdited: root.dirty = true }
                RowLayout {
                    SSegmented { model: [{ value: "edit", text: qsTr("Edit") }, { value: "view", text: qsTr("Preview") }]; current: root.preview ? "view" : "edit"; onActivated: (v) => root.preview = v === "view" }
                    Item { Layout.fillWidth: true }
                    SButton { compact: true; text: qsTr("Delete…"); kind: "danger"; visible: root.noteId !== ""; enabled: !root.readonlyMode; onClicked: root.delNote() }
                    SButton { compact: true; text: qsTr("Save"); kind: "primary"; enabled: root.dirty && !root.readonlyMode; onClicked: root.saveNote() }
                }
                STextArea { id: nBody; objectName: "noteBody"; visible: !root.preview; Layout.fillWidth: true; Layout.fillHeight: true; mono: false; placeholderText: qsTr("Markdown"); onEdited: root.dirty = true }
                Rectangle {
                    visible: root.preview; Layout.fillWidth: true; Layout.fillHeight: true
                    radius: Theme.radius; color: Theme.inputFill; border.color: Theme.line; clip: true
                    Flickable {
                        anchors.fill: parent; anchors.margins: 10; contentHeight: md.implicitHeight; clip: true
                        Text { id: md; width: parent.width; textFormat: Text.MarkdownText; wrapMode: Text.Wrap; text: nBody.text
                               color: Theme.text; font.family: Theme.fontUi; font.pixelSize: Theme.sizeUi; linkColor: Theme.cyan }
                    }
                }
            }
        }
        ColumnLayout {
            spacing: 8
            RowLayout { Item { Layout.fillWidth: true }
                SButton { compact: true; kind: "primary"; text: qsTr("+ Secret"); enabled: !root.readonlyMode; onClicked: root.openSecret(null) } }
            DataTable {
                objectName: "secretTable"
                Layout.fillWidth: true; Layout.fillHeight: true
                loading: root.loading; rows: root.secrets
                actions: [{ id: "reveal", text: qsTr("Show…") }, { id: "edit", text: qsTr("Change"), enabled: r => !root.readonlyMode },
                          { id: "del", text: qsTr("Delete…"), kind: "danger", enabled: r => !root.readonlyMode }]
                emptyText: qsTr("No secrets saved for this server")
                columns: [
                    { key: "title", title: qsTr("Title"), flex: 1 },
                    { key: "value", title: qsTr("Value"), w: 100, mono: true, text: r => r.has_secret ? "••••••••" : "—" },
                    { key: "updated_at", title: qsTr("Updated"), w: 150, text: r => root.fmtTime(r.updated_at) }
                ]
                onAction: (id, r) => { if (id === "reveal") root.reveal(r); else if (id === "edit") root.openSecret(r); else root.delSecret(r) }
            }
        }
    }

    SDialog {
        id: secretDlg
        title: root.editSecretId === "" ? qsTr("New secret") : qsTr("Change secret")
        dialogWidth: 440
        onOpened: sTitle.forceFocus()
        onAcceptRequested: root.saveSecret()
        STextField { id: sTitle; Layout.fillWidth: true; label: qsTr("Title"); placeholderText: "Grafana admin" }
        STextField { id: sValue; Layout.fillWidth: true; secret: true; label: root.editSecretId === "" ? qsTr("Value") : qsTr("New value (empty = keep)"); error: sErr.text; onAccepted: root.saveSecret() }
        SLabel { id: sErr; visible: false }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: secretDlg.close() }, SButton { text: qsTr("Save"); kind: "primary"; onClicked: root.saveSecret() } ]
    }
    SDialog {
        id: revealDlg
        title: root.revealTitle
        dialogWidth: 460
        tone: Theme.amber
        guardClose: true
        onCloseRequested: root.hideSecret()
        STextField { Layout.fillWidth: true; readOnly: true; mono: true; text: root.revealed; objectName: "revealedValue" }
        Rectangle { Layout.fillWidth: true; height: 4; radius: 2; color: Theme.alpha(Theme.text, 0.1)
                    Rectangle { height: parent.height; radius: 2; color: Theme.amber; width: parent.width * root.revealLeft / 10 } }
        SLabel { sub: true; Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone
                 text: qsTr("Hides in %1 s. The clipboard is cleared after hiding.").arg(root.revealLeft) }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Copy"); onClicked: ToolUtil.copy(root.revealed) }, SButton { text: qsTr("Hide"); kind: "primary"; onClicked: root.hideSecret() } ]
    }
}
