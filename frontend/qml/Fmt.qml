pragma Singleton
import QtQuick

// Форматирование для окон утилит.
QtObject {
    function bytes(b) {
        if (b === undefined || b === null) return "—"
        const u = [qsTr("B"), qsTr("KB"), qsTr("MB"), qsTr("GB"), qsTr("TB"), qsTr("PB")]
        let i = 0, v = Number(b)
        while (v >= 1024 && i < u.length - 1) { v /= 1024; i++ }
        return (i === 0 || v >= 100 ? v.toFixed(0) : v.toFixed(1)) + " " + u[i]
    }
    function pad(n) { return n < 10 ? "0" + n : String(n) }
    function time(usec) {
        if (usec === undefined || usec === null) return ""
        const d = new Date(Number(usec) / 1000)
        return pad(d.getHours()) + ":" + pad(d.getMinutes()) + ":" + pad(d.getSeconds())
    }
    function dateTime(usec) {
        if (usec === undefined || usec === null || usec === "" || Number(usec) === 0) return "—"
        const d = new Date(Number(usec) / 1000)
        return pad(d.getDate()) + "." + pad(d.getMonth() + 1) + " " + pad(d.getHours()) + ":" + pad(d.getMinutes()) + ":" + pad(d.getSeconds())
    }
    function pct(v) { return v === undefined || v === null ? "—" : Number(v).toFixed(0) + "%" }
    function usageTone(p) { return p === null || p === undefined ? Theme.gray : (p >= 90 ? Theme.red : (p >= 75 ? Theme.amber : Theme.green)) }
    // journald priority 0..7 → цвет
    function prioTone(p) {
        if (p === null || p === undefined) return Theme.gray
        return p <= 3 ? Theme.red : (p === 4 ? Theme.amber : (p === 5 ? Theme.cyan : (p === 6 ? Theme.textSub : Theme.gray)))
    }
    function prioName(p) { return [qsTr("emerg"), qsTr("alert"), qsTr("crit"), qsTr("err"), qsTr("warning"), qsTr("notice"), qsTr("info"), qsTr("debug")][p] || "" }
    function text(v) { return v === null || v === undefined ? "" : (typeof v === "string" ? v : (Array.isArray(v) ? "[" + v.length + " bytes]" : String(v))) }
}
