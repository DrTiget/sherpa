import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Компактный список серверов, выезжающий из панели задач — переход на другой рабочий стол.
Popup {
    id: root
    signal serverChosen(string id, string name)
    signal homeRequested()
    parent: Overlay.overlay
    width: 300
    height: Math.min(460, (parent ? parent.height : 600) - 70)
    padding: 0
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: GlassPanel { radius: Theme.radiusLarge; color: Theme.dialogFill; source: Theme.bgItem; borderColor: Theme.alpha(Theme.accent, 0.5); borderWidth: 1 }
    contentItem: ColumnLayout {
        spacing: 0
        STextField { id: q; Layout.fillWidth: true; Layout.margins: 10; placeholderText: qsTr("Search"); onEdited: Backend.searchText = text }
        ListView {
            Layout.fillWidth: true; Layout.fillHeight: true; Layout.leftMargin: 6; Layout.rightMargin: 6
            clip: true; spacing: 2
            model: Backend.servers
            delegate: Rectangle {
                required property string serverId
                required property string name
                required property string host
                required property string state
                required property bool checking
                width: ListView.view.width; height: 40; radius: 6
                color: ma.containsMouse ? Theme.hover : "transparent"
                RowLayout {
                    anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 8; spacing: 8
                    SStatusDot { state: parent.parent.state; checking: parent.parent.checking }
                    ColumnLayout {
                        Layout.fillWidth: true; spacing: 0
                        SLabel { Layout.fillWidth: true; text: name; font.bold: true }
                        SLabel { Layout.fillWidth: true; text: host; mono: true; sub: true; font.pixelSize: 10 }
                    }
                }
                MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; onClicked: { root.close(); root.serverChosen(serverId, name) } }
            }
        }
        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.line }
        SButton { Layout.alignment: Qt.AlignRight; Layout.margins: 8; compact: true; text: qsTr("Home"); onClicked: { root.close(); root.homeRequested() } }
    }
}
