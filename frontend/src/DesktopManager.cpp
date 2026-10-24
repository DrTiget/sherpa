#include "DesktopManager.h"

#include <QClipboard>
#include <QFile>
#include <QGuiApplication>

#include "ErrorText.h"

#include <QDir>
#include <QJSEngine>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QStandardPaths>
#include <memory>

// ---------------------------------------------------------------- DesktopState

DesktopState::DesktopState(DesktopManager *mgr, DaemonClient *daemon, const QString &id, const QString &name)
    : QObject(mgr), m_mgr(mgr), m_daemon(daemon), m_id(id), m_name(name)
{
    m_retryTimer.setInterval(1000);
    connect(&m_retryTimer, &QTimer::timeout, this, [this]() {
        if (--m_retryIn <= 0) {
            m_retryTimer.stop();
            m_retryIn = 0;
            emit retryChanged();
            open();
        } else {
            emit retryChanged();
        }
    });
}

void DesktopState::setName(const QString &n)
{
    if (n == m_name)
        return;
    m_name = n;
    emit nameChanged();
}

void DesktopState::setState(const QString &s)
{
    if (m_state == s)
        return;
    m_state = s;
    emit stateChanged();
}

void DesktopState::open()
{
    if (m_opening || m_closing)
        return;
    if (!m_daemon->isReady()) {
        markLost("client.daemon_unavailable", {});
        return;
    }
    m_opening = true;
    m_retryTimer.stop();
    m_retryIn = 0;
    emit retryChanged();
    if (m_state != "lost" || m_caps.isEmpty())
        setState("connecting");
    m_errorText.clear();
    m_errorCode.clear();
    m_daemon->request("desktop.open", {{"connection_id", m_id}}, [this](const DaemonReply &r) {
        m_opening = false;
        if (m_closing)
            return;
        if (!r.ok) {
            if (r.error.code == "client.daemon_unavailable")
                markLost(r.error.code, {});
            else
                markLost(r.error.code, ErrorText::text(r.error.code, r.error.params, r.error.message));
            return;
        }
        m_prompt.clear();
        emit promptChanged();
        m_caps = r.data.toMap().value("capabilities").toMap();
        emit capabilitiesChanged();
        m_retryAttempt = 0;
        setState("ready");
        if (!m_layoutReady)
            loadLayout();
        subscribeMonitor();
        m_daemon->request("priv.sudo.check", {{"connection_id", m_id}}, [this](const DaemonReply &p) {
            if (p.ok) {
                const bool ro = p.data.toMap().value("readonly").toBool();
                if (ro != m_readonly) {
                    m_readonly = ro;
                    emit readonlyChanged();
                }
            }
        });
    });
}

void DesktopState::subscribeMonitor()
{
    if (!m_caps.value("utilities").toMap().value("monitor", true).toBool())
        return;
    m_daemon->request("monitor.subscribe", {{"connection_id", m_id}, {"interval_ms", 2000}, {"processes", 60}, {"sort", "cpu"}},
                      [this](const DaemonReply &r) {
                          if (r.ok)
                              m_subscription = r.data.toMap().value("subscription").toString();
                      });
}

void DesktopState::markLost(const QString &code, const QString &text)
{
    m_errorCode = code;
    m_errorText = text;
    m_subscription.clear();
    // первое подключение, не удалось (ошибка входа / host key отклонён) — «error»: повтор вручную;
    // уже работавший стол — «lost»: повтор автоматический
    const bool wasWorking = !m_caps.isEmpty();
    setState(wasWorking ? "lost" : "error");
    if (wasWorking || code == "client.daemon_unavailable" || code.startsWith("net."))
        startRetry();
}

void DesktopState::startRetry()
{
    m_retryAttempt = qMin(m_retryAttempt + 1, 6);
    m_retryIn = qMin(30, 1 << m_retryAttempt);
    emit retryChanged();
    m_retryTimer.start();
}

void DesktopState::retryNow()
{
    m_retryTimer.stop();
    m_retryIn = 0;
    emit retryChanged();
    open();
}

void DesktopState::close()
{
    m_closing = true;
    m_retryTimer.stop();
    if (m_daemon->isReady()) {
        if (!m_subscription.isEmpty())
            m_daemon->request("monitor.unsubscribe", {{"subscription", m_subscription}});
        m_daemon->request("desktop.close", {{"connection_id", m_id}});
    }
}

void DesktopState::shutdown() { close(); }

void DesktopState::answerHostKey(const QString &decision)
{
    if (m_prompt.isEmpty())
        return;
    const QVariantMap p{{"prompt_id", m_prompt.value("prompt_id")}, {"decision", decision}};
    m_prompt.clear();
    emit promptChanged();
    m_daemon->request("session.hostkey_answer", p);
}

void DesktopState::answerAuth(const QString &response, bool save)
{
    if (m_prompt.isEmpty())
        return;
    const QVariantMap p{{"prompt_id", m_prompt.value("prompt_id")}, {"responses", QStringList{response}}, {"save", save}};
    m_prompt.clear();
    emit promptChanged();
    m_daemon->request("session.auth_answer", p);
}

void DesktopState::cancelAuth()
{
    if (m_prompt.isEmpty())
        return;
    const QVariantMap p{{"prompt_id", m_prompt.value("prompt_id")}, {"cancel", true}};
    m_prompt.clear();
    emit promptChanged();
    m_daemon->request("session.auth_answer", p);
}

void DesktopState::setReadonly(bool on)
{
    m_daemon->request("priv.readonly.set", {{"connection_id", m_id}, {"readonly", on}}, [this](const DaemonReply &r) {
        if (r.ok) {
            m_readonly = r.data.toMap().value("readonly").toBool();
            emit readonlyChanged();
        }
    });
}

TerminalSession *DesktopState::newTerminal()
{
    auto *s = new TerminalSession(m_daemon, m_id, this);
    m_mgr->registerSession(s);
    emit terminalCreated(s);
    return s;
}

int DesktopState::call(const QString &method, const QVariantMap &params)
{
    const int token = m_nextToken++;
    QVariantMap p = params;
    p.insert("connection_id", m_id);
    m_daemon->request(method, p, [this, token](const DaemonReply &r) {
        if (!r.ok)
            emit requestFailed(token, r.error.code, r.error.params);
        emit requestDone(token, r.ok, r.data,
                         r.ok ? QString() : ErrorText::text(r.error.code, r.error.params, r.error.message));
    });
    return token;
}

static void callJs(QJSEngine *eng, QJSValue cb, const DaemonReply &r)
{
    if (!cb.isCallable() || !eng)
        return;
    QJSValueList args;
    args << r.ok << eng->toScriptValue(r.data) << r.error.code
         << (r.ok ? QString() : ErrorText::text(r.error.code, r.error.params, r.error.message))
         << eng->toScriptValue(r.error.params);
    cb.call(args);
}

void DesktopState::rpc(const QString &method, const QVariantMap &params, const QJSValue &cb)
{
    QVariantMap p = params;
    p.insert("connection_id", m_id);
    rpcRaw(method, p, cb);
}

void DesktopState::rpcRaw(const QString &method, const QVariantMap &params, const QJSValue &cb)
{
    QJSEngine *eng = qjsEngine(this);
    m_daemon->request(method, params, [eng, cb](const DaemonReply &r) { callJs(eng, cb, r); });
}

QString DesktopState::decodeBase64(const QString &b64) const
{
    return QString::fromUtf8(QByteArray::fromBase64(b64.toLatin1()));
}

void DesktopState::copyText(const QString &text) const
{
    if (QClipboard *c = QGuiApplication::clipboard())
        c->setText(text);
}

bool DesktopState::saveText(const QUrl &url, const QString &text) const
{
    QFile f(url.isLocalFile() ? url.toLocalFile() : url.toString());
    if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate))
        return false;
    f.write(text.toUtf8());
    return true;
}

int DesktopState::callRaw(const QString &method, const QVariantMap &params)
{
    const int token = m_nextToken++;
    m_daemon->request(method, params, [this, token](const DaemonReply &r) {
        if (!r.ok)
            emit requestFailed(token, r.error.code, r.error.params);
        const QVariant payload = r.ok ? r.data : QVariant(QVariantMap{{"code", r.error.code}, {"params", r.error.params}});
        emit requestDone(token, r.ok, payload, r.ok ? QString() : ErrorText::text(r.error.code, r.error.params, r.error.message));
    });
    return token;
}

void DesktopState::handleEvent(const QVariantMap &ev)
{
    emit eventReceived(ev);
    const QString name = ev.value("event").toString();
    emit utilityEvent(ev);
    if (name == "hostkey_prompt" || name == "auth_prompt") {
        m_prompt = ev;
        m_prompt.insert("type", name == "hostkey_prompt" ? "hostkey" : "auth");
        emit promptChanged();
    } else if (name == "monitor") {
        m_monitor = ev.value("data").toMap();
        auto push = [](QVariantList &l, const QVariant &v) {
            l.append(v.isNull() ? 0.0 : v);
            if (l.size() > 90)
                l.removeFirst();
        };
        push(m_cpu, m_monitor.value("cpu").toMap().value("usage_pct"));
        push(m_mem, m_monitor.value("memory").toMap().value("usage_pct"));
        push(m_load, m_monitor.value("load").toMap().value("1m"));
        emit monitorChanged();
    } else if (name == "error" && ev.contains("subscription")) {
        const QVariantMap e = ev.value("error").toMap();
        if (ev.value("fatal").toBool() && ev.value("subscription").toString() == m_subscription) {
            markLost(e.value("code").toString(),
                     ErrorText::text(e.value("code").toString(), e.value("params").toMap(), e.value("message").toString()));
        }
    }
}

void DesktopState::onDaemonLost()
{
    m_subscription.clear();
    m_opening = false;
    if (m_state == "ready" || m_state == "connecting")
        markLost("client.daemon_unavailable", {});
}

void DesktopState::onDaemonBack()
{
    if (m_state == "lost" || m_state == "connecting")
        retryNow();
}

QString DesktopState::layoutPath() const
{
    const QString dir = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/desktops";
    return dir + "/" + m_id + ".json";
}

void DesktopState::loadLayout()
{
    auto finish = [this](const QVariantMap &l) {
        m_layout = l;
        m_layoutReady = true;
        emit layoutLoaded();
    };
    if (m_daemon->hasMethod("desktop.state.get") && m_daemon->isReady()) {
        m_daemon->request("desktop.state.get", {{"connection_id", m_id}}, [this, finish](const DaemonReply &r) {
            if (r.ok)
                finish(r.data.toMap().value("state").toMap());
            else
                finish({});
        });
        return;
    }
    // заглушка: пока ядро не хранит раскладку, держим её локально
    QFile f(layoutPath());
    QVariantMap l;
    if (f.open(QIODevice::ReadOnly)) {
        const QJsonDocument d = QJsonDocument::fromJson(f.readAll());
        if (d.isObject())
            l = d.object().toVariantMap();
    }
    finish(l);
}

void DesktopState::saveLayout(const QVariantMap &layout)
{
    m_layout = layout;
    if (m_daemon->hasMethod("desktop.state.set") && m_daemon->isReady()) {
        m_daemon->request("desktop.state.set", {{"connection_id", m_id}, {"state", layout}});
        return;
    }
    QDir().mkpath(QFileInfo(layoutPath()).absolutePath());
    QFile f(layoutPath());
    if (f.open(QIODevice::WriteOnly | QIODevice::Truncate))
        f.write(QJsonDocument(QJsonObject::fromVariantMap(layout)).toJson(QJsonDocument::Compact));
}

// ---------------------------------------------------------------- DesktopManager

DesktopManager::DesktopManager(DaemonClient *daemon, QObject *parent) : QObject(parent), m_daemon(daemon)
{
    connect(m_daemon, &DaemonClient::event, this, &DesktopManager::onEvent);
    connect(m_daemon, &DaemonClient::stateChanged, this, [this]() { emit daemonStateChanged(); });
    connect(m_daemon, &DaemonClient::connectionLost, this, [this]() {
        for (auto *s : std::as_const(m_sessions))
            s->onDaemonLost();
        for (auto *d : std::as_const(m_desktops))
            d->onDaemonLost();
    });
    connect(m_daemon, &DaemonClient::reconnected, this, [this]() {
        for (auto *d : std::as_const(m_desktops))
            d->onDaemonBack();
        for (auto *s : std::as_const(m_sessions))
            s->onDaemonBack();
    });
}

QString DesktopManager::daemonState() const
{
    switch (m_daemon->state()) {
    case DaemonClient::Ready: return "ready";
    case DaemonClient::Disconnected: return "offline";
    default: return "connecting";
    }
}

QVariantList DesktopManager::desktops() const
{
    QVariantList l;
    for (const QString &id : m_order) {
        DesktopState *d = m_desktops.value(id);
        if (d)
            l.append(QVariantMap{{"connectionId", id}, {"name", d->name()}, {"state", d->state()}});
    }
    return l;
}

DesktopState *DesktopManager::openDesktop(const QString &id, const QString &name)
{
    if (DesktopState *d = m_desktops.value(id))
        return d;
    auto *d = new DesktopState(this, m_daemon, id, name);
    m_desktops.insert(id, d);
    m_order.append(id);
    connect(d, &DesktopState::stateChanged, this, [this]() { emit desktopsChanged(); });
    connect(d, &DesktopState::nameChanged, this, [this]() { emit desktopsChanged(); });
    emit desktopsChanged();
    emit idsChanged();
    m_daemon->start();
    if (m_daemon->isReady())
        d->open();
    else {
        auto conn = std::make_shared<QMetaObject::Connection>();
        *conn = connect(m_daemon, &DaemonClient::stateChanged, d, [this, d, conn](DaemonClient::State st) {
            if (st != DaemonClient::Ready)
                return;
            disconnect(*conn);
            d->open();
        });
    }
    emit desktopOpened(id);
    return d;
}

void DesktopManager::closeDesktop(const QString &id)
{
    DesktopState *d = m_desktops.take(id);
    if (!d)
        return;
    m_order.removeAll(id);
    for (auto it = m_sessions.begin(); it != m_sessions.end();) {
        if (it.value()->connectionId() == id) {
            it.value()->close();
            it = m_sessions.erase(it);
        } else {
            ++it;
        }
    }
    d->close();
    emit desktopsChanged();
    emit idsChanged();
    emit desktopClosed(id);
    d->deleteLater();
}

void DesktopManager::registerSession(TerminalSession *s)
{
    connect(s, &TerminalSession::opened, this, [this, s](const QString &id) {
        m_sessions.insert(id, s);
        const auto pending = m_orphans.take(id);
        for (const QVariantMap &ev : pending)
            s->handleEvent(ev);
    });
    connect(s, &QObject::destroyed, this, [this, s]() {
        for (auto it = m_sessions.begin(); it != m_sessions.end();) {
            if (it.value() == s) it = m_sessions.erase(it); else ++it;
        }
    });
}

void DesktopManager::onEvent(const QVariantMap &ev)
{
    const QString session = ev.value("session").toString();
    if (!session.isEmpty()) {
        if (TerminalSession *s = m_sessions.value(session))
            s->handleEvent(ev);
        else if (m_orphans[session].size() < 500)
            m_orphans[session].append(ev);
        return;
    }
    const QString conn = ev.value("connection_id").toString();
    const QString name = ev.value("event").toString();
    if (name == "daemon_shutdown")
        return;
    if (name.startsWith("multi.")) { // результаты по серверам идут окну, откуда запущена пачка (сверка по run)
        for (auto *d2 : std::as_const(m_desktops))
            emit d2->utilityEvent(ev);
        return;
    }
    if (conn.isEmpty() && !(name == "error" && ev.contains("subscription"))) {
        // события подписок без connection_id (log.line, ошибки операций): окна сами фильтруют по subscription
        for (auto *d2 : std::as_const(m_desktops))
            emit d2->utilityEvent(ev);
        return;
    }
    if (DesktopState *d = m_desktops.value(conn)) {
        d->handleEvent(ev);
    } else if (name == "error" && ev.contains("subscription")) {
        for (auto *d2 : std::as_const(m_desktops))
            d2->handleEvent(ev);
    }
}
