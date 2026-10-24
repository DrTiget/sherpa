import QtQuick
import QtQuick.Layouts
import Sherpa

// Вкладки верхнего уровня: «Серверы» (Дом) всегда первая, дальше — рабочие столы серверов.
ColumnLayout {
    id: root
    spacing: 0
    property int current: 0
    property var extraTabs: []          // [{connectionId, name, state}]
    default property alias pages: stack.data
    signal closeTab(string connectionId)

    Rectangle {
        Layout.fillWidth: true
        Layout.preferredHeight: 38
        color: Theme.alpha(Theme.panelFill, Theme.translucent ? 0.7 : 1)
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
        Row {
            anchors.verticalCenter: parent.verticalCenter
            x: 12
            spacing: 6
            Repeater {
                model: [{ connectionId: "", name: qsTr("Servers"), state: "" }].concat(root.extraTabs)
                delegate: Rectangle {
                    id: tab
                    required property var modelData
                    required property int index
                    readonly property bool on: root.current === index
                    height: 28
                    width: row.implicitWidth + 24
                    radius: Theme.radius
                    color: on ? Theme.alpha(Theme.accent, 0.25) : (ma.containsMouse ? Theme.hover : "transparent")
                    border.color: on ? Theme.selectedBorder : "transparent"
                    MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; onClicked: root.current = tab.index }
                    Row {
                        id: row
                        anchors.centerIn: parent
                        spacing: 7
                        Rectangle {
                            visible: tab.index > 0
                            width: 8; height: 8; radius: 4; anchors.verticalCenter: parent.verticalCenter
                            color: modelData.state === "ready" ? Theme.green : (modelData.state === "connecting" ? Theme.amber : Theme.red)
                        }
                        SLabel { text: modelData.name; font.bold: true; color: tab.on ? Theme.text : Theme.textSub; anchors.verticalCenter: parent.verticalCenter }
                        SLabel {
                            visible: tab.index > 0
                            text: "×"; sub: true; font.pixelSize: 15; anchors.verticalCenter: parent.verticalCenter
                            MouseArea { anchors.fill: parent; anchors.margins: -5; cursorShape: Qt.PointingHandCursor; onClicked: root.closeTab(modelData.connectionId) }
                            Accessible.role: Accessible.Button
                            Accessible.name: qsTr("Close tab")
                        }
                    }
                    Accessible.role: Accessible.PageTab
                    Accessible.name: modelData.name
                }
            }
        }
    }
    StackLayout {
        id: stack
        Layout.fillWidth: true
        Layout.fillHeight: true
        currentIndex: root.current
    }
}
