import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Лаунчер: поиск, категории, сетка утилит (неготовые — неактивны с пометкой), действия с сервером.
Popup {
    id: root
    required property var desk
    signal launch(string appId)
    signal reconnectRequested()
    signal closeDesktopRequested()

    parent: Overlay.overlay
    width: 560
    height: Math.min(620, (parent ? parent.height : 700) - 70)
    padding: 0
    modal: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    property string category: "all"
    property string query: ""

    readonly property var cats: [
        { id: "fav", name: qsTr("Favorites") }, { id: "system", name: qsTr("System") }, { id: "network", name: qsTr("Network") },
        { id: "files", name: qsTr("Files") }, { id: "monitoring", name: qsTr("Monitoring") }, { id: "dev", name: qsTr("Development") },
        { id: "all", name: qsTr("All applications") }
    ]
    function filtered() {
        const q = query.toLowerCase()
        return AppRegistry.apps.filter(a => (category === "all" || (category === "fav" ? (a.id === "terminal" || a.id === "monitor") : a.cat === category))
                                && (q === "" || a.name.toLowerCase().indexOf(q) >= 0 || a.id.indexOf(q) >= 0))
    }
    function capsOk(a) { return AppRegistry.enabledFor(a, desk.capabilities) }
    onOpened: search.forceFocus()
    onAboutToShow: { query = ""; search.text = "" }

    background: GlassPanel {
        radius: Theme.radiusLarge
        color: Theme.dialogFill
        source: Theme.bgItem
        borderColor: Theme.alpha(Theme.accent, 0.5)
        borderWidth: 1
    }
    contentItem: ColumnLayout {
        spacing: 0
        STextField {
            id: search
            Layout.fillWidth: true; Layout.margins: 14
            placeholderText: qsTr("Search utilities, services, files…")
            onEdited: root.query = text
            onAccepted: { const f = root.filtered().filter(a => root.capsOk(a)); if (f.length) { root.launch(f[0].id); root.close() } }
        }
        RowLayout {
            Layout.fillWidth: true; Layout.fillHeight: true
            Layout.leftMargin: 10; Layout.rightMargin: 14
            spacing: 10
            ColumnLayout {
                Layout.preferredWidth: 150; Layout.minimumWidth: 150; Layout.maximumWidth: 150; Layout.alignment: Qt.AlignTop; spacing: 2
                Repeater {
                    model: root.cats
                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true; Layout.preferredHeight: 30; radius: 6
                        color: root.category === modelData.id ? Theme.selected : (cma.containsMouse ? Theme.hover : "transparent")
                        SLabel { x: 10; anchors.verticalCenter: parent.verticalCenter; text: modelData.name; color: root.category === modelData.id ? Theme.text : Theme.textSub }
                        MouseArea { id: cma; anchors.fill: parent; hoverEnabled: true; onClicked: root.category = modelData.id }
                    }
                }
            }
            GridView {
                id: grid
                Layout.fillWidth: true; Layout.fillHeight: true; Layout.minimumWidth: 250
                cellWidth: 124; cellHeight: 100
                clip: true
                model: root.filtered()
                delegate: Item {
                    required property var modelData
                    readonly property bool on: root.capsOk(modelData)
                    width: grid.cellWidth; height: grid.cellHeight
                    Rectangle {
                        anchors.fill: parent; anchors.margins: 4; radius: 8
                        color: on && gma.containsMouse ? Theme.hover : Theme.card
                        border.color: Theme.line
                        opacity: on ? 1 : 0.5
                        Column {
                            anchors.centerIn: parent; spacing: 6
                            Rectangle {
                                width: 40; height: 40; radius: 10; anchors.horizontalCenter: parent.horizontalCenter
                                color: Theme.alpha(Theme.cyan, 0.16); border.color: Theme.alpha(Theme.cyan, 0.5)
                                SLabel { anchors.centerIn: parent; text: modelData.glyph; mono: true; color: Theme.cyan; font.pixelSize: 17; font.bold: true }
                            }
                            SLabel { text: modelData.name; width: 108; horizontalAlignment: Text.AlignHCenter; font.pixelSize: Theme.sizeSmall }
                        }
                        SChip { visible: !on; anchors.top: parent.top; anchors.right: parent.right; anchors.margins: 5; text: modelData.ready ? qsTr("n/a") : qsTr("soon"); tone: Theme.amber; mono: false }
                        MouseArea { id: gma; anchors.fill: parent; hoverEnabled: true; enabled: on; cursorShape: Qt.PointingHandCursor; onClicked: { root.launch(modelData.id); root.close() } }
                        Accessible.role: Accessible.Button
                        Accessible.name: modelData.name + (on ? "" : " (" + qsTr("soon") + ")")
                    }
                }
            }
        }
        Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.line }
        RowLayout {
            Layout.fillWidth: true; Layout.margins: 12; spacing: 8
            SButton { text: qsTr("Reconnect"); compact: true; onClicked: { root.close(); root.reconnectRequested() } }
            Item { Layout.fillWidth: true }
            SButton { text: qsTr("Close desktop"); compact: true; kind: "danger"; onClicked: { root.close(); root.closeDesktopRequested() } }
        }
    }
}
