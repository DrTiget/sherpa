import QtQuick
import QtQuick.Layouts

// Полоса-уведомление сверху: ошибка ядра с «Повторить» или напоминание о копии ключа.
Rectangle {
    id: root
    property string text: ""
    property color tone: Theme.red
    property string actionText: ""
    property string secondaryText: ""
    signal action()
    signal secondary()
    signal dismissed()

    implicitHeight: row.implicitHeight + 16
    color: Theme.alpha(tone, 0.14)
    border.color: Theme.alpha(tone, 0.5)
    border.width: 1
    radius: Theme.radius

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.margins: 8
        anchors.leftMargin: 12
        spacing: 10
        Rectangle { width: 8; height: 8; radius: 4; color: root.tone }
        SLabel {
            Layout.fillWidth: true
            text: root.text
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            color: Theme.text
        }
        SButton { visible: root.actionText !== ""; text: root.actionText; compact: true; onClicked: root.action() }
        SButton { visible: root.secondaryText !== ""; text: root.secondaryText; compact: true; kind: "ghost"; onClicked: root.secondary() }
        SLabel {
            text: "×"
            font.pixelSize: 18
            sub: true
            MouseArea { anchors.fill: parent; anchors.margins: -6; cursorShape: Qt.PointingHandCursor; onClicked: root.dismissed() }
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Dismiss")
        }
    }
    Accessible.role: Accessible.AlertMessage
    Accessible.name: text
}
