import QtQuick
import QtQuick.Layouts
import Sherpa

// Окно терминала: вкладки сессий внутри окна («+» — новая сессия к тому же серверу), у каждой вкладки свой TerminalPane.
Item {
    id: root
    required property var session           // первая сессия
    property var desktop: null
    property var sessions: []
    property int current: 0
    readonly property var currentSession: sessions.length > current ? sessions[current] : null
    readonly property string currentTitle: currentSession && currentSession.title !== "" ? currentSession.title : qsTr("Terminal")
    property bool active: false             // есть живая сессия (для подтверждения закрытия окна)
    signal editConnection()
    signal allClosed()
    property alias tabsRepeater: panes

    function refreshActive() { active = sessions.some(s => s.active) }
    function addTab(s) {
        const ns = s || (desktop ? desktop.newTerminal() : null)
        if (!ns) return null
        ns.stateChanged.connect(refreshActive)
        sessions = sessions.concat([ns])
        current = sessions.length - 1
        refreshActive()
        return ns
    }
    function closeTab(i) {
        if (i < 0 || i >= sessions.length) return
        const s = sessions[i]
        s.close()
        sessions = sessions.filter((x, k) => k !== i)
        if (sessions.length === 0) { allClosed(); return }
        current = Math.min(current, sessions.length - 1)
        refreshActive()
    }
    // для DesktopPage: закрыть все сессии окна
    function close() { for (const s of sessions) s.close() }
    function focusCurrent() { const p = panes.itemAt(current); if (p) p.terminal.forceActiveFocus() }
    function paneAt(i) { return panes.itemAt(i) }
    Component.onCompleted: addTab(session)
    onCurrentChanged: Qt.callLater(focusCurrent)

    Shortcut { sequence: "Ctrl+Shift+T"; enabled: root.visible; onActivated: root.addTab() }
    Shortcut { sequence: "Ctrl+PgDown"; enabled: root.visible && root.sessions.length > 1; onActivated: root.current = (root.current + 1) % root.sessions.length }
    Shortcut { sequence: "Ctrl+PgUp"; enabled: root.visible && root.sessions.length > 1; onActivated: root.current = (root.current + root.sessions.length - 1) % root.sessions.length }

    // полоса вкладок
    Rectangle {
        id: bar
        width: parent.width
        height: 30
        color: Theme.alpha("#05070c", 0.75)
        Row {
            anchors.verticalCenter: parent.verticalCenter
            x: 6
            spacing: 4
            Repeater {
                model: root.sessions.length
                delegate: Rectangle {
                    id: tab
                    required property int index
                    readonly property var s: root.sessions[index]
                    readonly property bool on: root.current === index
                    height: 24
                    width: Math.min(190, lbl.implicitWidth + 50)
                    radius: 5
                    color: on ? Theme.alpha(Theme.accent, 0.3) : (tma.containsMouse ? Theme.hover : "transparent")
                    border.color: on ? Theme.selectedBorder : "transparent"
                    Row {
                        anchors.verticalCenter: parent.verticalCenter; x: 8; spacing: 6
                        Rectangle {
                            width: 7; height: 7; radius: 4; anchors.verticalCenter: parent.verticalCenter
                            color: !tab.s ? Theme.gray : tab.s.state === "connected" ? Theme.green : (tab.s.state === "connecting" || tab.s.state === "awaiting" ? Theme.amber : (tab.s.state === "lost" ? Theme.amber : Theme.red))
                        }
                        SLabel {
                            id: lbl
                            width: Math.min(implicitWidth, 120)
                            anchors.verticalCenter: parent.verticalCenter
                            font.pixelSize: 12
                            color: "#e6eaf2"
                            text: tab.s && tab.s.title !== "" ? tab.s.title : qsTr("Terminal %1").arg(tab.index + 1)
                        }
                        SLabel {
                            text: "×"; font.pixelSize: 14; color: "#8b93a7"; anchors.verticalCenter: parent.verticalCenter
                            MouseArea { anchors.fill: parent; anchors.margins: -4; cursorShape: Qt.PointingHandCursor; onClicked: root.closeTab(tab.index) }
                            Accessible.role: Accessible.Button
                            Accessible.name: qsTr("Close tab")
                        }
                    }
                    MouseArea { id: tma; anchors.fill: parent; anchors.rightMargin: 24; hoverEnabled: true; onClicked: root.current = tab.index }
                    Accessible.role: Accessible.PageTab
                    Accessible.name: lbl.text
                }
            }
            Rectangle {
                width: 24; height: 24; radius: 5
                color: pma.containsMouse ? Theme.hover : "transparent"
                SLabel { anchors.centerIn: parent; text: "+"; color: Theme.cyan; font.pixelSize: 16 }
                MouseArea { id: pma; anchors.fill: parent; hoverEnabled: true; onClicked: root.addTab() }
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("New tab")
            }
        }
    }

    Repeater {
        id: panes
        model: root.sessions.length
        delegate: TerminalPane {
            required property int index
            session: root.sessions[index]
            visible: root.current === index
            x: 0; y: bar.height
            width: root.width
            height: root.height - bar.height
            onEditConnection: root.editConnection()
        }
    }
}
