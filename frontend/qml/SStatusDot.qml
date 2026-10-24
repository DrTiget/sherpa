import QtQuick

// Индикатор статуса со свечением; при проверке — спиннер.
Item {
    id: root
    property string state: "unknown"
    property bool checking: false
    property int size: 10
    implicitWidth: size + 6
    implicitHeight: size + 6

    readonly property color tone: Theme.stateColor(state)

    Rectangle {
        visible: !root.checking && root.state !== "unknown" && Theme.fxAvailable
        anchors.centerIn: parent
        width: root.size + 8
        height: width
        radius: width / 2
        color: Theme.alpha(root.tone, 0.22)
    }
    Rectangle {
        visible: !root.checking
        anchors.centerIn: parent
        width: root.size
        height: root.size
        radius: width / 2
        color: root.tone
    }
    SSpinner {
        anchors.centerIn: parent
        width: root.size + 2
        height: root.size + 2
        running: root.checking
        visible: root.checking
    }
    Accessible.role: Accessible.StaticText
    Accessible.name: root.checking ? qsTr("checking") : Theme.stateLabel(root.state)
}
