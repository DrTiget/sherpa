import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Мультисервер: выбор серверов по группам, одна команда, результаты по серверам и режим «Сравнение».
ToolPane {
    id: root
    readonly property var quick: [{ t: qsTr("Kernel"), c: "uname -r" }, { t: qsTr("Uptime"), c: "uptime" }, { t: qsTr("Disk /"), c: "df -h /" },
                                  { t: qsTr("Memory"), c: "free -m" }, { t: qsTr("OS"), c: "cat /etc/os-release | head -2" }]
    property alias runner: runner
    MultiRunner { id: runner; pane: root }

    RowLayout {
        Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: 10; spacing: 10
        ServerPicker { id: picker; objectName: "serverPicker"; Layout.fillHeight: true; Layout.preferredWidth: 230; currentId: root.serverId }
        ColumnLayout {
            Layout.fillWidth: true; Layout.fillHeight: true; spacing: 8
            Flow {
                Layout.fillWidth: true; spacing: 6
                Repeater { model: root.quick
                    delegate: SChip { required property var modelData; text: modelData.t; mono: false; tone: Theme.cyan
                                      MouseArea { anchors.fill: parent; onClicked: cmd.text = modelData.c } } }
            }
            STextArea { id: cmd; objectName: "multiCommand"; Layout.fillWidth: true; Layout.preferredHeight: 70; placeholderText: qsTr("Command to run on the selected servers") }
            RowLayout {
                SLabel { sub: true; Layout.fillWidth: true; text: picker.selectedIds.length > 1 ? qsTr("Confirmation: type “выполнить”.") : "" }
                SButton { objectName: "multiRunButton"; kind: "primary"; text: qsTr("Run"); busy: runner.running
                          enabled: cmd.text.trim() !== "" && picker.selectedIds.length > 0 && !root.readonlyMode && !runner.running
                          onClicked: runner.start({ command: cmd.text.trim(), targets: picker.selectedIds, title: qsTr("Run on %1 server(s)").arg(picker.selectedIds.length) }) }
            }
            RunResults { runner: runner; Layout.fillWidth: true; Layout.fillHeight: true }
        }
    }
}
