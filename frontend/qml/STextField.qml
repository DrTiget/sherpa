import QtQuick
import QtQuick.Controls.Basic

// Поле ввода. label — подпись сверху, error — текст ошибки под полем, secret — маска с кнопкой «показать».
Item {
    id: root
    property alias text: input.text
    property alias placeholderText: input.placeholderText
    property alias inputItem: input
    property alias readOnly: input.readOnly
    property alias validator: input.validator
    property alias inputMethodHints: input.inputMethodHints
    property string label: ""
    property string error: ""
    property string hint: ""
    property bool mono: false
    property bool secret: false
    property bool required: false
    property bool revealed: false
    signal accepted()
    signal edited()

    implicitWidth: 200
    implicitHeight: (label !== "" ? 18 : 0) + Theme.fieldHeight + (error !== "" || hint !== "" ? 18 : 0)

    function forceFocus() { input.forceActiveFocus() }

    SLabel {
        id: cap
        visible: root.label !== ""
        text: root.label + (root.required ? " *" : "")
        caps: true
        anchors.top: parent.top
        anchors.left: parent.left
        width: parent.width
    }

    Rectangle {
        id: box
        anchors.top: cap.visible ? cap.bottom : parent.top
        anchors.topMargin: cap.visible ? 4 : 0
        width: parent.width
        height: Theme.fieldHeight
        radius: Theme.radius
        color: Theme.inputFill
        border.width: input.activeFocus ? 2 : 1
        border.color: root.error !== "" ? Theme.red : (input.activeFocus ? Theme.cyan : Theme.line)

        TextInput {
            id: input
            property string placeholderText: ""
            anchors.left: parent.left
            anchors.right: eye.visible ? eye.left : parent.right
            anchors.leftMargin: 10
            anchors.rightMargin: 10
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.text
            selectionColor: Theme.accent
            selectedTextColor: "#ffffff"
            font.family: root.mono ? Theme.fontMono : Theme.fontUi
            font.pixelSize: root.mono ? Theme.sizeMono + 0.5 : Theme.sizeUi
            echoMode: root.secret && !root.revealed ? TextInput.Password : TextInput.Normal
            clip: true
            selectByMouse: true
            onAccepted: root.accepted()
            onTextEdited: root.edited()
            Accessible.name: root.label !== "" ? root.label : placeholderText
            Accessible.role: Accessible.EditableText

            SLabel {
                visible: input.text === "" && !input.inputMethodComposing
                text: input.placeholderText
                sub: true
                mono: root.mono
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                opacity: 0.7
            }
        }
        SLabel {
            id: eye
            visible: root.secret
            text: root.revealed ? qsTr("hide") : qsTr("show")
            color: Theme.cyan
            font.pixelSize: Theme.sizeSmall
            anchors.right: parent.right
            anchors.rightMargin: 10
            anchors.verticalCenter: parent.verticalCenter
            MouseArea {
                anchors.fill: parent
                anchors.margins: -4
                cursorShape: Qt.PointingHandCursor
                onClicked: root.revealed = !root.revealed
            }
            Accessible.role: Accessible.Button
            Accessible.name: text
        }
    }

    SLabel {
        visible: root.error !== "" || root.hint !== ""
        text: root.error !== "" ? root.error : root.hint
        color: root.error !== "" ? Theme.red : Theme.textSub
        font.pixelSize: 11
        wrapMode: Text.Wrap
        elide: Text.ElideNone
        anchors.top: box.bottom
        anchors.topMargin: 3
        width: parent.width
    }
}
