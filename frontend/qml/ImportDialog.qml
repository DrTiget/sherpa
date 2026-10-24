import QtQuick
import Sherpa
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import QtQuick.Layouts

// Импорт ~/.ssh/config: путь -> предпросмотр с чекбоксами -> отчёт (SCREENS.md 2.5)
SDialog {
    id: root
    property string step: "path"      // path | loading | preview | applying | report
    property var preview: ({})
    property var report: ({})
    property string error: ""
    property var selected: ({})       // key -> bool
    readonly property var items: preview.items || []
    property int selectedCount: 0

    title: qsTr("Import from ~/.ssh/config")
    dialogWidth: 640

    function openDialog() {
        step = "path"; error = ""; preview = ({}); report = ({})
        pathF.text = Backend.defaultSshConfig
        open()
    }
    function recount() {
        var n = 0
        for (var k in selected) if (selected[k]) n++
        selectedCount = n
    }
    function load() {
        step = "loading"; error = ""
        Backend.importPreview(pathF.text)
    }
    function setAll(on) {
        var s = {}
        for (var i = 0; i < items.length; ++i)
            if (items[i].action === "create") s[items[i].key] = on
        selected = s; recount()
    }
    onAcceptRequested: if (step === "path") load()

    Connections {
        target: Backend
        enabled: root.visible
        function onImportPreviewReady(ok, data, message) {
            if (!ok) { root.error = message; root.step = "path"; return }
            root.preview = data
            root.step = "preview"
            root.setAll(true)
        }
        function onImportApplied(ok, data, message) {
            if (!ok) { root.error = message; root.step = "preview"; return }
            root.report = data
            root.step = "report"
        }
    }
    FileDialog {
        id: pick
        title: qsTr("Choose SSH config")
        onAccepted: pathF.text = Backend.toLocalPath(selectedFile.toString())
    }

    // шаг 1
    RowLayout {
        visible: root.step === "path"
        Layout.fillWidth: true
        spacing: 8
        STextField { id: pathF; Layout.fillWidth: true; label: qsTr("Config file"); mono: true; error: root.error; onAccepted: root.load() }
        SButton { text: qsTr("Browse…"); Layout.alignment: Qt.AlignTop; Layout.topMargin: 22; onClicked: pick.open() }
    }
    RowLayout {
        visible: root.step === "loading" || root.step === "applying"
        Layout.fillWidth: true
        Layout.preferredHeight: 80
        Item { Layout.fillWidth: true }
        SSpinner { width: 20; height: 20 }
        SLabel { text: root.step === "loading" ? qsTr("Reading config…") : qsTr("Importing…") }
        Item { Layout.fillWidth: true }
    }

    // шаг 2
    ColumnLayout {
        visible: root.step === "preview"
        Layout.fillWidth: true
        spacing: 8
        RowLayout {
            Layout.fillWidth: true
            SLabel { Layout.fillWidth: true; sub: true; mono: true; text: root.preview.source || "" }
            SButton { compact: true; kind: "ghost"; text: qsTr("Select all"); onClicked: root.setAll(true) }
            SButton { compact: true; kind: "ghost"; text: qsTr("Clear"); onClicked: root.setAll(false) }
        }
        SLabel {
            visible: root.items.length === 0
            Layout.fillWidth: true
            text: qsTr("No hosts found in this file.")
            sub: true
        }
        Rectangle {
            visible: root.items.length > 0
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(root.items.length * 46, 280)
            radius: Theme.radius
            color: Theme.inputFill
            border.color: Theme.line
            clip: true
            ListView {
                anchors.fill: parent
                model: root.items
                clip: true
                ScrollBar.vertical: ScrollBar {}
                delegate: Item {
                    required property var modelData
                    readonly property bool creatable: modelData.action === "create"
                    width: ListView.view.width
                    height: 46
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 10
                        anchors.rightMargin: 10
                        spacing: 10
                        SCheckBox {
                            enabled: creatable
                            checked: !!root.selected[modelData.key]
                            onToggled: { var s = root.selected; s[modelData.key] = checked; root.selected = s; root.recount() }
                            Accessible.name: modelData.key
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 1
                            SLabel { Layout.fillWidth: true; text: modelData.key; font.bold: true }
                            SLabel {
                                Layout.fillWidth: true; mono: true; sub: true; font.pixelSize: 11
                                text: creatable
                                      ? ((modelData.connection.user ? modelData.connection.user + "@" : "") + modelData.connection.host + ":" + modelData.connection.port
                                         + (modelData.connection.auth.key_path ? "  " + modelData.connection.auth.key_path : ""))
                                      : qsTr("line %1").arg(modelData.origin ? modelData.origin.line : 0)
                            }
                        }
                        SChip {
                            text: creatable ? qsTr("new") : qsTr("exists — skipped")
                            tone: creatable ? Theme.green : Theme.amber
                        }
                    }
                }
            }
        }
        SLabel {
            Layout.fillWidth: true
            sub: true
            font.pixelSize: 11
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            text: qsTr("Wildcard hosts (Host *) are not shown as servers; their values are applied as defaults. Include files are expanded.")
        }
        Repeater {
            model: root.preview.warning_texts || []
            delegate: SLabel { required property string modelData; Layout.fillWidth: true; color: Theme.amber; font.pixelSize: 11; text: "⚠ " + modelData; wrapMode: Text.Wrap; elide: Text.ElideNone }
        }
        SLabel { visible: root.error !== ""; Layout.fillWidth: true; color: Theme.red; text: root.error; wrapMode: Text.Wrap; elide: Text.ElideNone }
    }

    // шаг 3
    ColumnLayout {
        visible: root.step === "report"
        Layout.fillWidth: true
        spacing: 6
        SLabel { text: qsTr("Created: %1").arg((root.report.created || []).length); color: Theme.green; font.bold: true }
        SLabel { text: qsTr("Skipped: %1").arg((root.report.skipped || []).length) }
        SLabel { text: qsTr("Errors: %1").arg((root.report.failed || []).length); color: (root.report.failed || []).length ? Theme.red : Theme.text }
        Repeater {
            model: root.report.failed || []
            delegate: SLabel { required property var modelData; Layout.fillWidth: true; color: Theme.red; font.pixelSize: 11; text: modelData.key + ": " + modelData.text; wrapMode: Text.Wrap; elide: Text.ElideNone }
        }
    }

    buttons: [
        Item { Layout.fillWidth: true },
        SButton { visible: root.step !== "report"; text: qsTr("Cancel"); onClicked: root.close() },
        SButton { visible: root.step === "path"; kind: "primary"; text: qsTr("Preview"); onClicked: root.load() },
        SButton {
            visible: root.step === "preview"
            kind: "primary"
            enabled: root.selectedCount > 0
            text: qsTr("Import (%1)").arg(root.selectedCount)
            onClicked: {
                var keys = []
                for (var k in root.selected) if (root.selected[k]) keys.push(k)
                root.step = "applying"
                Backend.importApply({ path: pathF.text, previewId: root.preview.preview_id, select: keys })
            }
        },
        SButton { visible: root.step === "report"; kind: "primary"; text: qsTr("Close"); onClicked: root.close() }
    ]
}
