import QtQuick
import Sherpa
import QtQuick.Controls.Basic
import QtQuick.Layouts

// Список серверов: строка 48 px, статус, имя, user@host:port, метка группы
Item {
    id: root
    signal openRequested(string id)
    signal addRequested()
    signal importRequested()
    property alias view: list

    ListView {
        id: list
        anchors.fill: parent
        anchors.margins: 10
        spacing: 5
        clip: true
        model: Backend.servers
        currentIndex: Backend.servers.indexOfVisible(Backend.selectedId)
        keyNavigationEnabled: true
        activeFocusOnTab: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        onCurrentIndexChanged: if (activeFocus && currentItem) Backend.selectedId = currentItem.sid
        Keys.onReturnPressed: if (Backend.selectedId !== "") root.openRequested(Backend.selectedId)

        delegate: Rectangle {
            id: row
            required property int index
            required property string serverId
            required property string name
            required property string host
            required property int port
            required property string user
            required property string groupName
            required property string state
            required property bool checking
            required property int latencyMs
            required property string errorText
            readonly property string sid: serverId
            readonly property bool sel: Backend.selectedId === serverId
            width: ListView.view.width
            height: Theme.rowHeight
            radius: 8
            color: sel ? Theme.selected : (ma.containsMouse ? Theme.hover : Theme.card)
            border.color: sel ? Theme.selectedBorder : Theme.line

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 12
                spacing: 11
                SStatusDot { state: row.state; checking: row.checking }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    SLabel { Layout.fillWidth: true; text: row.name; font.bold: true }
                    SLabel {
                        Layout.fillWidth: true; mono: true; sub: true; font.pixelSize: 11
                        text: (row.user ? row.user + "@" : "") + row.host + ":" + row.port
                    }
                }
                SChip { visible: row.groupName !== ""; text: row.groupName; tone: Theme.purple; mono: false }
                SLabel {
                    Layout.preferredWidth: 78
                    horizontalAlignment: Text.AlignRight
                    mono: true; font.pixelSize: 11
                    color: row.checking ? Theme.cyan : Theme.stateColor(row.state)
                    text: row.checking ? qsTr("checking") : Theme.stateText(row.state, row.latencyMs)
                }
            }
            MouseArea {
                id: ma
                anchors.fill: parent
                hoverEnabled: true
                onClicked: { Backend.selectedId = row.serverId; list.forceActiveFocus() }
                onDoubleClicked: root.openRequested(row.serverId)
            }
            Accessible.role: Accessible.ListItem
            Accessible.name: name + ", " + host + ", " + Theme.stateLabel(state)
        }
    }

    // скелетон при загрузке
    Column {
        visible: Backend.loadingList && Backend.servers.totalCount === 0
        anchors.fill: parent
        anchors.margins: 10
        spacing: 5
        Repeater {
            model: 6
            Rectangle {
                width: parent.width; height: Theme.rowHeight; radius: 8; color: Theme.card
                opacity: 0.5 + 0.3 * Math.sin(index)
            }
        }
    }

    SEmptyState {
        anchors.fill: parent
        visible: !Backend.loadingList && Backend.servers.totalCount === 0
        title: qsTr("No servers yet")
        description: qsTr("Add a server or import hosts from your SSH config.")
        primaryText: qsTr("Add server")
        secondaryText: qsTr("Import from ~/.ssh/config")
        onPrimary: root.addRequested()
        onSecondary: root.importRequested()
    }
    SEmptyState {
        anchors.fill: parent
        glyph: "?"
        visible: !Backend.loadingList && Backend.servers.totalCount > 0 && Backend.servers.count === 0
        title: qsTr("Nothing found")
        primaryText: qsTr("Reset filter")
        onPrimary: Backend.resetFilters()
    }
}
