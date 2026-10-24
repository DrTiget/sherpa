import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Панель передач: очередь, прогресс, скорость, оставшееся время, отмена и повтор.
Rectangle {
    id: root
    required property var model          // ListModel: tid, direction, name, status, bytes, total, speed, error, errorCode
    property bool expanded: true
    readonly property int activeCount: {
        let n = 0
        for (let i = 0; i < model.count; ++i) { const s = model.get(i).status; if (s === "running" || s === "queued") n++ }
        return n
    }
    signal cancelRequested(string tid)
    signal retryRequested(string tid)
    signal removeRequested(string tid)
    signal clearFinished()

    color: Theme.alpha(Theme.panelFill, Theme.blurOn ? 0.5 : 1)
    implicitHeight: expanded ? 168 : 30
    Behavior on implicitHeight { NumberAnimation { duration: 120 } }
    clip: true
    Rectangle { width: parent.width; height: 1; color: Theme.line }

    function fmtBytes(b) {
        if (b === undefined || b === null) return "—"
        const u = ["B", "KB", "MB", "GB", "TB"]
        let i = 0, v = b
        while (v >= 1024 && i < 4) { v /= 1024; i++ }
        return v.toFixed(i > 1 ? 1 : 0) + " " + u[i]
    }
    function fmtEta(sec) {
        if (!isFinite(sec) || sec < 0) return ""
        if (sec < 60) return Math.ceil(sec) + qsTr("s")
        if (sec < 3600) return Math.floor(sec / 60) + qsTr("m") + " " + Math.ceil(sec % 60) + qsTr("s")
        return Math.floor(sec / 3600) + qsTr("h") + " " + Math.floor(sec % 3600 / 60) + qsTr("m")
    }
    function statusText(s) {
        switch (s) {
        case "queued": return qsTr("queued")
        case "running": return qsTr("in progress")
        case "completed": return qsTr("done")
        case "cancelled": return qsTr("cancelled")
        case "failed": return qsTr("failed")
        }
        return s
    }

    RowLayout {
        id: head
        x: 12; y: 0
        width: parent.width - 24
        height: 30
        SLabel { text: qsTr("Transfers"); caps: true }
        SChip { visible: root.activeCount > 0; text: String(root.activeCount); tone: Theme.cyan }
        Item { Layout.fillWidth: true }
        SButton { visible: root.expanded && root.model.count > 0; compact: true; kind: "ghost"; text: qsTr("Clear finished"); onClicked: root.clearFinished() }
        SLabel {
            text: root.expanded ? "▾" : "▴"
            sub: true
            font.pixelSize: 14
            MouseArea { anchors.fill: parent; anchors.margins: -6; cursorShape: Qt.PointingHandCursor; onClicked: root.expanded = !root.expanded }
            Accessible.role: Accessible.Button
            Accessible.name: root.expanded ? qsTr("Collapse transfers") : qsTr("Expand transfers")
        }
    }

    ListView {
        anchors.top: head.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        model: root.model
        ScrollBar.vertical: ScrollBar {}
        delegate: Item {
            id: row
            required property int index
            required property string tid
            required property string direction
            required property string name
            required property string status
            required property double bytes
            required property double total
            required property double speed
            required property string error
            width: ListView.view.width
            height: 34
            readonly property real frac: total > 0 ? Math.min(1, bytes / total) : (status === "completed" ? 1 : 0)
            readonly property color tone: status === "failed" ? Theme.red : (status === "completed" ? Theme.green : (status === "cancelled" ? Theme.amber : Theme.cyan))
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12; anchors.rightMargin: 12
                spacing: 10
                SLabel { text: row.direction === "upload" ? "↑" : "↓"; color: Theme.cyan; font.bold: true; font.pixelSize: 15 }
                SLabel { Layout.preferredWidth: 190; text: row.name; elide: Text.ElideMiddle }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 6
                    radius: 3
                    color: Theme.alpha(Theme.text, 0.12)
                    Rectangle { width: parent.width * row.frac; height: parent.height; radius: 3; color: row.tone }
                }
                SLabel {
                    Layout.preferredWidth: 220
                    mono: true; font.pixelSize: 11; horizontalAlignment: Text.AlignRight
                    elide: Text.ElideRight
                    color: row.status === "failed" ? Theme.red : Theme.text
                    text: row.status === "failed" ? row.error : row.status === "running"
                          ? root.fmtBytes(row.bytes) + (row.total > 0 ? " / " + root.fmtBytes(row.total) : "")
                          : (row.status === "completed" ? root.fmtBytes(row.bytes) : "")
                }
                SLabel {
                    Layout.preferredWidth: 110
                    mono: true; font.pixelSize: 11; sub: true; horizontalAlignment: Text.AlignRight
                    text: row.status === "running" && row.speed > 0
                          ? root.fmtBytes(row.speed) + "/s " + (row.total > 0 ? root.fmtEta((row.total - row.bytes) / row.speed) : "") : ""
                }
                SLabel {
                    Layout.preferredWidth: 80
                    font.pixelSize: 11; color: row.tone
                    text: root.statusText(row.status)
                }
                SButton { visible: row.status === "running" || row.status === "queued"; compact: true; text: qsTr("Cancel"); onClicked: root.cancelRequested(row.tid) }
                SButton { visible: row.status === "failed" || row.status === "cancelled"; compact: true; text: qsTr("Resume"); onClicked: root.retryRequested(row.tid) }
                SLabel {
                    visible: row.status !== "running" && row.status !== "queued"
                    text: "×"; sub: true; font.pixelSize: 15
                    MouseArea { anchors.fill: parent; anchors.margins: -5; cursorShape: Qt.PointingHandCursor; onClicked: root.removeRequested(row.tid) }
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("Remove from list")
                }
            }
        }
    }
    SLabel {
        visible: root.expanded && root.model.count === 0
        anchors.centerIn: parent
        anchors.verticalCenterOffset: 12
        sub: true
        text: qsTr("No transfers. Drop files onto the window to upload.")
    }
}
