import QtQuick
import QtQuick.Controls.Basic
import Sherpa

// Многострочное поле (команды, заметки, конфиги). mono — моноширинный шрифт.
Rectangle {
    id: root
    property alias text: area.text
    property alias readOnly: area.readOnly
    property alias area: area
    property bool mono: true
    property string placeholderText: ""
    property bool wrap: true
    signal edited()
    implicitHeight: 120
    radius: Theme.radius
    color: Theme.inputFill
    border.width: area.activeFocus ? 2 : 1
    border.color: area.activeFocus ? Theme.cyan : Theme.line
    clip: true
    function forceFocus() { area.forceActiveFocus() }
    Flickable {
        id: fl
        anchors.fill: parent; anchors.margins: 2
        contentWidth: width
        contentHeight: area.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        ScrollBar.vertical: ScrollBar {}
        TextArea.flickable: TextArea {
            id: area
            wrapMode: root.wrap ? TextEdit.Wrap : TextEdit.NoWrap
            color: Theme.text
            selectionColor: Theme.accent
            selectedTextColor: "#ffffff"
            font.family: root.mono ? Theme.fontMono : Theme.fontUi
            font.pixelSize: root.mono ? Theme.sizeMono : Theme.sizeUi
            leftPadding: 8; rightPadding: 8; topPadding: 6; bottomPadding: 6
            background: null
            onTextChanged: if (activeFocus) root.edited()
            Text { visible: area.text === "" && root.placeholderText !== ""; text: root.placeholderText; color: Theme.textSub
                   font: area.font; x: area.leftPadding; y: area.topPadding }
        }
    }
}
