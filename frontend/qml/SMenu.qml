import QtQuick
import QtQuick.Controls.Basic

// Контекстное меню в стиле Sherpa
Menu {
    id: root
    padding: 4
    background: Rectangle {
        implicitWidth: 200
        radius: Theme.radius
        color: Theme.light ? "#ffffff" : "#161a26"
        border.color: Theme.lineStrong
    }
    delegate: MenuItem {
        id: item
        implicitHeight: 30
        implicitWidth: 200
        leftPadding: 12
        rightPadding: 12
        background: Rectangle { radius: 4; color: item.highlighted ? Theme.hover : "transparent"; opacity: item.enabled ? 1 : 0.4 }
        contentItem: SLabel { text: item.text; verticalAlignment: Text.AlignVCenter; opacity: item.enabled ? 1 : 0.4 }
    }
}
