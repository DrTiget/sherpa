import QtQuick
import QtQuick.Layouts

// Вкладки верхнего уровня. «Серверы» всегда первая; сюда позже добавятся вкладки терминала/рабочего стола.
// tabs: [{title, kind}] кроме первой; content — по StackLayout.
ColumnLayout {
    id: root
    spacing: 0
    property int current: 0
    property var extraTabs: []          // [{id, title, state}]
    default property alias pages: stack.data
    signal closeTab(string id)

    Rectangle {
        Layout.fillWidth: true
        Layout.preferredHeight: 38
        color: Theme.alpha(Theme.panelFill, Theme.translucent ? 0.7 : 1)
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
        Row {
            anchors.verticalCenter: parent.verticalCenter
            x: 12
            spacing: 6
            Repeater {
                model: [{ id: "home", title: qsTr("Servers") }].concat(root.extraTabs)
                delegate: Rectangle {
                    required property var modelData
                    required property int index
                    readonly property bool on: root.current === index
                    height: 28
                    width: tl.implicitWidth + 28 + (index > 0 ? 18 : 0)
                    radius: Theme.radius
                    color: on ? Theme.alpha(Theme.accent, 0.25) : (ma.containsMouse ? Theme.hover : "transparent")
                    border.color: on ? Theme.selectedBorder : "transparent"
                    SLabel { id: tl; x: 14; anchors.verticalCenter: parent.verticalCenter; text: modelData.title; font.bold: true; color: parent.on ? Theme.text : Theme.textSub }
                    MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; onClicked: root.current = index }
                    Accessible.role: Accessible.PageTab
                    Accessible.name: modelData.title
                }
            }
        }
    }
    StackLayout {
        id: stack
        Layout.fillWidth: true
        Layout.fillHeight: true
        currentIndex: root.current
    }
}
