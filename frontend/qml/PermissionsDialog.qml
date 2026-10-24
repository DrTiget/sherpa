import QtQuick
import QtQuick.Layouts
import Sherpa

// Права и владелец файла: флажки rwx, восьмеричное значение, uid/gid, «с правами root (sudo)».
SDialog {
    id: root
    property string path: ""
    property string fileName: ""
    property int mode: 420                 // 0o644
    property bool wasOctalEdit: false
    signal apply(int mode, var uid, var gid, bool privileged)

    title: qsTr("Permissions: %1").arg(fileName)
    dialogWidth: 460

    function openFor(entry) {
        path = entry.path
        fileName = entry.name
        mode = (entry.permissions || 0) & 4095
        uidF.text = entry.uid === null || entry.uid === undefined ? "" : String(entry.uid)
        gidF.text = entry.gid === null || entry.gid === undefined ? "" : String(entry.gid)
        origUid = uidF.text; origGid = gidF.text; origMode = mode
        sudoBox.checked = false
        open()
    }
    property string origUid: ""
    property string origGid: ""
    property int origMode: 0

    function bit(b) { return (mode & b) !== 0 }
    function setBit(b, on) { mode = on ? (mode | b) : (mode & ~b); octal.text = mode.toString(8).padStart(3, "0") }
    onModeChanged: if (!octal.activeFocus) octal.text = mode.toString(8).padStart(3, "0")

    GridLayout {
        Layout.fillWidth: true
        columns: 4
        columnSpacing: 18
        rowSpacing: 6
        Item { width: 1 }
        SLabel { text: qsTr("Read"); caps: true }
        SLabel { text: qsTr("Write"); caps: true }
        SLabel { text: qsTr("Execute"); caps: true }
        Repeater {
            model: [{ n: qsTr("Owner"), s: 6 }, { n: qsTr("Group"), s: 3 }, { n: qsTr("Others"), s: 0 }]
            delegate: Item {
                id: grp
                required property var modelData
                readonly property int sh: modelData.s
                Layout.columnSpan: 4
                Layout.fillWidth: true
                implicitHeight: 28
                Row {
                    spacing: 0
                    SLabel { width: 110; text: grp.modelData.n; anchors.verticalCenter: parent.verticalCenter }
                    Repeater {
                        model: [4, 2, 1]
                        delegate: Item {
                            id: cell
                            required property int modelData
                            width: 96; height: 28
                            SCheckBox {
                                anchors.verticalCenter: parent.verticalCenter
                                checked: root.bit(cell.modelData << grp.sh)
                                onToggled: root.setBit(cell.modelData << grp.sh, checked)
                                Accessible.name: grp.modelData.n + " " + cell.modelData
                            }
                        }
                    }
                }
            }
        }
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: 12
        STextField {
            id: octal
            Layout.preferredWidth: 110
            label: qsTr("Octal")
            mono: true
            validator: RegularExpressionValidator { regularExpression: /[0-7]{0,4}/ }
            onEdited: { if (text !== "") root.mode = parseInt(text, 8) & 4095 }
        }
        STextField { id: uidF; Layout.fillWidth: true; label: "UID"; mono: true; validator: IntValidator { bottom: 0 } }
        STextField { id: gidF; Layout.fillWidth: true; label: "GID"; mono: true; validator: IntValidator { bottom: 0 } }
    }
    SCheckBox { id: sudoBox; text: qsTr("Apply as root (sudo)") }
    SLabel {
        Layout.fillWidth: true; sub: true; font.pixelSize: 11; wrapMode: Text.Wrap; elide: Text.ElideNone
        text: qsTr("Owner changes usually need root. Names are not shown: SFTP returns numeric IDs.")
    }
    buttons: [
        Item { Layout.fillWidth: true },
        SButton { text: qsTr("Cancel"); onClicked: root.close() },
        SButton {
            text: qsTr("Apply"); kind: "primary"
            onClicked: {
                root.close()
                root.apply(root.mode === root.origMode ? -1 : root.mode,
                           uidF.text !== root.origUid && uidF.text !== "" ? parseInt(uidF.text) : null,
                           gidF.text !== root.origGid && gidF.text !== "" ? parseInt(gidF.text) : null,
                           sudoBox.checked)
            }
        }
    ]
}
