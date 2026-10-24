import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// «Система»: общее (hostname, ядро, питание), время и NTP, пользователи и группы (sys.*).
UtilPane {
    id: root
    property string tab: "general"
    property var info: ({})
    property var timeInfo: ({})
    property string timeError: ""
    property var users: []
    property var groups: []
    property string userQuery: ""
    property var selUser: null
    property var selGroup: null
    readonly property var utils: desktop.capabilities.utilities || ({})
    readonly property var usersShown: users.filter(u => userQuery === "" || u.name.toLowerCase().indexOf(userQuery.toLowerCase()) >= 0)

    function loadInfo() { rpc("sys.info", {}, (d) => { info = d; hostname = d.hostname || ""; hostField.text = hostname }) }
    function loadTime() { rpc("sys.time.get", {}, (d) => { timeInfo = d; timeError = ""; zoneField.text = d.timezone || "" }, (t) => { timeInfo = ({}); timeError = t }) }
    function loadUsers() {
        rpc("sys.user.list", {}, (d) => { users = d.items || []; if (selUser) selUser = users.find(u => u.name === selUser.name) || null })
        rpc("sys.group.list", {}, (d) => { groups = d.items || []; if (selGroup) selGroup = groups.find(g => g.name === selGroup.name) || null })
    }
    function done(msg) { showNotice(msg, Theme.green) }
    function reloadAll() { loadInfo(); loadTime(); loadUsers() }
    onWorkingChanged: if (working) reloadAll()
    Component.onCompleted: if (working) reloadAll()

    function isSudoer(u) { return u.groups.indexOf("sudo") >= 0 || u.groups.indexOf("wheel") >= 0 }
    function fmtUptime(s) {
        if (!s && s !== 0) return "—"
        const d = Math.floor(s / 86400), h = Math.floor(s % 86400 / 3600), mi = Math.floor(s % 3600 / 60)
        return (d ? d + qsTr("d ") : "") + h + qsTr("h ") + mi + qsTr("m")
    }

    ColumnLayout {
        anchors.fill: parent; anchors.margins: 12; spacing: 10
        SSegmented {
            current: root.tab
            model: [{ value: "general", text: qsTr("General") }, { value: "time", text: qsTr("Time") }, { value: "users", text: qsTr("Users") }, { value: "groups", text: qsTr("Groups") }]
            onActivated: (v) => root.tab = v
        }
        StackLayout {
            Layout.fillWidth: true; Layout.fillHeight: true
            currentIndex: ["general", "time", "users", "groups"].indexOf(root.tab)

            // ---------------- общее
            Flickable {
                contentHeight: gen.implicitHeight; clip: true
                ColumnLayout {
                    id: gen
                    width: parent.width; spacing: 12
                    Rectangle {
                        Layout.fillWidth: true; implicitHeight: g1.implicitHeight + 24; radius: 8; color: Theme.card; border.color: Theme.line
                        GridLayout {
                            id: g1
                            anchors.fill: parent; anchors.margins: 12; columns: 2; columnSpacing: 16; rowSpacing: 6
                            SLabel { caps: true; text: qsTr("Operating system") }
                            SLabel { Layout.fillWidth: true; text: root.desktop.capabilities.os ? root.desktop.capabilities.os.name || "" : "" }
                            SLabel { caps: true; text: qsTr("Kernel") }
                            SLabel { Layout.fillWidth: true; mono: true; text: (root.info.kernel || "") + " " + (root.info.kernel_release || "") }
                            SLabel { caps: true; text: qsTr("Architecture") }
                            SLabel { Layout.fillWidth: true; mono: true; text: root.info.arch || "" }
                            SLabel { caps: true; text: qsTr("Uptime") }
                            SLabel { Layout.fillWidth: true; mono: true; text: root.fmtUptime(root.desktop.monitor.uptime_sec) }
                            SLabel { caps: true; text: qsTr("Current user") }
                            SLabel { Layout.fillWidth: true; mono: true; text: root.desktop.capabilities.user ? root.desktop.capabilities.user.name + (root.desktop.capabilities.user.is_root ? " (root)" : "") : "" }
                        }
                    }
                    Rectangle {
                        Layout.fillWidth: true; implicitHeight: g2.implicitHeight + 24; radius: 8; color: Theme.card; border.color: Theme.line
                        ColumnLayout {
                            id: g2
                            anchors.fill: parent; anchors.margins: 12; spacing: 8
                            SLabel { caps: true; text: qsTr("Host name") }
                            RowLayout {
                                Layout.fillWidth: true; spacing: 8
                                STextField { id: hostField; objectName: "hostField"; Layout.fillWidth: true; mono: true }
                                SButton {
                                    objectName: "hostApply"
                                    text: qsTr("Change"); enabled: hostField.text !== "" && hostField.text !== root.hostname
                                    onClicked: root.danger("sys.hostname.set", { hostname: hostField.text }, {
                                        title: qsTr("Change host name"),
                                        message: qsTr("The host name will be changed to the one below. Services and monitoring that rely on the old name may break."),
                                        details: root.hostname + " → " + hostField.text, confirmText: qsTr("Change"), needName: true
                                    }, () => { root.hostname = ""; root.loadInfo(); root.done(qsTr("Host name changed")) })
                                }
                            }
                        }
                    }
                    Rectangle {
                        Layout.fillWidth: true; implicitHeight: g3.implicitHeight + 24; radius: 8; color: Theme.alpha(Theme.red, 0.07); border.color: Theme.alpha(Theme.red, 0.4)
                        ColumnLayout {
                            id: g3
                            anchors.fill: parent; anchors.margins: 12; spacing: 8
                            SLabel { caps: true; text: qsTr("Power"); color: Theme.red }
                            SLabel { Layout.fillWidth: true; sub: true; wrapMode: Text.Wrap; elide: Text.ElideNone
                                     text: qsTr("The command is scheduled with a delay (from 1 minute); the terminal and utilities lose the connection when the server goes down.") }
                            RowLayout {
                                spacing: 8
                                SLabel { text: qsTr("Delay, min") }
                                STextField { id: delayField; objectName: "delayField"; Layout.preferredWidth: 70; text: "1"; validator: IntValidator { bottom: 1; top: 1440 } }
                                SButton { objectName: "rebootBtn"; text: qsTr("Reboot…"); kind: "danger"; onClicked: root.power("reboot") }
                                SButton { objectName: "shutdownBtn"; text: qsTr("Shut down…"); kind: "danger"; onClicked: root.power("shutdown") }
                            }
                        }
                    }
                }
            }

            // ---------------- время
            Flickable {
                contentHeight: tm.implicitHeight; clip: true
                ColumnLayout {
                    id: tm
                    width: parent.width; spacing: 12
                    SBanner { objectName: "timeError"; Layout.fillWidth: true; visible: root.timeError !== ""; tone: Theme.amber
                              text: qsTr("Time settings are not available on this server: %1").arg(root.timeError); onDismissed: root.timeError = "" }
                    Rectangle {
                        Layout.fillWidth: true; implicitHeight: t1.implicitHeight + 24; radius: 8; color: Theme.card; border.color: Theme.line
                        GridLayout {
                            id: t1
                            anchors.fill: parent; anchors.margins: 12; columns: 2; columnSpacing: 16; rowSpacing: 6
                            SLabel { caps: true; text: qsTr("Server time") }
                            SLabel { Layout.fillWidth: true; mono: true; objectName: "serverTime"; text: root.timeInfo.time || "—" }
                            SLabel { caps: true; text: qsTr("RTC") }
                            SLabel { Layout.fillWidth: true; mono: true; text: root.timeInfo.rtc_time || "—" }
                            SLabel { caps: true; text: qsTr("Time zone") }
                            SLabel { Layout.fillWidth: true; mono: true; text: root.timeInfo.timezone || "—" }
                            SLabel { caps: true; text: qsTr("Synchronized") }
                            SLabel { Layout.fillWidth: true; text: root.timeInfo.ntp_synchronized === true ? qsTr("yes") : (root.timeInfo.ntp_synchronized === false ? qsTr("no") : "—")
                                     color: root.timeInfo.ntp_synchronized === true ? Theme.green : Theme.amber }
                        }
                    }
                    Rectangle {
                        Layout.fillWidth: true; implicitHeight: t2.implicitHeight + 24; radius: 8; color: Theme.card; border.color: Theme.line
                        ColumnLayout {
                            id: t2
                            anchors.fill: parent; anchors.margins: 12; spacing: 8
                            RowLayout {
                                Layout.fillWidth: true
                                SLabel { Layout.fillWidth: true; text: qsTr("Automatic time sync (NTP)") }
                                SSwitch {
                                    objectName: "ntpSwitch"
                                    enabled: root.timeInfo.can_ntp !== false
                                    checked: root.timeInfo.ntp === true
                                    onToggled: root.rpc("sys.time.ntp", { enabled: checked }, () => root.loadTime(), (t) => { root.showNotice(t); root.loadTime() })
                                }
                            }
                            SLabel { caps: true; text: qsTr("Time zone"); Layout.topMargin: 6 }
                            RowLayout {
                                Layout.fillWidth: true; spacing: 8
                                STextField { id: zoneField; objectName: "zoneField"; Layout.fillWidth: true; mono: true; placeholderText: "Europe/Moscow" }
                                SButton { text: qsTr("Apply"); enabled: zoneField.text !== "" && zoneField.text !== root.timeInfo.timezone
                                          onClicked: root.rpc("sys.time.set_zone", { timezone: zoneField.text }, () => { root.loadTime(); root.done(qsTr("Time zone changed")) }) }
                            }
                            Flow {
                                Layout.fillWidth: true; spacing: 6
                                Repeater {
                                    model: ["UTC", "Europe/Moscow", "Europe/Kaliningrad", "Europe/London", "Europe/Berlin", "Asia/Yekaterinburg", "Asia/Novosibirsk", "Asia/Vladivostok", "America/New_York"]
                                    delegate: SChip { required property string modelData; text: modelData; active: zoneField.text === modelData; onClicked: zoneField.text = modelData }
                                }
                            }
                            SLabel { caps: true; text: qsTr("Set time manually"); Layout.topMargin: 6 }
                            RowLayout {
                                Layout.fillWidth: true; spacing: 8
                                STextField { id: timeField; objectName: "timeField"; Layout.fillWidth: true; mono: true; placeholderText: "2026-10-24 12:00:00" }
                                SButton { text: qsTr("Set"); enabled: timeField.text !== "" && root.timeInfo.ntp !== true
                                          onClicked: root.rpc("sys.time.set", { time: timeField.text }, () => { root.loadTime(); root.done(qsTr("Time set")) }) }
                            }
                            SLabel { visible: root.timeInfo.ntp === true; Layout.fillWidth: true; sub: true; wrapMode: Text.Wrap; elide: Text.ElideNone
                                     text: qsTr("Turn off NTP first to set the time manually.") }
                        }
                    }
                }
            }

            // ---------------- пользователи
            ColumnLayout {
                spacing: 8
                RowLayout {
                    Layout.fillWidth: true; spacing: 8
                    STextField { Layout.fillWidth: true; placeholderText: qsTr("Search users…"); onEdited: root.userQuery = text; Accessible.name: qsTr("Search users") }
                    SButton { objectName: "userAdd"; text: qsTr("Add user"); kind: "primary"; compact: true; onClicked: userDlg.openNew() }
                }
                RowLayout {
                    Layout.fillWidth: true; Layout.fillHeight: true; spacing: 10
                    UTable {
                        objectName: "userTable"
                        Layout.fillWidth: true; Layout.fillHeight: true
                        rows: root.usersShown
                        onRowClicked: (i, r) => root.selUser = r
                        columns: [
                            { key: "name", title: qsTr("Login"), w: 130, mono: true },
                            { key: "uid", title: "UID", w: 60, right: true },
                            { key: "description", title: qsTr("Name"), w: 150 },
                            { key: "home", title: qsTr("Home"), w: 0, mono: true },
                            { key: "shell", title: qsTr("Shell"), w: 130, mono: true },
                            { key: "sudo", title: "sudo", w: 52, fmt: (r) => root.isSudoer(r) ? "✓" : "", color: (r) => Theme.green }
                        ]
                    }
                    Rectangle {
                        visible: root.selUser !== null
                        Layout.preferredWidth: 270; Layout.fillHeight: true; radius: 8; color: Theme.card; border.color: Theme.line
                        ColumnLayout {
                            anchors.fill: parent; anchors.margins: 10; spacing: 8
                            SLabel { font.bold: true; mono: true; text: root.selUser ? root.selUser.name : "" }
                            SLabel { Layout.fillWidth: true; sub: true; text: root.selUser ? "uid " + root.selUser.uid + " · gid " + root.selUser.gid : "" }
                            SLabel { caps: true; text: qsTr("Groups") }
                            Flow { Layout.fillWidth: true; spacing: 4
                                   Repeater { model: root.selUser ? root.selUser.groups : []; delegate: SChip { required property string modelData; text: modelData } } }
                            Item { Layout.fillHeight: true }
                            RowLayout {
                                Layout.fillWidth: true
                                SLabel { Layout.fillWidth: true; text: qsTr("Administrator (sudo)") }
                                SSwitch {
                                    objectName: "sudoSwitch"
                                    checked: root.selUser ? root.isSudoer(root.selUser) : false
                                    onToggled: root.rpc("sys.user.sudo", { name: root.selUser.name, enabled: checked }, () => root.loadUsers(), (t) => { root.showNotice(t); root.loadUsers() })
                                }
                            }
                            SButton { Layout.fillWidth: true; text: qsTr("Change password…"); onClicked: pwDlg.openFor(root.selUser.name) }
                            SButton { Layout.fillWidth: true; text: qsTr("Delete user…"); kind: "danger"; enabled: root.selUser && root.selUser.uid !== 0
                                      onClicked: rmUserDlg.openFor(root.selUser.name) }
                        }
                    }
                }
            }

            // ---------------- группы
            ColumnLayout {
                spacing: 8
                RowLayout {
                    Layout.fillWidth: true
                    Item { Layout.fillWidth: true }
                    SButton { objectName: "groupAdd"; text: qsTr("Add group"); kind: "primary"; compact: true; onClicked: groupDlg.openNew() }
                }
                RowLayout {
                    Layout.fillWidth: true; Layout.fillHeight: true; spacing: 10
                    UTable {
                        objectName: "groupTable"
                        Layout.fillWidth: true; Layout.fillHeight: true
                        rows: root.groups
                        onRowClicked: (i, r) => root.selGroup = r
                        columns: [
                            { key: "name", title: qsTr("Group"), w: 160, mono: true },
                            { key: "gid", title: "GID", w: 70, right: true },
                            { key: "members", title: qsTr("Members"), w: 0, fmt: (r) => r.members.join(", ") }
                        ]
                    }
                    Rectangle {
                        visible: root.selGroup !== null
                        Layout.preferredWidth: 240; Layout.fillHeight: true; radius: 8; color: Theme.card; border.color: Theme.line
                        ColumnLayout {
                            anchors.fill: parent; anchors.margins: 10; spacing: 8
                            SLabel { font.bold: true; mono: true; text: root.selGroup ? root.selGroup.name : "" }
                            SLabel { Layout.fillWidth: true; sub: true; text: root.selGroup ? "gid " + root.selGroup.gid : "" }
                            SLabel { caps: true; text: qsTr("Members") }
                            Flow { Layout.fillWidth: true; spacing: 4
                                   Repeater { model: root.selGroup ? root.selGroup.members : []; delegate: SChip { required property string modelData; text: modelData } } }
                            Item { Layout.fillHeight: true }
                            SButton { Layout.fillWidth: true; text: qsTr("Delete group…"); kind: "danger"; enabled: root.selGroup && root.selGroup.gid !== 0
                                      onClicked: root.danger("sys.group.remove", { name: root.selGroup.name }, {
                                          title: qsTr("Delete group %1").arg(root.selGroup.name), message: qsTr("The group will be removed from the server."),
                                          confirmText: qsTr("Delete"), needName: true }, () => { root.selGroup = null; root.loadUsers(); root.done(qsTr("Group deleted")) }) }
                        }
                    }
                }
            }
        }
    }

    function power(action) {
        const mins = Math.max(1, parseInt(delayField.text) || 1)
        danger("sys.power", { action: action, delay_sec: mins * 60 }, {
            title: action === "reboot" ? qsTr("Reboot the server") : qsTr("Shut down the server"),
            message: action === "reboot" ? qsTr("The server will reboot in %1 min. All sessions will be interrupted.").arg(mins)
                                         : qsTr("The server will shut down in %1 min. You will need physical or provider access to start it again.").arg(mins),
            confirmText: action === "reboot" ? qsTr("Reboot") : qsTr("Shut down"), needName: true
        }, () => done(qsTr("Scheduled: %1").arg(action === "reboot" ? qsTr("reboot") : qsTr("shutdown"))))
    }

    // ---- диалоги
    SDialog {
        id: userDlg
        title: qsTr("Add user")
        dialogWidth: 440
        function openNew() { uName.text = ""; uHome.checked = true; uShell.text = "/bin/bash"; uPass.text = ""; err = ""; open(); uName.forceFocus() }
        property string err: ""
        function submit() {
            if (uName.text === "") { err = qsTr("Enter a login"); return }
            const p = { name: uName.text, create_home: uHome.checked }
            if (uShell.text !== "") p.shell = uShell.text
            root.rpc("sys.user.add", p, () => {
                const fin = () => { userDlg.close(); root.loadUsers(); root.done(qsTr("User created")) }
                if (uPass.text !== "") root.rpc("sys.user.passwd", { name: uName.text, password: uPass.text }, fin, (t) => { userDlg.err = t; root.loadUsers() })
                else fin()
            }, (t) => err = t)
        }
        STextField { id: uName; objectName: "newUserName"; Layout.fillWidth: true; label: qsTr("Login"); mono: true; error: userDlg.err; onAccepted: userDlg.submit() }
        STextField { id: uShell; Layout.fillWidth: true; label: qsTr("Shell"); mono: true }
        STextField { id: uPass; Layout.fillWidth: true; label: qsTr("Password (optional)"); secret: true }
        SCheckBox { id: uHome; text: qsTr("Create home directory") }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: userDlg.close() },
                   SButton { objectName: "newUserOk"; text: qsTr("Create"); kind: "primary"; onClicked: userDlg.submit() } ]
    }
    SDialog {
        id: groupDlg
        title: qsTr("Add group")
        dialogWidth: 380
        property string err: ""
        function openNew() { gName.text = ""; err = ""; open(); gName.forceFocus() }
        function submit() { if (gName.text !== "") root.rpc("sys.group.add", { name: gName.text }, () => { close(); root.loadUsers() }, (t) => err = t) }
        STextField { id: gName; objectName: "newGroupName"; Layout.fillWidth: true; label: qsTr("Group name"); mono: true; error: groupDlg.err; onAccepted: groupDlg.submit() }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: groupDlg.close() },
                   SButton { objectName: "newGroupOk"; text: qsTr("Create"); kind: "primary"; onClicked: groupDlg.submit() } ]
    }
    SDialog {
        id: pwDlg
        title: qsTr("Change password")
        dialogWidth: 400
        property string user: ""
        property string err: ""
        function openFor(u) { user = u; pw1.text = ""; err = ""; open(); pw1.forceFocus() }
        function submit() { if (pw1.text !== "") root.rpc("sys.user.passwd", { name: user, password: pw1.text }, () => { close(); root.done(qsTr("Password changed")) }, (t) => err = t) }
        SLabel { text: qsTr("New password for %1").arg(pwDlg.user); Layout.fillWidth: true }
        STextField { id: pw1; Layout.fillWidth: true; secret: true; error: pwDlg.err; onAccepted: pwDlg.submit() }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: pwDlg.close() },
                   SButton { text: qsTr("Change"); kind: "primary"; onClicked: pwDlg.submit() } ]
    }
    SDialog {
        id: rmUserDlg
        title: qsTr("Delete user")
        dialogWidth: 440
        tone: Theme.red
        property string user: ""
        function openFor(u) { user = u; rmHome.checked = false; open() }
        SLabel { Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone; text: qsTr("User %1 will be deleted from the server.").arg(rmUserDlg.user) }
        SCheckBox { id: rmHome; text: qsTr("Also delete the home directory") }
        buttons: [ Item { Layout.fillWidth: true }, SButton { text: qsTr("Cancel"); onClicked: rmUserDlg.close() },
                   SButton { text: qsTr("Continue…"); kind: "danger"
                             onClicked: { const u = rmUserDlg.user, h = rmHome.checked; rmUserDlg.close()
                                          root.danger("sys.user.remove", { name: u, remove_home: h }, { title: qsTr("Delete user %1").arg(u),
                                              message: h ? qsTr("The user and the home directory will be deleted. This cannot be undone.") : qsTr("The user will be deleted; the home directory is kept."),
                                              confirmText: qsTr("Delete"), needName: true }, () => { root.selUser = null; root.loadUsers(); root.done(qsTr("User deleted")) }) } } ]
    }
}
