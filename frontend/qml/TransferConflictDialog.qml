import QtQuick
import QtQuick.Layouts
import Sherpa

// Целевой файл уже существует: заменить (удалить и записать заново), оставить оба, докачать, пропустить.
SDialog {
    id: root
    property string name: ""
    property bool download: false
    property var pending: null            // { start(mode) } — вызывается выбранным вариантом
    signal chosen(string mode)            // replace | both | resume | skip

    title: qsTr("File already exists")
    dialogWidth: 480
    SLabel {
        Layout.fillWidth: true
        wrapMode: Text.Wrap; elide: Text.ElideNone
        text: root.download ? qsTr("“%1” already exists in the local folder.").arg(root.name)
                            : qsTr("“%1” already exists on the server.").arg(root.name)
    }
    buttons: [
        SButton { text: qsTr("Skip"); onClicked: { root.close(); root.chosen("skip") } },
        SButton { text: qsTr("Resume"); onClicked: { root.close(); root.chosen("resume") } },
        Item { Layout.fillWidth: true },
        SButton { text: qsTr("Keep both"); onClicked: { root.close(); root.chosen("both") } },
        SButton { text: qsTr("Replace"); kind: "danger"; onClicked: { root.close(); root.chosen("replace") } }
    ]
}
