import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Редактор текстовых файлов сервера: подсветка, номера строк, поиск/замена, сохранение с проверкой mtime.
Item {
    id: root
    required property var desktop
    property string path: ""
    property double mtime: 0
    property int fileSize: 0
    property bool modified: false
    property bool loading: false
    property string errorText: ""
    property bool findOpen: false
    property var closeCallback: null
    property bool suppress: false
    property string savedText: ""
    property alias textArea: area
    property alias highlighter: hl
    property alias conflictDialog: conflictDlg
    property alias unsavedDialog: unsavedDlg

    function startWith(args) { if (args.path) load(args.path) }

    function load(p) {
        path = p
        loading = true
        errorText = ""
        desktop.rpc("files.read", { path: p }, function (ok, data, code, text) {
            loading = false
            if (!ok) { errorText = text; return }
            savedText = data.text
            area.text = data.text
            mtime = data.mtime
            fileSize = data.size
            modified = false
            hl.fileName = p
            area.cursorPosition = 0
        })
    }

    function save(force) {
        if (loading) return
        errorText = ""
        const sentText = area.text
        desktop.rpc("files.write", { path: path, text: sentText, mtime: force && conflictMtime !== null ? conflictMtime : mtime }, function (ok, data, code, text, eparams) {
            if (ok) {
                mtime = data.mtime
                fileSize = data.size
                savedText = sentText
                modified = area.text !== savedText
                conflictMtime = null
                toast.show(qsTr("Saved"))
                if (closeCallback) { const c = closeCallback; closeCallback = null; c() }
                return
            }
            if (code === "fs.conflict") {
                conflictMtime = eparams && eparams.mtime !== undefined ? eparams.mtime : null
                conflictDlg.open()
                return
            }
            errorText = text
        })
    }
    property var conflictMtime: null

    // вызывается DesktopPage перед закрытием окна: true — можно закрывать сразу
    function beforeClose(proceed) {
        if (!modified) return true
        closeCallback = null
        unsavedDlg.proceed = proceed
        unsavedDlg.open()
        return false
    }

    function findNext(back) {
        const q = findField.text
        if (q === "") return
        const text = caseBox.checked ? area.text : area.text.toLowerCase()
        const needle = caseBox.checked ? q : q.toLowerCase()
        let idx
        if (back) {
            idx = text.lastIndexOf(needle, Math.max(0, area.selectionStart - 1))
            if (idx < 0) idx = text.lastIndexOf(needle)
        } else {
            idx = text.indexOf(needle, area.selectionEnd)
            if (idx < 0) idx = text.indexOf(needle)
        }
        if (idx >= 0) { area.select(idx, idx + q.length); matchLabel.text = "" }
        else matchLabel.text = qsTr("not found")
    }
    function replaceOne() {
        if (area.selectedText !== "" && (caseBox.checked ? area.selectedText === findField.text : area.selectedText.toLowerCase() === findField.text.toLowerCase())) {
            area.remove(area.selectionStart, area.selectionEnd)
            area.insert(area.cursorPosition, replField.text)
        }
        findNext(false)
    }
    function replaceAll() {
        const q = findField.text
        if (q === "") return
        const flags = caseBox.checked ? "g" : "gi"
        const re = new RegExp(q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), flags)
        const n = (area.text.match(re) || []).length
        area.text = area.text.replace(re, () => replField.text)
        matchLabel.text = qsTr("%1 replaced").arg(n)
    }

    Shortcut { sequence: "Ctrl+S"; enabled: root.visible; onActivated: root.save(false) }
    Shortcut { sequence: "Ctrl+F"; enabled: root.visible; onActivated: { root.findOpen = true; findField.forceFocus() } }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 38
            color: Theme.alpha(Theme.panelFill, Theme.blurOn ? 0.4 : 1)
            RowLayout {
                anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 8
                SLabel { Layout.fillWidth: true; mono: true; font.pixelSize: 12; text: root.path + (root.modified ? "  ●" : ""); elide: Text.ElideMiddle; color: root.modified ? Theme.amber : Theme.text }
                SButton { compact: true; kind: "ghost"; text: qsTr("Reload"); onClicked: root.modified ? reloadDlg.open() : root.load(root.path) }
                SButton { compact: true; kind: "ghost"; text: qsTr("Find"); onClicked: { root.findOpen = !root.findOpen; if (root.findOpen) findField.forceFocus() } }
                SButton { compact: true; kind: "primary"; text: qsTr("Save"); enabled: root.modified && !root.desktop.readOnlyMode; onClicked: root.save(false) }
            }
        }
        SBanner { Layout.fillWidth: true; Layout.margins: 6; Layout.bottomMargin: 0; visible: root.errorText !== ""; text: root.errorText; onDismissed: root.errorText = "" }
        SBanner { Layout.fillWidth: true; Layout.margins: 6; Layout.bottomMargin: 0; visible: root.desktop.readOnlyMode; tone: Theme.amber; text: qsTr("Read-only mode: saving is disabled.") }

        // поиск и замена
        Rectangle {
            visible: root.findOpen
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            color: Theme.alpha(Theme.panelFill, 0.6)
            RowLayout {
                anchors.fill: parent; anchors.margins: 6; spacing: 6
                STextField { id: findField; Layout.preferredWidth: 200; placeholderText: qsTr("Find"); onAccepted: root.findNext(false); Accessible.name: qsTr("Find") }
                STextField { id: replField; Layout.preferredWidth: 180; placeholderText: qsTr("Replace with"); Accessible.name: qsTr("Replace with") }
                SCheckBox { id: caseBox; text: qsTr("Match case") }
                SButton { compact: true; text: "↑"; onClicked: root.findNext(true) }
                SButton { compact: true; text: "↓"; onClicked: root.findNext(false) }
                SButton { compact: true; text: qsTr("Replace"); onClicked: root.replaceOne() }
                SButton { compact: true; text: qsTr("Replace all"); onClicked: root.replaceAll() }
                SLabel { id: matchLabel; sub: true; font.pixelSize: 11 }
                Item { Layout.fillWidth: true }
                SLabel { text: "×"; sub: true; font.pixelSize: 16; MouseArea { anchors.fill: parent; anchors.margins: -5; onClicked: root.findOpen = false } }
            }
        }

        // текст
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: Theme.alpha(Theme.light ? "#ffffff" : "#05070c", Theme.blurOn ? 0.45 : 1)
            clip: true

            Rectangle {   // колонка номеров строк
                id: gutter
                width: 14 + String(Math.max(1, area.lineCount)).length * 8
                height: parent.height
                color: Theme.alpha(Theme.panelFill, 0.5)
                clip: true
                Text {
                    x: 0; width: gutter.width - 8
                    y: -flick.contentY + area.topPadding
                    horizontalAlignment: Text.AlignRight
                    color: Theme.textSub
                    font.family: Theme.fontMono; font.pixelSize: 12
                    lineHeight: area.lineHeight > 0 ? area.lineHeight : 1
                    lineHeightMode: area.lineHeight > 0 ? Text.FixedHeight : Text.ProportionalHeight
                    text: { let s = ""; for (let i = 1; i <= area.lineCount; ++i) s += i + "\n"; return s }
                }
            }
            Flickable {
                id: flick
                anchors.left: gutter.right; anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom
                contentWidth: Math.max(width, area.contentWidth + 24)
                contentHeight: Math.max(height, area.contentHeight + 12)
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                ScrollBar.horizontal: ScrollBar {}
                function ensureVisible(r) {
                    if (contentX >= r.x) contentX = r.x
                    else if (contentX + width <= r.x + r.width) contentX = r.x + r.width - width
                    if (contentY >= r.y) contentY = r.y
                    else if (contentY + height <= r.y + r.height) contentY = r.y + r.height - height
                }
                TextArea.flickable: TextArea {
                    id: area
                    readonly property real lineHeight: contentHeight > 0 && lineCount > 0 ? contentHeight / lineCount : 0
                    width: flick.contentWidth
                    wrapMode: TextEdit.NoWrap
                    color: Theme.text
                    selectionColor: Theme.accent
                    selectedTextColor: "#ffffff"
                    font.family: Theme.fontMono
                    font.pixelSize: 13
                    topPadding: 4; leftPadding: 10
                    background: null
                    selectByMouse: true
                    persistentSelection: true
                    readOnly: root.loading
                    tabStopDistance: 32
                    onTextChanged: root.modified = (text !== root.savedText)
                    onCursorRectangleChanged: flick.ensureVisible(cursorRectangle)
                    Accessible.name: qsTr("File text")
                    Keys.onPressed: (e) => {
                        if (e.key === Qt.Key_Tab && !(e.modifiers & Qt.ControlModifier)) { insert(cursorPosition, "    "); e.accepted = true }
                    }
                }
            }
            CodeHighlighter { id: hl; document: area.textDocument; dark: !Theme.light }
            SSpinner { anchors.centerIn: parent; visible: root.loading; width: 22; height: 22 }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 24
            color: Theme.alpha(Theme.panelFill, 0.7)
            RowLayout {
                anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; spacing: 16
                SLabel { mono: true; font.pixelSize: 11; sub: true; text: qsTr("Ln %1, Col %2").arg(area.text.substring(0, area.cursorPosition).split("\n").length).arg(area.cursorPosition - (area.cursorPosition > 0 ? area.text.lastIndexOf("\n", area.cursorPosition - 1) : -1)) }
                SLabel { mono: true; font.pixelSize: 11; sub: true; text: hl.language }
                SLabel { mono: true; font.pixelSize: 11; sub: true; text: "UTF-8" }
                SLabel { mono: true; font.pixelSize: 11; sub: true; text: root.fileSize + " B" }
                Item { Layout.fillWidth: true }
                SLabel { id: toast; property string msg: ""; function show(t) { msg = t; text = t; tt.restart() } mono: false; font.pixelSize: 11; color: Theme.green
                         Timer { id: tt; interval: 2500; onTriggered: toast.text = "" } }
            }
        }
    }

    SDialog {
        id: unsavedDlg
        property var proceed: null
        tone: Theme.amber
        title: qsTr("Unsaved changes")
        dialogWidth: 460
        SLabel { Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone; text: qsTr("Save changes to %1 before closing?").arg(root.path) }
        buttons: [
            SButton { text: qsTr("Discard"); kind: "danger"; onClicked: { unsavedDlg.close(); root.modified = false; if (unsavedDlg.proceed) unsavedDlg.proceed() } },
            Item { Layout.fillWidth: true },
            SButton { text: qsTr("Cancel"); onClicked: unsavedDlg.close() },
            SButton { text: qsTr("Save and close"); kind: "primary"; onClicked: { unsavedDlg.close(); root.closeCallback = unsavedDlg.proceed; root.save(false) } }
        ]
    }
    ConfirmDialog {
        id: reloadDlg
        title: qsTr("Discard changes?")
        message: qsTr("Reload the file from the server and lose your edits?")
        confirmText: qsTr("Reload")
        danger: true
        onConfirmed: root.load(root.path)
    }
    SDialog {
        id: conflictDlg
        tone: Theme.red
        title: qsTr("File changed on the server")
        dialogWidth: 500
        SLabel { Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone
                 text: qsTr("Someone modified “%1” after you opened it. Overwrite their version with yours, or reload their version?").arg(root.path) }
        buttons: [
            SButton { text: qsTr("Cancel"); onClicked: conflictDlg.close() },
            Item { Layout.fillWidth: true },
            SButton { text: qsTr("Reload from server"); onClicked: { conflictDlg.close(); root.modified = false; root.load(root.path) } },
            SButton { text: qsTr("Overwrite"); kind: "danger"; onClicked: { conflictDlg.close(); root.save(true) } }
        ]
    }
}
