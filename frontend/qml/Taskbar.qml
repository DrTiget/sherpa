import QtQuick
import QtQuick.Layouts
import Sherpa

// Панель задач 44 px: лаунчер, серверы, окна, трей (CPU / RAM / load / uptime / связь / часы).
GlassPanel {
    id: bar
    required property var desk
    property var windowList: []         // [{id, title, glyph, active, minimized}]
    property bool launcherOpen: false
    signal launcherClicked()
    signal serversClicked()
    signal windowClicked(string id)
    signal monitorClicked()

    readonly property var m: desk.monitor
    color: Theme.panelFill
    height: 44

    function fmtUp(s) {
        if (s === undefined) return "—"
        const d = Math.floor(s / 86400), h = Math.floor(s % 86400 / 3600), mi = Math.floor(s % 3600 / 60)
        return d > 0 ? d + qsTr("d") + " " + h + qsTr("h") : h + qsTr("h") + " " + mi + qsTr("m")
    }
    Rectangle { width: parent.width; height: 1; color: Theme.line }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 8
        anchors.rightMargin: 10
        spacing: 6

        Rectangle {
            Layout.preferredWidth: 34; Layout.preferredHeight: 32; radius: 7
            color: bar.launcherOpen ? Theme.selected : (lma.containsMouse ? Theme.hover : "transparent")
            border.color: bar.launcherOpen ? Theme.selectedBorder : "transparent"
            SLabel { anchors.centerIn: parent; text: "S"; font.pixelSize: 17; font.bold: true; color: Theme.cyan }
            MouseArea { id: lma; anchors.fill: parent; hoverEnabled: true; onClicked: bar.launcherClicked() }
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Launcher")
        }
        Rectangle {
            Layout.preferredWidth: svl.implicitWidth + 20; Layout.preferredHeight: 32; radius: 7
            color: sma.containsMouse ? Theme.hover : "transparent"
            SLabel { id: svl; anchors.centerIn: parent; text: qsTr("Servers"); color: Theme.textSub }
            MouseArea { id: sma; anchors.fill: parent; hoverEnabled: true; onClicked: bar.serversClicked() }
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Servers")
        }
        Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 24; color: Theme.line }

        Repeater {
            model: bar.windowList
            delegate: Rectangle {
                required property var modelData
                Layout.preferredWidth: Math.min(170, tl.implicitWidth + 50); Layout.preferredHeight: 32; radius: 7
                color: modelData.active ? Theme.selected : (wma.containsMouse ? Theme.hover : Theme.card)
                border.color: modelData.active ? Theme.selectedBorder : Theme.line
                opacity: modelData.minimized ? 0.65 : 1
                Rectangle { visible: !modelData.minimized; anchors.bottom: parent.bottom; anchors.horizontalCenter: parent.horizontalCenter; width: modelData.active ? 22 : 10; height: 2; radius: 1; color: Theme.cyan }
                Row {
                    anchors.verticalCenter: parent.verticalCenter; x: 8; spacing: 6
                    SLabel { text: modelData.glyph; mono: true; color: Theme.cyan; font.bold: true; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter }
                    SLabel { id: tl; text: modelData.title; width: Math.min(implicitWidth, 120); anchors.verticalCenter: parent.verticalCenter; font.pixelSize: Theme.sizeSmall }
                }
                MouseArea { id: wma; anchors.fill: parent; hoverEnabled: true; onClicked: bar.windowClicked(modelData.id) }
                Accessible.role: Accessible.Button
                Accessible.name: modelData.title
            }
        }
        Item { Layout.fillWidth: true }

        // трей
        MouseArea {
            Layout.preferredWidth: trayRow.implicitWidth + 12; Layout.fillHeight: true
            cursorShape: Qt.PointingHandCursor
            onClicked: bar.monitorClicked()
            Row {
                id: trayRow
                anchors.centerIn: parent
                spacing: 14
                Row {
                    spacing: 6; anchors.verticalCenter: parent.verticalCenter
                    SLabel { text: "CPU"; caps: true; anchors.verticalCenter: parent.verticalCenter }
                    Rectangle { width: 38; height: 5; radius: 2; anchors.verticalCenter: parent.verticalCenter; color: Theme.alpha(Theme.text, 0.12)
                                Rectangle { width: parent.width * Math.min(1, (m.cpu && m.cpu.usage_pct ? m.cpu.usage_pct : 0) / 100); height: 5; radius: 2; color: Theme.cyan } }
                    SLabel { text: m.cpu && m.cpu.usage_pct !== null && m.cpu.usage_pct !== undefined ? m.cpu.usage_pct.toFixed(0) + "%" : "—"; mono: true; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter; width: 32 }
                }
                Row {
                    spacing: 6; anchors.verticalCenter: parent.verticalCenter
                    SLabel { text: "RAM"; caps: true; anchors.verticalCenter: parent.verticalCenter }
                    Rectangle { width: 38; height: 5; radius: 2; anchors.verticalCenter: parent.verticalCenter; color: Theme.alpha(Theme.text, 0.12)
                                Rectangle { width: parent.width * Math.min(1, (m.memory && m.memory.usage_pct ? m.memory.usage_pct : 0) / 100); height: 5; radius: 2; color: Theme.purple } }
                    SLabel { text: m.memory && m.memory.usage_pct !== null && m.memory.usage_pct !== undefined ? m.memory.usage_pct.toFixed(0) + "%" : "—"; mono: true; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter; width: 32 }
                }
                SLabel { text: qsTr("load") + " " + (m.load ? m.load["1m"] : "—"); mono: true; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter }
                SLabel { text: qsTr("up") + " " + bar.fmtUp(m.uptime_sec); mono: true; font.pixelSize: 11; sub: true; anchors.verticalCenter: parent.verticalCenter }
            }
        }
        Row {
            spacing: 8
            Rectangle {
                width: 9; height: 9; radius: 5; anchors.verticalCenter: parent.verticalCenter
                color: bar.desk.state === "ready" ? Theme.green : (bar.desk.state === "connecting" ? Theme.amber : Theme.red)
                Accessible.name: bar.desk.state
            }
            SLabel { mono: true; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter
                     text: m.timestamp ? Qt.formatTime(new Date(m.timestamp), "HH:mm") + " UTC" : "—" }
        }
    }
}
