// Клиент демона ядра: Unix socket / Windows named pipe (QLocalSocket), JSON Lines (CORE_API §4.2).
// Запускает демон через `sherpa-core daemon ensure`, делает hello, сопоставляет ответы по id,
// раздаёт события и сам переподключается с растущей паузой.
#pragma once

#include "CoreClient.h"

#include <QHash>
#include <QLocalSocket>
#include <QObject>
#include <QTimer>
#include <QVariantMap>
#include <functional>

struct DaemonReply {
    bool ok = false;
    QVariant data;
    CoreError error;
};

class DaemonClient : public QObject {
    Q_OBJECT
public:
    enum State { Disconnected, Starting, Connecting, Ready };
    Q_ENUM(State)
    using Callback = std::function<void(const DaemonReply &)>;

    explicit DaemonClient(CoreClient *core, QObject *parent = nullptr);

    State state() const { return m_state; }
    bool isReady() const { return m_state == Ready; }
    QString endpoint() const { return m_endpoint; }
    QStringList methods() const { return m_methods; }
    bool hasMethod(const QString &m) const { return m_methods.contains(m); }
    QStringList knownSessions() const { return m_helloSessions; }
    QString lastError() const { return m_lastError; }

    // Запустить демон и подключиться. Безопасно вызывать повторно.
    void start();
    // Остановить переподключение и закрыть сокет.
    void stop();
    // Автопереподключение (по умолчанию включено).
    void setAutoReconnect(bool on) { m_autoReconnect = on; }
    // Для тестов: оборвать сокет, как при сбое (демон жив, клиент переподключится сам).
    void dropConnectionForTest() { m_sock.abort(); onDisconnected(); }
    // Запрос. Если соединения нет — ответ ошибкой client.daemon_unavailable.
    quint64 request(const QString &method, const QVariantMap &params, Callback cb = {});

signals:
    void stateChanged(DaemonClient::State state);
    // Соединение установлено заново после обрыва (первое подключение — только stateChanged).
    void reconnected();
    void connectionLost();
    void event(const QVariantMap &ev);
    void failed(const QString &code, const QString &text);

private:
    void ensureDaemon();
    void connectSocket();
    void onReadyRead();
    void handleLine(const QByteArray &line);
    void setState(State s);
    void onDisconnected();
    void scheduleReconnect();
    void failAll(const QString &code);

    CoreClient *m_core;
    QLocalSocket m_sock;
    State m_state = Disconnected;
    QString m_endpoint, m_lastError;
    QByteArray m_buf;
    qint64 m_nextId = 1;
    QHash<qint64, Callback> m_pending;
    QStringList m_methods, m_helloSessions;
    bool m_autoReconnect = true;
    bool m_wasReady = false;
    bool m_stopped = true;
    int m_attempt = 0;
    QTimer m_retry;
};
