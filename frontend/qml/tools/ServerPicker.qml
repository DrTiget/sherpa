import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Выбор серверов для пакетного запуска: список с флажками, быстрый выбор по группам.
Rectangle {
    id: root
    property string currentId: ""               // сервер этого окна
    property var servers: Backend.allServers()
    property var selected: ({})                 // id → true
    readonly property var selectedIds: servers.filter(s => selected[s.id]).map(s => s.id)
    readonly property var groupNames: {
        const g = []
        for (const s of servers) { const n = s.group_name || ""; if (n !== "" && g.indexOf(n) < 0) g.push(n) }
        return g
    }
    function setAll(on) { const m = {}; if (on) for (const s of servers) m[s.id] = true; selected = m }
    function setOnly(id) { const m = {}; m[id] = true; selected = m }
    function toggleGroup(name) {
        const ids = servers.filter(s => s.group_name === name).map(s => s.id)
        const all = ids.every(i => selected[i])
        const m = Object.assign({}, selected)
        for (const i of ids) { if (all) delete m[i]; else m[i] = true }
        selected = m
    }
    Component.onCompleted: if (currentId !== "") setOnly(currentId)

    radius: 8; color: Theme.card; border.color: Theme.line
    implicitWidth: 230
    ColumnLayout {
        anchors.fill: parent; anchors.margins: 8; spacing: 6
        SLabel { caps: true; text: qsTr("Servers") + " · " + root.selectedIds.length + "/" + root.servers.length }
        Flow {
            Layout.fillWidth: true; spacing: 4
            SChip { text: qsTr("this"); mono: false; tone: Theme.cyan; MouseArea { anchors.fill: parent; onClicked: root.setOnly(root.currentId) } }
            SChip { text: qsTr("all"); mono: false; tone: Theme.cyan; MouseArea { anchors.fill: parent; onClicked: root.setAll(true) } }
            SChip { text: qsTr("none"); mono: false; tone: Theme.cyan; MouseArea { anchors.fill: parent; onClicked: root.setAll(false) } }
            Repeater {
                model: root.groupNames
                delegate: SChip { required property var modelData; text: modelData; mono: false; tone: Theme.purple
                                  MouseArea { anchors.fill: parent; onClicked: root.toggleGroup(modelData) } }
            }
        }
        ListView {
            Layout.fillWidth: true; Layout.fillHeight: true
            clip: true; model: root.servers
            ScrollBar.vertical: ScrollBar {}
            delegate: SCheckBox {
                required property var modelData
                width: ListView.view.width; height: 26
                text: modelData.name
                checked: !!root.selected[modelData.id]
                onToggled: { const m = Object.assign({}, root.selected); if (checked) m[modelData.id] = true; else delete m[modelData.id]; root.selected = m }
            }
        }
    }
}
