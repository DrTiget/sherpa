#include "DaemonClient.h"

#include <QJsonDocument>
#include <QJsonObject>

DaemonClient::DaemonClient(CoreClient *core, QObject *parent) : QObject(parent), m_core(core)
{
    m_retry.setSingleShot(true);
    connect(&m_retry, &QTimer::timeout, this, &DaemonClient::start);
    connect(&m_sock, &QLocalSocket::readyRead, this, &DaemonClient::onReadyRead);
    connect(&m_sock, &QLocalSocket::disconnected, this, &DaemonClient::onDisconnected);
    connect(&m_sock, &QLocalSocket::errorOccurred, this, [this](QLocalSocket::LocalSocketError) {
        if (m_state == Connecting) {
            m_lastError = m_sock.errorString();
            onDisconnected();
        }
    });
    connect(&m_sock, &QLocalSocket::connected, this, [this]() {
        request("hello", {{"protocol", 1}, {"client", "sherpa-qt/0.1.0"}}, [this](const DaemonReply &r) {
            if (!r.ok) {
                m_lastError = r.error.message;
                emit failed(r.error.code, r.error.message);
                m_sock.abort();
                return;
            }
            m_helloSessions = r.data.toMap().value("sessions").toStringList();
            request("daemon.methods", {}, [this](const DaemonReply &m) {
                m_methods = m.ok ? m.data.toMap().value("items").toStringList() : QStringList();
                m_attempt = 0;
                const bool again = m_wasReady;
                m_wasReady = true;
                setState(Ready);
                if (again)
                    emit reconnected();
            });
        });
    });
}

void DaemonClient::setState(State s)
{
    if (m_state == s)
        return;
    m_state = s;
    emit stateChanged(s);
}

void DaemonClient::start()
{
    m_stopped = false;
    m_retry.stop();
    if (m_state == Starting || m_state == Connecting || m_state == Ready)
        return;
    ensureDaemon();
}

void DaemonClient::stop()
{
    m_stopped = true;
    m_retry.stop();
    m_sock.abort();
    failAll("client.cancelled");
    setState(Disconnected);
}

void DaemonClient::ensureDaemon()
{
    setState(Starting);
    const quint64 id = m_core->run({"daemon", "ensure"});
    auto conn = std::make_shared<QMetaObject::Connection>();
    *conn = connect(m_core, &CoreClient::finished, this, [this, id, conn](quint64 fid, const CoreResponse &r) {
        if (fid != id)
            return;
        disconnect(*conn);
        if (!r.ok) {
            m_lastError = r.error.message;
            emit failed(r.error.code, r.error.message);
            setState(Disconnected);
            scheduleReconnect();
            return;
        }
        m_endpoint = r.data.value("endpoint").toString();
        connectSocket();
    });
}

void DaemonClient::connectSocket()
{
    setState(Connecting);
    m_buf.clear();
    QString name = m_endpoint;
#ifdef Q_OS_WIN
    if (name.startsWith(QLatin1String("\\\\.\\pipe\\")))
        name = name.mid(9);
#endif
    m_sock.abort();
    m_sock.connectToServer(name);
}

void DaemonClient::onReadyRead()
{
    m_buf += m_sock.readAll();
    int nl;
    while ((nl = m_buf.indexOf('\n')) >= 0) {
        const QByteArray line = m_buf.left(nl);
        m_buf.remove(0, nl + 1);
        if (!line.trimmed().isEmpty())
            handleLine(line);
    }
}

void DaemonClient::handleLine(const QByteArray &line)
{
    QJsonParseError pe;
    const QJsonDocument doc = QJsonDocument::fromJson(line, &pe);
    if (pe.error != QJsonParseError::NoError || !doc.isObject())
        return;
    const QJsonObject o = doc.object();
    if (o.contains("event")) {
        emit event(o.toVariantMap());
        return;
    }
    const qint64 id = o.value("id").toVariant().toLongLong();
    auto it = m_pending.find(id);
    if (it == m_pending.end())
        return;
    Callback cb = std::move(*it);
    m_pending.erase(it);
    DaemonReply r;
    r.ok = o.value("ok").toBool();
    if (r.ok) {
        r.data = o.value("data").toVariant();
    } else {
        const QJsonObject e = o.value("error").toObject();
        r.error.code = e.value("code").toString();
        r.error.message = e.value("message").toString();
        r.error.params = e.value("params").toObject().toVariantMap();
    }
    if (cb)
        cb(r);
}

quint64 DaemonClient::request(const QString &method, const QVariantMap &params, Callback cb)
{
    const bool handshake = method == QLatin1String("hello") || method == QLatin1String("daemon.methods");
    if (m_sock.state() != QLocalSocket::ConnectedState || (!handshake && m_state != Ready)) {
        if (cb) {
            DaemonReply r;
            r.error.code = "client.daemon_unavailable";
            r.error.message = "Daemon is not connected";
            QMetaObject::invokeMethod(this, [cb, r]() { cb(r); }, Qt::QueuedConnection);
        }
        return 0;
    }
    const qint64 id = m_nextId++;
    m_pending.insert(id, std::move(cb));
    QJsonObject o{{"id", id}, {"method", method}, {"params", QJsonObject::fromVariantMap(params)}};
    m_sock.write(QJsonDocument(o).toJson(QJsonDocument::Compact) + "\n");
    return quint64(id);
}

void DaemonClient::failAll(const QString &code)
{
    const auto pending = std::exchange(m_pending, {});
    for (auto it = pending.begin(); it != pending.end(); ++it) {
        if (!it.value())
            continue;
        DaemonReply r;
        r.error.code = code;
        r.error.message = code;
        it.value()(r);
    }
}

void DaemonClient::onDisconnected()
{
    const bool wasReady = m_state == Ready;
    setState(Disconnected);
    failAll("client.daemon_unavailable");
    if (wasReady)
        emit connectionLost();
    if (!m_stopped)
        scheduleReconnect();
}

void DaemonClient::scheduleReconnect()
{
    if (!m_autoReconnect || m_stopped || m_retry.isActive())
        return;
    const int ms = qMin(15000, 500 << qMin(m_attempt, 5));
    ++m_attempt;
    m_retry.start(ms);
}
