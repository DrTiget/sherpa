import QtQuick
import Sherpa
import QtQuick.Layouts

// Ядро недоступно / хранилище не открывается
Item {
    id: root
    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 520)
        spacing: 12
        SLabel { Layout.alignment: Qt.AlignHCenter; text: qsTr("Cannot start"); font.pixelSize: 18; font.bold: true; color: Theme.red }
        SLabel { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: Backend.fatalText; wrapMode: Text.Wrap; elide: Text.ElideNone }
        SLabel { Layout.alignment: Qt.AlignHCenter; mono: true; sub: true; text: Backend.fatalCode }
        SButton { Layout.alignment: Qt.AlignHCenter; kind: "primary"; text: qsTr("Retry"); onClicked: Backend.start() }
    }
}
