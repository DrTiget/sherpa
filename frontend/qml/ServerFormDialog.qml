import QtQuick
import Sherpa
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import QtQuick.Layouts

// Форма сервера: создание и изменение (SCREENS.md 2.3).
// Ядро не умеет проверять несохранённое подключение, поэтому вместо «Проверить» — «Сохранить и проверить».
SDialog {
    id: root
    property string editId: ""          // пусто — создание
    property string originalName: ""
    property string savedId: ""         // id после «Сохранить и проверить»
    property bool saving: false
    property bool wantCheck: false
    property string initialSnapshot: ""
    property string generalError: ""
    property var fieldErrors: ({})
    readonly property bool editing: editId !== ""
    readonly property var savedServer: savedId !== "" ? Backend.serverById(savedId) : ({})
    property var selectedTick: Backend.selected   // пересчёт savedServer при обновлении статусов

    title: editing ? qsTr("Edit: %1").arg(originalName) : qsTr("New server")
    dialogWidth: 580
    guardClose: true

    ListModel { id: groupOptions }

    function groupModel() {
        var m = [{ value: "", text: qsTr("No group") }]
        for (var i = 0; i < Backend.groups.length; ++i) {
            var g = Backend.groups[i]
            m.push({ value: g.id, text: "  ".repeat(g.depth) + g.name })
        }
        m.push({ value: "__new__", text: qsTr("New group…") })
        return m
    }

    function snapshot() {
        return JSON.stringify([nameF.text, hostF.text, portF.text, userF.text, auth.current, passF.text,
                               keyPathF.text, keyTextF.text, phraseF.text, groupSel.current, newGroupF.text,
                               tagsF.tags, descF.text])
    }

    function resetFields() {
        nameF.text = ""; hostF.text = ""; portF.text = "22"; userF.text = ""
        auth.current = "password"; passF.text = ""; keyPathF.text = ""; keyTextF.text = ""; phraseF.text = ""
        passF.revealed = false; phraseF.revealed = false
        groupSel.current = ""; newGroupF.text = ""; tagsF.tags = []; descF.text = ""
        fieldErrors = ({}); generalError = ""; saving = false; wantCheck = false; savedId = ""
    }

    function openCreate() {
        editId = ""; originalName = ""
        resetFields()
        groupSel.model = groupModel()
        if (Backend.groupFilter !== "" && Backend.groupFilter !== "__none__")
            groupSel.current = Backend.groupFilter
        initialSnapshot = snapshot()
        open()
    }

    function openEdit(id) {
        var s = Backend.serverById(id)
        if (!s.id)
            return
        editId = id; originalName = s.name
        resetFields()
        groupSel.model = groupModel()
        nameF.text = s.name; hostF.text = s.host; portF.text = String(s.port)
        userF.text = s.user || ""
        auth.current = s.auth.method
        keyPathF.text = s.auth.key_path || ""
        groupSel.current = s.group_id || ""
        tagsF.tags = s.tags
        descF.text = s.description || ""
        initialSnapshot = snapshot()
        open()
    }

    function err(f) { return fieldErrors[f] || "" }

    function validate() {
        var e = {}
        if (nameF.text.trim() === "") e.name = qsTr("This field is required")
        if (hostF.text.trim() === "") e.host = qsTr("This field is required")
        var p = parseInt(portF.text)
        if (!(p >= 1 && p <= 65535)) e.port = qsTr("Port must be between 1 and 65535")
        if (auth.current === "key_file" && keyPathF.text.trim() === "") e.key_path = qsTr("This field is required")
        if (auth.current === "key_text" && keyTextF.text.trim() === "" && !(editing && Backend.serverById(editId).auth.has_key_text))
            e.key_text = qsTr("This field is required")
        if (groupSel.current === "__new__" && newGroupF.text.trim() === "") e.group = qsTr("This field is required")
        fieldErrors = e
        return Object.keys(e).length === 0
    }

    function submit(check) {
        generalError = ""
        if (!validate())
            return
        saving = true
        wantCheck = check
        Backend.saveConnection({
            name: nameF.text, host: hostF.text, port: parseInt(portF.text), user: userF.text,
            authMethod: auth.current, password: passF.text, keyPath: keyPathF.text,
            keyText: keyTextF.text, passphrase: phraseF.text, description: descF.text,
            groupId: groupSel.current === "__new__" ? "" : groupSel.current,
            newGroupName: groupSel.current === "__new__" ? newGroupF.text : "",
            tags: tagsF.tags
        }, savedId !== "" ? savedId : editId)
    }

    Connections {
        target: Backend
        enabled: root.visible
        function onSaveFinished(ok, id, field, message) {
            root.saving = false
            if (ok) {
                if (root.wantCheck) {
                    root.savedId = id
                    root.editId = id
                    root.originalName = nameF.text
                    root.initialSnapshot = root.snapshot()
                    Backend.checkOne(id)
                } else {
                    root.close()
                }
                return
            }
            var e = {}
            var f = field.indexOf("auth") === 0 ? "secret" : field
            if (["name", "host", "port", "user", "secret", "group", "description"].indexOf(f) >= 0)
                e[f] = message
            else
                root.generalError = message
            root.fieldErrors = e
        }
    }

    function requestCloseGuarded() {
        if (savedId === "" && snapshot() !== initialSnapshot)
            discard.open()
        else
            close()
    }
    onCloseRequested: requestCloseGuarded()
    onAcceptRequested: if (!saving) submit(false)

    ConfirmDialog {
        id: discard
        title: qsTr("Discard changes?")
        message: qsTr("The form has unsaved changes. Close it anyway?")
        confirmText: qsTr("Discard")
        danger: true
        onConfirmed: root.close()
    }

    FileDialog {
        id: keyPick
        title: qsTr("Choose private key")
        onAccepted: keyPathF.text = Backend.toLocalPath(selectedFile.toString())
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 12
        STextField { id: nameF; Layout.fillWidth: true; label: qsTr("Name"); required: true; error: root.err("name"); onAccepted: root.submit(false) }
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: 12
        STextField { id: hostF; Layout.fillWidth: true; Layout.preferredWidth: 3; label: qsTr("Host"); required: true; mono: true; error: root.err("host"); placeholderText: "10.0.0.5" }
        STextField { id: portF; Layout.fillWidth: true; Layout.preferredWidth: 1; label: qsTr("Port"); mono: true; error: root.err("port"); validator: IntValidator { bottom: 0; top: 99999 } }
        STextField { id: userF; Layout.fillWidth: true; Layout.preferredWidth: 2; label: qsTr("User"); mono: true; error: root.err("user"); placeholderText: "root" }
    }

    SLabel { text: qsTr("Sign-in method"); caps: true }
    SSegmented {
        id: auth
        current: "password"
        model: [{ value: "password", text: qsTr("Password") },
                { value: "key_file", text: qsTr("Key file") },
                { value: "key_text", text: qsTr("Key text") }]
    }

    STextField {
        id: passF
        visible: auth.current === "password"
        Layout.fillWidth: true
        label: qsTr("Password")
        secret: true
        error: root.err("secret")
        hint: root.editing && Backend.serverById(root.editId).auth.has_password ? qsTr("Saved. Leave empty to keep it unchanged.") : qsTr("Optional: if empty, it is not saved")
    }
    RowLayout {
        visible: auth.current === "key_file"
        Layout.fillWidth: true
        spacing: 8
        STextField { id: keyPathF; Layout.fillWidth: true; label: qsTr("Key file path"); mono: true; required: true; error: root.err("key_path"); placeholderText: "~/.ssh/id_ed25519" }
        SButton { text: qsTr("Browse…"); Layout.alignment: Qt.AlignBottom; Layout.bottomMargin: root.err("key_path") !== "" ? 18 : 0; onClicked: keyPick.open() }
    }
    Item {
        visible: auth.current === "key_text"
        Layout.fillWidth: true
        implicitHeight: 22 + 100 + (root.err("key_text") !== "" ? 18 : 0)
        SLabel { id: ktCap; text: qsTr("Private key text") + " *"; caps: true }
        Rectangle {
            y: 22
            width: parent.width
            height: 100
            radius: Theme.radius
            color: Theme.inputFill
            border.width: keyTextF.activeFocus ? 2 : 1
            border.color: root.err("key_text") !== "" ? Theme.red : (keyTextF.activeFocus ? Theme.cyan : Theme.line)
            ScrollView {
                anchors.fill: parent
                anchors.margins: 6
                TextArea {
                    id: keyTextF
                    padding: 2
                    color: Theme.text
                    font.family: Theme.fontMono
                    font.pixelSize: 11
                    wrapMode: TextEdit.WrapAnywhere
                    selectByMouse: true
                    background: null
                    placeholderText: root.editing && Backend.serverById(root.editId).auth.has_key_text ? qsTr("Saved. Paste a new key to replace it.") : "-----BEGIN OPENSSH PRIVATE KEY-----"
                    placeholderTextColor: Theme.textSub
                    Accessible.name: qsTr("Private key text")
                }
            }
        }
        SLabel { visible: root.err("key_text") !== ""; y: 126; text: root.err("key_text"); color: Theme.red; font.pixelSize: 11 }
    }
    STextField {
        id: phraseF
        visible: auth.current !== "password"
        Layout.fillWidth: true
        label: qsTr("Key passphrase")
        secret: true
        hint: root.editing ? qsTr("Leave empty to keep it unchanged") : qsTr("Only if the key is protected")
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 12
        SSelect {
            id: groupSel
            Layout.fillWidth: true
            label: qsTr("Group")
            model: root.groupModel()
        }
        STextField {
            id: newGroupF
            visible: groupSel.current === "__new__"
            Layout.fillWidth: true
            label: qsTr("New group name")
            error: root.err("group")
        }
    }
    STagInput { id: tagsF; Layout.fillWidth: true; label: qsTr("Tags") }
    STextField { id: descF; Layout.fillWidth: true; label: qsTr("Description") }

    // Результат «Сохранить и проверить»
    RowLayout {
        visible: root.savedId !== ""
        Layout.fillWidth: true
        spacing: 8
        SStatusDot { state: root.savedServer.state || "unknown"; checking: !!root.savedServer.checking }
        SLabel {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            color: root.savedServer.state === "online" ? Theme.green : (root.savedServer.state === "unknown" ? Theme.textSub : Theme.red)
            text: root.savedServer.checking ? qsTr("Checking…")
                  : (root.savedServer.state === "online" ? Theme.stateText("online", root.savedServer.latency_ms)
                     : (root.savedServer.error_text || Theme.stateLabel(root.savedServer.state)))
        }
    }
    SLabel {
        visible: root.generalError !== ""
        Layout.fillWidth: true
        text: root.generalError
        color: Theme.red
        wrapMode: Text.Wrap
        elide: Text.ElideNone
    }

    buttons: [
        SButton {
            text: qsTr("Save and check")
            enabled: !root.saving
            busy: root.saving && root.wantCheck
            onClicked: root.submit(true)
        },
        Item { Layout.fillWidth: true },
        SButton { text: root.savedId !== "" ? qsTr("Close") : qsTr("Cancel"); onClicked: root.requestCloseGuarded() },
        SButton {
            visible: root.savedId === ""
            text: qsTr("Save")
            kind: "primary"
            enabled: !root.saving
            busy: root.saving && !root.wantCheck
            onClicked: root.submit(false)
        }
    ]
}
