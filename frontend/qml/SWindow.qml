import QtQuick
import Sherpa

// Окно утилиты внутри рабочего стола (не окно ОС): перетаскивание, ресайз, свернуть/развернуть,
// привязка к краям. Геометрию «в обычном виде» помнит отдельно (normalRect).
Item {
    id: win
    required property Item area                 // область, в которой живёт окно
    property string title: ""
    property string subtitle: ""                // имя сервера
    property string glyph: ">_"
    property color accent: Theme.cyan
    property bool active: false
    property bool prod: false                   // красная полоса «Prod»
    property bool dimmed: false                 // обрыв связи — окно тускнеет
    property string wstate: "normal"            // normal | max | min | snapped
    property int minW: 360
    property int minH: 220
    property bool contentTransparent: false     // терминал рисует свой фон
    default property alias content: body.data
    property rect normalRect: Qt.rect(x, y, width, height)
    readonly property bool minimized: wstate === "min"
    readonly property Item contentItem: body

    signal closeRequested()
    signal activated()
    signal layoutChanged()
    signal snapPreview(var rect)                // rect или null

    visible: !minimized
    opacity: dimmed ? 0.55 : 1

    function rememberNormal() { if (wstate === "normal") normalRect = Qt.rect(x, y, width, height) }
    function setRect(r) { x = r.x; y = r.y; width = r.width; height = r.height }
    function maximize() {
        if (wstate === "normal") rememberNormal()
        wstate = "max"
        setRect(Qt.rect(0, 0, area.width, area.height))
        layoutChanged()
    }
    function restore() {
        wstate = "normal"
        setRect(normalRect)
        layoutChanged()
    }
    function toggleMax() { wstate === "max" ? restore() : maximize() }
    function minimize() { rememberNormal(); wstate = "min"; layoutChanged() }
    function unminimize() { wstate = "normal"; setRect(normalRect); activated(); layoutChanged() }
    function snapTo(zone) {
        if (zone === "") return
        if (wstate === "normal") rememberNormal()
        const W = area.width, H = area.height
        const r = {
            "left": Qt.rect(0, 0, W / 2, H), "right": Qt.rect(W / 2, 0, W / 2, H),
            "top": Qt.rect(0, 0, W, H),
            "tl": Qt.rect(0, 0, W / 2, H / 2), "tr": Qt.rect(W / 2, 0, W / 2, H / 2),
            "bl": Qt.rect(0, H / 2, W / 2, H / 2), "br": Qt.rect(W / 2, H / 2, W / 2, H / 2)
        }[zone]
        wstate = zone === "top" ? "max" : "snapped"
        setRect(r)
        layoutChanged()
    }
    function zoneAt(px, py) {
        const m = 14, W = area.width, H = area.height
        const l = px < m, r = px > W - m, t = py < m, b = py > H - m
        if (t && l) return "tl"
        if (t && r) return "tr"
        if (b && l) return "bl"
        if (b && r) return "br"
        if (t) return "top"
        if (l) return "left"
        if (r) return "right"
        return ""
    }
    function zoneRect(zone) {
        const W = area.width, H = area.height
        return {
            "left": Qt.rect(0, 0, W / 2, H), "right": Qt.rect(W / 2, 0, W / 2, H), "top": Qt.rect(0, 0, W, H),
            "tl": Qt.rect(0, 0, W / 2, H / 2), "tr": Qt.rect(W / 2, 0, W / 2, H / 2),
            "bl": Qt.rect(0, H / 2, W / 2, H / 2), "br": Qt.rect(W / 2, H / 2, W / 2, H / 2)
        }[zone]
    }

    // свечение активного окна
    Rectangle {
        visible: win.active && Theme.fxAvailable
        anchors.fill: frame
        anchors.margins: -3
        radius: Theme.radiusLarge + 3
        color: "transparent"
        border.width: 3
        border.color: Theme.alpha(win.accent, 0.22)
    }
    GlassPanel {
        id: frame
        anchors.fill: parent
        radius: win.wstate === "max" ? 0 : Theme.radiusLarge
        color: win.contentTransparent ? "transparent" : Theme.windowFill
        source: Theme.bgItem
        borderColor: win.active ? Theme.alpha(win.accent, 0.7) : Theme.line
        borderWidth: 1
    }

    // заголовок
    Rectangle {
        id: bar
        x: 1; y: 1
        width: parent.width - 2
        height: 34
        radius: win.wstate === "max" ? 0 : Theme.radiusLarge - 1
        color: Theme.alpha(Theme.panelFill, win.active ? 0.9 : 0.5)
        Rectangle { y: parent.height - radius; width: parent.width; height: radius; color: parent.color; visible: parent.radius > 0 }
        Rectangle { visible: win.prod; width: parent.width; height: 2; color: Theme.red }
        Row {
            anchors.verticalCenter: parent.verticalCenter
            x: 10
            spacing: 8
            Rectangle {
                width: 20; height: 20; radius: 5
                color: Theme.alpha(win.accent, 0.22); border.color: Theme.alpha(win.accent, 0.6)
                SLabel { anchors.centerIn: parent; text: win.glyph; mono: true; color: win.accent; font.pixelSize: 10; font.bold: true }
            }
            SLabel { text: win.title; font.bold: true; anchors.verticalCenter: parent.verticalCenter; color: win.active ? Theme.text : Theme.textSub }
            SLabel { text: win.subtitle; sub: true; mono: true; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter }
        }
        MouseArea {
            id: drag
            anchors.fill: parent
            anchors.rightMargin: 100
            property point startMouse
            property point startPos
            property bool moved: false
            onPressed: (m) => {
                win.activated()
                startMouse = mapToItem(win.area, m.x, m.y)
                startPos = Qt.point(win.x, win.y)
                moved = false
            }
            onPositionChanged: (m) => {
                if (!pressed) return
                const p = mapToItem(win.area, m.x, m.y)
                if (!moved && Math.abs(p.x - startMouse.x) + Math.abs(p.y - startMouse.y) < 4) return
                if (!moved && (win.wstate === "max" || win.wstate === "snapped")) {
                    // вытаскиваем из развёрнутого: возвращаем прежний размер под курсор
                    const nr = win.normalRect
                    win.wstate = "normal"
                    win.width = nr.width; win.height = nr.height
                    startPos = Qt.point(p.x - nr.width * (startMouse.x / Math.max(1, area.width)), 0)
                    startMouse = p
                }
                moved = true
                win.x = Math.max(-win.width + 80, Math.min(win.area.width - 80, startPos.x + p.x - startMouse.x))
                win.y = Math.max(0, Math.min(win.area.height - 34, startPos.y + p.y - startMouse.y))
                const z = win.zoneAt(p.x, p.y)
                win.snapPreview(z === "" ? null : win.zoneRect(z))
            }
            onReleased: (m) => {
                win.snapPreview(null)
                if (!moved) return
                const p = mapToItem(win.area, m.x, m.y)
                const z = win.zoneAt(p.x, p.y)
                if (z !== "") win.snapTo(z)
                else { win.rememberNormal(); win.layoutChanged() }
            }
            onDoubleClicked: win.toggleMax()
        }
        Row {
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Repeater {
                model: [{ g: "–", a: "min" }, { g: win.wstate === "max" ? "❐" : "□", a: "max" }, { g: "×", a: "close" }]
                delegate: Rectangle {
                    required property var modelData
                    width: 26; height: 22; radius: 5
                    color: bma.containsMouse ? (modelData.a === "close" ? Theme.alpha(Theme.red, 0.4) : Theme.hover) : "transparent"
                    SLabel { anchors.centerIn: parent; text: modelData.g; font.pixelSize: 14; color: Theme.text }
                    MouseArea {
                        id: bma
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: modelData.a === "min" ? win.minimize() : (modelData.a === "max" ? win.toggleMax() : win.closeRequested())
                    }
                    Accessible.role: Accessible.Button
                    Accessible.name: modelData.a === "min" ? qsTr("Minimize") : (modelData.a === "max" ? qsTr("Maximize") : qsTr("Close"))
                }
            }
        }
    }

    Item {
        id: body
        x: 1; y: bar.height + 1
        width: parent.width - 2
        height: parent.height - bar.height - 2
        clip: true
    }

    // клик по содержимому делает окно активным (не мешая детям)
    TapHandler { acceptedButtons: Qt.LeftButton | Qt.RightButton; onPressedChanged: if (pressed) win.activated(); gesturePolicy: TapHandler.ReleaseWithinBounds }

    // ресайз за края
    component Grip: MouseArea {
        property int ex: 0   // -1 левый, 1 правый
        property int ey: 0   // -1 верх, 1 низ
        property rect startR
        property point startM
        enabled: win.wstate === "normal" || win.wstate === "snapped"
        hoverEnabled: true
        cursorShape: ex !== 0 && ey !== 0 ? (ex === ey ? Qt.SizeFDiagCursor : Qt.SizeBDiagCursor) : (ex !== 0 ? Qt.SizeHorCursor : Qt.SizeVerCursor)
        onPressed: (m) => { win.activated(); startR = Qt.rect(win.x, win.y, win.width, win.height); startM = mapToItem(win.area, m.x, m.y) }
        onPositionChanged: (m) => {
            if (!pressed) return
            const p = mapToItem(win.area, m.x, m.y)
            const dx = p.x - startM.x, dy = p.y - startM.y
            let r = Qt.rect(startR.x, startR.y, startR.width, startR.height)
            if (ex === 1) r.width = Math.max(win.minW, startR.width + dx)
            if (ex === -1) { r.width = Math.max(win.minW, startR.width - dx); r.x = startR.x + startR.width - r.width }
            if (ey === 1) r.height = Math.max(win.minH, startR.height + dy)
            if (ey === -1) { r.height = Math.max(win.minH, startR.height - dy); r.y = startR.y + startR.height - r.height }
            win.wstate = "normal"
            win.setRect(r)
        }
        onReleased: { win.rememberNormal(); win.layoutChanged() }
    }
    Grip { ex: -1; x: -3; y: 8; width: 7; height: parent.height - 16 }
    Grip { ex: 1; x: parent.width - 4; y: 8; width: 7; height: parent.height - 16 }
    Grip { ey: -1; x: 8; y: -3; width: parent.width - 16; height: 7 }
    Grip { ey: 1; x: 8; y: parent.height - 4; width: parent.width - 16; height: 7 }
    Grip { ex: -1; ey: -1; x: -3; y: -3; width: 12; height: 12 }
    Grip { ex: 1; ey: -1; x: parent.width - 9; y: -3; width: 12; height: 12 }
    Grip { ex: -1; ey: 1; x: -3; y: parent.height - 9; width: 12; height: 12 }
    Grip { ex: 1; ey: 1; x: parent.width - 9; y: parent.height - 9; width: 12; height: 12 }

    Behavior on opacity { NumberAnimation { duration: 150 } }
    Accessible.role: Accessible.Window
    Accessible.name: title + " " + subtitle
}
