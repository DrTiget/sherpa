pragma Singleton
import QtQuick

// Единая точка для цветов, шрифтов, радиусов и отступов (SCREENS.md §6).
// Тёмная тема — основная; светлая и акцент переключаются здесь, а не в контролах.
QtObject {
    id: theme

    // Выставляются из Main.qml
    property Item bgItem: null        // фон окна (градиент) — источник blur для боковых панелей
    property Item contentItem: null   // весь интерфейс — источник blur для диалогов
    property bool fxAvailable: true   // false при программном рендере: без MultiEffect
    property string mode: "dark"      // "dark" | "light" (уже с учётом «системной»)
    property bool translucent: true
    readonly property bool light: mode === "light"
    readonly property bool blurOn: translucent && fxAvailable

    // --- палитра
    readonly property color accent: "#367bf0"
    readonly property color cyan: light ? "#007c99" : "#00e5ff"
    readonly property color green: light ? "#0e9a4a" : "#3dff8a"
    readonly property color amber: light ? "#a86f00" : "#ffc53d"
    readonly property color red: light ? "#d61f45" : "#ff4d6d"
    readonly property color purple: light ? "#7b4fd6" : "#b388ff"
    readonly property color gray: "#6b7488"

    readonly property color text: light ? "#1b2030" : "#e6eaf2"
    readonly property color textSub: light ? "#5b6477" : "#8b93a7"
    readonly property color line: light ? Qt.rgba(0, 0, 0, 0.12) : Qt.rgba(1, 1, 1, 0.12)
    readonly property color lineStrong: light ? Qt.rgba(0, 0, 0, 0.2) : Qt.rgba(1, 1, 1, 0.2)
    readonly property color card: light ? Qt.rgba(1, 1, 1, 0.55) : Qt.rgba(1, 1, 1, 0.05)
    readonly property color hover: light ? Qt.rgba(0, 0, 0, 0.06) : Qt.rgba(1, 1, 1, 0.09)
    readonly property color selected: Qt.rgba(54 / 255, 123 / 255, 240 / 255, light ? 0.18 : 0.22)
    readonly property color selectedBorder: Qt.rgba(54 / 255, 123 / 255, 240 / 255, 0.6)

    // фон окна и панелей: при выключенной прозрачности — сплошные цвета
    readonly property color windowFill: light ? Qt.rgba(244 / 255, 246 / 255, 250 / 255, blurOn ? 0.78 : 1)
                                              : Qt.rgba(18 / 255, 21 / 255, 30 / 255, blurOn ? 0.74 : 1)
    readonly property color panelFill: light ? Qt.rgba(1, 1, 1, blurOn ? 0.55 : 1)
                                             : Qt.rgba(5 / 255, 7 / 255, 12 / 255, blurOn ? 0.5 : 1)
    readonly property color dialogFill: light ? Qt.rgba(248 / 255, 250 / 255, 253 / 255, blurOn ? 0.86 : 1)
                                              : Qt.rgba(18 / 255, 21 / 255, 30 / 255, blurOn ? 0.84 : 1)
    readonly property color inputFill: light ? Qt.rgba(1, 1, 1, 0.8) : Qt.rgba(5 / 255, 7 / 255, 12 / 255, 0.55)
    readonly property color scrim: Qt.rgba(0, 0, 0, light ? 0.3 : 0.5)

    // --- шрифты
    readonly property string fontUi: "Inter"
    readonly property string fontMono: "JetBrains Mono"
    readonly property int sizeUi: 13
    readonly property int sizeSmall: 12
    readonly property int sizeMono: 12
    readonly property int sizeTitle: 20

    // --- размеры (SCREENS.md §6.4)
    readonly property int rowHeight: 48
    readonly property int fieldHeight: 34
    readonly property int radius: 6
    readonly property int radiusLarge: 10
    readonly property int gapS: 8
    readonly property int gapM: 12
    readonly property int gapL: 22
    readonly property int sidebarWidth: 220
    readonly property int listWidth: 480

    function stateColor(state) {
        switch (state) {
        case "online": return green
        case "offline": return amber
        case "auth_failed": return red
        case "error": return red
        default: return gray
        }
    }

    function stateLabel(state) {
        switch (state) {
        case "online": return qsTr("online")
        case "offline": return qsTr("offline")
        case "auth_failed": return qsTr("auth failed")
        case "error": return qsTr("error")
        default: return qsTr("not checked")
        }
    }

    // «online · 38 ms»
    function stateText(state, latencyMs) {
        var t = stateLabel(state)
        if (state === "online" && latencyMs >= 0)
            t += " · " + latencyMs + " ms"
        return t
    }

    function alpha(c, a) { return Qt.rgba(c.r, c.g, c.b, a) }
}
