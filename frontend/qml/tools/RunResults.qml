import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Результаты пакетного запуска: по серверам (код выхода, время, вывод) или «Сравнение» одинаковых выводов.
ColumnLayout {
    id: root
    required property var runner
    property bool compare: false
    property int sel: -1
    spacing: 6
    RowLayout {
        Layout.fillWidth: true
        SLabel { caps: true; Layout.fillWidth: true
                 text: qsTr("Results") + (root.runner.results.length ? " · " + root.runner.results.filter(r => r.state !== "pending").length + "/" + root.runner.results.length : "")
                       + (root.runner.failed ? " · " + qsTr("%1 failed").arg(root.runner.failed) : "") }
        SSpinner { visible: root.runner.running; width: 14; height: 14 }
        SSegmented { model: [{ value: "servers", text: qsTr("By server") }, { value: "compare", text: qsTr("Compare") }]; current: root.compare ? "compare" : "servers"
                     onActivated: (v) => root.compare = v === "compare" }
    }
    DataTable {
        objectName: "resultTable"
        visible: !root.compare
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredHeight: 2
        rows: root.runner.results; compactRows: true
        emptyText: qsTr("No runs yet")
        columns: [
            { key: "name", title: qsTr("Server"), w: 130 },
            { key: "state", title: qsTr("Result"), w: 100, chip: true,
              text: r => r.state === "pending" ? qsTr("running") : (r.state === "error" ? qsTr("error") : qsTr("exit %1").arg(r.exit)),
              tone: r => r.state === "pending" ? Theme.textSub : (r.state === "error" || r.exit !== 0 ? Theme.red : Theme.green) },
            { key: "elapsed", title: qsTr("Time"), w: 70, right: true, mono: true, text: r => r.state === "ok" ? (r.elapsed / 1000).toFixed(2) + " s" : "—" },
            { key: "out", title: qsTr("Output"), flex: 1, mono: true, text: r => r.state === "error" ? r.error : ((r.stdout || r.stderr || "").trim().split("\n")[0] || "—") }
        ]
        onRowClicked: (i, r) => root.sel = root.runner.results.findIndex(x => x.id === r.id)
    }
    ListView {
        visible: root.compare
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredHeight: 2
        clip: true; spacing: 6
        model: root.runner.groups
        delegate: Rectangle {
            required property var modelData
            width: ListView.view.width; implicitHeight: gc.implicitHeight + 16; height: implicitHeight
            radius: 8; color: Theme.card; border.color: modelData.error ? Theme.alpha(Theme.red, 0.5) : Theme.line
            ColumnLayout {
                id: gc; anchors.fill: parent; anchors.margins: 8; spacing: 4
                RowLayout {
                    SLabel { font.bold: true; color: modelData.error ? Theme.red : Theme.cyan
                             text: qsTr("%1 of %2").arg(modelData.count).arg(root.runner.results.length) + (modelData.error ? "" : (modelData.exit !== 0 ? " · " + qsTr("exit %1").arg(modelData.exit) : "")) }
                    SLabel { Layout.fillWidth: true; sub: true; text: modelData.names.join(", ") }
                }
                SLabel { Layout.fillWidth: true; mono: true; font.pixelSize: 11; elide: Text.ElideNone; wrapMode: Text.Wrap; maximumLineCount: 6
                         text: modelData.sample || qsTr("(empty output)") }
            }
        }
    }
    Rectangle {
        visible: !root.compare && root.sel >= 0 && root.sel < root.runner.results.length
        Layout.fillWidth: true; Layout.preferredHeight: 130
        radius: 8; color: Theme.inputFill; border.color: Theme.line
        STextArea {
            anchors.fill: parent; readOnly: true; wrap: false; color: "transparent"; border.width: 0
            text: root.sel >= 0 && root.sel < root.runner.results.length
                  ? (root.runner.results[root.sel].state === "error" ? root.runner.results[root.sel].error
                     : root.runner.results[root.sel].stdout + (root.runner.results[root.sel].stderr ? "\n[stderr]\n" + root.runner.results[root.sel].stderr : "")) : ""
        }
    }
}
