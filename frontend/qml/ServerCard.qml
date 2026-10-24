import QtQuick
import Sherpa
import QtQuick.Layouts

// Карточка выбранного сервера
Item {
    id: root
    signal editRequested(string id)
    signal deleteRequested(string id)
    signal terminalRequested(string id)
    readonly property var s: Backend.selected
    readonly property bool has: !!s.id
    property var hk: ({})
    function loadHk() { if (has) Backend.hostKeyFor(s.host, s.port); else hk = ({}) }
    onHasChanged: loadHk()
    Connections {
        target: Backend
        function onHostKeyLoaded(host, port, key) { if (host === root.s.host && port === root.s.port) root.hk = key }
        function onSelectedChanged() { root.loadHk() }
    }

    SEmptyState {
        anchors.fill: parent
        visible: !root.has
        glyph: "◧"
        title: qsTr("Select a server")
        description: qsTr("Details and actions appear here.")
    }

    Flickable {
        anchors.fill: parent
        anchors.margins: 22
        visible: root.has
        contentHeight: col.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ColumnLayout {
            id: col
            width: parent.width
            spacing: 14

            RowLayout {
                spacing: 10
                SStatusDot { size: 12; state: root.s.state || "unknown"; checking: !!root.s.checking }
                SLabel { Layout.fillWidth: true; text: root.s.name || ""; font.pixelSize: Theme.sizeTitle; font.bold: true }
            }
            SLabel {
                Layout.fillWidth: true
                mono: true
                color: root.s.checking ? Theme.cyan : Theme.stateColor(root.s.state)
                text: root.s.checking ? qsTr("checking…")
                      : Theme.stateText(root.s.state, root.s.latency_ms === undefined ? -1 : root.s.latency_ms)
                        + (root.s.checked_at ? "  ·  " + Backend.formatTime(root.s.checked_at) : "")
            }
            SLabel {
                visible: !!root.s.error_text
                Layout.fillWidth: true
                text: root.s.error_text || ""
                color: Theme.red
                wrapMode: Text.Wrap
                elide: Text.ElideNone
            }

            Flow {
                Layout.fillWidth: true
                spacing: 8
                SButton { text: qsTr("Terminal (soon)"); kind: "primary"; enabled: false; onClicked: root.terminalRequested(root.s.id) }
                SButton { text: qsTr("Check"); busy: !!root.s.checking; enabled: !root.s.checking; onClicked: Backend.checkOne(root.s.id) }
                SButton { text: qsTr("Edit"); onClicked: root.editRequested(root.s.id) }
                SButton { text: qsTr("Duplicate"); onClicked: Backend.duplicateConnection(root.s.id) }
                SButton { text: qsTr("Delete"); kind: "danger"; onClicked: root.deleteRequested(root.s.id) }
            }

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: grid.implicitHeight + 24
                radius: 8
                color: Theme.card
                border.color: Theme.line
                GridLayout {
                    id: grid
                    anchors.fill: parent
                    anchors.margins: 12
                    columns: 2
                    columnSpacing: 16
                    rowSpacing: 9
                    SLabel { caps: true; text: qsTr("Host") }
                    SLabel { Layout.fillWidth: true; mono: true; text: root.s.host || "" }
                    SLabel { caps: true; text: qsTr("Port") }
                    SLabel { Layout.fillWidth: true; mono: true; text: String(root.s.port || "") }
                    SLabel { caps: true; text: qsTr("User") }
                    SLabel { Layout.fillWidth: true; mono: true; text: root.s.user || "—" }
                    SLabel { caps: true; text: qsTr("Sign-in") }
                    SLabel {
                        Layout.fillWidth: true; mono: true
                        text: {
                            var a = root.s.auth || ({})
                            if (a.method === "password") return qsTr("password") + (a.has_password ? "" : " (" + qsTr("not saved") + ")")
                            if (a.method === "key_file") return qsTr("key file") + " " + (a.key_path || "")
                            return qsTr("key text") + (a.key_type ? " " + a.key_type : "")
                        }
                    }
                    SLabel { caps: true; text: qsTr("Group") }
                    SLabel { Layout.fillWidth: true; text: root.s.group_name || "—" }
                    SLabel { caps: true; text: qsTr("Tags"); Layout.alignment: Qt.AlignTop }
                    Flow {
                        Layout.fillWidth: true; spacing: 5
                        Repeater { model: root.s.tags || []; delegate: SChip { required property string modelData; text: "#" + modelData; tone: Theme.cyan } }
                        SLabel { visible: !(root.s.tags && root.s.tags.length); text: "—" }
                    }
                    SLabel { caps: true; text: qsTr("Description"); Layout.alignment: Qt.AlignTop }
                    SLabel { Layout.fillWidth: true; text: root.s.description || "—"; wrapMode: Text.Wrap; elide: Text.ElideNone }
                    SLabel { caps: true; text: qsTr("Host key") }
                    SLabel { Layout.fillWidth: true; mono: true; sub: true; font.pixelSize: 11; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone; text: root.hk.fingerprint ? root.hk.key_type + "\n" + root.hk.fingerprint + "\n" + Backend.formatTime(root.hk.accepted_at || "") : qsTr("not saved yet") }
                }
            }
        }
    }
}
