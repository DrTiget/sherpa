import QtQuick

// Тост снизу справа. show(text, kind): kind = "ok" | "error" | "info"
Item {
    id: root
    width: 360
    height: card.height
    property bool shown: false

    function show(text, kind) {
        label.text = text
        card.tone = kind === "error" ? Theme.red : (kind === "ok" ? Theme.green : Theme.cyan)
        shown = true
        hideTimer.restart()
    }

    Rectangle {
        id: card
        property color tone: Theme.cyan
        width: parent.width
        height: label.implicitHeight + 24
        radius: Theme.radius
        color: Theme.light ? "#ffffff" : "#161a26"
        border.color: Theme.alpha(tone, 0.7)
        opacity: root.shown ? 1 : 0
        y: root.shown ? 0 : 12
        Behavior on opacity { NumberAnimation { duration: 150 } }
        Behavior on y { NumberAnimation { duration: 150 } }
        Rectangle { width: 3; height: parent.height - 14; x: 7; anchors.verticalCenter: parent.verticalCenter; radius: 2; color: card.tone }
        SLabel {
            id: label
            x: 20
            width: parent.width - 32
            anchors.verticalCenter: parent.verticalCenter
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
        MouseArea { anchors.fill: parent; onClicked: root.shown = false }
    }
    Timer { id: hideTimer; interval: 4500; onTriggered: root.shown = false }
    visible: shown || card.opacity > 0.01
    Accessible.role: Accessible.AlertMessage
    Accessible.name: label.text
}
