import QtQuick
import Sherpa
import QtQuick.Dialogs
import QtQuick.Layouts

// Настройки (SCREENS.md 2.7): язык, тема, прозрачность, таймаут, ключ-файл.
SDialog {
    id: root
    title: qsTr("Settings")
    dialogWidth: 560
    property string exportError: ""

    FileDialog {
        id: saveDlg
        title: qsTr("Save key file copy")
        fileMode: FileDialog.SaveFile
        currentFile: "file://" + Backend.defaultSshConfig.replace(/\/\.ssh\/config$/, "") + "/sherpa.key"
        onAccepted: Backend.exportKey(selectedFile.toString())
    }
    Connections {
        target: Backend
        function onExportFinished(ok, path, message) { root.exportError = ok ? "" : message }
    }


    SLabel { text: qsTr("Language"); caps: true }
    SSegmented {
        current: Backend.language
        model: [{ value: "ru", text: "Русский" }, { value: "en", text: "English" }]
        onActivated: (v) => Backend.language = v
    }
    SLabel { text: qsTr("Theme"); caps: true }
    SSegmented {
        current: Backend.theme
        model: [{ value: "system", text: qsTr("System") }, { value: "dark", text: qsTr("Dark") }, { value: "light", text: qsTr("Light") }]
        onActivated: (v) => Backend.theme = v
    }
    SSwitch {
        text: qsTr("Window transparency and blur")
        checked: Backend.translucent
        onToggled: Backend.translucent = checked
    }
    STextField {
        Layout.preferredWidth: 160
        label: qsTr("Check timeout, s")
        mono: true
        text: String(Backend.timeoutSec)
        validator: IntValidator { bottom: 1; top: 300 }
        onEdited: if (text !== "") Backend.timeoutSec = parseInt(text)
    }
    SLabel { text: qsTr("Key file"); caps: true }
    SLabel { Layout.fillWidth: true; mono: true; text: Backend.displayPath(Backend.keyPath); elide: Text.ElideMiddle }
    SLabel {
        Layout.fillWidth: true; sub: true; font.pixelSize: 11; wrapMode: Text.Wrap; elide: Text.ElideNone
        text: Backend.backupPending ? qsTr("No backup copy has been saved yet. Without the key file saved secrets cannot be restored.")
                                    : qsTr("Saved secrets are encrypted with this file.")
    }
    SLabel { visible: root.exportError !== ""; Layout.fillWidth: true; color: Theme.red; text: root.exportError; wrapMode: Text.Wrap; elide: Text.ElideNone }
    SButton { text: qsTr("Export copy…"); onClicked: saveDlg.open() }
    SLabel { sub: true; font.pixelSize: 11; mono: true; text: "sherpa-core " + Backend.coreVersion }

    buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Close"); kind: "primary"; onClicked: root.close() } ]
}
