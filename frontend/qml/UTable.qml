import QtQuick
import QtQuick.Controls.Basic
import Sherpa

// Таблица с заголовком (сортировка по клику), выбором строки и разными видами ячеек.
// columns: [{key, title, w (px, 0 = остаток), kind: text|meter|dot, mono, right, fmt: function(row), color: function(row)}]
Rectangle {
    id: t
    property var columns: []
    property var rows: []
    property int currentIndex: -1
    property string sortKey: ""
    property bool sortDesc: false
    property int rowH: 26
    property bool mono: false
    property string checkKey: ""          // если задано — первая колонка с флажками; checked: {значение checkKey: true}
    property var checked: ({})
    readonly property real checkW: checkKey !== "" ? 30 : 0
    function toggleCheck(row) { const c = Object.assign({}, checked); const k = row[checkKey]; if (c[k]) delete c[k]; else c[k] = true; checked = c }
    function checkedKeys() { return Object.keys(checked) }
    property string emptyText: ""
    property alias view: list
    signal rowClicked(int index, var row)
    signal rowDoubleClicked(int index, var row)
    signal rowContext(int index, var row)
    signal sortClicked(string key)

    radius: 8; color: Theme.inputFill; border.color: Theme.line; clip: true
    readonly property real fixedW: { let s = 0; for (const c of columns) s += (c.w || 0); return s + checkW }
    function colW(c) { return c.w ? c.w : Math.max(60, t.width - 2 - fixedW) }
    function cell(c, row) { return c.fmt ? c.fmt(row) : (row[c.key] === undefined || row[c.key] === null ? "" : String(row[c.key])) }

    Row {
        id: head
        height: 24
        Rectangle {
            visible: t.checkKey !== ""; width: t.checkW; height: 24; color: Theme.alpha(Theme.panelFill, 0.8)
            SLabel { anchors.centerIn: parent; text: t.rows.length > 0 && t.checkedKeys().length === t.rows.length ? "☑" : "☐"; mono: true; sub: true }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                        onClicked: { if (t.checkedKeys().length === t.rows.length) t.checked = ({}); else { const c = {}; for (const r of t.rows) c[r[t.checkKey]] = true; t.checked = c } } }
        }
        Repeater {
            model: t.columns
            delegate: Rectangle {
                required property var modelData
                width: t.colW(modelData); height: 24
                color: Theme.alpha(Theme.panelFill, 0.8)
                SLabel { x: 8; width: parent.width - 12; anchors.verticalCenter: parent.verticalCenter; caps: true
                         horizontalAlignment: modelData.right ? Text.AlignRight : Text.AlignLeft
                         text: modelData.title + (t.sortKey === modelData.key ? (t.sortDesc ? " ▼" : " ▲") : "") }
                MouseArea { anchors.fill: parent; enabled: !!modelData.sortable; cursorShape: modelData.sortable ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: t.sortClicked(modelData.key) }
            }
        }
    }
    ListView {
        id: list
        objectName: "tableList"
        anchors { top: head.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        clip: true
        model: t.rows
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        delegate: Rectangle {
            id: rowItem
            required property var modelData
            required property int index
            width: ListView.view.width; height: t.rowH
            color: t.currentIndex === index ? Theme.selected : (rma.containsMouse ? Theme.hover : (index % 2 ? Theme.alpha(Theme.text, 0.03) : "transparent"))
            Row {
                anchors.fill: parent
                Item {
                    visible: t.checkKey !== ""; width: t.checkW; height: t.rowH
                    SLabel { anchors.centerIn: parent; mono: true; color: t.checked[rowItem.modelData[t.checkKey]] ? Theme.cyan : Theme.textSub; text: t.checked[rowItem.modelData[t.checkKey]] ? "☑" : "☐" }
                    MouseArea { anchors.fill: parent; onClicked: t.toggleCheck(rowItem.modelData) }
                }
                Repeater {
                    model: t.columns
                    delegate: Item {
                        id: cellItem
                        required property var modelData
                        readonly property var row: rowItem.modelData
                        readonly property string kind: modelData.kind || "text"
                        width: t.colW(modelData); height: t.rowH
                        SLabel {
                            visible: cellItem.kind !== "meter"
                            x: cellItem.kind === "dot" ? 22 : 8; width: parent.width - x - 4; anchors.verticalCenter: parent.verticalCenter
                            mono: modelData.mono || t.mono; font.pixelSize: (modelData.mono || t.mono) ? 11 : Theme.sizeSmall
                            horizontalAlignment: modelData.right ? Text.AlignRight : Text.AlignLeft
                            color: modelData.color ? modelData.color(cellItem.row) : Theme.text
                            text: t.cell(modelData, cellItem.row)
                        }
                        Rectangle { visible: cellItem.kind === "dot"; x: 8; width: 8; height: 8; radius: 4; anchors.verticalCenter: parent.verticalCenter
                                    color: modelData.color ? modelData.color(cellItem.row) : Theme.gray }
                        Item {
                            visible: cellItem.kind === "meter"
                            anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 8
                            readonly property real pct: cellItem.kind === "meter" ? Number(modelData.value ? modelData.value(cellItem.row) : 0) : 0
                            Rectangle { width: parent.width; height: 8; radius: 4; anchors.verticalCenter: parent.verticalCenter; color: Theme.alpha(Theme.text, 0.1) }
                            Rectangle { width: parent.width * Math.min(1, parent.pct / 100); height: 8; radius: 4; anchors.verticalCenter: parent.verticalCenter
                                        color: modelData.color ? modelData.color(cellItem.row) : Theme.green }
                        }
                    }
                }
            }
            MouseArea {
                id: rma
                anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.LeftButton | Qt.RightButton
                onClicked: (m) => { t.currentIndex = rowItem.index; m.button === Qt.RightButton ? t.rowContext(rowItem.index, rowItem.modelData) : t.rowClicked(rowItem.index, rowItem.modelData) }
                onDoubleClicked: t.rowDoubleClicked(rowItem.index, rowItem.modelData)
            }
        }
    }
    SLabel { visible: t.rows.length === 0 && t.emptyText !== ""; anchors.centerIn: parent; sub: true; text: t.emptyText }
}
