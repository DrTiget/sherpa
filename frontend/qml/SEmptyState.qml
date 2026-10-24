import QtQuick
import QtQuick.Layouts

// Пустое состояние: «иллюстрация» (рисуется кодом), заголовок, пояснение и до двух кнопок.
Item {
    id: root
    property string title: ""
    property string description: ""
    property string glyph: ">_"
    property string primaryText: ""
    property string secondaryText: ""
    signal primary()
    signal secondary()

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 380)
        spacing: 12

        Rectangle {
            Layout.alignment: Qt.AlignHCenter
            width: 72; height: 72; radius: 18
            color: Theme.alpha(Theme.cyan, 0.12)
            border.color: Theme.alpha(Theme.cyan, 0.5)
            SLabel {
                anchors.centerIn: parent
                text: root.glyph
                mono: true
                color: Theme.cyan
                font.pixelSize: 26
                font.bold: true
            }
        }
        SLabel {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: root.title
            font.pixelSize: 16
            font.bold: true
        }
        SLabel {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            visible: root.description !== ""
            text: root.description
            sub: true
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: 8
            SButton { visible: root.primaryText !== ""; kind: "primary"; text: root.primaryText; onClicked: root.primary() }
            SButton { visible: root.secondaryText !== ""; text: root.secondaryText; onClicked: root.secondary() }
        }
    }
}
