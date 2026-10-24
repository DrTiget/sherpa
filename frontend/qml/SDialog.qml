import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

// Модальный диалог: заголовок, прокручиваемое содержимое, строка кнопок.
// tone — цвет свечения (синий обычный, красный для опасных действий и смены host key).
Popup {
    id: root
    property string title: ""
    property color tone: Theme.accent
    property int dialogWidth: 520
    property bool showClose: true
    property Item blurSource: Theme.contentItem
    default property alias content: bodyCol.data
    property alias buttons: buttonRow.data
    property alias bodyImplicitHeight: bodyCol.implicitHeight
    property bool guardClose: false   // × и Esc не закрывают сами, а шлют closeRequested (проверка несохранённого)
    signal acceptRequested()
    signal closeRequested()
    function requestClose() { guardClose ? closeRequested() : close() }

    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    padding: 0
    width: Math.min(dialogWidth, (parent ? parent.width : dialogWidth) - 32)
    closePolicy: guardClose ? Popup.NoAutoClose : Popup.CloseOnEscape
    Overlay.modal: Rectangle { color: Theme.scrim }

    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 120 } }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 90 } }

    background: Item {
        // свечение цвета смысла
        Rectangle {
            visible: Theme.fxAvailable
            anchors.fill: panel
            anchors.margins: -3
            radius: Theme.radiusLarge + 3
            color: "transparent"
            border.width: 3
            border.color: Theme.alpha(root.tone, 0.25)
        }
        GlassPanel {
            id: panel
            anchors.fill: parent
            source: root.blurSource
            color: Theme.dialogFill
            radius: Theme.radiusLarge
            borderColor: Theme.alpha(root.tone, 0.7)
            borderWidth: 1
        }
    }



    Shortcut {
        sequence: "Escape"
        enabled: root.opened && root.guardClose
        onActivated: root.closeRequested()
    }

    contentItem: ColumnLayout {
        spacing: 0
        Keys.onReturnPressed: root.acceptRequested()
        Keys.onEnterPressed: root.acceptRequested()

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 18
            Layout.bottomMargin: 8
            SLabel {
                Layout.fillWidth: true
                text: root.title
                font.pixelSize: 17
                font.bold: true
                color: root.tone === Theme.red ? Theme.red : Theme.text
            }
            SLabel {
                visible: root.showClose
                text: "×"
                font.pixelSize: 20
                sub: true
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -6
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.requestClose()
                }
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Close")
            }
        }

        Flickable {
            id: flick
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 18
            Layout.preferredHeight: Math.min(bodyCol.implicitHeight,
                                             (root.parent ? root.parent.height : 700) - 48 - 130)
            contentWidth: width
            contentHeight: bodyCol.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: flick.contentHeight > flick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff }
            ColumnLayout {
                id: bodyCol
                width: flick.width - (flick.contentHeight > flick.height ? 12 : 0)
                spacing: 12
            }
        }

        RowLayout {
            id: buttonRow
            Layout.fillWidth: true
            Layout.margins: 18
            Layout.topMargin: 14
            spacing: 8
        }
    }
}
