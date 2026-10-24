import QtQuick
import Sherpa

// Пакетный запуск команды на нескольких серверах: подтверждение (слово «выполнить» для >1 сервера),
// события multi.result по каждому серверу, итог multi.done.
Item {
    id: runner
    visible: false
    required property var pane
    property var results: []                    // [{id, name, state: pending|ok|error, exit, elapsed, stdout, stderr, error}]
    property string run: ""
    property int total: 0
    property bool running: false
    property bool done: false
    property var early: []
    readonly property int failed: results.filter(r => r.state === "error" || (r.state === "ok" && r.exit !== 0)).length
    readonly property var groups: {
        const m = {}, order = []
        for (const r of results) {
            if (r.state === "pending") continue
            const key = r.state === "error" ? "E:" + r.error : "O:" + r.exit + ":" + (r.stdout || "").trim()
            if (!m[key]) { m[key] = { count: 0, names: [], sample: r.state === "error" ? r.error : (r.stdout || "").trim(), exit: r.exit, error: r.state === "error" }; order.push(key) }
            m[key].count++; m[key].names.push(r.name)
        }
        return order.map(k => m[k]).sort((a, b) => b.count - a.count)
    }

    function nameOf(id) { const s = Backend.serverById(id); return s && s.name ? s.name : id }

    // opts: {command | snippetId, targets: [ids], title}
    function start(opts) {
        const t = opts.targets
        if (!t || t.length === 0) { pane.errorText = qsTr("Select at least one server"); return }
        const p = { targets: t }
        let method = "multi.run"
        if (opts.snippetId) { method = "snippet.run"; p.id = opts.snippetId } else p.command = opts.command
        if (t.length > 1) p.confirm_text = "выполнить"
        const single = t.length === 1
        pane.dangerous({ title: opts.title || qsTr("Run command"),
                         message: single ? qsTr("The command runs on the server as the SSH user. Output is shown here.") : qsTr("The command runs on %1 servers at once (up to 8 in parallel).").arg(t.length),
                         items: [opts.preview || opts.command || "", ""].concat(t.map(nameOf)).slice(0, 40), confirmText: qsTr("Run"),
                         needName: single && pane.prod && t[0] === pane.serverId, word: single ? "" : "выполнить" },
                       method, p, (d) => {
            run = d.run
            total = d.total
            for (const ev of early) handle(ev)
            early = []
        })
        results = t.map(id => ({ id: id, name: nameOf(id), state: "pending", exit: -1, elapsed: 0, stdout: "", stderr: "", error: "" }))
        running = true; done = false; run = ""; early = []
    }
    function handle(ev) {
        if (ev.event === "multi.result") {
            if (ev.run !== run) return
            const l = results.slice()
            const i = l.findIndex(r => r.id === ev.connection_id)
            if (i < 0) return
            const r = Object.assign({}, l[i])
            if (ev.ok) {
                r.state = "ok"; r.exit = ev.data.exit_status; r.elapsed = ev.data.elapsed_ms
                r.stdout = ev.data.stdout || ""; r.stderr = ev.data.stderr || ""
            } else {
                r.state = "error"
                const e = ev.error || {}
                r.error = Backend.errorText(e.code || "", e.params || {}, e.message || "")
            }
            l[i] = r
            results = l
        } else if (ev.event === "multi.done") {
            if (ev.run !== run) return
            running = false; done = true
        }
    }
    Connections {
        target: runner.pane
        function onEvent(ev) {
            if (ev.event !== "multi.result" && ev.event !== "multi.done") return
            if (runner.run === "" && runner.running) { runner.early.push(ev); return }
            runner.handle(ev)
        }
    }
}
