import QtQuick

// Метка (группа, тег). tone — цвет смысла; clickable — для фильтра по тегу; removable — крестик.
Rectangle {
    id: root
    property string text: ""
    property color tone: Theme.textSub
    property bool removable: false
    property bool active: false
    property bool mono: true
    signal clicked()
    signal removed()

    implicitHeight: 20
    implicitWidth: row.implicitWidth + 14
    radius: 5
    color: Theme.alpha(tone, active ? 0.3 : 0.14)
    border.color: Theme.alpha(tone, active ? 0.8 : 0.4)
    border.width: 1

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 5
        SLabel {
            text: root.text
            mono: root.mono
            color: root.tone
            font.pixelSize: 11
            anchors.verticalCenter: parent.verticalCenter
        }
        SLabel {
            visible: root.removable
            text: "×"
            color: root.tone
            font.pixelSize: 13
            anchors.verticalCenter: parent.verticalCenter
            MouseArea {
                anchors.fill: parent
                anchors.margins: -4
                cursorShape: Qt.PointingHandCursor
                onClicked: root.removed()
            }
            Accessible.role: Accessible.Button
            Accessible.name: qsTr("Remove %1").arg(root.text)
        }
    }
    MouseArea {
        anchors.fill: parent
        enabled: !root.removable
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
    }
    Accessible.role: Accessible.StaticText
    Accessible.name: root.text
}
