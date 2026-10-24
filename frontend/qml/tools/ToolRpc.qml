import QtQuick
import Sherpa

// Вызовы демона для окна утилиты: учёт занятости, событий утилит и priv.required (без интерфейса).
Item {
    id: root
    required property var desktop               // DesktopState (так окна подключает AppRegistry)
    readonly property var desk: desktop
    readonly property string serverId: desk.connectionId
    readonly property string serverName: desk.name
    property string errorText: ""
    property int busy: 0
    signal event(var ev)                        // событие утилиты (nettools.output, docker.logs, multi.result …)
    signal privRequired(var entry)              // нужен пароль sudo; entry — повторяемый вызов

    property var _pending: ({})

    // Низкоуровневый вызов: params уходят как есть (connection_id — на совести вызывающего).
    function call(method, params, ok, fail) {
        busy++
        const t = desk.callRaw(method, params || {})
        _pending[t] = { method: method, params: params || {}, ok: ok, fail: fail }
        return t
    }
    // Вызов для сервера этого окна.
    function req(method, params, ok, fail) {
        return call(method, Object.assign({ connection_id: serverId }, params || {}), ok, fail)
    }
    Connections {
        target: root.desk
        function onRequestDone(token, ok, data, err) {
            const e = root._pending[token]
            if (!e) return
            delete root._pending[token]
            root.busy = Math.max(0, root.busy - 1)
            if (ok) {
                if (e.ok) e.ok(data)
            } else if (data && data.code === "priv.required") {
                root.privRequired(e)
            } else if (e.fail) {
                e.fail(err, data)
            } else {
                root.errorText = err
            }
        }
        function onUtilityEvent(ev) { root.event(ev) }
    }
}
