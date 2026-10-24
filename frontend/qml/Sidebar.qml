import QtQuick
import Sherpa
import QtQuick.Controls.Basic
import QtQuick.Layouts

// Боковая панель: все серверы / группы / теги / импорт
GlassPanel {
    id: root
    color: Theme.panelFill
    signal importRequested()
    signal groupMenu(string id, string name)
    signal newGroupRequested()

    component Item2: Rectangle {
        id: it
        property string text: ""
        property string count: ""
        property bool active: false
        property int indent: 0
        signal clicked()
        signal rightClicked()
        Layout.fillWidth: true
        implicitHeight: 32
        radius: Theme.radius
        color: active ? Theme.selected : (ma.containsMouse ? Theme.hover : "transparent")
        border.color: active ? Theme.selectedBorder : "transparent"
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 10 + it.indent * 14
            anchors.rightMargin: 10
            SLabel { Layout.fillWidth: true; text: it.text; color: it.active ? Theme.text : Theme.textSub; font.bold: it.active }
            SLabel { text: it.count; mono: true; sub: true; font.pixelSize: 11 }
        }
        MouseArea {
            id: ma
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            cursorShape: Qt.PointingHandCursor
            onClicked: (e) => e.button === Qt.RightButton ? it.rightClicked() : it.clicked()
        }
        Accessible.role: Accessible.Button
        Accessible.name: text
    }

    Flickable {
        anchors.fill: parent
        anchors.margins: 8
        contentHeight: col.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ColumnLayout {
            id: col
            width: parent.width
            spacing: 2

            Item2 {
                text: qsTr("All servers")
                count: String(Backend.servers.totalCount)
                active: Backend.groupFilter === "" && Backend.tagFilter === ""
                onClicked: { Backend.groupFilter = ""; Backend.tagFilter = "" }
            }
            RowLayout {
                Layout.topMargin: 12
                Layout.leftMargin: 10
                SLabel { Layout.fillWidth: true; caps: true; text: qsTr("Groups") }
                SLabel {
                    text: "+"; color: Theme.cyan; font.pixelSize: 16
                    MouseArea { anchors.fill: parent; anchors.margins: -5; cursorShape: Qt.PointingHandCursor; onClicked: root.newGroupRequested() }
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("New group")
                }
            }
            Repeater {
                model: Backend.groups
                delegate: Item2 {
                    required property var modelData
                    text: modelData.name
                    indent: modelData.depth
                    count: String(modelData.total_count)
                    active: Backend.groupFilter === modelData.id
                    onClicked: { Backend.tagFilter = ""; Backend.groupFilter = modelData.id }
                    onRightClicked: root.groupMenu(modelData.id, modelData.name)
                }
            }
            Item2 {
                text: qsTr("Without group")
                active: Backend.groupFilter === "__none__"
                onClicked: { Backend.tagFilter = ""; Backend.groupFilter = "__none__" }
            }
            SLabel { Layout.topMargin: 12; Layout.leftMargin: 10; caps: true; text: qsTr("Tags"); visible: Backend.tags.length > 0 }
            Flow {
                Layout.fillWidth: true
                Layout.leftMargin: 6
                spacing: 5
                Repeater {
                    model: Backend.tags
                    delegate: SChip {
                        required property var modelData
                        text: "#" + modelData.name
                        tone: Theme.cyan
                        active: Backend.tagFilter === modelData.name
                        onClicked: Backend.tagFilter = active ? "" : modelData.name
                    }
                }
            }
            Item2 {
                Layout.topMargin: 14
                text: qsTr("Import from ~/.ssh/config")
                onClicked: root.importRequested()
            }
        }
    }
}
