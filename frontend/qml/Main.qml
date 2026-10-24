import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

ApplicationWindow {
    id: win
    width: 1280
    height: 800
    minimumWidth: 900
    minimumHeight: 560
    visible: true
    title: "Sherpa"
    color: "#05070c"
    font.family: Theme.fontUi
    font.pixelSize: Theme.sizeUi

    // свойства для тестов
    property alias home: homePage
    property alias formDialog: serverForm
    property alias importDialog: importDlg
    property alias settingsDialog: settingsDlg
    property alias confirmDelete: delDlg
    property alias toastItem: toast

    Binding { target: Theme; property: "mode"; value: Backend.theme === "light" ? "light" : "dark" }
    Binding { target: Theme; property: "translucent"; value: Backend.translucent }
    Binding { target: Theme; property: "bgItem"; value: bg }
    Binding { target: Theme; property: "contentItem"; value: content }
    Binding { target: Theme; property: "fxAvailable"; value: bg.GraphicsInfo.api !== GraphicsInfo.Software }

    // фон «рабочего стола» с размытыми пятнами
    Item {
        id: bg
        anchors.fill: parent
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Theme.light ? "#c9daf5" : "#0a1020" }
                GradientStop { position: 0.55; color: Theme.light ? "#d9d4f5" : "#1a1040" }
                GradientStop { position: 1; color: Theme.light ? "#c5e6ee" : "#06202e" }
            }
        }
        Rectangle { x: -120; y: parent.height - 380; width: 620; height: 520; radius: 260; color: Theme.alpha(Theme.cyan, Theme.light ? 0.25 : 0.18) }
        Rectangle { x: parent.width - 520; y: -140; width: 640; height: 560; radius: 280; color: Theme.alpha(Theme.accent, Theme.light ? 0.3 : 0.3) }
        Rectangle { x: parent.width - 420; y: parent.height - 360; width: 520; height: 460; radius: 230; color: Theme.alpha(Theme.purple, 0.2) }
    }

    Item {
        id: content
        anchors.fill: parent

        // окно-«стекло»
        GlassPanel {
            anchors.fill: parent
            color: Theme.windowFill
            visible: Backend.appState === "ready"
        }

        Item {
            anchors.fill: parent
            visible: Backend.appState === "loading"
            ColumnLayout {
                anchors.centerIn: parent
                SSpinner { Layout.alignment: Qt.AlignHCenter; width: 28; height: 28; thickness: 3 }
                SLabel { text: qsTr("Starting core…"); sub: true }
            }
        }
        FirstRunPage {
            anchors.fill: parent
            visible: Backend.appState === "firstrun"
            onImportRequested: importDlg.openDialog()
        }
        FatalPage { anchors.fill: parent; visible: Backend.appState === "fatal" }

        TabHost {
            id: tabs
            anchors.fill: parent
            visible: Backend.appState === "ready"
            HomePage {
                id: homePage
                onAddRequested: serverForm.openCreate()
                onEditRequested: (id) => serverForm.openEdit(id)
                onDeleteRequested: (id) => { delDlg.targetId = id; delDlg.message = qsTr("Delete server “%1”? Its saved secrets will be removed too.").arg(Backend.serverById(id).name); delDlg.open() }
                onImportRequested: importDlg.openDialog()
                onSettingsRequested: settingsDlg.open()
                onNewGroupRequested: { groupDlg.mode = "new"; groupDlg.groupId = ""; groupDlg.value = ""; groupDlg.title = qsTr("New group"); groupDlg.open() }
                onGroupMenu: (id, name) => { groupMenu.gid = id; groupMenu.gname = name; groupMenu.popup() }
            }
        }
    }

    Menu {
        id: groupMenu
        property string gid
        property string gname
        MenuItem { text: qsTr("Rename…"); onTriggered: { groupDlg.mode = "rename"; groupDlg.groupId = groupMenu.gid; groupDlg.value = groupMenu.gname; groupDlg.title = qsTr("Rename group"); groupDlg.open() } }
        MenuItem { text: qsTr("Delete group"); onTriggered: { delGroup.targetId = groupMenu.gid; delGroup.message = qsTr("Delete group “%1”? Its servers move to the parent group.").arg(groupMenu.gname); delGroup.open() } }
    }

    ServerFormDialog { id: serverForm }
    ImportDialog { id: importDlg }
    SettingsDialog { id: settingsDlg }
    HostKeyDialog { id: hostKeyDlg }
    ConfirmDialog {
        id: delDlg
        property string targetId
        title: qsTr("Delete server")
        confirmText: qsTr("Delete")
        danger: true
        onConfirmed: Backend.deleteConnection(targetId)
    }
    ConfirmDialog {
        id: delGroup
        property string targetId
        title: qsTr("Delete group")
        confirmText: qsTr("Delete")
        danger: true
        onConfirmed: Backend.deleteGroup(targetId)
    }
    NameDialog {
        id: groupDlg
        property string mode: "new"
        property string groupId
        label: qsTr("Group name")
        onSubmitted: (n) => { mode === "new" ? Backend.createGroup(n) : Backend.renameGroup(groupId, n) }
    }
    Connections {
        target: Backend
        function onGroupActionFinished(ok, message) {
            if (ok) groupDlg.close(); else groupDlg.error = message
        }
        function onDeleteFinished(ok, message) { if (!ok) toast.show(message, "error") }
        function onDuplicateFinished(ok, id, message) { if (!ok) toast.show(message, "error") }
        function onToast(text, kind) { toast.show(text, kind) }
    }

    SToast { id: toast; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: 20; z: 100 }

    Shortcut { sequence: "Ctrl+N"; onActivated: serverForm.openCreate() }
    Shortcut { sequence: "Ctrl+F"; onActivated: homePage.searchField.forceFocus() }
    Shortcut { sequences: ["Ctrl+R", "F5"]; onActivated: Backend.checkOne(Backend.selectedId) }
    Shortcut { sequences: ["Ctrl+Shift+R", "Ctrl+F5"]; onActivated: Backend.checkAll() }
    Shortcut { sequence: "Ctrl+E"; onActivated: if (Backend.selectedId !== "") serverForm.openEdit(Backend.selectedId) }
    Shortcut { sequence: "Ctrl+,"; onActivated: settingsDlg.open() }

    Component.onCompleted: Backend.start()
}
