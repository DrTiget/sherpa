import QtQuick
import Sherpa
import QtQuick.Dialogs
import QtQuick.Layouts

// Первый запуск (SCREENS.md 2.1)
Item {
    id: root
    signal importRequested()
    property string error: ""

    FileDialog {
        id: saveDlg
        title: qsTr("Save key file copy")
        fileMode: FileDialog.SaveFile
        onAccepted: Backend.exportKey(selectedFile.toString())
    }
    Connections {
        target: Backend
        function onExportFinished(ok, path, message) {
            if (!root.visible) return
            if (ok) Backend.finishFirstRun(); else root.error = message
        }
    }

    GlassPanel {
        anchors.centerIn: parent
        width: Math.min(560, parent.width - 40)
        height: col.implicitHeight + 56
        radius: Theme.radiusLarge
        color: Theme.dialogFill
        borderColor: Theme.alpha(Theme.accent, 0.6)
        borderWidth: 1
        source: Theme.bgItem

        ColumnLayout {
            id: col
            anchors.fill: parent
            anchors.margins: 28
            spacing: 14
            Rectangle {
                Layout.alignment: Qt.AlignHCenter
                width: 64; height: 64; radius: 16
                color: Theme.alpha(Theme.accent, 0.22)
                border.color: Theme.alpha(Theme.cyan, 0.6)
                SLabel { anchors.centerIn: parent; text: ">_"; mono: true; color: Theme.cyan; font.pixelSize: 24; font.bold: true }
            }
            SLabel { Layout.alignment: Qt.AlignHCenter; text: qsTr("Welcome to Sherpa"); font.pixelSize: Theme.sizeTitle; font.bold: true }
            SLabel {
                Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; elide: Text.ElideNone; sub: true
                text: qsTr("Passwords and keys are encrypted with a key file. Without it saved secrets cannot be restored — keep a copy somewhere safe.")
            }
            SLabel { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; mono: true; text: Backend.displayPath(Backend.keyPath); elide: Text.ElideMiddle }
            SLabel { visible: root.error !== ""; Layout.fillWidth: true; color: Theme.red; text: root.error; wrapMode: Text.Wrap; elide: Text.ElideNone; horizontalAlignment: Text.AlignHCenter }
            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                spacing: 10
                SButton { text: qsTr("Later"); onClicked: Backend.finishFirstRun() }
                SButton { text: qsTr("Save copy…"); kind: "primary"; onClicked: saveDlg.open() }
            }
            SLabel {
                Layout.alignment: Qt.AlignHCenter
                text: qsTr("Import from ~/.ssh/config")
                color: Theme.cyan
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { Backend.finishFirstRun(); root.importRequested() } }
                Accessible.role: Accessible.Link
                Accessible.name: text
            }
        }
    }
}
