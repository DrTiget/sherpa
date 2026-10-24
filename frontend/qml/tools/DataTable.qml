import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Таблица данных с сортировкой по клику на заголовок и кнопками действий у строки.
// columns: [{key, title, w (px) | flex, mono, right, text(row), tone(row)->цвет, chip}]
Rectangle {
    id: t
    property var columns: []
    property var rows: []
    property int current: -1
    property var actions: []            // [{id, text, kind, visible(row)}]
    property string emptyText: qsTr("Nothing to show")
    property bool loading: false
    property string sortKey: ""
    property bool sortDesc: false
    property bool compactRows: false
    signal rowClicked(int index, var row)
    signal rowDoubleClicked(int index, var row)
    signal action(string id, var row)

    readonly property int rowH: compactRows ? 24 : 28
    readonly property var view: {
        const l = (rows || []).slice()
        if (sortKey !== "") {
            const col = columns.find(c => c.key === sortKey)
            const val = (r) => col && col.sort ? col.sort(r) : (col && col.text ? col.text(r) : r[sortKey])
            const d = sortDesc ? -1 : 1
            l.sort((a, b) => {
                const x = val(a), y = val(b)
                if (x === y) return 0
                if (x === undefined || x === null) return 1
                if (y === undefined || y === null) return -1
                return (typeof x === "string" ? x.localeCompare(y) : (x - y)) * d
            })
        }
        return l
    }
    readonly property real flexTotal: columns.reduce((s, c) => s + (c.w ? 0 : (c.flex || 1)), 0)
    readonly property real fixedTotal: columns.reduce((s, c) => s + (c.w || 0), 0)
    function colW(c) { return c.w ? c.w : Math.max(40, (width - fixedTotal - 2) * (c.flex || 1) / Math.max(1, flexTotal)) }
    function selectedRow() { return current >= 0 && current < view.length ? view[current] : null }

    radius: 8
    color: Theme.inputFill
    border.color: Theme.line
    clip: true

    Row {
        id: head
        height: 26
        width: parent.width
        Repeater {
            model: t.columns
            delegate: Rectangle {
                required property var modelData
                width: t.colW(modelData); height: head.height
                color: Theme.alpha(Theme.panelFill, 0.8)
                SLabel { x: 8; width: parent.width - 12; anchors.verticalCenter: parent.verticalCenter; caps: true
                         horizontalAlignment: modelData.right ? Text.AlignRight : Text.AlignLeft
                         text: modelData.title + (t.sortKey === modelData.key ? (t.sortDesc ? " ▼" : " ▲") : "") }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                            onClicked: { if (t.sortKey === modelData.key) t.sortDesc = !t.sortDesc; else { t.sortKey = modelData.key; t.sortDesc = false } } }
            }
        }
    }
    ListView {
        id: list
        anchors.top: head.bottom; anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        model: t.view
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        delegate: Rectangle {
            id: rowItem
            required property var modelData
            required property int index
            width: ListView.view.width
            height: t.rowH
            color: t.current === index ? Theme.selected : (rma.containsMouse ? Theme.hover : (index % 2 ? Theme.alpha(Theme.text, 0.03) : "transparent"))
            MouseArea { id: rma; anchors.fill: parent; hoverEnabled: true
                        onClicked: { t.current = rowItem.index; t.rowClicked(rowItem.index, rowItem.modelData) }
                        onDoubleClicked: t.rowDoubleClicked(rowItem.index, rowItem.modelData) }
            Row {
                anchors.fill: parent
                Repeater {
                    model: t.columns
                    delegate: Item {
                        id: cell
                        required property var modelData
                        readonly property var row: rowItem.modelData
                        readonly property string str: {
                            const v = modelData.text ? modelData.text(row) : row[modelData.key]
                            return v === undefined || v === null ? "—" : String(v)
                        }
                        width: t.colW(modelData); height: rowItem.height
                        SLabel {
                            visible: !modelData.chip
                            x: 8; width: parent.width - 12; anchors.verticalCenter: parent.verticalCenter
                            horizontalAlignment: modelData.right ? Text.AlignRight : Text.AlignLeft
                            mono: !!modelData.mono; font.pixelSize: modelData.mono ? 11 : Theme.sizeSmall
                            text: cell.str
                            color: modelData.tone ? modelData.tone(cell.row) : Theme.text
                        }
                        SChip { visible: !!modelData.chip && cell.str !== "—"; x: 8; anchors.verticalCenter: parent.verticalCenter
                                text: cell.str; tone: modelData.tone ? modelData.tone(cell.row) : Theme.textSub; mono: false }
                    }
                }
            }
            // действия: поверх правого края строки, только при наведении или выборе
            Rectangle {
                visible: t.actions.length > 0 && (rma.containsMouse || oh.hovered || t.current === rowItem.index)
                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                height: parent.height - 2; width: actRow.implicitWidth + 12
                radius: 6
                color: Theme.light ? "#eef2f8" : "#10151f"
                border.color: Theme.line
                HoverHandler { id: oh }
                Row {
                    id: actRow
                    anchors.centerIn: parent
                    spacing: 4
                    Repeater {
                        model: t.actions
                        delegate: SButton {
                            required property var modelData
                            visible: !modelData.visible || modelData.visible(rowItem.modelData)
                            compact: true; text: modelData.text; kind: modelData.kind || "secondary"
                            enabled: !modelData.enabled || modelData.enabled(rowItem.modelData)
                            onClicked: { t.current = rowItem.index; t.action(modelData.id, rowItem.modelData) }
                        }
                    }
                }
            }
        }
    }
    SLabel { visible: !t.loading && t.view.length === 0; anchors.centerIn: parent; sub: true; text: t.emptyText }
    SSpinner { visible: t.loading && t.view.length === 0; anchors.centerIn: parent; width: 20; height: 20 }
}
