import QtQuick
import Sherpa
import QtQuick.Controls.Basic
import QtQuick.Layouts

// «Дом»: заголовок с поиском, три панели, статус-бар
Item {
    id: root
    signal addRequested()
    signal editRequested(string id)
    signal deleteRequested(string id)
    signal importRequested()
    signal settingsRequested()
    signal newGroupRequested()
    signal groupMenu(string id, string name)
    property alias searchField: search
    property bool hideBackup: false

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // заголовок
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 52
            color: Theme.alpha(Theme.panelFill, Theme.translucent ? 0.35 : 1)
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 14
                spacing: 10
                SLabel { text: "Sherpa"; font.pixelSize: 16; font.bold: true; color: Theme.cyan }
                STextField {
                    id: search
                    Layout.preferredWidth: 340
                    placeholderText: qsTr("Search by name, host or tag")
                    text: Backend.searchText
                    onEdited: Backend.searchText = text
                    Accessible.name: qsTr("Search")
                }
                Item { Layout.fillWidth: true }
                SButton {
                    id: checkAllBtn
                    visible: !Backend.bulkChecking
                    text: qsTr("Check all")
                    enabled: Backend.servers.totalCount > 0
                    onClicked: Backend.checkAll()
                }
                SButton { visible: Backend.bulkChecking; text: qsTr("Cancel check"); onClicked: Backend.cancelBulkCheck() }
                SButton { text: qsTr("Settings"); kind: "ghost"; onClicked: root.settingsRequested() }
                SButton { text: "+ " + qsTr("Add"); kind: "primary"; onClicked: root.addRequested() }
            }
        }

        // баннер
        SBanner {
            Layout.fillWidth: true
            Layout.margins: 8
            Layout.bottomMargin: 0
            visible: Backend.banner.text !== undefined
            text: Backend.banner.text || ""
            actionText: Backend.banner.retryable ? qsTr("Retry") : ""
            onAction: Backend.reload()
            onDismissed: Backend.dismissBanner()
        }
        SBanner {
            Layout.fillWidth: true
            Layout.margins: 8
            Layout.bottomMargin: 0
            tone: Theme.amber
            visible: Backend.backupPending && !root.hideBackup && Backend.banner.text === undefined
            text: qsTr("No backup copy of the key file yet — without it saved secrets cannot be restored.")
            actionText: qsTr("Save copy…")
            onAction: root.settingsRequested()
            onDismissed: root.hideBackup = true
        }
        // прогресс массовой проверки
        Rectangle {
            visible: Backend.bulkChecking
            Layout.fillWidth: true
            Layout.preferredHeight: 26
            color: Theme.alpha(Theme.cyan, 0.1)
            SLabel { anchors.verticalCenter: parent.verticalCenter; x: 14; mono: true; color: Theme.cyan
                     text: qsTr("Checked %1 of %2").arg(Backend.checkDone).arg(Backend.checkTotal) }
            Rectangle { anchors.bottom: parent.bottom; height: 2; color: Theme.cyan
                        width: Backend.checkTotal ? parent.width * Backend.checkDone / Backend.checkTotal : 0 }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0
            Sidebar {
                Layout.preferredWidth: Theme.sidebarWidth
                Layout.fillHeight: true
                onImportRequested: root.importRequested()
                onGroupMenu: (id, name) => root.groupMenu(id, name)
                onNewGroupRequested: root.newGroupRequested()
            }
            ServerList {
                id: list
                Layout.preferredWidth: Theme.listWidth
                Layout.fillHeight: true
                onAddRequested: root.addRequested()
                onImportRequested: root.importRequested()
                onOpenRequested: (id) => Backend.checkOne(id)   // терминал появится позже; пока двойной клик проверяет
            }
            Rectangle { Layout.fillHeight: true; width: 1; color: Theme.line }
            ServerCard {
                Layout.fillWidth: true
                Layout.fillHeight: true
                onEditRequested: (id) => root.editRequested(id)
                onDeleteRequested: (id) => root.deleteRequested(id)
            }
        }

        // статус-бар
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 26
            color: Theme.alpha(Theme.panelFill, Theme.translucent ? 0.6 : 1)
            Rectangle { width: parent.width; height: 1; color: Theme.line }
            Row {
                anchors.verticalCenter: parent.verticalCenter
                x: 14
                spacing: 18
                SLabel { mono: true; font.pixelSize: 11; color: Theme.green; text: "● " + qsTr("online") + " " + (Backend.stats.online || 0) }
                SLabel { mono: true; font.pixelSize: 11; color: Theme.red; text: "● " + qsTr("error") + " " + ((Backend.stats.error || 0) + (Backend.stats.auth_failed || 0)) }
                SLabel { mono: true; font.pixelSize: 11; color: Theme.amber; text: "● " + qsTr("offline") + " " + (Backend.stats.offline || 0) }
                SLabel { mono: true; font.pixelSize: 11; sub: true; text: qsTr("shown %1 of %2").arg(Backend.servers.count).arg(Backend.servers.totalCount) }
            }
        }
    }
}
