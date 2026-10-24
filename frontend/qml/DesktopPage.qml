import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Sherpa

// Рабочий стол сервера: обои, иконки, окна (свой оконный менеджер), панель задач, состояния соединения.
Item {
    id: page
    required property var desk                 // DesktopState
    property bool current: false
    property bool startWithTerminal: false
    signal homeRequested()
    signal switchServer(string id, string name)
    signal closeDesktopRequested()
    signal editConnectionRequested(string id)

    readonly property var info: Backend.serverById(desk.connectionId)
    readonly property string groupName: (info.group_name || "").toLowerCase()
    readonly property bool prod: groupName.indexOf("prod") >= 0
    readonly property color tint: prod ? Theme.red : (groupName.indexOf("stag") >= 0 || groupName.indexOf("stage") >= 0 ? Theme.amber : Theme.accent)
    readonly property bool working: desk.state === "ready"
    property var wins: []                      // [{id, kind, win}]
    property var windowList: []
    property int nextWid: 1
    property int zCounter: 10
    property bool restored: false
    property alias area: area
    property alias widgets: widgets

    // ------------------------------------------------------------ окна
    Component { id: winComp; SWindow { area: page.area } }
    Component { id: termComp; TerminalWindow { } }

    function activeWin() { for (const w of wins) if (w.win.active) return w.win; return null }
    function refreshList() {
        windowList = wins.map(w => ({ id: w.id, title: w.win.title, glyph: w.win.glyph, active: w.win.active, minimized: w.win.minimized }))
    }
    function activate(win) {
        for (const w of wins) w.win.active = (w.win === win)
        if (win) { if (win.minimized) win.unminimize(); win.z = ++zCounter }
        refreshList()
    }
    function findKind(kind) { for (const w of wins) if (w.kind === kind) return w; return null }
    function cascade() { const n = wins.length; return { x: 40 + (n % 8) * 28, y: 30 + (n % 8) * 28 } }

    function openWindow(kind, rect, wstate, session) {
        const app = AppRegistry.byId(kind)
        if (!app || !AppRegistry.enabledFor(app, desk.capabilities)) return null
        if (app.single) {
            const ex = findKind(kind)
            if (ex) { activate(ex.win); return ex.win }
        }
        const c = cascade()
        const r = rect || { x: c.x, y: c.y, w: app.w, h: app.h }
        const isTerm = kind === "terminal"
        const id = "w" + (nextWid++)
        const win = winComp.createObject(area, {
            x: Math.max(0, Math.min(r.x, Math.max(0, area.width - 200))), y: Math.max(0, Math.min(r.y, Math.max(0, area.height - 80))),
            width: r.w, height: r.h,
            glyph: app.glyph,
            subtitle: desk.name, prod: page.prod,
            contentTransparent: isTerm,
            accent: isTerm ? Theme.cyan : Theme.green
        })
        win.normalRect = Qt.rect(win.x, win.y, win.width, win.height)
        if (!isTerm) win.title = Qt.binding(() => AppRegistry.byId(kind).name)
        let content
        if (isTerm) {
            const s = session || desk.newTerminal()
            content = termComp.createObject(win.contentItem, { session: s, desktop: desk })
            content.anchors.fill = win.contentItem
            win.title = Qt.binding(() => content.currentTitle)
            content.editConnection.connect(() => page.editConnectionRequested(desk.connectionId))
            content.allClosed.connect(() => page.forceClose(id))
        } else {
            const comp = Qt.createComponent(app.source)
            if (comp.status !== Component.Ready) { console.warn("cannot load", app.source, comp.errorString()); win.destroy(); return null }
            content = comp.createObject(win.contentItem, { desktop: desk })
            if (!content) { win.destroy(); return null }
            content.anchors.fill = win.contentItem
            // окна утилит могут просить открыть другое окно (например, «Терминал здесь»)
            if (content.openApp) content.openApp.connect((id2, args) => page.openApp(id2, args))
        }
        const entry = { id: id, kind: kind, win: win, content: content, session: kind === "terminal" ? content : null }
        wins = wins.concat([entry])
        win.activated.connect(() => page.activate(win))
        win.layoutChanged.connect(() => { page.refreshList(); saveTimer.restart() })
        win.closeRequested.connect(() => page.closeWindow(id))
        win.snapPreview.connect((r) => { snap.rect = r })
        win.titleChanged.connect(() => page.refreshList())
        if (wstate === "max") win.maximize(); else if (wstate === "min") win.minimize()
        activate(win)
        saveTimer.restart()
        if (kind === "terminal" && wstate !== "min") content.focusCurrent()
        return win
    }
    function closeWindow(id) {
        const e = wins.find(w => w.id === id)
        if (!e) return
        if (e.kind === "terminal" && e.session && e.session.active) { closeConfirm.targetId = id; closeConfirm.open(); return }
        // окно может задержать закрытие (несохранённые правки): beforeClose(proceed) → true, если закрывать можно сразу
        if (e.content && e.content.beforeClose && !e.content.beforeClose(() => page.forceClose(id))) return
        forceClose(id)
    }
    function forceClose(id) {
        const e = wins.find(w => w.id === id)
        if (!e) return
        if (e.session) e.session.close()
        wins = wins.filter(w => w.id !== id)
        e.win.destroy()
        const next = wins.length ? wins[wins.length - 1].win : null
        activate(next && !next.minimized ? next : null)
        saveTimer.restart()
    }
    function onTaskbarClick(id) {
        const e = wins.find(w => w.id === id)
        if (!e) return
        if (e.win.active && !e.win.minimized) e.win.minimize()
        else activate(e.win)
    }
    function showDesktop() { for (const w of wins) w.win.minimize(); activate(null) }
    function newTerminal() { return openWindow("terminal") }
    // Открыть утилиту по id с необязательными аргументами (окно сможет прочитать `startArgs`)
    function openApp(id, args) {
        const w = openWindow(id)
        if (!w || !args) return w
        const e = wins.find(x => x.win === w)
        if (id === "terminal" && args.cwd && e && e.content) {
            const q = "'" + String(args.cwd).replace(/'/g, "'\\''") + "'"
            const s = e.content.currentSession
            const send = () => { if (s.state === "connected") { s.stateChanged.disconnect(send); Qt.callLater(() => s.sendInput("cd " + q + " && clear\r")) } }
            s.stateChanged.connect(send)
        } else if (e && e.content && e.content.startWith) e.content.startWith(args)
        return w
    }

    function layoutData() {
        return { widgets: widgets.config, windows: wins.map(w => ({ kind: w.kind, x: w.win.normalRect.x, y: w.win.normalRect.y, w: w.win.normalRect.width, h: w.win.normalRect.height,
                                            state: w.win.wstate === "snapped" ? "normal" : w.win.wstate })) }
    }
    Timer { id: saveTimer; interval: 700; onTriggered: if (page.restored) page.desk.saveLayout(page.layoutData()) }
    function restoreLayout() {
        if (restored || !desk.layoutReady) return
        restored = true
        widgets.config = desk.layout.widgets || {}
        const l = desk.layout.windows || []
        for (const w of l) {
            if (AppRegistry.byId(w.kind) && !AppRegistry.byId(w.kind).noRestore)
                openWindow(w.kind, { x: w.x, y: w.y, w: w.w, h: w.h }, w.state)
        }
        if (startWithTerminal && !findKind("terminal")) newTerminal()
    }
    Connections { target: page.desk; function onLayoutLoaded() { if (page.working) page.restoreLayout() } }
    onWorkingChanged: if (working) restoreLayout()
    Component.onCompleted: if (working) restoreLayout()

    // ------------------------------------------------------------ обои
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(page.tint.r * 0.25, page.tint.g * 0.25, page.tint.b * 0.25, 1) }
            GradientStop { position: 1; color: Theme.light ? "#cfd9ee" : "#07101c" }
        }
    }
    Rectangle { x: -100; y: parent.height - 360; width: 600; height: 500; radius: 250; color: Theme.alpha(page.tint, 0.18) }
    Rectangle { x: parent.width - 460; y: -120; width: 560; height: 520; radius: 260; color: Theme.alpha(Theme.purple, 0.14) }
    SLabel {
        anchors.centerIn: parent
        text: page.desk.name
        font.pixelSize: 84; font.bold: true
        color: Theme.alpha(Theme.text, 0.06)
        elide: Text.ElideNone
    }

    // иконки рабочего стола
    Column {
        x: 22; y: 22; spacing: 16
        visible: page.working
        Repeater {
            model: [{ k: "terminal", t: qsTr("Terminal"), g: ">_" }, { k: "monitor", t: qsTr("Monitor"), g: "▤" }]
            delegate: Item {
                required property var modelData
                width: 84; height: 80
                Rectangle { anchors.fill: parent; radius: 8; color: ima.containsMouse ? Theme.hover : "transparent" }
                Column {
                    anchors.centerIn: parent; spacing: 6
                    Rectangle { width: 44; height: 44; radius: 11; anchors.horizontalCenter: parent.horizontalCenter
                                color: Theme.alpha(Theme.cyan, 0.16); border.color: Theme.alpha(Theme.cyan, 0.5)
                                SLabel { anchors.centerIn: parent; text: modelData.g; mono: true; color: Theme.cyan; font.pixelSize: 18; font.bold: true } }
                    SLabel { text: modelData.t; anchors.horizontalCenter: parent.horizontalCenter; font.pixelSize: Theme.sizeSmall }
                }
                MouseArea { id: ima; anchors.fill: parent; hoverEnabled: true; onDoubleClicked: page.openWindow(modelData.k) }
                Accessible.role: Accessible.Button
                Accessible.name: modelData.t
            }
        }
    }
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.RightButton | Qt.LeftButton
        onPressed: (m) => { if (m.button === Qt.RightButton && page.working) { ctx.popup() } else page.activate(null) }
    }
    DesktopWidgets {
        id: widgets
        desktop: page.desk
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
        anchors.bottom: taskbar.top
        visible: page.working
        onChanged: saveTimer.restart()
    }
    SMenu {
        id: ctx
        MenuItem { text: qsTr("New terminal"); onTriggered: page.newTerminal() }
        MenuItem { text: qsTr("Resource monitor"); onTriggered: page.openWindow("monitor") }
        MenuItem { text: qsTr("Show desktop"); onTriggered: page.showDesktop() }
        MenuSeparator { }
        MenuItem { text: (widgets.isOn("cpu") ? "✓  " : "     ") + qsTr("Widget: CPU"); onTriggered: widgets.setOn("cpu", !widgets.isOn("cpu")) }
        MenuItem { text: (widgets.isOn("server") ? "✓  " : "     ") + qsTr("Widget: Server"); onTriggered: widgets.setOn("server", !widgets.isOn("server")) }
        MenuItem { text: (widgets.isOn("disks") ? "✓  " : "     ") + qsTr("Widget: Disks"); onTriggered: widgets.setOn("disks", !widgets.isOn("disks")) }
        MenuItem { text: (widgets.isOn("alerts") ? "✓  " : "     ") + qsTr("Widget: Alerts"); onTriggered: widgets.setOn("alerts", !widgets.isOn("alerts")) }
    }

    // ------------------------------------------------------------ область окон
    Item {
        id: area
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
        anchors.bottom: taskbar.top
        clip: false
        visible: page.working || page.desk.state === "lost"
        enabled: page.working
        // привязка: предпросмотр
        Rectangle {
            id: snap
            property var rect: null
            visible: rect !== null
            x: rect ? rect.x : 0; y: rect ? rect.y : 0; width: rect ? rect.width : 0; height: rect ? rect.height : 0
            z: 5000
            radius: Theme.radiusLarge
            color: Theme.alpha(Theme.accent, 0.18)
            border.color: Theme.alpha(Theme.cyan, 0.7)
            border.width: 2
        }
        onWidthChanged: { for (const w of page.wins) { if (w.win.wstate === "max") w.win.setRect(Qt.rect(0, 0, width, height)) } }
        onHeightChanged: { for (const w of page.wins) { if (w.win.wstate === "max") w.win.setRect(Qt.rect(0, 0, width, height)) } }
    }
    // обрыв: окна тускнеют
    Connections {
        target: page.desk
        function onStateChanged() { for (const w of page.wins) { w.win.dimmed = (page.desk.state === "lost") } }
    }

    Taskbar {
        id: taskbar
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
        desk: page.desk
        windowList: page.windowList
        launcherOpen: launcher.opened
        enabled: page.working || page.desk.state === "lost"
        onLauncherClicked: launcher.opened ? launcher.close() : launcher.open()
        onServersClicked: dock.opened ? dock.close() : dock.open()
        onWindowClicked: (id) => page.onTaskbarClick(id)
        onMonitorClicked: page.openWindow("monitor")
    }

    Launcher {
        id: launcher
        objectName: "launcher"
        desk: page.desk
        x: 8
        y: page.height - taskbar.height - height - 6
        onLaunch: (id) => page.openWindow(id)
        onReconnectRequested: page.desk.retryNow()
        onCloseDesktopRequested: page.closeDesktopRequested()
    }
    ServersDock {
        id: dock
        x: 90
        y: page.height - taskbar.height - height - 6
        onServerChosen: (id, name) => page.switchServer(id, name)
        onHomeRequested: page.homeRequested()
    }

    // ------------------------------------------------------------ состояния соединения
    Rectangle {
        visible: page.desk.state === "connecting" || page.desk.state === "error"
        anchors.fill: area
        color: "transparent"
        Rectangle {
            anchors.centerIn: parent
            width: 420; height: cardCol.implicitHeight + 40
            radius: Theme.radiusLarge
            color: Theme.dialogFill
            border.color: Theme.alpha(page.desk.state === "error" ? Theme.red : Theme.accent, 0.6)
            ColumnLayout {
                id: cardCol
                anchors.fill: parent; anchors.margins: 20; spacing: 12
                RowLayout {
                    spacing: 10
                    SSpinner { visible: page.desk.state === "connecting"; width: 20; height: 20 }
                    SLabel { Layout.fillWidth: true; font.bold: true; font.pixelSize: 15
                             text: page.desk.state === "connecting" ? qsTr("Connecting to %1…").arg(page.desk.name) : qsTr("Cannot connect to %1").arg(page.desk.name)
                             color: page.desk.state === "error" ? Theme.red : Theme.text }
                }
                SLabel { visible: page.desk.state === "error"; Layout.fillWidth: true; wrapMode: Text.Wrap; elide: Text.ElideNone; text: page.desk.errorText; sub: true }
                SLabel { visible: page.desk.state === "error"; mono: true; sub: true; font.pixelSize: 11; text: page.desk.errorCode }
                RowLayout {
                    visible: page.desk.state === "error"
                    spacing: 8
                    SButton { text: qsTr("Retry"); kind: "primary"; onClicked: page.desk.retryNow() }
                    SButton { text: qsTr("Edit connection"); onClicked: page.editConnectionRequested(page.desk.connectionId) }
                    SButton { text: qsTr("Home"); kind: "ghost"; onClicked: page.homeRequested() }
                }
            }
        }
    }
    // баннер обрыва
    SBanner {
        visible: page.desk.state === "lost"
        anchors.top: parent.top; anchors.left: parent.left; anchors.right: parent.right; anchors.margins: 10
        tone: Theme.amber
        text: qsTr("Connection lost") + (page.desk.errorText !== "" ? ": " + page.desk.errorText : "")
              + (page.desk.retryIn > 0 ? " · " + qsTr("reconnecting in %1 s").arg(page.desk.retryIn) : "")
        actionText: qsTr("Retry now")
        onAction: page.desk.retryNow()
        onDismissed: {}
        z: 6000
    }

    PromptDialog { source: page.desk }

    ConfirmDialog {
        id: closeConfirm
        property string targetId
        title: qsTr("Close terminal?")
        message: qsTr("The session is still active. Closing the window ends it.")
        confirmText: qsTr("Close")
        danger: true
        onConfirmed: page.forceClose(targetId)
    }

    // ------------------------------------------------------------ горячие клавиши (только на активной вкладке)
    Shortcut { enabled: page.current; sequence: "Ctrl+Alt+T"; onActivated: page.newTerminal() }
    Shortcut { enabled: page.current; sequence: "Ctrl+Shift+Esc"; onActivated: page.openWindow("monitor") }
    Shortcut { enabled: page.current; sequences: ["Ctrl+Space", "Meta"]; onActivated: launcher.opened ? launcher.close() : launcher.open() }
    Shortcut { enabled: page.current; sequence: "Ctrl+D"; onActivated: page.showDesktop() }
    Shortcut { enabled: page.current; sequence: "Ctrl+M"; onActivated: { const a = page.activeWin(); if (a) a.minimize() } }
    Shortcut { enabled: page.current; sequence: "Ctrl+Up"; onActivated: { const a = page.activeWin(); if (a) a.toggleMax() } }
    Shortcut { enabled: page.current; sequence: "Ctrl+Shift+W"; onActivated: { const a = page.wins.find(w => w.win.active); if (a) page.closeWindow(a.id) } }
    Shortcut { enabled: page.current; sequence: "Ctrl+Alt+Left"; onActivated: { const a = page.activeWin(); if (a) a.snapTo("left") } }
    Shortcut { enabled: page.current; sequence: "Ctrl+Alt+Right"; onActivated: { const a = page.activeWin(); if (a) a.snapTo("right") } }
    Shortcut { enabled: page.current; sequence: "Ctrl+Alt+Up"; onActivated: { const a = page.activeWin(); if (a) a.snapTo("top") } }
    Shortcut { enabled: page.current; sequence: "Ctrl+Alt+Down"; onActivated: { const a = page.activeWin(); if (a) a.restore() } }
    Shortcut { enabled: page.current; sequence: "Alt+Tab"; onActivated: { if (page.wins.length > 1) page.activate(page.wins[0].win === page.activeWin() ? page.wins[page.wins.length - 1].win : page.wins[0].win) } }
    Shortcut { enabled: page.current; sequence: "Ctrl+Alt+R"; onActivated: page.desk.setReadonly(!page.desk.readOnlyMode) }
}
