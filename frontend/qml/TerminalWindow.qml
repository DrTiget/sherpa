import QtQuick
import QtQuick.Layouts
import Sherpa

// Содержимое окна терминала: вид эмулятора, состояния сессии, подтверждение вставки.
Item {
    id: root
    required property var session
    property bool autoOpen: true
    property alias term: term
    readonly property string st: session ? session.state : "closed"

    Timer {
        id: openTimer
        interval: 0
        running: root.autoOpen
        onTriggered: if (root.session && root.session.state === "closed") root.session.open(term.cols, term.rows)
    }

    TerminalItem {
        id: term
        anchors.fill: parent
        anchors.bottomMargin: status.height
        session: root.session
        focus: true
        onMultilinePaste: (t) => pasteDlg.askPaste(t)
        Component.onCompleted: forceActiveFocus()
    }

    // подключение
    Rectangle {
        visible: root.st === "connecting"
        anchors.centerIn: term
        width: 260; height: 56; radius: Theme.radius
        color: Theme.alpha("#000000", 0.55)
        border.color: Theme.line
        Row {
            anchors.centerIn: parent
            spacing: 10
            SSpinner { width: 18; height: 18 }
            SLabel { text: qsTr("Connecting…"); color: "#e6eaf2" }
        }
    }

    // обрыв / завершение / ошибка
    Rectangle {
        id: overlay
        visible: root.st === "lost" || root.st === "closed" || root.st === "error"
        anchors.horizontalCenter: term.horizontalCenter
        anchors.bottom: term.bottom
        anchors.bottomMargin: 14
        width: Math.min(term.width - 24, 460)
        height: ovCol.implicitHeight + 24
        radius: Theme.radiusLarge
        color: Theme.alpha("#10131c", 0.92)
        border.color: Theme.alpha(root.st === "closed" && root.session.reason === "exit" ? Theme.cyan : Theme.amber, 0.6)
        ColumnLayout {
            id: ovCol
            anchors.fill: parent
            anchors.margins: 12
            spacing: 8
            SLabel {
                Layout.fillWidth: true
                font.bold: true
                color: "#e6eaf2"
                wrapMode: Text.Wrap; elide: Text.ElideNone
                text: root.st === "lost" ? qsTr("Connection lost")
                      : root.st === "error" ? (root.session.errorText || qsTr("Connection error"))
                      : (root.session.reason === "exit" ? qsTr("Session ended") : qsTr("Session disconnected"))
            }
            RowLayout {
                spacing: 8
                SButton { visible: root.st !== "lost"; text: qsTr("Reconnect"); kind: "primary"; compact: true; onClicked: { root.session.reconnect(); term.forceActiveFocus() } }
                SButton { visible: root.st === "error" && root.session.errorCode.indexOf("auth.") === 0; text: qsTr("Edit connection"); compact: true; onClicked: root.editConnection() }
                SButton { visible: root.st === "lost"; text: qsTr("Waiting for the core…"); compact: true; enabled: false }
            }
        }
    }
    signal editConnection()

    // подвал: размер, баннер
    Rectangle {
        id: status
        anchors.bottom: parent.bottom
        width: parent.width
        height: 22
        color: Theme.alpha("#05070c", 0.7)
        SLabel { x: 10; anchors.verticalCenter: parent.verticalCenter; mono: true; font.pixelSize: 11; color: "#8b93a7"; text: root.session ? root.session.banner : "" }
        SLabel { anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter; mono: true; font.pixelSize: 11; color: "#8b93a7"; text: term.cols + "×" + term.rows }
    }

    PromptDialog { source: root.session }

    ConfirmDialog {
        id: pasteDlg
        property string pending
        function askPaste(t) { pending = t; open() }
        title: qsTr("Paste multiple lines?")
        message: qsTr("The text contains line breaks and will run as several commands.")
        confirmText: qsTr("Paste")
        onConfirmed: term.pasteText(pending)
    }
}
