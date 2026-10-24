import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects

// Кнопка Sherpa. kind: "secondary" (по умолчанию) | "primary" | "danger" | "ghost".
Button {
    id: root
    property string kind: "secondary"
    property bool busy: false
    property bool compact: false

    implicitHeight: compact ? 26 : Theme.fieldHeight
    implicitWidth: Math.max(compact ? 0 : 72, label.implicitWidth + (compact ? 20 : 28) + (busy ? 22 : 0))
    leftPadding: 0
    rightPadding: 0
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    opacity: enabled ? 1 : 0.4

    readonly property color tone: kind === "danger" ? Theme.red : (kind === "primary" ? Theme.accent : Theme.textSub)

    background: Item {
        Rectangle {
            id: glow
            visible: root.kind === "primary" && Theme.fxAvailable && root.enabled
            anchors.fill: face
            anchors.margins: -1
            radius: face.radius
            color: Theme.alpha(Theme.accent, 0.5)
            layer.enabled: visible
            layer.effect: MultiEffect {
                blurEnabled: true
                blurMax: 24
                blur: 1.0
                autoPaddingEnabled: true
            }
        }
        Rectangle {
            id: face
            anchors.fill: parent
            radius: Theme.radius
            color: {
                if (root.kind === "primary")
                    return root.down ? Qt.darker(Theme.accent, 1.2) : (root.hovered ? Qt.lighter(Theme.accent, 1.12) : Theme.accent)
                if (root.kind === "danger")
                    return Theme.alpha(Theme.red, root.hovered ? 0.28 : 0.16)
                if (root.kind === "ghost")
                    return root.hovered ? Theme.hover : "transparent"
                return root.hovered ? Theme.hover : Theme.card
            }
            border.width: root.kind === "ghost" ? 0 : 1
            border.color: root.kind === "primary" ? Theme.alpha(Theme.cyan, 0.55)
                          : (root.kind === "danger" ? Theme.alpha(Theme.red, 0.5) : Theme.line)
        }
        Rectangle {
            visible: root.visualFocus
            anchors.fill: parent
            anchors.margins: -2
            radius: Theme.radius + 2
            color: "transparent"
            border.width: 2
            border.color: Theme.cyan
        }
    }

    contentItem: Item { Row {
        spacing: 7
        anchors.centerIn: parent
        SSpinner {
            running: root.busy
            visible: root.busy
            color: root.kind === "primary" ? "#ffffff" : Theme.cyan
            anchors.verticalCenter: parent.verticalCenter
        }
        SLabel {
            id: label
            text: root.text
            anchors.verticalCenter: parent.verticalCenter
            font.bold: true
            font.pixelSize: root.compact ? Theme.sizeSmall : Theme.sizeUi
            color: root.kind === "primary" ? "#ffffff" : (root.kind === "danger" ? Theme.red : Theme.text)
        }
    } }

    Keys.onReturnPressed: if (visualFocus) clicked()
    Keys.onEnterPressed: if (visualFocus) clicked()
    Accessible.role: Accessible.Button
    Accessible.name: text
}
