import QtQuick

// Теги чипами: Enter или запятая добавляет, Backspace в пустом поле удаляет последний.
Item {
    id: root
    property var tags: []
    property string label: ""
    implicitHeight: (label !== "" ? 22 : 0) + Math.max(Theme.fieldHeight, flow.implicitHeight + 10)
    implicitWidth: 200

    function addTag(raw) {
        var t = raw.trim().toLowerCase()
        if (t === "" || tags.indexOf(t) >= 0)
            return
        tags = tags.concat([t])
    }

    SLabel {
        id: cap
        visible: root.label !== ""
        text: root.label
        caps: true
    }
    Rectangle {
        anchors.top: cap.visible ? cap.bottom : parent.top
        anchors.topMargin: cap.visible ? 4 : 0
        width: parent.width
        height: Math.max(Theme.fieldHeight, flow.implicitHeight + 10)
        radius: Theme.radius
        color: Theme.inputFill
        border.width: input.activeFocus ? 2 : 1
        border.color: input.activeFocus ? Theme.cyan : Theme.line

        Flow {
            id: flow
            x: 8
            y: 5
            width: parent.width - 16
            spacing: 5
            Repeater {
                model: root.tags
                delegate: SChip {
                    required property string modelData
                    text: modelData
                    tone: Theme.cyan
                    removable: true
                    onRemoved: root.tags = root.tags.filter(function (t) { return t !== modelData })
                }
            }
            TextInput {
                id: input
                width: Math.max(80, flow.width - 4)
                height: 24
                color: Theme.text
                font.family: Theme.fontMono
                font.pixelSize: Theme.sizeMono
                verticalAlignment: TextInput.AlignVCenter
                clip: true
                Accessible.name: root.label
                onTextChanged: {
                    if (text.indexOf(",") >= 0) {
                        root.addTag(text.replace(/,/g, ""))
                        text = ""
                    }
                }
                Keys.onReturnPressed: (e) => { if (text.trim() !== "") { root.addTag(text); text = ""; e.accepted = true } else e.accepted = false }
                Keys.onEnterPressed: (e) => { if (text.trim() !== "") { root.addTag(text); text = ""; e.accepted = true } else e.accepted = false }
                Keys.onBackPressed: (e) => { if (text === "" && root.tags.length) { root.tags = root.tags.slice(0, -1); e.accepted = true } else e.accepted = false }
                onActiveFocusChanged: if (!activeFocus && text.trim() !== "") { root.addTag(text); text = "" }
                SLabel {
                    visible: input.text === "" && root.tags.length === 0
                    text: qsTr("prod, nginx…")
                    sub: true
                    mono: true
                    opacity: 0.7
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }
    }
}
