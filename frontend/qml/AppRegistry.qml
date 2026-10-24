pragma Singleton
import QtQuick

// Реестр утилит рабочего стола: лаунчер, значки на обоях и окна читают его.
// Чтобы подключить окно: поставьте ready: true и source: "ВашеОкно.qml" (корневой элемент принимает
// `required property var desktop` — DesktopState сервера). Остальное (заголовок, привязка, раскладка) делает DesktopPage.
// cap — ключ в desktop.capabilities.utilities (пусто — доступно всегда). single — одно окно на сервер.
// Каждая запись отделена пустой строкой: правки разных исполнителей не конфликтуют.
QtObject {
    readonly property var apps: [
        { id: "terminal", name: qsTr("Terminal"), glyph: ">_", cat: "system", cap: "terminal", ready: true, source: "", w: 760, h: 460, single: false },

        { id: "monitor", name: qsTr("Resource monitor"), glyph: "▤", cat: "monitoring", cap: "monitor", ready: true, source: "MonitorWindow.qml", w: 820, h: 560, single: true },

        { id: "files", name: qsTr("Files"), glyph: "▣", cat: "files", cap: "", ready: true, source: "FilesWindow.qml", w: 980, h: 600, single: false },

        { id: "editor", name: qsTr("Editor"), glyph: "✎", cat: "files", cap: "", ready: true, source: "EditorWindow.qml", w: 860, h: 580, single: false, hidden: true, noRestore: true },

        { id: "tunnels", name: qsTr("Tunnels"), glyph: "⇌", cat: "network", cap: "", ready: true, source: "TunnelsWindow.qml", w: 760, h: 480, single: true },

        { id: "services", name: qsTr("Services"), glyph: "⚙", cat: "system", cap: "services", ready: false, source: "", w: 860, h: 560, single: true },

        { id: "logs", name: qsTr("Logs"), glyph: "≡", cat: "monitoring", cap: "logs", ready: false, source: "", w: 900, h: 560, single: false },

        { id: "network", name: qsTr("Network"), glyph: "⇄", cat: "network", cap: "network", ready: false, source: "", w: 860, h: 560, single: true },

        { id: "nettools", name: qsTr("Network tools"), glyph: "⌁", cat: "network", cap: "nettools", ready: false, source: "", w: 800, h: 520, single: true },

        { id: "firewall", name: qsTr("Firewall"), glyph: "▥", cat: "network", cap: "firewall", ready: false, source: "", w: 860, h: 560, single: true },

        { id: "disks", name: qsTr("Disks"), glyph: "◍", cat: "system", cap: "disks", ready: false, source: "", w: 820, h: 540, single: true },

        { id: "packages", name: qsTr("Packages"), glyph: "▦", cat: "system", cap: "packages", ready: false, source: "", w: 900, h: 560, single: true },

        { id: "system", name: qsTr("System"), glyph: "◈", cat: "system", cap: "system", ready: false, source: "", w: 820, h: 560, single: true },

        { id: "cron", name: qsTr("Scheduler"), glyph: "◷", cat: "system", cap: "cron", ready: false, source: "", w: 860, h: 540, single: true },

        { id: "docker", name: "Docker", glyph: "◫", cat: "dev", cap: "docker", ready: false, source: "", w: 920, h: 580, single: true },

        { id: "databases", name: qsTr("Databases"), glyph: "▤", cat: "dev", cap: "db", ready: false, source: "", w: 900, h: 560, single: true },

        { id: "web", name: qsTr("Web servers"), glyph: "◎", cat: "dev", cap: "web", ready: false, source: "", w: 900, h: 560, single: true },

        { id: "security", name: qsTr("Security"), glyph: "⛨", cat: "system", cap: "security", ready: false, source: "", w: 860, h: 560, single: true },

        { id: "backup", name: qsTr("Backups"), glyph: "⎘", cat: "files", cap: "backup", ready: false, source: "", w: 860, h: 560, single: true },

        { id: "snippets", name: qsTr("Snippets"), glyph: "{}", cat: "dev", cap: "snippet", ready: false, source: "", w: 800, h: 520, single: true },

        { id: "diff", name: qsTr("Compare configs"), glyph: "≠", cat: "dev", cap: "diff", ready: false, source: "", w: 900, h: 560, single: false },

        { id: "notes", name: qsTr("Notes"), glyph: "✎", cat: "dev", cap: "notes", ready: false, source: "", w: 760, h: 520, single: true },

        { id: "audit", name: qsTr("Sherpa journal"), glyph: "☰", cat: "monitoring", cap: "audit", ready: false, source: "", w: 860, h: 520, single: true }
    ]

    function byId(id) {
        for (let i = 0; i < apps.length; ++i)
            if (apps[i].id === id) return apps[i]
        return null
    }
    // Доступно ли окно на этом сервере (окно написано и сервер умеет)
    function enabledFor(app, caps) {
        if (!app.ready) return false
        const u = caps && caps.utilities
        return !app.cap || !u || u[app.cap] !== false
    }
}
